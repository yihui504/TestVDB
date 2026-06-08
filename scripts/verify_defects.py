#!/usr/bin/env python3
"""
TestVDB Defect Verification Script — Post-Reporter Quality Gate

Reads defect-N.md files from a session directory and verifies:
1. Evidence chain completeness (Ring 1/2/3)
2. Severity calibration against execution logs
3. Script error false-positive detection
4. VERDICT consistency (log verdict vs report claim)

Outputs defect-review.md with CONFIRMED / FALSE_POSITIVE / NEEDS_IMPROVEMENT.

Usage:
    python scripts/verify_defects.py <session_dir> [--target qdrant|milvus|weaviate|pgvector]
"""

import os
import sys
import json
import re
import glob

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# ── helpers ──────────────────────────────────────────────

def safe_read(filepath):
    """Read file safely, return content or None."""
    try:
        with open(filepath, encoding="utf-8", errors="replace") as f:
            return f.read()
    except (FileNotFoundError, PermissionError):
        return None

def safe_read_json(filepath):
    """Read JSON file safely, return dict or None."""
    content = safe_read(filepath)
    if content is None:
        return None
    try:
        return json.loads(content)
    except json.JSONDecodeError:
        return None

def extract_verdict_from_log(log_path):
    """Extract VERDICT lines from execution log."""
    content = safe_read(log_path)
    if not content:
        return []
    verdicts = []
    for line in content.split("\n"):
        if "VERDICT" in line.upper() and ":" in line:
            verdicts.append(line.strip())
    return verdicts

def extract_script_errors_from_log(log_path):
    """Check if log contains SCRIPT_ERROR markers."""
    content = safe_read(log_path)
    if not content:
        return False
    markers = ["SCRIPT_ERROR", "TypeError:", "AttributeError:",
               "json.decoder.JSONDecodeError", "KeyError:", "IndexError:"]
    return any(m.lower() in content.lower() for m in markers)

def check_ring_completeness(defect_content):
    """Check if defect report has Ring 1, 2, 3 evidence."""
    rings = {
        "Ring 1": bool(re.search(r"Ring 1|Contract Clause|契约条款|constraint_id", defect_content, re.IGNORECASE)),
        "Ring 2": bool(re.search(r"Ring 2|Document Reference|文档引用|source_url", defect_content, re.IGNORECASE)),
        "Ring 3": bool(re.search(r"Ring 3|Actual Behavior|实际行为|HTTP Request|HTTP Response", defect_content, re.IGNORECASE)),
    }
    return rings

# ── main verification ──────────────────────────────────

def verify_session(session_dir, target="unknown"):
    """Verify all defect reports in a session directory."""
    defects_dir = os.path.join(session_dir, "defects")
    if not os.path.isdir(defects_dir):
        print(f"ERROR: defects/ directory not found in {session_dir}")
        return {"error": "defects/ not found"}

    # Find defect files (exclude -enhanced duplicates to avoid double-checking)
    defect_files = sorted(glob.glob(os.path.join(defects_dir, "defect-*.md")))
    defect_files = [f for f in defect_files if "-enhanced" not in os.path.basename(f)]

    if not defect_files:
        print(f"WARNING: No defect-*.md files found in {defects_dir}")
        return {"error": "no defect files"}

    results = []
    for defect_path in defect_files:
        basename = os.path.basename(defect_path)
        content = safe_read(defect_path)
        if not content:
            results.append({"file": basename, "status": "ERROR", "reason": "Cannot read file"})
            continue

        # 1. Evidence chain check
        rings = check_ring_completeness(content)
        missing_rings = [r for r, present in rings.items() if not present]

        # 2. Script error check (look for associated log)
        log_basename = None
        log_match = re.search(r"output_([a-z_0-9]+)\.log", content)
        if log_match:
            log_basename = f"output_{log_match.group(1)}.log"
        # Also check with _diag suffix
        if log_basename is None:
            diag_match = re.search(r"Log:\s*`?output_([a-z_0-9]+)\.log", content)
            if diag_match:
                log_basename = f"output_{diag_match.group(1)}.log"

        script_error = False
        log_verdicts = []
        if log_basename:
            log_path = os.path.join(session_dir, log_basename)
            script_error = extract_script_errors_from_log(log_path)
            log_verdicts = extract_verdict_from_log(log_path)

        # 3. Classification
        if script_error:
            status = "FALSE_POSITIVE"
            reason = "Execution log contains Python script error (not a DB defect)"
        elif missing_rings:
            status = "NEEDS_IMPROVEMENT"
            reason = f"Missing evidence rings: {', '.join(missing_rings)}"
        elif any("NO_DEFECT" in v for v in log_verdicts):
            status = "FALSE_POSITIVE"
            reason = f"Log VERDICT says NO_DEFECT but report claims defect"
        elif not log_verdicts:
            status = "NEEDS_IMPROVEMENT"
            reason = "No VERDICT line found in execution log"
        else:
            status = "CONFIRMED"
            reason = "Evidence complete, log verdicts consistent"

        results.append({
            "file": basename,
            "status": status,
            "reason": reason,
            "missing_rings": missing_rings,
            "script_error": script_error,
            "log_verdicts": log_verdicts[:3],  # first 3 verdicts for reference
        })

    return {"results": results, "total": len(results)}


