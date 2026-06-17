#!/usr/bin/env python3
"""PostToolUse(Write) guard — keep scratch files out of the plugin root.

Catches the "everything dumped in the project root" anti-pattern that caused
the original file mess (root-level ``_*.json``, ``push_*.json``, ``*.log``,
``run_*.py``, ``deepseekapikey.txt``, …). Non-blocking: prints guidance to
stderr and exits 0 so it never breaks a legitimate write.

stdin: Claude Code PostToolUse hook JSON, ``tool_input.file_path``.
"""
from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _session_utils import _plugin_root  # noqa: E402

# Sanctioned top-level entries; anything else sitting directly at the root is suspect.
LEGAL_TOPLEVEL = {
    "results", "intelligence", "issues", "strategy_registry", "agents",
    "scripts", "commands", "contracts", "docker", "skills", "docs", "data",
    "hooks", "tests", "verify", ".testvdb", ".claude", ".claude-plugin",
    ".git", ".omc", ".venv", "target", "AGENTS.md", "README.md", "README_zh.md",
    "LICENSE", "pytest.ini", "settings.json", ".mcp.json", ".env", ".gitignore",
}
SCRATCH_EXT = {".json", ".log", ".tmp", ".txt", ".csv", ".out"}
SCRATCH_NAME_PREFIXES = ("_", "tmp", "push", "run_", "check_")
SECRET_HINTS = ("apikey", "secret", "token", "password", "credential")


def main() -> int:
    raw = sys.stdin.read()
    try:
        data = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        return 0
    tool_input = data.get("tool_input") or {}
    fp = tool_input.get("file_path") or tool_input.get("path") or ""
    if not fp:
        return 0
    root = os.path.abspath(_plugin_root())
    ap = os.path.abspath(fp)
    try:
        rel = os.path.relpath(ap, root)
    except ValueError:
        return 0
    if rel == "." or rel.startswith(".." + os.sep):
        return 0  # outside plugin root, not our concern
    parts = rel.split(os.sep)
    if len(parts) > 1:
        return 0  # nested under some subdir — leave it alone
    name = parts[0]
    if name in LEGAL_TOPLEVEL:
        return 0
    _base, ext = os.path.splitext(name)
    lower = name.lower()
    is_scratch = (
        ext.lower() in SCRATCH_EXT
        or name.startswith(SCRATCH_NAME_PREFIXES)
        or any(h in lower for h in SECRET_HINTS)
    )
    if not is_scratch:
        return 0
    print(f"[TestVDB] 写位置警告: '{name}' 直接落在插件根目录。", file=sys.stderr)
    print("[TestVDB] 流水线产出 → results/<target>/<version>/；", file=sys.stderr)
    print("[TestVDB] 中间/临时产物 → .testvdb/tmp/；密钥 → 环境变量(不入库)。", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
