#!/usr/bin/env python3
"""TestVDB Post-Compact State Recovery Verification.

Verifies that pipeline state can be recovered after context compaction,
and prints recovery instructions for the agent.

v3 update: Reads pipeline_state.json (v3 schema) as primary state source,
with fallback to mine_state.json for legacy sessions.
"""
import json
import os
import sys
from _session_utils import find_session_id
from _pipeline_utils import read_json, setup_encoding


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_latest_pipeline_state():
    """Find the latest pipeline_state.json, preferring checkpoint then session dirs."""
    plugin_root = _plugin_root()
    ckpt_dir = os.path.join(plugin_root, "results", ".checkpoints")

    # 1. Check checkpoint directory first
    ckpt_ps = os.path.join(ckpt_dir, "pipeline_state.json")
    if os.path.isfile(ckpt_ps):
        return ckpt_ps

    # 2. Try session_id match
    session_id = find_session_id()
    if session_id:
        for root, dirs, files in os.walk(os.path.join(plugin_root, "results")):
            if "pipeline_state.json" in files:
                ps_path = os.path.join(root, "pipeline_state.json")
                ps = read_json(ps_path)
                if ps and ps.get("session_id") == session_id:
                    return ps_path

    # 3. Fallback: most recently modified pipeline_state.json
    import glob
    candidates = glob.glob(os.path.join(plugin_root, "results", "*", "*", "*", "pipeline_state.json"))
    if not candidates:
        return None
    return max(candidates, key=os.path.getmtime)


def find_latest_mine_state():
    """Find the latest mine_state.json (fallback for legacy sessions)."""
    import glob
    plugin_root = _plugin_root()
    ckpt_dir = os.path.join(plugin_root, "results", ".checkpoints")

    # Checkpoint first
    ckpt_ms = os.path.join(ckpt_dir, "mine_state.json")
    if os.path.isfile(ckpt_ms):
        return ckpt_ms

    # Session ID match
    session_id = find_session_id()
    if session_id:
        for root, dirs, files in os.walk(os.path.join(plugin_root, "results")):
            if "mine_state.json" in files:
                ms_path = os.path.join(root, "mine_state.json")
                ms = read_json(ms_path)
                if ms and ms.get("session_id") == session_id:
                    return ms_path

    # Fallback: most recent
    candidates = glob.glob(os.path.join(plugin_root, "results", "*", "*", "*", "mine_state.json"))
    if not candidates:
        return None
    return max(candidates, key=os.path.getmtime)


# ---------------------------------------------------------------------------
# Phase descriptions for recovery output
# ---------------------------------------------------------------------------

PHASE_NAMES = {
    "ROUND_START": "轮次开始（注入 reflection/threat_model）",
    "ATTACK_GEN": "攻击脚本生成",
    "DEBATE_S1": "辩论 Stage 1（自动审查+去重）",
    "EXECUTION": "Docker 沙箱执行",
    "EVIDENCE_BUILD": "证据链构建（evidence-builder fan-out，ADR-0008）",
    "CHAIN_AUDIT": "证据链审计（chain-auditor 收口，ADR-0008）",
    "REPORTING": "缺陷报告生成",
    "DEFECT_REVIEW": "逐缺陷审查",
    "STATE_SAVE": "状态保存 + 终止检查",
    "CLEANUP": "汇总 + 清理",
}


def main():
    setup_encoding()

    print("[TestVDB] PostCompact: Context compressed. Verifying state...")

    # Try pipeline_state.json (v3) first
    ps_path = find_latest_pipeline_state()
    ps = read_json(ps_path) if ps_path else None

    if _is_v3_state(ps):
        # v3 schema — precise phase recovery
        session_id = ps.get("session_id", "?")
        target = ps.get("target", "?")
        version_target = ps.get("version_target", "?")
        current_round = ps.get("current_round", "?")
        max_rounds = ps.get("max_rounds", "?")
        phase = ps.get("phase", "UNKNOWN")
        phases_completed = ps.get("phases_completed", [])
        turn_type = ps.get("turn_type", "?")
        global_state = ps.get("global_state", {})
        total_defects = global_state.get("total_defects_confirmed", 0)
        coverage = global_state.get("overall_coverage_pct", 0.0)
        docker_running = global_state.get("docker_container_running", False)
        session_dir = ps.get("session_dir", "?")

        phase_desc = PHASE_NAMES.get(phase, phase)
        completed_str = ", ".join(phases_completed) if phases_completed else "无"

        print(f"[TestVDB] === 上下文压缩恢复 (v3) ===")
        print(f"[TestVDB] Session: {session_id}")
        print(f"[TestVDB] Target: {target} {version_target}")
        print(f"[TestVDB] Round: {current_round}/{max_rounds}")
        print(f"[TestVDB] 当前阶段: {phase} — {phase_desc}")
        print(f"[TestVDB] 已完成阶段: {completed_str}")
        print(f"[TestVDB] 总确认缺陷: {total_defects}")
        print(f"[TestVDB] 覆盖率: {coverage:.1f}%")
        print(f"[TestVDB] Docker: {'运行中' if docker_running else '未运行'}")
        print(f"[TestVDB] Turn type: {turn_type}")
        print(f"[TestVDB] State file: {ps_path}")

        level, rmsg = _recovery_message(phase, phases_completed)
        prefix = "[TestVDB] Recovery:" if level == "OK" else "[TestVDB] WARNING:"
        print(f"{prefix} {rmsg}")

        # If loop turn, remind about reconstruct_context.py
        if turn_type == "loop":
            print(f"[TestVDB] Recovery: 运行 python scripts/reconstruct_context.py --session-dir \"{session_dir}\" 获取完整上下文。")

        print(f"[TestVDB] Recovery: session_dir={session_dir}")

    else:
        # Fallback to legacy mine_state.json
        ms_path = find_latest_mine_state()
        ms = read_json(ms_path) if ms_path else None

        if ms:
            pipeline_state = ms.get("pipeline_state", "unknown")
            current_round = ms.get("current_round", "?")
            max_rounds = ms.get("max_rounds", "?")
            target = ms.get("target", "?")
            version = ms.get("version", "?")
            defects_count = len(ms.get("defects", []))

            # Try to get phase from pipeline_state.json in same dir
            phase = "unknown"
            if ms_path:
                legacy_ps_path = os.path.join(os.path.dirname(ms_path), "pipeline_state.json")
                legacy_ps = read_json(legacy_ps_path)
                if legacy_ps:
                    phase = legacy_ps.get("phase", "unknown")

            print(f"[TestVDB] === 上下文压缩恢复 (legacy) ===")
            print(f"[TestVDB] Target: {target} v{version}")
            print(f"[TestVDB] Pipeline state: {pipeline_state}")
            print(f"[TestVDB] Phase: {phase}")
            print(f"[TestVDB] Round: {current_round}/{max_rounds}")
            print(f"[TestVDB] Confirmed defects: {defects_count}")
            print(f"[TestVDB] State file: {ms_path}")
            print("[TestVDB] Recovery: re-read mine_state.json and experience_handoff.json, "
                  f"then resume from round {current_round}, phase '{phase}'.")
        else:
            print("[TestVDB] WARNING: No state files found. Full pipeline restart needed.")

    print("[TestVDB] PostCompact: Recovery instructions printed.")


