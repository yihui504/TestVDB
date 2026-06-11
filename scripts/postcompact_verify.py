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


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def _read_json(path: str):
    """Read JSON file, return None on failure."""
    if not os.path.isfile(path):
        return None
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return None


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
                ps = _read_json(ps_path)
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
                ms = _read_json(ms_path)
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
    "DEBATE_S2": "辩论 Stage 2（Judge Quartet + 去重）",
    "REPORTING": "缺陷报告生成",
    "DEFECT_REVIEW": "逐缺陷审查",
    "STATE_SAVE": "状态保存 + 终止检查",
    "CLEANUP": "汇总 + 清理",
}


def main():
    # Ensure UTF-8 output on Windows
    if sys.stdout.encoding and sys.stdout.encoding.lower() != "utf-8":
        sys.stdout.reconfigure(encoding="utf-8")

    print("[TestVDB] PostCompact: Context compressed. Verifying state...")

    # Try pipeline_state.json (v3) first
    ps_path = find_latest_pipeline_state()
    ps = _read_json(ps_path) if ps_path else None

    if ps and ps.get("version", 0) >= 3:
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

        if phases_completed:
            print(f"[TestVDB] Recovery: 从 {phase} 阶段继续（跳过已完成的: {completed_str}）。")
        else:
            print(f"[TestVDB] Recovery: 从 {phase} 阶段开始执行。")

        # If loop turn, remind about reconstruct_context.py
        if turn_type == "loop":
            print(f"[TestVDB] Recovery: 运行 python scripts/reconstruct_context.py --session-dir \"{session_dir}\" 获取完整上下文。")

        print(f"[TestVDB] Recovery: session_dir={session_dir}")

    else:
        # Fallback to legacy mine_state.json
        ms_path = find_latest_mine_state()
        ms = _read_json(ms_path) if ms_path else None

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
                legacy_ps = _read_json(legacy_ps_path)
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


if __name__ == "__main__":
    main()
