#!/usr/bin/env python3
"""TestVDB Post-Bash Execution Logger.

Appends a timestamped entry to the session execution log after every
Bash tool invocation. Reads tool invocation context from stdin (JSON).
"""
import json
import os
import sys
import time
from _session_utils import find_session_id


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    # Read tool invocation context from stdin (if available)
    tool_context = {}
    if not sys.stdin.isatty():
        try:
            raw = sys.stdin.read()
            if raw.strip():
                tool_context = json.loads(raw)
        except (json.JSONDecodeError, UnicodeDecodeError):
            tool_context = {"raw_input": "parse_error"}

    plugin_root = _plugin_root()
    session_id = find_session_id()
    results_dir = os.environ.get("TESTVDB_RESULTS_DIR", os.path.join(plugin_root, "results"))
    os.makedirs(results_dir, exist_ok=True)

    entry = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "session_id": session_id,
        "type": "Bash",
        "command": tool_context.get("command", ""),
        "exit_code": tool_context.get("exit_code"),
        "note": "TestVDB agent execution",
    }

    log_path = os.path.join(results_dir, "session_execution_log.jsonl")
    with open(log_path, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
