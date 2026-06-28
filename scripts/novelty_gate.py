#!/usr/bin/env python3
"""
TestVDB Novelty Gate — Pre-submission credibility governance (v1 two-layer)

Consumes:
- Layer 1 (Consumer): Precise match against threat_model.json + local corpora
- Layer 2 (Corrector): GitHub Search API + by-design heuristics

Outputs:
- novelty_gate.json: Per-candidate grading + evidence + endorsement
- final_verdict.json: Aggregated 4-Judge + Gate + endorsement (ADR-0002)

Exit codes:
- 0 = At least one NOVEL endorsement
- 1 = All rejected
- 2 = Has UNVERIFIED (fail-closed signal)

Usage:
    python scripts/novelty_gate.py --session-dir <path> [--github-token <token>]
"""

import argparse
import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Optional, Dict, List

# Add scripts directory to path for imports
SCRIPTS_DIR = Path(__file__).parent
sys.path.insert(0, str(SCRIPTS_DIR))

try:
    from github_search import REPO_MAP, _cache_path, _read_cache, _write_cache
except ImportError:
    # Fallback definitions if github_search.py not available
    REPO_MAP = {
        "milvus": "milvus-io/milvus",
        "qdrant": "qdrant/qdrant",
        "weaviate": "weaviate/weaviate",
        "pgvector": "pgvector/pgvector",
        "meilisearch": "meilisearch/meilisearch",
        "chroma": "chroma-core/chroma",
    }
    CACHE_DIR = Path.home() / ".testvdb" / "github_cache"
    CACHE_TTL = 86400

    def _cache_path(query: str) -> Path:
        h = hashlib.sha256(query.encode()).hexdigest()
        return CACHE_DIR / f"{h}.json"

    def _read_cache(query: str):
        p = _cache_path(query)
        if p.exists():
            data = json.loads(p.read_text(encoding="utf-8"))
            if time.time() - data.get("ts", 0) < CACHE_TTL:
                return data.get("items", [])
        return None

    def _write_cache(query: str, items: list):
        CACHE_DIR.mkdir(parents=True, exist_ok=True)
        p = _cache_path(query)
        p.write_text(json.dumps({"ts": time.time(), "items": items}), encoding="utf-8")


# ── Constants ───────────────────────────────────────────────

# By-design heuristic patterns (ADR-0001 v1)
BY_DESIGN_PATTERNS = [
    r"sentinel",
    r"by\s*\.?\s*design",
    r"intentional",
    r"documented\s+behavior",
    r"let\s+.*?\s+pick",
    r"default\s+value",
    r"expected\s+behavior",
]

GITHUB_API = "https://api.github.com/search/issues"

# ── Helpers ──────────────────────────────────────────────────

