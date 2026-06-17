#!/usr/bin/env python3
"""TestVDB session index — list all sessions with live status.

Replaces a hand-maintained ``results/.sessions/index.json`` (which always
drifted out of sync). Reads ``mine_state.json`` / ``pipeline_state.json``
directly, so the index is always accurate.

Usage:
    python scripts/session_index.py              # all sessions, newest first
    python scripts/session_index.py --target qdrant
    python scripts/session_index.py --running    # only status == running
    python scripts/session_index.py --json       # machine-parseable
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _session_utils import _plugin_root  # noqa: E402

if sys.platform == "win32":
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")  # type: ignore[attr-defined]
    except (AttributeError, OSError):
        pass


def _read_json(path: Path) -> dict | None:
    try:
        with path.open(encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else None
    except (OSError, json.JSONDecodeError):
        return None


def collect(root: Path) -> list[dict]:
    """Walk <root>/results and build one row per mine_state.json found."""
    results_dir = root / "results"
    if not results_dir.is_dir():
        return []
    rows: list[dict] = []
    for dirpath, dirs, files in os.walk(results_dir):
        dirs[:] = [d for d in dirs if not d.startswith(".")]  # skip .checkpoints / hidden
        if "mine_state.json" not in files:
            continue
        ms_path = Path(dirpath) / "mine_state.json"
        ms = _read_json(ms_path) or {}
        ps = _read_json(Path(dirpath) / "pipeline_state.json") or {}
        try:
            mtime = datetime.fromtimestamp(ms_path.stat().st_mtime, tz=timezone.utc)
        except OSError:
            mtime = datetime.min.replace(tzinfo=timezone.utc)
        target = ms.get("target") or ps.get("target") or "?"
        version = ms.get("version") or ps.get("version_target") or "?"
        rows.append({
            "session_id": ms.get("session_id") or ps.get("session_id") or "?",
            "target": target,
            "version": version,
            "status": ms.get("status") or ps.get("phase") or "?",
            "phase": ps.get("phase") or ms.get("phase") or "-",
            "round": f"{ps.get('current_round', '?')}/{ps.get('max_rounds', '?')}",
            "mtime": mtime,
            "dir": str(Path(dirpath).relative_to(root)),
        })
    rows.sort(key=lambda r: r["mtime"], reverse=True)
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--target", help="filter by target (milvus/qdrant/weaviate/pgvector)")
    ap.add_argument("--running", action="store_true", help="only status == running")
    ap.add_argument("--json", action="store_true", help="emit JSON")
    args = ap.parse_args()

    root = Path(_plugin_root())
    rows = collect(root)
    if args.target:
        rows = [r for r in rows if r["target"] == args.target]
    if args.running:
        rows = [r for r in rows if str(r["status"]).lower() == "running"]

    if args.json:
        for r in rows:
            r["mtime"] = r["mtime"].isoformat(timespec="seconds")
        print(json.dumps(rows, ensure_ascii=False, indent=2))
        return 0

    if not rows:
        print(f"[session_index] no sessions under {root / 'results'}")
        return 0

    print(f"[session_index] {len(rows)} session(s) under {root / 'results'}  (newest first)\n")
    hdr = f"{'SESSION_ID':<32} {'TARGET':<10} {'VERSION':<10} {'STATUS':<10} {'PHASE':<12} {'ROUND':<8} {'DIR'}"
    print(hdr)
    print("-" * len(hdr))
    for r in rows:
        print(
            f"{str(r['session_id'])[:32]:<32} {r['target']:<10} {str(r['version'])[:10]:<10} "
            f"{str(r['status'])[:10]:<10} {str(r['phase'])[:12]:<12} {r['round']:<8} {r['dir']}"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
