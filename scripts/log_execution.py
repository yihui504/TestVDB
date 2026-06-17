#!/usr/bin/env python3
"""PostToolUse(Bash) — append execution trace to results/session_execution_log.jsonl.

Best-effort, non-blocking. Re-implements the trace that the (previously missing)
script was meant to provide; never fails the tool call.
"""
from __future__ import annotations

import datetime
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _session_utils import _plugin_root  # noqa: E402


def main() -> int:
    raw = sys.stdin.read()
    try:
        data = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        return 0
    cmd = (data.get("tool_input") or {}).get("command", "")
    if not cmd:
        return 0
    log_path = os.path.join(_plugin_root(), "results", "session_execution_log.jsonl")
    try:
        os.makedirs(os.path.dirname(log_path), exist_ok=True)
        entry = {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "type": "Bash",
            "command": cmd[:500],
        }
        with open(log_path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
