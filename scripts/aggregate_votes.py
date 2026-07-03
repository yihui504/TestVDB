#!/usr/bin/env python3
"""aggregate_votes — 代码化 debate 聚合（设计 §5，决策 D）。

把"确认 defect 的决策"从 LLM 手写 aggregation（policy，可跳过）变成代码（mechanism）。
chroma v1.5.9 触发案例：LLM aggregation 在 judge-severity 整体失败（stage2_severity.json={}）
时仍确认 6 个 defect（severity 反对权全丢）→ 5 假阳性流入。代码化后 severity 缺失 → 不确认。

规则（最小通用版，设计 §5 规则 1-3）：
  1. evidence vote != is_defect → rejected
  2. evidence vote == is_defect AND severity 缺失 → rejected（保守，触发 gate_severity_coverage retry）
  3. evidence vote == is_defect AND severity trivial → rejected
  4. evidence vote == is_defect AND severity 非 trivial → confirmed
novelty/doc 规则（设计 §5 规则 4-6）留后续 — schema 稳定后补，不阻塞当前规则。

输入：debate_logs/stage2_evidence.json + stage2_severity.json
输出：debate_logs/stage2_aggregation.json（覆盖 LLM 版；原版备份到 stage2_aggregation_llm.json）

契约：转换器（非检查器）— status=pass 表示成功转换，confirmed 数在 details（0 合法）。
"""
from __future__ import annotations

import json
import os
import shutil
import sys
from pathlib import Path

from _pipeline_utils import setup_encoding, read_json, debate_log_path

setup_encoding()

TRIVIAL_LEVELS = {"trivial", "none", "info", "negligible"}


def _evidence_votes(ev: dict) -> dict:
    """返回 {defect_id: vote}。通用：兼容 {votes:[...]} 各种 schema。"""
    if not isinstance(ev, dict):
        return {}
    votes = ev.get("votes", [])
    if isinstance(votes, list):
        return {v.get("defect_id"): v.get("vote")
                for v in votes if isinstance(v, dict) and v.get("defect_id")}
    return {}


def _extract_level(v) -> str | None:
    if isinstance(v, str):
        return v.lower()
    if isinstance(v, dict):
        for k in ("level", "vote", "severity", "rating"):
            val = v.get(k)
            if val:
                return str(val).lower()
    return None


def _severity_levels(sev: dict) -> dict:
    """返回 {defect_id: level}。通用：兼容 {votes:[...]} / 顶层 {defect_id: {...|level}} / 空。"""
    if not sev or not isinstance(sev, dict):
        return {}
    _META_KEYS = {"judge", "timestamp", "target", "version", "session_dir"}
    votes = sev.get("votes")
    if isinstance(votes, list):
        return {v.get("defect_id"): _extract_level(v)
                for v in votes if isinstance(v, dict) and v.get("defect_id")}
    if votes is None:
        return {k: _extract_level(v) for k, v in sev.items()
                if k not in _META_KEYS and isinstance(v, (dict, str))}
    return {}


# 规则 4-6 辅助（设计 §5，schema 来自 judge-novelty / judge-doc 真实产出）
def _novelty_votes(nv: dict) -> dict:
    """{defect_id: {vote, rating, related_issues}}。兼容 {votes:[...]}。"""
    if not isinstance(nv, dict):
        return {}
    votes = nv.get("votes", [])
    if not isinstance(votes, list):
        return {}
    out = {}
    for v in votes:
        if not isinstance(v, dict) or not v.get("defect_id"):
            continue
        rating = v.get("novelty_rating")
        out[v.get("defect_id")] = {
            "vote": v.get("vote"),
            "rating": rating.lower() if isinstance(rating, str) else "",
            "related_issues": v.get("related_issue_numbers", []) or [],
        }
    return out


def _doc_results(doc: dict) -> dict:
    """{defect_id: doc_verification_result}。兼容 judge-doc 的 {results:[...]} schema。"""
    if not isinstance(doc, dict):
        return {}
    results = doc.get("results", [])
    if not isinstance(results, list):
        return {}
    return {r.get("defect_id"): str(r.get("doc_verification_result") or "").upper()
            for r in results if isinstance(r, dict) and r.get("defect_id")}


_SEVERITY_LADDER = ["trivial", "low", "medium", "high", "critical"]


def _demote_severity(level: str | None, steps: int) -> str | None:
    """severity 降 N 级（DOC_MISMATCH→2，DOC_PARTIAL→1）。floor=trivial。未知 level 不动。"""
    if not level or level not in _SEVERITY_LADDER:
        return level
    return _SEVERITY_LADDER[max(0, _SEVERITY_LADDER.index(level) - steps)]


