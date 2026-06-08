#!/usr/bin/env python3
"""TestVDB Pre-Compact State Save.

Saves critical state files to a checkpoint directory before context
compaction occurs, ensuring no progress is lost.
"""
import json
import os
import shutil
import glob
from _session_utils import find_session_id


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_session_dir():
    """Find the active session directory by looking for mine_state.json."""
    plugin_root = _plugin_root()
    # Check current directory first
    if os.path.exists("mine_state.json"):
        return "."

    # Search results/{target}/{version}/{timestamp}/ pattern
    for state_file in glob.glob(os.path.join(plugin_root, "results", "*", "*", "*", "mine_state.json")):
        return os.path.dirname(state_file)

    return None


def main():
    print("[TestVDB] PreCompact: Saving state before compaction...")

    session_id = find_session_id()
    session_dir = find_session_dir()

    plugin_root = _plugin_root()
    ckpt_dir = os.path.join(plugin_root, "results", ".checkpoints")
    os.makedirs(ckpt_dir, exist_ok=True)

    state_files = ["mine_state.json", "coverage.json", "pipeline_state.json",
                   "experience_handoff.json"]

    saved = []
    for filename in state_files:
        src = filename if session_dir is None else os.path.join(session_dir, filename)
        if os.path.exists(src):
            dst = os.path.join(ckpt_dir, filename)
            shutil.copy2(src, dst)
            saved.append(filename)

    # Also save debate logs if they exist
    for debate_log in glob.glob(os.path.join(plugin_root, "results", "*", "*", "*", "debate_logs", "*.json")):
        dst = os.path.join(ckpt_dir, os.path.basename(debate_log))
        shutil.copy2(debate_log, dst)
        saved.append(os.path.basename(debate_log))

    if saved:
        print(f"[TestVDB] PreCompact: Saved {len(saved)} files: {', '.join(saved)}")
    else:
        print("[TestVDB] PreCompact: WARNING - No state files found to save.")

    print("[TestVDB] PreCompact: State checkpoint saved.")


if __name__ == "__main__":
    main()
