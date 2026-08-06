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

# stage2_severity 顶层 metadata 键（非 defect_id 条目）— _severity_levels / _severity_defect_types 共用
# ponytail: 提取为模块常量避免 DRY 漂移（两 helper 同 schema 范式）
_SEV_META_KEYS = frozenset({"judge", "timestamp", "target", "version", "session_dir"})


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
        # P3-22: 移除 "vote" — stage2_severity entry 的 vote 是 "is_defect"（判断）非级别，
        # 先命中会返回 "is_defect" 而非 severity 真值，导致全投票被误判 trivial/missing
        for k in ("level", "severity", "rating"):
            val = v.get(k)
            if val:
                return str(val).lower()
    return None


def _severity_levels(sev: dict) -> dict:
    """返回 {defect_id: level}。通用：兼容 {votes:[...]} / 顶层 {defect_id: {...|level}} / 空。"""
    if not sev or not isinstance(sev, dict):
        return {}
    votes = sev.get("votes")
    if isinstance(votes, list):
        return {v.get("defect_id"): _extract_level(v)
                for v in votes if isinstance(v, dict) and v.get("defect_id")}
    if votes is None:
        return {k: _extract_level(v) for k, v in sev.items()
                if k not in _SEV_META_KEYS and isinstance(v, (dict, str))}
    return {}


def _severity_defect_types(sev: dict) -> dict:
    """{defect_id: defect_type} — P3-18a+: 从 stage2_severity 提取 Type 分类。

    实测 qdrant session judge-severity vote entry schema =
    {vote: "is_defect", severity: "High", defect_type: "Type1_IllegalSuccess", ...}。
    Type 分类（Type1_IllegalSuccess/Type2_PoorDiagnostics/Type3_RuntimeFailure/Type4_StateLogic）
    比 attack_type 分类（boundary/semantic/state，来自 stage2_doc category）对 novelty_gate
    consumer_layer_check 更直接对应缺陷语义。缺 defect_type → 不入 dict（run() fallback doc/unknown）。

    ponytail: 与 _severity_levels 同 schema 兼容范式（{votes:[...]} / 顶层 dict / 空）。
    """
    if not sev or not isinstance(sev, dict):
        return {}
    votes = sev.get("votes")
    if isinstance(votes, list):
        return {v.get("defect_id"): v.get("defect_type")
                for v in votes
                if isinstance(v, dict) and v.get("defect_id") and v.get("defect_type")}
    if votes is None:
        return {k: v.get("defect_type")
                for k, v in sev.items()
                if k not in _SEV_META_KEYS and isinstance(v, dict) and v.get("defect_type")}
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


def _doc_categories(doc: dict) -> dict:
    """{defect_id: category} — P3-18a: 从 stage2_doc 提取 defect_type 来源。

    judge-doc results[i] 含 category 字段（实测 qdrant session：boundary/semantic/state），
    作为 novelty_gate grade_candidate 的 defect_type 输入。缺 category → "unknown"。
    ponytail: 与 _doc_results 并列（不合并），保留 _doc_results 纯 str 返回契约。
    """
    if not isinstance(doc, dict):
        return {}
    results = doc.get("results", [])
    if not isinstance(results, list):
        return {}
    return {r.get("defect_id"): r.get("category", "unknown")
            for r in results if isinstance(r, dict) and r.get("defect_id")}


