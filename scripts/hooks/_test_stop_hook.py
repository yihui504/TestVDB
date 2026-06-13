#!/usr/bin/env python3
"""
TestVDB Step 1 — Stop hook trigger verification (temporary).

Appends a timestamp + session hint each time Claude Code dispatches a Stop event.
If this log gets entries, the Stop hook fires correctly on this Claude Code version
and we can safely wire `pipeline_gate.py` onto it. If it stays empty, the
PostToolUse/Stop not-firing issue (github.com/thedotmack/claude-mem/issues/504)
is present and the hooks plan must be re-evaluated.

Remove or replace this file once Step 1 passes and pipeline_gate.py is wired.
"""
from __future__ import annotations

import os
import sys
from datetime import datetime

# Hook input arrives on stdin (JSON). We don't need to parse it for the test,
# but we capture its length to prove the harness actually invoked us.
try:
    raw_stdin = sys.stdin.read()
except Exception:
    raw_stdin = ""

log_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "_stop_hook_test.log")
timestamp = datetime.now().isoformat(timespec="seconds")
cwd = os.getcwd()

with open(log_path, "a", encoding="utf-8") as fh:
    fh.write(f"[{timestamp}] Stop hook FIRED | cwd={cwd} | stdin_len={len(raw_stdin)}\n")

# exit 0 = allow Claude to stop. Do NOT exit 2 here — this is a passive probe.
sys.exit(0)
