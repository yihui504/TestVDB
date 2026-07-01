#!/usr/bin/env python3
"""
_pipeline_utils — shared script infrastructure (ADR-0007).

Common utilities consumed by all scripts in scripts/:
  - JSON safe I/O
  - Pipeline path conventions
  - Windows encoding setup
  - Log file discovery

Underscore prefix signals "internal module, not public API".

Usage:
  from _pipeline_utils import read_json, write_json, debate_log_path
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any, Optional

# ── Encoding (idempotent) ─────────────────────────────────────

def setup_encoding() -> None:
    """Reconfigure stdout/stderr for UTF-8 on Windows.  Idempotent."""
    if sys.platform == "win32":
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
            sys.stderr.reconfigure(encoding="utf-8", errors="replace")
        except AttributeError:
            pass  # already done or not a tty


# ── JSON I/O ──────────────────────────────────────────────────

def read_json(path: str | Path) -> dict | None:
    """Safely read a JSON file.  Returns None on missing/corrupt file."""
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None


def write_json(path: str | Path, data: Any) -> bool:
    """Safely write JSON.  Creates parent directories.  Returns True on success."""
    p = Path(path)
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        return True
    except OSError:
        return False


# ── Path conventions ──────────────────────────────────────────

def debate_log_path(session_dir: str | Path, name: str) -> Path:
    """Build path to a debate log JSON file.

    Example: debate_log_path(sd, 'stage2_aggregation') → sd/debate_logs/stage2_aggregation.json
    """
    return Path(session_dir) / "debate_logs" / f"{name}.json"


def session_path(session_dir: str | Path, *parts: str) -> Path:
    """Build path inside a session directory.

    Example: session_path(sd, 'defects', 'defect-1.md')
    """
    return Path(session_dir).joinpath(*parts)


def plugin_root() -> Optional[Path]:
    """Find the TestVDB plugin root directory.

    Priority: TESTVDB_PLUGIN_ROOT env var → walk up from cwd looking
    for commands/mine.md.
    """
    env = os.environ.get("TESTVDB_PLUGIN_ROOT")
    if env and Path(env).is_dir():
        return Path(env)

    cur = Path.cwd()
    for _ in range(7):
        if (cur / "commands" / "mine.md").is_file():
            return cur
        parent = cur.parent
        if parent == cur:
            break
        cur = parent
    return None


# ── Log discovery ─────────────────────────────────────────────

def find_log(session_dir: str | Path, script_name: str) -> Optional[Path]:
    """Find the output log for a given script in the session directory.

    Searches for output_*{script_name}*.log, falls back to any output_*.log.
    """
    sd = Path(session_dir)
    candidates = list(sd.glob(f"output_*{script_name}*.log"))
    if not candidates:
        candidates = list(sd.glob("output_*.log"))
    return candidates[0] if candidates else None


def find_logs(session_dir: str | Path, script_name: str = "") -> list[Path]:
    """Find all output logs, optionally filtered by script name."""
    sd = Path(session_dir)
    if script_name:
        return sorted(sd.glob(f"output_*{script_name}*.log"))
    return sorted(sd.glob("output_*.log"))


# ── File helpers ──────────────────────────────────────────────

def is_done(path: str | Path) -> bool:
    """Check if a .done marker exists for the given file."""
    return Path(str(path) + ".done").exists()


def touch_done(path: str | Path) -> None:
    """Create a .done marker for the given file."""
    Path(str(path) + ".done").touch()
