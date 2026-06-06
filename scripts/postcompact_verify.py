#!/usr/bin/env python3
"""TestVDB Post-Compact State Recovery Verification.

Verifies that pipeline state can be recovered after context compaction,
and prints recovery instructions for the agent.
"""
import json
import os
import glob


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_session_id():
    """Find TESTVDB_SESSION_ID from env, .env file, or settings.json."""
    # 1. Environment variable
    sid = os.environ.get("TESTVDB_SESSION_ID", "")
    if sid:
        return sid

    # 2. .env file in plugin root
    plugin_root = _plugin_root()
    dot_env = os.path.join(plugin_root, ".env")
    if os.path.exists(dot_env):
        try:
            with open(dot_env, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line.startswith("TESTVDB_SESSION_ID="):
                        return line.split("=", 1)[1].strip()
        except OSError:
            pass

    # 3. settings.json
    settings_path = os.path.join(plugin_root, "settings.json")
    if os.path.exists(settings_path):
        try:
            with open(settings_path, encoding="utf-8") as f:
                settings = json.load(f)
            sid = settings.get("session", {}).get("session_id", "")
            if sid:
                return sid
        except (OSError, json.JSONDecodeError):
            pass

    return ""


def find_latest_state():
    """Find the latest mine_state.json, preferring current session if known."""
    plugin_root = _plugin_root()

    # If session_id is known, try to find its state file directly
    session_id = find_session_id()
    if session_id:
        # Search results/{target}/{version}/{timestamp}/ for matching session
        for root, dirs, files in os.walk(os.path.join(plugin_root, "results")):
            if "mine_state.json" in files:
                state_path = os.path.join(root, "mine_state.json")
                try:
                    with open(state_path, encoding="utf-8") as f:
                        state = json.load(f)
                    if state.get("session_id") == session_id:
                        return state_path
                except (OSError, json.JSONDecodeError):
                    continue

    # Fallback: search all session directories
    candidates = glob.glob(os.path.join(plugin_root, "results", "*", "*", "*", "mine_state.json"))
    if not candidates:
        return None

    return max(candidates, key=os.path.getmtime)


def main():
    print("[TestVDB] PostCompact: Context compressed. Verifying state...")

    # Try checkpoint first, then live state
    plugin_root = _plugin_root()
    ckpt_dir = os.path.join(plugin_root, "results", ".checkpoints")
    ckpt_state = os.path.join(ckpt_dir, "mine_state.json")

    state_path = None
    if os.path.exists(ckpt_state):
        state_path = ckpt_state
        print("[TestVDB] PostCompact: Using checkpoint state.")
    else:
        state_path = find_latest_state()

    if state_path and os.path.exists(state_path):
        with open(state_path, encoding="utf-8") as f:
            state = json.load(f)

        # Read correct fields from mine_state.json
        pipeline_state = state.get("pipeline_state", "unknown")
        current_round = state.get("current_round", "?")
        max_rounds = state.get("max_rounds", "?")
        target = state.get("target", "?")
        version = state.get("version", "?")
        defects_count = len(state.get("defects", []))

        # Read pipeline_state.json for phase info if available
        phase = "unknown"
        ps_path = os.path.join(os.path.dirname(state_path), "pipeline_state.json")
        if os.path.exists(ps_path):
            with open(ps_path, encoding="utf-8") as f:
                ps = json.load(f)
            phase = ps.get("phase", "unknown")

        print(f"[TestVDB] Target: {target} v{version}")
        print(f"[TestVDB] Pipeline state: {pipeline_state}")
        print(f"[TestVDB] Phase: {phase}")
        print(f"[TestVDB] Round: {current_round}/{max_rounds}")
        print(f"[TestVDB] Confirmed defects: {defects_count}")
        print(f"[TestVDB] State file: {state_path}")
        print("[TestVDB] Recovery: re-read mine_state.json and experience_handoff.json, "
              f"then resume from round {current_round}, phase '{phase}'.")
    else:
        print("[TestVDB] WARNING: mine_state.json not found. Full pipeline restart needed.")


if __name__ == "__main__":
    main()