def safe_read(filepath: Path) -> Optional[Any]:
    """Read JSON file safely."""
    try:
        with open(filepath, encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def safe_read_text(filepath: Path) -> Optional[str]:
    """Read text file safely."""
    try:
        with open(filepath, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return None


def extract_param_name(param: Optional[str]) -> Optional[str]:
    """Extract the leading param identifier from a stage2 aggregation `param` field.

    'ef=0' -> 'ef', 'dynamicEfMin(512)>dynamicEfMax(8)' -> 'dynamicEfMin',
    'vectorCacheMaxObjects=-1' -> 'vectorCacheMaxObjects',
    'pq.centroids=0' -> 'pq.centroids'. The dotted form is preserved on purpose so
    'pq.centroids' does not over-match a corpus title/body that only has 'centroids'.
    """
    if not param:
        return None
    m = re.match(r"[A-Za-z][\w.]*", param)
    return m.group(0) if m else None


def param_in(param_name: Optional[str], text: Optional[str]) -> bool:
    """Word-boundary param match (case-insensitive).

    'ef' matches '"ef": -1' / ' ef ' but NOT 'default' / 'before'. The lookarounds
    treat [A-Za-z0-9_] as word chars so 'pq.centroids' matches literally while
    'centroids' alone (in a JSON schema dump) does not over-match for 'pq.centroids'.
    """
    if not param_name or not text:
        return False
    return re.search(
        rf"(?<![A-Za-z0-9_]){re.escape(param_name)}(?![A-Za-z0-9_])",
        text,
        re.IGNORECASE,
    ) is not None


def extract_endpoint(defect_id: str) -> str:
    """Extract endpoint from defect_id."""
    parts = defect_id.split("_")
    if len(parts) >= 2:
        return parts[1]
    return ""


def is_boundary_defect(defect_id: str) -> bool:
    """Check if defect_id contains 'boundary'."""
    return "boundary" in defect_id.lower()


def precision_level(defect_id: str) -> str:
    """Determine precision level from defect_id."""
    if "boundary" in defect_id.lower():
        return "HIGH"  # boundary + param = precise match
    return "LOW"  # state/semantic = low precision


# ── Layer 1: Consumer ─────────────────────────────────────────

def load_consumer_data(session_dir: Path, target: str) -> Dict:
    """Load threat_model.json and local corpora for consumer layer."""
    intelligence_dir = Path("intelligence") / target

    threat_model = safe_read(intelligence_dir / "threat_model.json")
    issue_corpus = safe_read(intelligence_dir / "issue_corpus.json")
    commit_corpus = safe_read(intelligence_dir / "commit_corpus.json")

    # issue_corpus / commit_corpus are {_meta, issues|merged_prs: [...]} dicts
    return {
        "threat_model": threat_model or {},
        "issue_corpus": (issue_corpus or {}).get("issues", []),
        "commit_corpus": (commit_corpus or {}).get("merged_prs", []),
        "repo": REPO_MAP.get(target, ""),
    }


def consumer_layer_check(
    defect_id: str,
    endpoint: str,
    param_name: Optional[str],
    defect_type: str,
    consumer_data: Dict,
) -> Optional[Dict]:
    """Layer 1: Precise match against threat_model.json + local corpora."""
    threat_model = consumer_data["threat_model"]
    if not param_name:
        return None
    repo = consumer_data.get("repo", "")

    novelty_ctx = threat_model.get("judge_enhancements", {}).get("novelty_context", {})

    # known_ongoing_issues is a list of issue NUMBERS (ints) — it cannot be matched
    # against a param name; it is bridged via the issue_corpus title search below.

    # recently_fixed_patterns: [{pattern, fix_pr}] -> COVERED_BY_PR (only if a fix PR exists)
    for fix in novelty_ctx.get("recently_fixed_patterns", []):
        if param_in(param_name, fix.get("pattern", "")):
            pr = str(fix.get("fix_pr", "")).strip()
            if pr:
                pr_url = f"https://github.com/{repo}/pull/{pr}" if (repo and pr.isdigit()) else pr
                return {
                    "layer": "consumer",
                    "grade": "COVERED_BY_PR",
                    "evidence_url": pr_url,
                    "match_type": "recently_fixed_pattern",
                    "confidence": "HIGH",
                }

    # by_design_behaviors: [{pattern, rationale}] -> BY_DESIGN
    for behavior in threat_model.get("defect_criteria", {}).get("by_design_behaviors", []):
        if param_in(param_name, behavior.get("pattern", "")):
            return {
                "layer": "consumer",
                "grade": "BY_DESIGN",
                "evidence_url": "",
                "match_type": "by_design_behavior",
                "confidence": "HIGH",
            }

    # issue_corpus: title match only (body is avoided — schema dumps trigger false positives)
    for issue in consumer_data.get("issue_corpus", []):
        if param_in(param_name, issue.get("title", "")):
            return {
                "layer": "consumer",
                "grade": "KNOWN_OPEN",
                "evidence_url": issue.get("url", ""),
                "match_type": "issue_corpus_match",
                "confidence": "HIGH",
            }

    # commit_corpus (merged PRs): title match
    for pr in consumer_data.get("commit_corpus", []):
        if param_in(param_name, pr.get("title", "")):
            return {
                "layer": "consumer",
                "grade": "COVERED_BY_PR",
                "evidence_url": pr.get("url", ""),
                "match_type": "commit_corpus_match",
                "confidence": "HIGH",
            }

    return None


# ── Layer 2: Corrector ────────────────────────────────────────

def search_github_api(
    query: str,
    token: Optional[str],
    repo: str,
) -> tuple[List[Dict], bool, Any]:
    """Search GitHub API with caching."""
    import requests

    headers = {"Accept": "application/vnd.github.v3+json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    try:
        # Extended query: is:issue+is:pr, open+closed
        full_query = f"repo:{repo} {query} is:issue is:pr"
        resp = requests.get(
            GITHUB_API,
            params={"q": full_query, "per_page": 30},
            headers=headers,
            timeout=15
        )
        remaining = int(resp.headers.get("X-RateLimit-Remaining", 999))

        if remaining < 5:
            cached = _read_cache(full_query)
            if cached is not None:
                return cached, True, remaining

        resp.raise_for_status()
        items = resp.json().get("items", [])
        _write_cache(full_query, items)
        return items, False, remaining
    except Exception as e:
        cached = _read_cache(query)
        if cached is not None:
            return cached, True, "cache-fallback"
        return [], False, str(e)


def check_by_design_heuristic(title: str, body: str, param_name: Optional[str]) -> bool:
    """Check if PR/issue shows by-design behavior."""
    text = f"{title} {body}".lower()

    # Check if any by-design pattern matches
    for pattern in BY_DESIGN_PATTERNS:
        if re.search(pattern, text, re.IGNORECASE):
            # If param_name provided, check it's mentioned
            if param_name and param_name.lower() in text:
                return True
            elif not param_name:
                # No param_name but by-design signal present
                return True

    return False


def corrector_layer_check(
    defect_id: str,
    endpoint: str,
    param_name: Optional[str],
    defect_type: str,
    target: str,
    github_token: Optional[str],
) -> Optional[Dict]:
    """Layer 2: GitHub Search API + by-design heuristics."""
    repo = REPO_MAP.get(target, "")
    if not repo:
        return None

    # Build search query
    query_parts = []
    if param_name:
        query_parts.append(param_name)
    if defect_type and defect_type != "unknown":
        query_parts.append(defect_type.replace("_", " "))

    query = " ".join(query_parts) if query_parts else ""
    if not query:
        return None

    # Search GitHub
    items, from_cache, remaining = search_github_api(query, github_token, repo)

    if not param_name:
        return None

    # Check results. A param that appears in title OR body of a returned issue/PR is
    # prior art (reject). Title match is precise; body match additionally catches PRs
    # whose title is generic (e.g. #11439 "validate hnsw numeric ranges") but whose
    # body lists the param. Closed issues count too (known, possibly fixed).
    for item in items:
        title = item.get("title", "") or ""
        body = item.get("body", "") or ""
        url = item.get("html_url", "") or ""
        text = f"{title} {body}".lower()
        is_pr = bool(item.get("pull_request"))

        # by-design heuristic (semi-auto reject)
        if check_by_design_heuristic(title, body, param_name):
            return {
                "layer": "corrector",
                "grade": "BY_DESIGN_SUSPECTED",
                "evidence_url": url,
                "match_type": "by_design_heuristic",
                "confidence": "MEDIUM",
                "from_cache": from_cache,
            }

        if param_in(param_name, text):
            grade = "COVERED_BY_PR" if is_pr else "KNOWN_OPEN"
            return {
                "layer": "corrector",
                "grade": grade,
                "evidence_url": url,
                "match_type": "github_pr" if is_pr else "github_issue",
                "confidence": "HIGH",
                "from_cache": from_cache,
            }

    return None


# ── Grading Logic ─────────────────────────────────────────────

def grade_candidate(
    defect_id: str,
    endpoint: str,
    param_name: Optional[str],
    defect_type: str,
    consumer_data: Dict,
    target: str,
    github_token: Optional[str],
) -> Dict:
    """Grade a candidate defect through both layers."""

    # Try Layer 1 (Consumer) first
    consumer_result = consumer_layer_check(
        defect_id, endpoint, param_name, defect_type, consumer_data
    )

    if consumer_result:
        return consumer_result

    # Try Layer 2 (Corrector)
    try:
        corrector_result = corrector_layer_check(
            defect_id, endpoint, param_name, defect_type, target, github_token
        )

        if corrector_result:
            return corrector_result
    except Exception as e:
        # Query failed - mark UNVERIFIED
        return {
            "layer": "corrector",
            "grade": "UNVERIFIED",
            "evidence_url": "",
            "match_type": "query_failed",
            "confidence": "NONE",
            "error": str(e),
        }

    # No hits - NOVEL
    return {
        "layer": "gate",
        "grade": "NOVEL",
        "evidence_url": "",
        "match_type": "no_known_hits",
        "confidence": "HIGH",
    }


def apply_precision_grading(result: Dict, defect_id: str) -> Dict:
    """Apply precision-based grading (ADR-0001)."""
    grade = result["grade"]
    precision = precision_level(defect_id)

    # High precision (boundary + param) → direct rejection
    if precision == "HIGH":
        if grade in ["KNOWN_OPEN", "COVERED_BY_PR", "BY_DESIGN"]:
            result["endorsement"] = False
            result["endorsement_reason"] = f"High-precision {grade} match"
            return result

    # Low precision (state/semantic) → downgrade to UNVERIFIED
    if precision == "LOW":
        if grade in ["KNOWN_OPEN", "COVERED_BY_PR"]:
            result["original_grade"] = grade
            result["grade"] = "UNVERIFIED"
            result["endorsement"] = False
            result["endorsement_reason"] = f"Low-precision {grade} downgraded to UNVERIFIED"
            return result

    # BY_DESIGN_SUSPECTED stays as-is (semi-auto)
    if grade == "BY_DESIGN_SUSPECTED":
        result["endorsement"] = False
        result["endorsement_reason"] = "BY_DESIGN suspected (manual review needed)"
        return result

    # NOVEL gets endorsement
    if grade == "NOVEL":
        result["endorsement"] = True
        result["endorsement_reason"] = "No known hits"

    # UNVERIFIED gets no endorsement
    if grade == "UNVERIFIED":
        result["endorsement"] = False
        result["endorsement_reason"] = "Query failed or evidence incomplete"

    return result


# ── Main Execution ───────────────────────────────────────────

def load_stage2_aggregation(session_dir: Path) -> Optional[Dict]:
    """Load the latest stage2_aggregation*.json file."""
    debate_logs_dir = session_dir / "debate_logs"

    # Find all stage2_aggregation files
    aggregation_files = sorted(
        debate_logs_dir.glob("stage2_aggregation*.json"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )

    if not aggregation_files:
        return None

    # Use the most recent one
    return safe_read(aggregation_files[0])


def extract_confirmed_defects(aggregation: Dict) -> List[Dict]:
    """Extract confirmed defects from stage2_aggregation.json."""
    confirmed = aggregation.get("confirmed_defects", [])
    return confirmed if isinstance(confirmed, list) else []


def run_novelty_gate(session_dir: Path, github_token: Optional[str]) -> Dict:
    """Run the full novelty gate pipeline."""

    # Load stage2 aggregation first — it is the authoritative target source. The
    # contract file is not always present in the session dir; without this fallback
    # target becomes "unknown", which empties both gate layers and endorses everything.
    aggregation = load_stage2_aggregation(session_dir)
    if not aggregation:
        return {"error": "No stage2_aggregation*.json found"}

    contract = safe_read(session_dir / "structured_contract.json") or {}
    target = (aggregation.get("target") or contract.get("target") or "unknown").lower()

    # Extract confirmed defects
    confirmed_defects = extract_confirmed_defects(aggregation)

    if not confirmed_defects:
        return {"error": "No confirmed defects found"}

    # Load consumer data
    consumer_data = load_consumer_data(session_dir, target)

    # Grade each candidate. Stage2 aggregation entries are param-level and share a
    # single defect_id ("defect-2"); that collapses all 7 into one bucket and must
    # not be used as the key. Key by `script` (unique per candidate) and derive
    # param_name from the `param` field.
    results = {}
    for defect in confirmed_defects:
        script = defect.get("script", "") or defect.get("candidate", "")
        param_str = defect.get("param", "")
        param_name = extract_param_name(param_str)
        defect_type = defect.get("defect_type", "unknown")
        identifier = script or defect.get("defect_id", "")

        grade_result = grade_candidate(
            identifier, "", param_name, defect_type, consumer_data, target, github_token
        )

        # Apply precision-based grading
        final_result = apply_precision_grading(grade_result, identifier)

        # Add defect metadata
        final_result.update({
            "defect_id": defect.get("defect_id", ""),
            "script": script,
            "param": param_str,
            "param_name": param_name,
            "defect_type": defect_type,
            "precision": precision_level(identifier),
        })

        results[identifier] = final_result

    return results


def generate_final_verdict(
    session_dir: Path,
    gate_results: Dict,
    aggregation: Dict,
) -> Dict:
    """Generate final_verdict.json (ADR-0002)."""
    verdict = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "session_dir": str(session_dir),
        "total_defects": len(gate_results),
        "defects": [],
    }

    for key, gate_result in gate_results.items():
        # Locate the aggregation entry by `script`/`candidate` (defect_id collapses
        # to a single bucket and cannot identify a candidate).
        defect_data = next(
            (d for d in aggregation.get("confirmed_defects", [])
             if d.get("script") == key or d.get("candidate") == key),
            {},
        )

        # Judge fields are FLAT strings in the aggregation (doc/evidence/novelty/
        # severity), not nested {verdict: ...} dicts.
        judge_novelty = defect_data.get("novelty", "UNKNOWN")
        verdict_entry = {
            "defect_id": defect_data.get("defect_id", gate_result.get("defect_id", "")),
            "script": key,
            "param": defect_data.get("param", ""),
            "param_name": gate_result.get("param_name", ""),
            "defect_type": gate_result.get("defect_type", ""),
            "judge_doc": defect_data.get("doc", "UNKNOWN"),
            "judge_evidence": defect_data.get("evidence", "UNKNOWN"),
            "judge_novelty": judge_novelty,
            "judge_severity": defect_data.get("severity", "UNKNOWN"),
            "gate_grade": gate_result.get("grade", "UNKNOWN"),
            "gate_layer": gate_result.get("layer", "unknown"),
            "gate_evidence_url": gate_result.get("evidence_url", ""),
            "endorsement": gate_result.get("endorsement", False),
            "endorsement_reason": gate_result.get("endorsement_reason", ""),
            # discrepancy = judge leaned NOVEL/NOVEL_SIMILAR but the gate rejected
            # (the gate's value-add over the recall-biased judge).
            "judge_discrepancy": (
                "NOVEL" in str(judge_novelty).upper()
                and gate_result.get("grade") != "NOVEL"
            ),
        }

        verdict["defects"].append(verdict_entry)

    return verdict


def main():
    parser = argparse.ArgumentParser(
        description="TestVDB Novelty Gate — Pre-submission credibility governance"
    )
    parser.add_argument("--session-dir", required=True, help="Session directory path")
    parser.add_argument("--github-token", default=None, help="GitHub API token")
    args = parser.parse_args()

    session_dir = Path(args.session_dir)
    if not session_dir.exists():
        print(f"ERROR: Session directory not found: {args.session_dir}", file=sys.stderr)
        sys.exit(2)

    # Get GitHub token
    github_token = args.github_token or os.environ.get("GITHUB_TOKEN")

    # Run novelty gate
    gate_results = run_novelty_gate(session_dir, github_token)

    if "error" in gate_results:
        print(f"ERROR: {gate_results['error']}", file=sys.stderr)
        sys.exit(2)

    # Load aggregation for final verdict
    aggregation = load_stage2_aggregation(session_dir)

    # Generate final verdict
    final_verdict = generate_final_verdict(session_dir, gate_results, aggregation or {})

    # Write outputs
    debate_logs_dir = session_dir / "debate_logs"
    debate_logs_dir.mkdir(parents=True, exist_ok=True)

    # Write novelty_gate.json
    gate_output = debate_logs_dir / "novelty_gate.json"
    with open(gate_output, "w", encoding="utf-8") as f:
        json.dump(gate_results, f, indent=2, ensure_ascii=False)

    # Write final_verdict.json
    verdict_output = debate_logs_dir / "final_verdict.json"
    with open(verdict_output, "w", encoding="utf-8") as f:
        json.dump(final_verdict, f, indent=2, ensure_ascii=False)

    # Calculate exit code
    endorsed = sum(1 for r in gate_results.values() if r.get("endorsement"))
    unverified = sum(1 for r in gate_results.values() if r.get("grade") == "UNVERIFIED")
    total = len(gate_results)

    print(f"Novelty Gate: {endorsed}/{total} endorsed (NOVEL), {unverified} UNVERIFIED")
    print(f"Outputs: {gate_output}, {verdict_output}")

    # Exit codes
    if endorsed > 0:
        sys.exit(0)  # At least one NOVEL
    elif unverified > 0:
        sys.exit(2)  # Has UNVERIFIED (fail-closed)
    else:
        sys.exit(1)  # All rejected


if __name__ == "__main__":
    if sys.platform == "win32":
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    main()