def _is_v3_state(ps):
    """v3 schema 判断（main 原用 `ps and ps.get("version", 0) >= 3`）。

    抽出为纯函数以便 self-check 守护（version 判断错误会导致走 legacy 分支丢失 v3 信息）。
    """
    return bool(ps) and ps.get("version", 0) >= 3


def _recovery_message(phase, phases_completed):
    """决定 PostCompact 恢复指令（设计附录 A CRITICAL：phase 不应在 phases_completed）。

    返回 (level, msg)：
    - level="WARNING"：phase 在 phases_completed 中 → 不一致（rollback 未正确移除），
      恢复时不应跳过 phase（否则 retry 被跳过）
    - level="OK"：phase 不在 → 正常继续/开始指令
    """
    if phase in phases_completed:
        return ("WARNING",
                f"phase={phase} 在 phases_completed 中 — 不一致（rollback 可能未正确移除），"
                f"恢复时不应跳过 {phase}")
    completed_str = ", ".join(phases_completed) if phases_completed else "无"
    if phases_completed:
        return ("OK", f"从 {phase} 阶段继续（跳过已完成的: {completed_str}）。")
    return ("OK", f"从 {phase} 阶段开始执行。")


def _self_check():
    """守护 v3 schema 判断 + 恢复指令决策（设计附录 A CRITICAL 路径）。

    覆盖：_is_v3_state 5 case（v3/legacy/空/None/新版）、_recovery_message 3 case
   （phase 不在 → OK 继续跳过；phase 在 → WARNING 不一致；phases_completed 空 → OK 开始）。
    纯函数测试，不读真实 state 文件。
    """
    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    # _is_v3_state — version 判断
    expect(_is_v3_state({"version": 3}) is True, "v3: version=3 应 True")
    expect(_is_v3_state({"version": 5}) is True, "v3: version=5 应 True")
    expect(_is_v3_state({"version": 2}) is False, "v3: version=2 应 False（legacy）")
    expect(_is_v3_state({}) is False, "v3: 缺 version 字段应 False")
    expect(_is_v3_state(None) is False, "v3: None 应 False")

    # _recovery_message — 恢复指令 + 一致性 WARNING
    lvl_ok, msg_ok = _recovery_message("REPORTING", ["ATTACK_GEN", "EXECUTION"])
    expect(lvl_ok == "OK", f"phase 不在 phases_completed → OK，实际 {lvl_ok}")
    expect("REPORTING" in msg_ok and "ATTACK_GEN" in msg_ok,
           f"OK msg 应含 phase 和 completed，实际 {msg_ok}")

    lvl_warn, msg_warn = _recovery_message("REPORTING", ["ATTACK_GEN", "REPORTING"])
    expect(lvl_warn == "WARNING",
           f"phase 在 phases_completed → WARNING 不一致，实际 {lvl_warn}")
    expect("不一致" in msg_warn, f"WARNING msg 应含 '不一致'，实际 {msg_warn}")

    lvl_empty, msg_empty = _recovery_message("ATTACK_GEN", [])
    expect(lvl_empty == "OK", f"空 phases_completed → OK 开始，实际 {lvl_empty}")
    expect("开始执行" in msg_empty, f"空 phases msg 应含 '开始执行'，实际 {msg_empty}")

    if failures:
        for m in failures:
            print(f"  FAIL: {m}", file=sys.stderr)
        print(f"self-check FAILED: {len(failures)} assertion(s)", file=sys.stderr)
        sys.exit(1)
    print("self-check OK")


if __name__ == "__main__":
    if "--self-check" in sys.argv:
        _self_check()
    else:
        main()
