#!/usr/bin/env python3
"""TestVDB Novelty Gate — Pre-submission credibility governance.

Usage:
    python scripts/novelty_gate.py --session-dir <path> [--github-token <token>]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path
from typing import Any, Optional, Dict, List

SCRIPTS_DIR = Path(__file__).parent
sys.path.insert(0, str(SCRIPTS_DIR))

from _pipeline_utils import setup_encoding, extract_confirmed

try:
    from github_search import REPO_MAP, _cache_path, _read_cache, _write_cache
except ImportError:
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
    try:
        with open(filepath, encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def safe_read_text(filepath: Path) -> Optional[str]:
    try:
        with open(filepath, encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return None


def extract_param_name(param: Optional[str]) -> Optional[str]:
    """Extract the leading param identifier from a stage2 aggregation `param` field."""
    if not param:
        return None
    m = re.match(r"[A-Za-z][\w.]*", param)
    return m.group(0) if m else None


def param_in(param_name: Optional[str], text: Optional[str]) -> bool:
    """Word-boundary param match (case-insensitive)."""
    if not param_name or not text:
        return False
    return re.search(
        rf"(?<![A-Za-z0-9_]){re.escape(param_name)}(?![A-Za-z0-9_])",
        text,
        re.IGNORECASE,
    ) is not None


def extract_endpoint(defect_id: str) -> str:
    parts = defect_id.split("_")
    if len(parts) >= 2:
        return parts[1]
    return ""


def is_boundary_defect(defect_id: str) -> bool:
    return "boundary" in defect_id.lower()


def precision_level(defect_id: str) -> str:
    if "boundary" in defect_id.lower():
        return "HIGH"
    return "LOW"


# ── Layer 1: Consumer ─────────────────────────────────────────

def load_consumer_data(session_dir: Path, target: str) -> Dict:
    intelligence_dir = Path("intelligence") / target

    threat_model = safe_read(intelligence_dir / "threat_model.json")
    issue_corpus = safe_read(intelligence_dir / "issue_corpus.json")
    commit_corpus = safe_read(intelligence_dir / "commit_corpus.json")

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
    threat_model = consumer_data["threat_model"]
    if not param_name:
        return None
    repo = consumer_data.get("repo", "")

    novelty_ctx = threat_model.get("judge_enhancements", {}).get("novelty_context", {})

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
    import requests

    headers = {"Accept": "application/vnd.github.v3+json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    try:
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
    text = f"{title} {body}".lower()
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
    repo = REPO_MAP.get(target, "")
    if not repo:
        return None

    query_parts = []
    if param_name:
        query_parts.append(param_name)
    if defect_type and defect_type != "unknown":
        query_parts.append(defect_type.replace("_", " "))

    query = " ".join(query_parts) if query_parts else ""
    if not query:
        return None
    items, from_cache, remaining = search_github_api(query, github_token, repo)

    if not param_name:
        return None
    for item in items:
        title = item.get("title", "") or ""
        body = item.get("body", "") or ""
        url = item.get("html_url", "") or ""
        text = f"{title} {body}".lower()
        is_pr = bool(item.get("pull_request"))
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

    consumer_result = consumer_layer_check(
        defect_id, endpoint, param_name, defect_type, consumer_data
    )

    if consumer_result:
        return consumer_result

    try:
        corrector_result = corrector_layer_check(
            defect_id, endpoint, param_name, defect_type, target, github_token
        )

        if corrector_result:
            return corrector_result
    except Exception as e:
        return {
            "layer": "corrector",
            "grade": "UNVERIFIED",
            "evidence_url": "",
            "match_type": "query_failed",
            "confidence": "NONE",
            "error": str(e),
        }

    if not param_name:
        return {
            "layer": "gate",
            "grade": "UNVERIFIED",
            "evidence_url": "",
            "match_type": "no_param_identifier",
            "confidence": "LOW",
            "reason": "param 不可用 — 无法对照 threat_model/corpus/github 验证（代码版 aggregation schema）",
        }
    return {
        "layer": "gate",
        "grade": "NOVEL",
        "evidence_url": "",
        "match_type": "no_known_hits",
        "confidence": "HIGH",
    }


def apply_precision_grading(result: Dict, defect_id: str) -> Dict:
    grade = result["grade"]
    precision = precision_level(defect_id)

    if precision == "HIGH":
        if grade in ["KNOWN_OPEN", "COVERED_BY_PR", "BY_DESIGN"]:
            result["endorsement"] = False
            result["endorsement_reason"] = f"High-precision {grade} match"
            return result

    if precision == "LOW":
        if grade in ["KNOWN_OPEN", "COVERED_BY_PR"]:
            result["original_grade"] = grade
            result["grade"] = "UNVERIFIED"
            result["endorsement"] = False
            result["endorsement_reason"] = f"Low-precision {grade} downgraded to UNVERIFIED"
            return result

    if grade == "BY_DESIGN_SUSPECTED":
        result["endorsement"] = False
        result["endorsement_reason"] = "BY_DESIGN suspected (manual review needed)"
        return result

    if grade == "NOVEL":
        result["endorsement"] = True
        result["endorsement_reason"] = "No known hits"

    if grade == "UNVERIFIED":
        result["endorsement"] = False
        result["endorsement_reason"] = "Query failed or evidence incomplete"

    return result


# ── Main Execution ───────────────────────────────────────────

def load_stage2_aggregation(session_dir: Path) -> Optional[Dict]:
    debate_logs_dir = session_dir / "debate_logs"
    aggregation_files = sorted(
        debate_logs_dir.glob("stage2_aggregation*.json"),
        key=lambda p: p.stat().st_mtime,
        reverse=True,
    )

    if not aggregation_files:
        return None

    return safe_read(aggregation_files[0])


def run_novelty_gate(session_dir: Path, github_token: Optional[str]) -> Dict:
    aggregation = load_stage2_aggregation(session_dir)
    if not aggregation:
        return {"error": "No stage2_aggregation*.json found"}

    contract = safe_read(session_dir / "structured_contract.json") or {}
    target = (aggregation.get("target") or contract.get("target") or "unknown").lower()

    confirmed_defects = extract_confirmed(aggregation)

    if not confirmed_defects:
        return {"error": "No confirmed defects found"}

    consumer_data = load_consumer_data(session_dir, target)

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

        final_result = apply_precision_grading(grade_result, identifier)

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
    verdict = {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "session_dir": str(session_dir),
        "total_defects": len(gate_results),
        "defects": [],
    }

    for key, gate_result in gate_results.items():
        all_defects = extract_confirmed(aggregation)
        defect_data = next(
            (d for d in all_defects
             if d.get("script") == key or d.get("candidate") == key
             or d.get("defect_id") == key),
            {},
        )

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
            "judge_discrepancy": (
                str(judge_novelty).lower() in ("new", "new_similar", "novel", "novel_similar")
                and gate_result.get("grade") not in ("NOVEL",)
            ),
        }

        verdict["defects"].append(verdict_entry)

    return verdict


def _self_check() -> None:
    """Test schema compatibility, param handling, and precision grading."""
    import tempfile

    failures = []

    def expect(cond, msg):
        failures.append(msg) if not cond else None

    expect(extract_confirmed({"confirmed": {"d1": {"severity_level": "high"}}}) ==
           [{"defect_id": "d1", "severity_level": "high"}], "dict schema → list with defect_id injected")
    expect(extract_confirmed({"confirmed_defects": [{"defect_id": "x"}]}) ==
           [{"defect_id": "x"}], "legacy list passthrough")
    expect(extract_confirmed({"defects": [{"defect_id": "y"}]}) == [],
           "BUG 回归：`defects` key 不是合法 schema → 空")
    expect(extract_confirmed({}) == [], "空 aggregation → 空")
    expect(extract_confirmed(None) == [], "None → 空")

    # 2. grade_candidate：param_name=None → UNVERIFIED（保守治理，不 endorse 未验证）
    # Monkeypatch 两个 layer 为 no-op，隔离 GitHub API
    orig_consumer = consumer_layer_check
    orig_corrector = corrector_layer_check
    novelty_gate_module = sys.modules[__name__]
    novelty_gate_module.consumer_layer_check = lambda *a, **k: None
    novelty_gate_module.corrector_layer_check = lambda *a, **k: None
    try:
        # param_name=None（代码版 schema，无 param 字段）
        r = grade_candidate("d1", "", None, "unknown", {"threat_model": {}, "repo": ""}, "chroma", None)
        expect(r["grade"] == "UNVERIFIED", f"param 缺失 → UNVERIFIED（非 NOVEL），实际 {r['grade']}")
        expect(r["confidence"] == "LOW", "UNVERIFIED 保守 confidence=LOW")
        # param_name 有值 + 两 layer 都 None → 真正搜过 → NOVEL
        r2 = grade_candidate("d1", "", "ef", "unknown", {"threat_model": {}, "repo": ""}, "chroma", None)
        expect(r2["grade"] == "NOVEL", f"有 param + 无 hit → NOVEL，实际 {r2['grade']}")
    finally:
        novelty_gate_module.consumer_layer_check = orig_consumer
        novelty_gate_module.corrector_layer_check = orig_corrector

    # 3. apply_precision_grading：boundary KNOWN_OPEN → reject；NOVEL → endorse
    res_known = apply_precision_grading(
        {"grade": "KNOWN_OPEN", "layer": "consumer"}, "qdrant_boundary_01_x")
    expect(res_known.get("endorsement") is False, "boundary HIGH precision KNOWN_OPEN → reject")
    res_novel = apply_precision_grading(
        {"grade": "NOVEL", "layer": "gate"}, "qdrant_boundary_01_x")
    expect(res_novel.get("endorsement") is True, "NOVEL → endorse")

    # 4. generate_final_verdict：defect_id 匹配（不只 script/candidate）
    with tempfile.TemporaryDirectory() as td:
        sd = Path(td)
        (sd / "debate_logs").mkdir()
        agg = {"confirmed": {"qdrant_boundary_01_x": {"defect_id": "qdrant_boundary_01_x", "severity_level": "high"}}}
        gate_results = {"qdrant_boundary_01_x": {"grade": "NOVEL", "layer": "gate",
                                                  "evidence_url": "", "param_name": None,
                                                  "defect_type": "unknown", "endorsement": True,
                                                  "endorsement_reason": "test"}}
        v = generate_final_verdict(sd, gate_results, agg)
        expect(v["total_defects"] == 1, "final_verdict total=1")
        expect(v["defects"][0]["defect_id"] == "qdrant_boundary_01_x",
               f"final_verdict defect_id 匹配（dict schema），实际 {v['defects'][0]['defect_id']}")

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        sys.exit(1)
    print("self-check OK")


def main():
    if "--self-check" in sys.argv or "-s" in sys.argv:
        _self_check()
        return
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
    setup_encoding()
    main()