def _load_meta(session_dir: str) -> dict:
    """{defect_id: {param, endpoint, ...}} — P3-18b: 从 debate_logs/*.meta.json 读。

    attack agent SOP（P3-18b）要求产 {defect_id}.meta.json（schema: defect_id/endpoint/
    param/expected_defect_type/strategy）。aggregate_votes 合并 param/endpoint 到 confirmed
    entry，让 novelty_gate grade_candidate 能用 param_name 做真 GitHub/corpus 搜索（产出
    NOVEL/KNOWN 判决，非全 UNVERIFIED）。

    缺 meta.json / 解析失败 / param=None → 该字段不入 dict（run() 时 entry 不出现对应字段，
    与现状一致，向后兼容）。坏文件 silent skip（不阻塞聚合 — meta 是 enrich，非关键路径）。
    ponytail: pathlib glob + json.loads，无新依赖。
    """
    out: dict[str, dict] = {}
    meta_dir = Path(session_dir) / "debate_logs"
    if not meta_dir.is_dir():
        return out
    for meta_path in meta_dir.glob("*.meta.json"):
        try:
            m = json.loads(meta_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(m, dict) and m.get("defect_id"):
            out[m["defect_id"]] = {
                "param": m.get("param"),
                "endpoint": m.get("endpoint"),
                "expected_defect_type": m.get("expected_defect_type"),
                "strategy": m.get("strategy"),
            }
    return out


_SEVERITY_LADDER = ["trivial", "low", "medium", "high", "critical"]


def _demote_severity(level: str | None, steps: int) -> str | None:
    """severity 降 N 级（DOC_MISMATCH→2，DOC_PARTIAL→1）。floor=trivial。未知 level 不动。"""
    if not level or level not in _SEVERITY_LADDER:
        return level
    return _SEVERITY_LADDER[max(0, _SEVERITY_LADDER.index(level) - steps)]


# ponytail: v2.2 崩溃放行 — Type3 崩溃是客观事实，无需文档契约支撑（反"无文档约束误杀崩溃缺陷"）
# 精确模式避免误匹配（如 "500ms"/"status: 200"）
_CRASH_PATTERNS = (
    "out of memory", "oom", "oom-killer", "memory exhausted",
    "panic occurred", "panic backtrace", "capacity overflow",
    "hash table capacity overflow",
    "sigsegv", "sigabrt", "sigkill",
    "segmentation fault", "stack overflow",
    "killed",  # OOM killer / entrypoint.sh: PID Killed
    "thread panicked",
)
_CRASH_STATUS_5XX = (  # 精确 5xx 状态（避免 "500" 子串误匹配，全小写因匹配 low）
    '"status":500', '"status": 500',
    '"status":502', '"status": 502',
    '"status":503', '"status": 503',
    '"status":504', '"status": 504',
    'status": 500', 'status": 502', 'status": 503',  # 带空格变体
    "http/1.1 500", "http/1.1 502", "http/1.1 503", "http/1.1 504",
    "http 500", "http 502", "http 503",
    "status:500", "status: 500", "status=500", "status = 500",
    "status:502", "status: 502", "status=502",
    "status:503", "status: 503", "status=503",
    " 500 internal", " 502 bad", " 503 service",
)
# by-design 崩溃白名单（平台/外部限制，非 server 缺陷）
_BY_DESIGN_CRASH_KEYWORDS = (
    "jemalloc", "aarch64", "arm64",  # jemalloc ARM64 对齐限制（qdrant #3831/#4298/#6512/#7246）
    "address sizes",  # 平台地址空间限制
    "libc", "musl",  # 特定 libc 兼容
)


def _read_defect_log(session_dir: str, defect_id: str) -> str:
    """读 output_{defect_id}.log（执行日志），缺失返空串。"""
    # 候选路径：output_{did}.log（主流）+ debate_logs/{did}.log（fallback）
    for p in (Path(session_dir) / f"output_{defect_id}.log",
              Path(session_dir) / "debate_logs" / f"{defect_id}.log"):
        if p.exists():
            try:
                return p.read_text(encoding="utf-8", errors="replace")
            except Exception:
                return ""
    return ""


def _is_crash_signal(log_text: str) -> tuple[bool, str]:
    """检测日志是否含崩溃信号。返回 (is_crash, matched_pattern)。

    排除 by-design 崩溃（jemalloc ARM64 等平台限制）。
    """
    if not log_text:
        return False, ""
    low = log_text.lower()
    # by-design 白名单优先（即使含 panic/oom，若是 jemalloc ARM64 = 平台限制非缺陷）
    if any(kw in low for kw in _BY_DESIGN_CRASH_KEYWORDS):
        return False, ""  # by-design，不触发崩溃放行
    for pat in _CRASH_PATTERNS:
        if pat in low:
            return True, pat
    for pat in _CRASH_STATUS_5XX:
        if pat in low:
            return True, pat
    return False, ""


def _load_threat_model_by_design(target: str) -> list[str]:
    """从 intelligence/{target}/threat_model.json 读 by_design_behaviors 派生崩溃白名单关键词。

    缺失/threat_model 无该字段 → 返回空列表（不阻塞，仅减少白名单覆盖）。
    """
    if not target:
        return []
    tm_path = Path("intelligence") / target / "threat_model.json"
    if not tm_path.exists():
        return []
    try:
        tm = json.loads(tm_path.read_text(encoding="utf-8"))
    except Exception:
        return []
    out = []
    # by_design_behaviors 可能在 judge_enhancements 或 defect_criteria 下，结构多样
    for section_key in ("defect_criteria", "judge_enhancements"):
        section = tm.get(section_key) or {}
        bds = section.get("by_design_behaviors") or []
        if isinstance(bds, list):
            for b in bds:
                if isinstance(b, dict):
                    # 取 name/behavior/rationale 字段的关键词
                    for k in ("name", "behavior", "rationale", "pattern"):
                        v = b.get(k)
                        if isinstance(v, str) and len(v) < 80:
                            out.append(v.lower())
    return out


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
    doc_categories = _doc_categories(doc or {})  # P3-18a: defect_type 来源 (fallback)
    sev_defect_types = _severity_defect_types(sev or {})  # P3-18a+: Type 分类优先
    meta_info = _load_meta(session_dir)  # P3-18b: param/endpoint 来源（meta.json 缺时 {}）

    confirmed, rejected = {}, {}
    # ponytail: v2.2 崩溃放行 — 读 threat_model by_design 派生额外白名单关键词
    tm_by_design_kw = _load_threat_model_by_design(target)
    for did, vote in ev_votes.items():
        # 规则 0: 崩溃现象自动确认（v2.2 — Type3 崩溃是客观事实，跳过 severity/doc/evidence 检查）
        # 反"无文档约束误杀崩溃缺陷"（如 limit=1e8 OOM 被 DOC_MISMATCH 降级后 reject）
        log_text = _read_defect_log(session_dir, did)
        is_crash, crash_pat = _is_crash_signal(log_text)
        # threat_model by_design 关键词补充检查（若日志含 by_design 描述 → 不触发）
        if is_crash and tm_by_design_kw:
            low_log = log_text.lower()
            if any(kw in low_log for kw in tm_by_design_kw if kw):
                is_crash = False
        if is_crash:
            confirmed[did] = {
                "defect_id": did,
                "severity_level": "critical",
                "defect_type": "Type3_RuntimeFailure",
                "script": f"{did}.py",
                "confirmed": True,
                "crash_auto_confirmed": True,
                "crash_signal": crash_pat,
                "note": "崩溃现象自动确认（v2.2）— Type3 崩溃无需文档契约支撑，DOC_MISMATCH 不降级",
            }
            continue
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
        # P3-18a+: defect_type 优先级 — severity.defect_type (Type 分类) > doc.category (attack 分类) > "unknown"
        # 让 novelty_gate.run_novelty_gate (L446-449) 能读到非 unknown/"" 值
        entry = {"defect_id": did, "severity_level": level, "confirmed": True,
                 "defect_type": sev_defect_types.get(did) or doc_categories.get(did, "unknown"),
                 "script": f"{did}.py"}
        if nv_info.get("rating") == "already_reported":
            entry["related_issue_numbers"] = nv_info.get("related_issues", [])
            entry["note"] = "already_reported: 保留，related_issues 传给 Novelty Gate"
        # P3-18b: 合并 param/endpoint（从 debate_logs/{did}.meta.json 读；缺 meta 时字段不出现，向后兼容）
        # 让 novelty_gate grade_candidate 能用 param_name 做真 GitHub/corpus 搜索（非全 UNVERIFIED）
        m_info = meta_info.get(did, {})
        if m_info.get("param"):
            entry["param"] = m_info["param"]
        if m_info.get("endpoint"):
            entry["endpoint"] = m_info["endpoint"]
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

        # 场景 7 (P3-18a): confirmed entry 含 defect_type (stage2_doc category) + script ({defect_id}.py)
        (bdir / "stage2_evidence.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect"}]}), encoding="utf-8")
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "high"}]}), encoding="utf-8")
        (bdir / "stage2_doc.json").write_text(json.dumps({"results": [
            {"defect_id": "a", "doc_verification_result": "DOC_VERIFIED", "category": "boundary"}]}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["defect_type"] == "boundary", \
            f"P3-18a: defect_type 应从 stage2_doc category 合并, got {entry.get('defect_type')}"
        assert entry["script"] == "a.py", \
            f"P3-18a: script 应为 f'{{defect_id}}.py', got {entry.get('script')}"

        # 场景 8 (P3-18a): 缺 stage2_doc → defect_type="unknown"（fallback）；script 不依赖 stage2_doc
        (bdir / "stage2_doc.json").unlink()
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["defect_type"] == "unknown", \
            f"P3-18a: 缺 stage2_doc 应 fallback 'unknown', got {entry.get('defect_type')}"
        assert entry["script"] == "a.py", \
            f"P3-18a: script 不依赖 stage2_doc, got {entry.get('script')}"

        # 场景 9 (P3-22): severity entry 含 vote + severity → _extract_level 返回 severity 真值
        # 原 bug: 键顺序 ("level","vote",...) 先命中 vote → 返回 "is_defect" 误作 severity_level
        (bdir / "stage2_evidence.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect"}]}), encoding="utf-8")
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "vote": "is_defect", "severity": "High"}]}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["severity_level"] == "high", \
            f"P3-22: severity_level 应 'high'（非 'is_defect'）, got {entry.get('severity_level')}"

        # 场景 10 (P3-18a+): defect_type 优先级 —
        #   severity.defect_type (Type 分类) > doc.category (attack 分类) > "unknown"
        # (a) severity 含 Type1 + doc 含 boundary → Type1_IllegalSuccess（severity 优先）
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "high", "defect_type": "Type1_IllegalSuccess"}]}), encoding="utf-8")
        (bdir / "stage2_doc.json").write_text(json.dumps({"results": [
            {"defect_id": "a", "doc_verification_result": "DOC_VERIFIED", "category": "boundary"}]}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["defect_type"] == "Type1_IllegalSuccess", \
            f"P3-18a+(a): severity 含 Type1 应优先, got {entry.get('defect_type')}"

        # (b) severity 缺 defect_type + doc 含 category → fallback doc category
        (bdir / "stage2_severity.json").write_text(json.dumps({"votes": [
            {"defect_id": "a", "level": "high"}]}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["defect_type"] == "boundary", \
            f"P3-18a+(b): severity 缺 defect_type 应 fallback doc category, got {entry.get('defect_type')}"

        # (c) 两者都缺 → "unknown"
        (bdir / "stage2_doc.json").write_text(json.dumps({"results": [
            {"defect_id": "a", "doc_verification_result": "DOC_VERIFIED"}]}), encoding="utf-8")  # 无 category
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry["defect_type"] == "unknown", \
            f"P3-18a+(c): 两者都缺应 'unknown', got {entry.get('defect_type')}"

        # 场景 11 (P3-18b): debate_logs/{did}.meta.json → confirmed entry 合并 param/endpoint
        # (a) meta.json param=vector_dim endpoint=search+points → entry 含两字段
        (bdir / "a.meta.json").write_text(json.dumps({
            "defect_id": "a", "endpoint": "search+points", "param": "vector_dim",
            "expected_defect_type": "Type1_IllegalSuccess", "strategy": "boundary"}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert entry.get("param") == "vector_dim", \
            f"P3-18b(a): meta.json param 应合并到 entry, got {entry.get('param')}"
        assert entry.get("endpoint") == "search+points", \
            f"P3-18b(a): meta.json endpoint 应合并到 entry, got {entry.get('endpoint')}"

        # (b) 删 meta.json → entry 不含 param/endpoint（向后兼容回归，无 meta 时字段消失）
        (bdir / "a.meta.json").unlink()
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert "param" not in entry, \
            f"P3-18b(b): 无 meta.json 时 param 字段应不出现（向后兼容）, got {entry}"
        assert "endpoint" not in entry, \
            f"P3-18b(b): 无 meta.json 时 endpoint 字段应不出现, got {entry}"

        # (c) meta.json param=null → entry 不含 param（null 等价缺字段）；endpoint 独立合并
        (bdir / "a.meta.json").write_text(json.dumps({
            "defect_id": "a", "endpoint": "search+points", "param": None,
            "expected_defect_type": "Type2_PoorDiagnostics", "strategy": "diagnosis_quality"}), encoding="utf-8")
        run(td)
        agg = json.loads((bdir / "stage2_aggregation.json").read_text(encoding="utf-8"))
        entry = agg["confirmed"]["a"]
        assert "param" not in entry, \
            f"P3-18b(c): meta param=null 应等价无 param 字段, got {entry}"
        assert entry.get("endpoint") == "search+points", \
            f"P3-18b(c): endpoint 与 param 独立合并, got {entry.get('endpoint')}"
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