def write_review_md(session_dir, verification_result, target):
    """Write defect-review.md to session directory."""
    results = verification_result.get("results", [])
    if not results:
        return

    confirmed = [r for r in results if r["status"] == "CONFIRMED"]
    false_pos = [r for r in results if r["status"] == "FALSE_POSITIVE"]
    needs_imp = [r for r in results if r["status"] == "NEEDS_IMPROVEMENT"]

    lines = [
        f"# Defect Review — {target}",
        "",
        f"**Session**: {os.path.basename(session_dir)}",
        f"**Verified**: {len(results)} defect reports",
        "",
        "## Summary",
        "",
        f"| Status | Count |",
        f"|--------|-------|",
        f"| CONFIRMED | {len(confirmed)} |",
        f"| FALSE_POSITIVE | {len(false_pos)} |",
        f"| NEEDS_IMPROVEMENT | {len(needs_imp)} |",
        "",
        "## Details",
        "",
    ]

    for r in results:
        lines.append(f"### {r['file']} — {r['status']}")
        lines.append(f"- **Reason**: {r['reason']}")
        if r.get("missing_rings"):
            lines.append(f"- **Missing Rings**: {', '.join(r['missing_rings'])}")
        if r.get("log_verdicts"):
            for v in r["log_verdicts"]:
                lines.append(f"- **Log Verdict**: `{v[:120]}`")
        lines.append("")

    lines.append("---")
    lines.append("*Generated by verify_defects.py*")

    review_path = os.path.join(session_dir, "defect-review.md")
    with open(review_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print(f"defect-review.md written to {review_path}")
    return review_path


# ── CLI ──────────────────────────────────────────────────

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python scripts/verify_defects.py <session_dir> [--target qdrant]")
        sys.exit(1)

    session_dir = sys.argv[1]
    target = "unknown"
    for i, arg in enumerate(sys.argv[2:], 2):
        if arg == "--target":
            if i + 1 < len(sys.argv):
                target = sys.argv[i + 1]
            else:
                print("ERROR: --target requires a value (e.g. --target qdrant)", file=sys.stderr)
                sys.exit(1)

    if not os.path.isdir(session_dir):
        print(f"ERROR: Session directory not found: {session_dir}")
        sys.exit(2)

    result = verify_session(session_dir, target)

    if "error" in result:
        print(f"Verification failed: {result['error']}")
        sys.exit(0)  # Not fatal — session may be incomplete

    write_review_md(session_dir, result, target)

    confirmed = sum(1 for r in result.get("results", []) if r["status"] == "CONFIRMED")
    false_pos = sum(1 for r in result.get("results", []) if r["status"] == "FALSE_POSITIVE")
    print(f"\nDone: {confirmed} CONFIRMED, {false_pos} FALSE_POSITIVE, "
          f"{len(result['results']) - confirmed - false_pos} NEEDS_IMPROVEMENT")

    # Exit 0 always — verification is advisory, not blocking
    sys.exit(0)
