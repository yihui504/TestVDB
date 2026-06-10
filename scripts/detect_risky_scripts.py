#!/usr/bin/env python3
"""Stage 1 script error heuristic detector — scans attack scripts for risky patterns.

Detects Python error patterns (TypeError, AttributeError, JSONDecodeError, etc.)
that appear in script source code without corresponding error handling (safe_request
or try/except blocks). Scripts with these patterns are marked as RISKY_SCRIPT for
priority review in execution logs.

Usage:
  python scripts/detect_risky_scripts.py <session_dir>
"""
from __future__ import annotations
import glob
import json
import os
import sys

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

ERROR_PATTERNS = [
    "'str' object has no attribute 'get'",
    "TypeError",
    "AttributeError",
    "json.decoder.JSONDecodeError",
    "KeyError:",
    "IndexError:",
]


def detect_risky_scripts(session_dir: str) -> list[dict]:
    """Scan all .py files in session_dir for risky patterns without error handling.

    Returns list of dicts with 'file' and 'risk' fields.
    """
    risky = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f:
            continue
        try:
            with open(f, encoding="utf-8", errors="replace") as fh:
                content = fh.read()
        except OSError:
            continue

        for pat in ERROR_PATTERNS:
            if pat.lower() in content.lower():
                # Check if the script has corresponding robust handling
                if "safe_request" not in content and "try:" not in content:
                    rel = os.path.relpath(f, session_dir)
                    risky.append(
                        {
                            "file": rel,
                            "risk": f"contains '{pat}' without error handling",
                        }
                    )
                    break  # One risk flag per script

    return risky


def main():
    if len(sys.argv) < 2:
        print("Usage: python scripts/detect_risky_scripts.py <session_dir>", file=sys.stderr)
        sys.exit(1)

    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        sys.exit(2)

    findings = detect_risky_scripts(session_dir)

    if findings:
        print(f"[Stage 1] Script Error Heuristic: {len(findings)} RISKY_SCRIPT(s) detected")
        for f in findings:
            print(f"  RISKY_SCRIPT: {f['file']} — {f['risk']}")
        print(json.dumps({"risky_scripts": findings}, indent=2, ensure_ascii=False))
    else:
        print("[Stage 1] Script Error Heuristic: all scripts pass")

    sys.exit(0)


if __name__ == "__main__":
    main()
