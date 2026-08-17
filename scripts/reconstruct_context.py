#!/usr/bin/env python3
"""TestVDB Context Reconstruction — rebuilds agent context from disk state.

Called at the start of each loop turn (and optionally after PostCompact)
to provide the agent with a complete, self-contained summary of the
current pipeline state, enabling it to resume from the exact breakpoint.

Usage:
    python scripts/reconstruct_context.py --session-dir <path> [--format text|json]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from typing import Any

from _pipeline_utils import read_json, setup_encoding
from debate_record import FinalVerdict


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _safe_get(d: dict | None, *keys: str, default: Any = "") -> Any:
    """Safely traverse nested dicts."""
    if d is None:
        return default
    cur = d
    for k in keys:
        if not isinstance(cur, dict):
            return default
        cur = cur.get(k, default)
    return cur


# ---------------------------------------------------------------------------
# Phase descriptions for human-readable output
# ---------------------------------------------------------------------------

PHASE_DESCRIPTIONS: dict[str, str] = {
    "SETUP": "初始化（Step 1-7）",
    "ROUND_START": "轮次开始，注入 reflection/threat_model（Step 8a）",
    "ATTACK_GEN": "并发派发 Attack Trio 生成攻击脚本（Step 8b）",
    "DEBATE_S1": "辩论 Stage 1 — 自动审查+去重（Step 8c）",
    "EXECUTION": "Docker 沙箱执行 + 打回修改（Step 8d-8d.5）",
    "EVIDENCE_BUILD": "证据链构建 — evidence-builder 按候选并发 fan-out（Step 8e，ADR-0008）",
    "CHAIN_AUDIT": "证据链审计 — chain-auditor 单实例收口三查+三视角聚合（Step 8e.7，ADR-0008）",
    "REPORTING": "派发 Reporter 生成缺陷报告（Step 8f）",
    "DEFECT_REVIEW": "逐缺陷全面审查（Step 8f.5）",
    "STATE_SAVE": "保存状态 + 分析产出 + 终止检查（Step 8g-8i）",
    "CLEANUP": "汇总报告 + 容器清理 + 标记完成（Step 9-10）",
    "DONE": "流水线已完成",
}

PHASE_ORDER = [
    "SETUP", "ROUND_START", "ATTACK_GEN", "DEBATE_S1", "EXECUTION",
    "EVIDENCE_BUILD", "CHAIN_AUDIT", "REPORTING", "DEFECT_REVIEW", "STATE_SAVE",
]

NEXT_ACTION_MAP: dict[str, str] = {
    "ROUND_START": (
        "执行 ROUND_START：注入 reflection_context + threat_model + cognitive_blindspots 到 Attack Agents。"
        "准备并发派发 Attack Trio。"
    ),
    "ATTACK_GEN": (
        "执行 ATTACK_GEN：并发派发 3 个 Attack Agent（boundary + state + semantic）。"
        "每个 Agent 读取 structured_contract.json 和 reflection_context。"
    ),
    "DEBATE_S1": (
        "执行 DEBATE_S1：收集 Attack Agent 产出的脚本 → 自动去重 → 语法验证 → "
        "约束存在性验证 → API 格式 AST 验证。将审查结果写入 debate_logs/stage1.json。"
    ),
    "EXECUTION": (
        "执行 EXECUTION：派发 Docker Executor 在沙箱中执行通过辩论的脚本。"
        "执行完成后检查 output_*.log.done。如有脚本错误，执行打回修改（8d.5）。"
    ),
    "EVIDENCE_BUILD": (
        "执行 EVIDENCE_BUILD：python scripts/extract_candidates.py 提取候选清单 → "
        "python scripts/verify_live_l1.py L1 机械闸门（REFUTED 淘汰）→ "
        "按候选并发派发 evidence-builder（每候选一个实例，产出 evidence_chain/{defect_id}.json）。"
    ),
    "CHAIN_AUDIT": (
        "执行 CHAIN_AUDIT：全部 builder .done 收口后派发 chain-auditor 单实例。"
        "产出 debate_logs/chain_verdicts.json（DEFECT/NOT_DEFECT/NEEDS_MORE_EVIDENCE + "
        "fp_evidence_source + root_cause 分布）。"
    ),
    "REPORTING": (
        "执行 REPORTING：派发 Reporter 为确认的缺陷生成报告（含 Pre-Submit Gate 复现验证）。"
        "产出 defects/defect-N.md。"
    ),
    "DEFECT_REVIEW": (
        "执行 DEFECT_REVIEW：运行 verify_defects.py 对每个 defect-N.md 审查。"
        "FALSE_POSITIVE → 删除。NEEDS_IMPROVEMENT → 打回 Reporter 重写（最多 1 次）。"
    ),
    "STATE_SAVE": (
        "执行 STATE_SAVE：保存 mine_state.json + coverage.json + experience_handoff.json + "
        "pipeline_state.json。分析本轮产出，检查终止条件。"
        "终止 → 执行 CLEANUP。继续 → ScheduleWakeup 触发下一轮。"
    ),
}


# ---------------------------------------------------------------------------
# Core reconstruction logic
# ---------------------------------------------------------------------------

def reconstruct(session_dir: str) -> dict[str, Any]:
    """Read all state files and produce a structured context blob."""

    result: dict[str, Any] = {
        "session_dir": session_dir,
        "status": "ok",
        "errors": [],
    }

    # 1. pipeline_state.json (primary state source)
    ps_path = os.path.join(session_dir, "pipeline_state.json")
    ps = read_json(ps_path)
    if ps is None:
        result["status"] = "no_pipeline_state"
        result["errors"].append(f"pipeline_state.json not found at {ps_path}")
        return result

    result["pipeline_state"] = ps
    target = ps.get("target", "?")
    version_target = ps.get("version_target", "?")
    session_id = ps.get("session_id", "?")
    current_round = ps.get("current_round", 0)
    max_rounds = ps.get("max_rounds", 0)
    phase = ps.get("phase", "UNKNOWN")
    phases_completed = ps.get("phases_completed", [])
    phase_data = ps.get("phase_data", {})
    global_state = ps.get("global_state", {})

    # 2. mine_state.json
    ms = read_json(os.path.join(session_dir, "mine_state.json"))
    defects_count = len(_safe_get(ms, "defects", default=[]))
    pipeline_state_str = _safe_get(ms, "pipeline_state", default="unknown")

    # 3. experience_handoff.json
    eh_path = os.path.join(session_dir, "experience_handoff.json")
    eh = read_json(eh_path)
    key_learnings = _safe_get(eh, "key_learnings", default=[])
    rejection_patterns = _safe_get(eh, "rejection_patterns", default=[])
    high_value_endpoints = _safe_get(eh, "high_value_endpoints", default=[])
    exhausted_endpoints = _safe_get(eh, "exhausted_endpoints", default=[])
    next_action = _safe_get(eh, "next_action", default="")

    # 4. coverage.json
    cov = read_json(os.path.join(session_dir, "coverage.json"))
    overall_coverage = _safe_get(cov, "overall_coverage_pct", default=0.0)
    core_crud_coverage = _safe_get(cov, "core_crud_coverage_pct", default=0.0)

    # 5. structured_contract.json — summary + target_reference 速查表（组件 C）
    contract_path = os.path.join(session_dir, "structured_contract.json")
    contract = read_json(contract_path)
    endpoint_count = 0
    constraint_count = 0
    endpoint_cheatsheet: list[dict[str, str]] = []
    if contract:
        endpoints = contract.get("api_endpoints", [])
        endpoint_count = len(endpoints)
        for ep in endpoints:
            constraint_count += len(ep.get("constraints", []))
        endpoint_cheatsheet = [
            {
                "method": str(ep.get("method", "")),
                "path": str(ep.get("path", "")),
                "category": str(ep.get("category", "")),
            }
            for ep in endpoints
            if isinstance(ep, dict)
        ]

    # target_reference：契约端点速查表，注入 attack agent prompt 供生成脚本引用
    result["target_reference"] = {
        "target": str(target),
        "endpoint_cheatsheet": endpoint_cheatsheet,
        "key_data_types": contract.get("data_types", []) if contract else [],
    }

    # 6. threat_model.json (summary, if exists)
    project_root = _safe_get(ps, "project_root", default="")
    tm_path = os.path.join(project_root, "intelligence", target, "threat_model.json")
    tm = read_json(tm_path)
    blindspot_count = 0
    priority_areas: list[str] = []
    if tm:
        blindspot_count = len(
            _safe_get(tm, "cognitive_blindspots", "blindspots", default=[])
        )
        areas = _safe_get(tm, "attack_surface", "high_priority_areas", default=[])
        priority_areas = [a.get("area", "") for a in areas if isinstance(a, dict)]

    # 7. final_verdict.json (ADR-0005 — typed access via debate_record)
    debate_verdict = None
    try:
        debate_verdict = FinalVerdict.from_file(session_dir)
    except (FileNotFoundError, ValueError):
        pass  # not yet generated (early phases)

    defect_summary = {}
    if debate_verdict:
        ds = debate_verdict.summary()
        defect_summary = {
            "total_judged": ds["total"],
            "endorsed": ds["endorsed"],
            "rejected": ds["rejected"],
            "grades": ds["grades"],
            "discrepancies": len(debate_verdict.discrepancies()),
        }

    result["defect_verdict"] = defect_summary

    # 8. Termination condition check
    consecutive_no_defect = global_state.get("consecutive_no_defect_rounds", 0)
    total_defects = global_state.get("total_defects_confirmed", defects_count)
    min_defects = ps.get("min_defects", 1)

    termination_reason = ""
    if consecutive_no_defect >= 5:
        termination_reason = f"僵局终止（连续 {consecutive_no_defect} 轮无新缺陷）"
    elif overall_coverage >= 95.0:
        termination_reason = f"覆盖率达标（{overall_coverage:.1f}% ≥ 95%）"
    elif max_rounds > 0 and current_round > max_rounds:
        termination_reason = f"达到最大轮次（{current_round}/{max_rounds}）"
    elif min_defects > 0 and total_defects >= min_defects:
        # min_defects reached (0 = 无下限，不检查) — soft termination
        termination_reason = ""

    # Assemble structured result
    result["summary"] = {
        "session_id": session_id,
        "target": target,
        "version": version_target,
        "current_round": current_round,
        "max_rounds": max_rounds,
        "phase": phase,
        "phases_completed": phases_completed,
        "total_defects_confirmed": total_defects,
        "min_defects": min_defects,
        "overall_coverage_pct": overall_coverage,
        "core_crud_coverage_pct": core_crud_coverage,
        "endpoint_count": endpoint_count,
        "constraint_count": constraint_count,
        "blindspot_count": blindspot_count,
        "priority_areas": priority_areas[:5],
        "consecutive_no_defect_rounds": consecutive_no_defect,
        "termination_reason": termination_reason,
        "docker_running": global_state.get("docker_container_running", False),
    }

    result["round_context"] = {
        "key_learnings": key_learnings[:5],
        "rejection_patterns": rejection_patterns[:5],
        "high_value_endpoints": high_value_endpoints[:5],
        "exhausted_endpoints": exhausted_endpoints[:5],
        "next_action": next_action,
    }

    result["phase_data"] = phase_data

    # Determine next action text
    next_phase = _get_next_phase(phase, phases_completed)
    result["next_action"] = {
        "phase": next_phase,
        "description": NEXT_ACTION_MAP.get(next_phase, "流水线已完成或状态异常。"),
        "resume_from_phase": phase,
        "skip_phases": phases_completed,
    }

    return result


def _get_next_phase(current_phase: str, phases_completed: list[str]) -> str:
    """Determine the next phase to execute."""
    if current_phase in ("CLEANUP", "DONE"):
        return current_phase

    # Find current phase in order, return next uncompleted one
    try:
        idx = PHASE_ORDER.index(current_phase)
    except ValueError:
        return "ROUND_START"  # fallback

    # Return the current phase if not yet completed, otherwise next
    if current_phase not in phases_completed:
        return current_phase

    for i in range(idx + 1, len(PHASE_ORDER)):
        if PHASE_ORDER[i] not in phases_completed:
            return PHASE_ORDER[i]

    return "STATE_SAVE"


# ---------------------------------------------------------------------------
# Output formatters
# ---------------------------------------------------------------------------

def format_text(data: dict[str, Any]) -> str:
    """Format context as human-readable text for agent prompt injection."""
    if data["status"] == "no_pipeline_state":
        return f"[ERROR] 无法重建上下文: {'; '.join(data['errors'])}"

    s = data["summary"]
    rc = data["round_context"]
    na = data["next_action"]
    pd = data.get("phase_data", {})
    session_dir = data["session_dir"]

    lines = [
        f"## TestVDB 上下文重建 — Session {s['session_id']}",
        "",
        "### 基本信息",
        f"- Target: {s['target']} {s['version']}",
        f"- Round: {s['current_round']}/{s['max_rounds'] or '∞'}",
        f"- 当前阶段: {s['phase']} — {PHASE_DESCRIPTIONS.get(s['phase'], '')}",
        f"- 已完成阶段: {', '.join(s['phases_completed']) or '无'}",
        f"- 总确认缺陷: {s['total_defects_confirmed']}",
        f"- 覆盖率: {s['overall_coverage_pct']:.1f}% (核心 CRUD: {s['core_crud_coverage_pct']:.1f}%)",
        f"- Docker 容器: {'运行中' if s['docker_running'] else '未运行'}",
        "",
        "### 恢复指令",
        "你正在执行 TestVDB 缺陷挖掘流水线。上下文已跨 turn 恢复。",
    ]

    if s["phases_completed"]:
        lines.append(
            f"请从 **{na['resume_from_phase']}** 阶段继续。"
            f"以下阶段已完成，不要重做: {', '.join(s['phases_completed'])}。"
        )
    else:
        lines.append(f"请从 **{na['phase']}** 阶段开始本轮执行。")

    # Phase-specific context
    if na["resume_from_phase"] == "DEBATE_S1" or "ATTACK_GEN" in s["phases_completed"]:
        attack_data = pd.get("ATTACK_GEN", {})
        gen_count = attack_data.get("scripts_generated", "?")
        agents_done = attack_data.get("agents_completed", [])
        lines.extend([
            f"- 脚本已生成: {gen_count} 个（agents: {', '.join(agents_done) or '?'}）",
            f"- 脚本位置: {session_dir}/boundary_scripts/, state_scripts/, scripts/",
        ])

    if na["resume_from_phase"] == "EXECUTION" or "DEBATE_S1" in s["phases_completed"]:
        s1_data = pd.get("DEBATE_S1", {})
        lines.append(
            f"- Stage 1 审查: {s1_data.get('approved_count', '?')} 通过, "
            f"{s1_data.get('rejected_count', '?')} 驳回"
        )

    if na["resume_from_phase"] in ("EVIDENCE_BUILD", "CHAIN_AUDIT") or "EXECUTION" in s["phases_completed"]:
        exec_data = pd.get("EXECUTION", {})
        lines.append(
            f"- 执行结果: {exec_data.get('scripts_executed', '?')} 执行, "
            f"{exec_data.get('scripts_passed', '?')} 通过, "
            f"{exec_data.get('scripts_error', '?')} 错误"
        )

    # 组件 C：端点速查表 section（注入 attack agent，供生成脚本引用）
    tr = data.get("target_reference", {})
    cheatsheet = tr.get("endpoint_cheatsheet", [])
    if cheatsheet:
        lines.extend([
            "",
            "### 当前 Target 端点速查表（契约驱动——生成脚本时引用此表，禁止硬编码端口/路径）",
            f"- Target: {tr.get('target', '?')}  |  端点数: {len(cheatsheet)}",
            "| Method | Path | Category |",
            "|--------|------|----------|",
        ])
        for ep in cheatsheet[:40]:
            lines.append(f"| {ep.get('method','')} | {ep.get('path','')} | {ep.get('category','')} |")
        if len(cheatsheet) > 40:
            lines.append(f"| ... | (另 {len(cheatsheet)-40} 条见 structured_contract.json) | ... |")
        lines.append("- 数据字段命名/向量格式: 见 contract.data_types（key_data_types 已注入）")
        lines.append("- ⛔ 禁止写死端口(6333/8080/19530)、路径、payload/properties 字段名——一律从本表或 contract 推导")

    lines.extend(["",
        "### 本轮关键信息",
    ])

    if rc["key_learnings"]:
        lines.append(f"- 上轮经验: {'; '.join(str(l) for l in rc['key_learnings'][:3])}")
    if rc["rejection_patterns"]:
        patterns = [f"{p.get('endpoint', '?')}: {p.get('reason', '?')}" for p in rc["rejection_patterns"][:3]]
        lines.append(f"- 驳回模式: {'; '.join(patterns)}")
    if rc["high_value_endpoints"]:
        lines.append(f"- 高价值端点: {', '.join(rc['high_value_endpoints'][:5])}")
    if rc["exhausted_endpoints"]:
        lines.append(f"- 已耗尽端点: {', '.join(rc['exhausted_endpoints'][:5])}")

    lines.extend(["",
        "### 全局进度",
        f"- 总确认缺陷: {s['total_defects_confirmed']} (min_defects={s['min_defects']})",
        f"- 连续无缺陷轮次: {s['consecutive_no_defect_rounds']}",
    ])

    if s["termination_reason"]:
        lines.append(f"- ⚠️ 终止条件已满足: {s['termination_reason']}")
        lines.append("- 应执行 CLEANUP 流程（Step 9-10），不调用 ScheduleWakeup。")
    else:
        lines.append("- 终止条件: 未满足，继续挖掘。")

    if s["blindspot_count"] > 0:
        lines.extend(["",
            f"### 威胁模型 ({s['blindspot_count']} 个认知盲点)",
            f"- 优先攻击面: {', '.join(s['priority_areas'][:3])}",
        ])

    lines.extend(["",
        "### 下一步行动",
        na["description"],
    ])

    return "\n".join(lines)


def format_json(data: dict[str, Any]) -> str:
    """Format context as compact JSON for machine consumption."""
    return json.dumps(data, ensure_ascii=False, indent=2)


# ---------------------------------------------------------------------------
# CLI entry point
# ---------------------------------------------------------------------------

def main() -> None:
    setup_encoding()

    parser = argparse.ArgumentParser(
        description="Reconstruct TestVDB agent context from disk state files."
    )
    parser.add_argument(
        "--session-dir",
        required=False,
        default=None,
        help="Path to the session directory. If omitted, auto-discovers the newest "
        "mine_state.json under <plugin_root>/results (robust to cwd drift and to a "
        "post-compact agent that no longer remembers the session_dir).",
    )
    parser.add_argument(
        "--format",
        choices=["text", "json"],
        default="text",
        help="Output format: text (human-readable, for agent prompt) or json (machine-parseable)",
    )
    args = parser.parse_args()

    if args.session_dir:
        session_dir = os.path.abspath(args.session_dir)
    else:
        # Auto-discover newest session so resume works even when the caller
        # (e.g. a post-compact agent) has lost the session_dir from context.
        from _session_utils import find_latest_session_dir

        session_dir = find_latest_session_dir(require_running=False)
        if not session_dir:
            print(
                "[ERROR] No --session-dir given and no mine_state.json found under "
                "<plugin_root>/results. Pass --session-dir explicitly.",
                file=sys.stderr,
            )
            sys.exit(1)
        print(f"[reconstruct] auto-discovered session: {session_dir}", file=sys.stderr)
    if not os.path.isdir(session_dir):
        print(f"[ERROR] Session directory not found: {session_dir}", file=sys.stderr)
        sys.exit(1)

    data = reconstruct(session_dir)

    if args.format == "json":
        print(format_json(data))
    else:
        print(format_text(data))


if __name__ == "__main__":
    main()