def run(session_dir: str, target: str = "", strict: bool = False) -> dict:
    ev = read_json(debate_log_path(session_dir, "stage2_evidence"))
    sev = read_json(debate_log_path(session_dir, "stage2_severity"))
    nv = read_json(debate_log_path(session_dir, "stage2_novelty"))
    doc = read_json(debate_log_path(session_dir, "stage2_doc"))
    if not ev:
        return {"status": "fail", "reason": "stage2_evidence.json 缺失或空 — 无法聚合",
                "details": {"confirmed": 0, "rejected": 0}}

    ev_votes = _evidence_votes(ev)
    sev_levels = _severity_levels(sev or {})
    nv_votes = _novelty_votes(nv or {})
    doc_results = _doc_results(doc or {})

    confirmed, rejected = {}, {}
    for did, vote in ev_votes.items():
        nv_info = nv_votes.get(did, {})
        # 规则 4: novelty vote=not_defect（judge-novelty 唯一 not_defect 场景 = known_wontfix）→ rejected
        if nv_info.get("vote") == "not_defect":
            rejected[did] = {"reason": "novelty vote=not_defect (known_wontfix)", "confirmed": False}
            continue
        # 规则 1: evidence vote != is_defect → rejected
        if vote != "is_defect":
            rejected[did] = {"reason": f"evidence vote={vote}", "confirmed": False}
            continue
        level = sev_levels.get(did)
        # 规则 6: DOC_MISMATCH 降两级 / DOC_PARTIAL 降一级（可能降到 trivial → 规则 3 拒）
        doc_r = doc_results.get(did, "")
        if doc_r == "DOC_MISMATCH":
            level = _demote_severity(level, 2)
        elif doc_r == "DOC_PARTIAL":
            level = _demote_severity(level, 1)
        # 规则 2: severity 缺失 → rejected（保守，触发 gate_severity_coverage retry）
        if level is None:
            rejected[did] = {"reason": "severity 缺失（judge-severity 未投票）", "confirmed": False}
            continue
        # 规则 3: severity trivial → rejected
        if level in TRIVIAL_LEVELS:
            suffix = f" (after DOC demote: {doc_r})" if doc_r in ("DOC_MISMATCH", "DOC_PARTIAL") else f" ({level})"
            rejected[did] = {"reason": f"severity trivial{suffix}", "confirmed": False}
            continue
        # 规则 5: already_reported → 保留 + related_issue_numbers（不 kill，传给 Novelty Gate）
        entry = {"defect_id": did, "severity_level": level, "confirmed": True}
        if nv_info.get("rating") == "already_reported":
            entry["related_issue_numbers"] = nv_info.get("related_issues", [])
            entry["note"] = "already_reported: 保留，related_issues 传给 Novelty Gate"
        confirmed[did] = entry

    agg_out = {
        "summary": f"{len(confirmed)} confirmed, {len(rejected)} rejected (code-aggregated)",
        "confirmed": confirmed,
        "rejected": rejected,
        "aggregator": "aggregate_votes.py v1",
    }

    # 备份 LLM 版（首次覆盖时）+ 写 code 版
    agg_path = debate_log_path(session_dir, "stage2_aggregation")
    if agg_path.exists():
        backup = debate_log_path(session_dir, "stage2_aggregation_llm")
        if not backup.exists():
            shutil.copy2(agg_path, backup)
    agg_path.write_text(json.dumps(agg_out, indent=2, ensure_ascii=False), encoding="utf-8")

    return {"status": "pass",  # 转换器：成功转换即 pass（0 confirmed 合法）
            "reason": f"code-aggregated: {len(confirmed)} confirmed / {len(rejected)} rejected",
            "details": {"confirmed": len(confirmed), "rejected": len(rejected),
                        "severity_present": bool(sev_levels)}}


def _self_check() -> None:
    """ponytail: 规则 1-6 各一场景（chroma severity 空 + novelty/doc 规则）。"""
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        bdir = Path(td) / "debate_logs"
        bdir.mkdir()

        # 场景 1：severity 空（chroma 案例）→ is_defect 也 rejected
        (bdir / "stage2_evidence.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect"},
            {"defect_id": "b", "vote": "not_defect"}]}), encoding="utf-8")
        (bdir / "stage2_severity.json").write_text("{}", encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 0, "severity 缺失 → 0 confirmed"
        assert r["details"]["rejected"] == 2, "both rejected"

        # 场景 2：severity 非 trivial → confirmed
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "high"}]}), encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 1, "severity high → confirmed"

        # 场景 3：severity trivial → rejected
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "trivial"}]}), encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 0, "severity trivial → rejected"

        # 场景 4：novelty known_wontfix (vote=not_defect) → rejected（即使 evidence is_defect + severity high）
        (bdir / "stage2_evidence.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect"}]}), encoding="utf-8")
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "high"}]}), encoding="utf-8")
        (bdir / "stage2_novelty.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "not_defect", "novelty_rating": "known_wontfix"}]}), encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 0, "known_wontfix → rejected"

        # 场景 5：novelty already_reported → confirmed + related_issue_numbers
        (bdir / "stage2_novelty.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect", "novelty_rating": "already_reported",
             "related_issue_numbers": [123, 456]}]}), encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 1, "already_reported → 保留"

        # 场景 6：DOC_MISMATCH + severity low → 降到 trivial → rejected
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "low"}]}), encoding="utf-8")
        (bdir / "stage2_doc.json").write_text(json.dumps({"results": [
            {"defect_id": "a", "doc_verification_result": "DOC_MISMATCH"}]}), encoding="utf-8")
        r = run(td)
        assert r["details"]["confirmed"] == 0, "DOC_MISMATCH low→trivial → rejected"
    print("self-check OK")


def main() -> None:
    args = sys.argv[1:]
    if not args or args[0] in ("--self-check", "-s"):
        _self_check()
        return
    session_dir = args[0]
    target = ""
    if "--target" in args:
        i = args.index("--target")
        if i + 1 < len(args):
            target = args[i + 1]
    if not os.path.isdir(session_dir):
        print(json.dumps({"status": "fail", "reason": f"session_dir not found: {session_dir}"}, ensure_ascii=False))
        sys.exit(1)
    r = run(session_dir, target)
    print(json.dumps(r, ensure_ascii=False, indent=2))
    sys.exit(0)  # 转换器：不 fail-exit（0 confirmed 合法，由下游 gate 决定）


if __name__ == "__main__":
    main()
