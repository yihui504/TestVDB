#!/usr/bin/env python3
"""TestVDB Post-Compact State Recovery Verification.

Verifies that pipeline state can be recovered after context compaction,
and prints recovery instructions for the agent.
"""
import json
import os
import glob


def find_latest_state():
    """Find the latest mine_state.json across all session directories."""
    # Check current directory first
    if os.path.exists("mine_state.json"):
        return "mine_state.json"

    # Search results/{target}/{version}/{timestamp}/ pattern
    candidates = glob.glob(os.path.join("results", "*", "*", "*", "mine_state.json"))
    if not candidates:
        return None

    # Return the most recently modified one
    return max(candidates, key=os.path.getmtime)


def main():
    print("[TestVDB] PostCompact: Context compressed. Verifying state...")

    # Try checkpoint first, then live state
    ckpt_dir = os.path.join("results", ".checkpoints")
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
