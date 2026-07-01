#!/usr/bin/env python3
"""
PipelineState — TestVDB pipeline state machine (ADR-0004).

Provides a deep module that owns pipeline_state.json:
  - Small interface: create / load / phase / is_running / summary / advance / mutate / mark_done
  - Validates phase transitions at the seam (hardcoded transition map)
  - CLI wrapper for mine.md Bash steps

Usage:
  import: from pipeline_state import PipelineState
  CLI:    python scripts/pipeline_state.py {init|advance|mutate|status} ...

Transition map (ADR-0004 — hardcoded, not config):
  ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → DEBATE_S2
              → VERIFY_LIVE → REPORTING → DEFECT_REVIEW → STATE_SAVE
              → CLEANUP → DONE
  ROUND_START may repeat (multi-round loop).
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Optional

from _pipeline_utils import setup_encoding

setup_encoding()

# ── Constants ─────────────────────────────────────────────────

PHASE_ORDER = [
    "ROUND_START",
    "ATTACK_GEN",
    "DEBATE_S1",
    "EXECUTION",
    "DEBATE_S2",
    "VERIFY_LIVE",
    "REPORTING",
    "DEFECT_REVIEW",
    "STATE_SAVE",
    "CLEANUP",
    "DONE",
]

# Valid forward transitions.  ROUND_START can also self-loop for multi-round.
_TRANSITIONS: dict[str, set[str]] = {
    "ROUND_START":  {"ATTACK_GEN", "ROUND_START"},
    "ATTACK_GEN":   {"DEBATE_S1"},
    "DEBATE_S1":    {"EXECUTION"},
    "EXECUTION":    {"DEBATE_S2"},
    "DEBATE_S2":    {"VERIFY_LIVE"},
    "VERIFY_LIVE":  {"REPORTING"},
    "REPORTING":    {"DEFECT_REVIEW"},
    "DEFECT_REVIEW": {"STATE_SAVE"},
    "STATE_SAVE":   {"CLEANUP", "ROUND_START"},
    "CLEANUP":      {"DONE"},
    "DONE":         set(),
}

# Fields that mutate() is allowed to update (whitelist).
_MUTABLE_GLOBAL_STATE = {
    "total_defects_confirmed",
    "consecutive_no_defect_rounds",
    "overall_coverage_pct",
    "docker_container_running",
}
_MUTABLE_TOP = {
    "current_round",
    "phase_step_index",
    "turn_type",
    "project_root",
    "timestamp_dir",
}


# ── Exceptions ────────────────────────────────────────────────

class InvalidTransition(ValueError):
    """Raised when advance() is called with an illegal phase transition."""
    def __init__(self, current: str, target: str):
        super().__init__(
            f"Invalid transition: {current} → {target}. "
            f"Allowed targets from {current}: {_TRANSITIONS.get(current, set())}"
        )


class StateNotFound(FileNotFoundError):
    """Raised when pipeline_state.json does not exist at the given session_dir."""


# ── PipelineState ─────────────────────────────────────────────

@dataclass
class PipelineState:
    """Owns the pipeline state machine (ADR-0004).

    All mutations flow through advance() / mutate() / mark_done() — no
    direct field writes from outside this module.
    """

    _path: Path
    _data: dict

    # -- constructors -------------------------------------------------

    @classmethod
    def create(
        cls,
        target: str,
        version: str,
        max_rounds: int,
        min_defects: int,
        session_dir: str | Path,
        project_root: str = "",
    ) -> "PipelineState":
        """Initialise a fresh pipeline_state.json (mine.md Step 7)."""
        sd = Path(session_dir)
        sd.mkdir(parents=True, exist_ok=True)
        timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H-%M-%SZ")
        now_iso = datetime.now(timezone.utc).isoformat()

        session_id = _make_session_id(target, version)

        data = {
            "version": 3,
            "session_id": session_id,
            "target": target,
            "version_target": version,
            "current_round": 1,
            "max_rounds": max_rounds,
            "min_defects": min_defects,
            "phase": "ROUND_START",
            "phase_step_index": 0,
            "turn_type": "setup",
            "project_root": project_root or str(sd.parent),
            "session_dir": str(sd),
            "timestamp_dir": timestamp,
            "phases_completed": [],
            "phase_data": {},
            "global_state": {
                "total_defects_confirmed": 0,
                "consecutive_no_defect_rounds": 0,
                "overall_coverage_pct": 0.0,
                "docker_container_running": False,
            },
            "error_log": [],
            "timestamps": {
                "session_started": now_iso,
                "last_phase_change": now_iso,
            },
        }

        path = sd / "pipeline_state.json"
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)

        return cls(_path=path, _data=data)

    @classmethod
    def load(cls, session_dir: str | Path) -> "PipelineState":
        """Load existing pipeline_state.json from a session directory."""
        sd = Path(session_dir)
        path = sd / "pipeline_state.json"
        if not path.exists():
            raise StateNotFound(str(path))
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        return cls(_path=path, _data=data)

    # -- queries (read-only) ------------------------------------------

    @property
    def phase(self) -> str:
        """Current phase. Read-only — mutate via advance()."""
        return self._data["phase"]

    @property
    def is_running(self) -> bool:
        """True while the pipeline has not reached DONE."""
        return self.phase != "DONE"

    @property
    def current_round(self) -> int:
        return self._data["current_round"]

    def summary(self) -> dict:
        """Return a stable, small dict for consumers (reconstruct_context, pipeline_gate)."""
        gs = self._data["global_state"]
        return {
            "phase": self.phase,
            "round": self._data["current_round"],
            "max_rounds": self._data["max_rounds"],
            "total_defects": gs["total_defects_confirmed"],
            "coverage_pct": gs["overall_coverage_pct"],
            "is_running": self.is_running,
            "turn_type": self._data["turn_type"],
        }

    # -- mutations ----------------------------------------------------

    def advance(self, to_phase: str, *, phase_data: dict | None = None) -> None:
        """Transition to *to_phase*, validating legality at the seam.

        Raises InvalidTransition if the move is not allowed.
        Updates phases_completed, phase_step_index, and writes to disk.
        """
        current = self.phase
        allowed = _TRANSITIONS.get(current, set())
        if to_phase not in allowed:
            raise InvalidTransition(current, to_phase)

        # New round?  ROUND_START self-loop resets the per-round tracking.
        if to_phase == "ROUND_START":
            if current != "ROUND_START":
                self._data["current_round"] += 1
            self._data["phases_completed"] = ["ROUND_START"]
            self._data["phase_step_index"] = 0
        else:
            completed = self._data.get("phases_completed", [])
            if current not in completed and current != "ROUND_START":
                completed.append(current)
            self._data["phases_completed"] = completed
            self._data["phase_step_index"] = PHASE_ORDER.index(to_phase)

        self._data["phase"] = to_phase

        if phase_data is not None:
            pd = self._data.setdefault("phase_data", {})
            pd[to_phase] = phase_data

        # ROUND_START from STATE_SAVE or CLEANUP → turn_type becomes "loop"
        if to_phase == "ROUND_START" and current in ("STATE_SAVE", "CLEANUP"):
            self._data["turn_type"] = "loop"

        self._touch()
        self._write()

    def mutate(self, **kwargs) -> None:
        """Update whitelisted counters / top-level fields.  Write to disk.

        Accepted fields:
          global_state: total_defects_confirmed, consecutive_no_defect_rounds,
                        overall_coverage_pct, docker_container_running
          top-level:    current_round, phase_step_index, turn_type,
                        project_root, timestamp_dir
        """
        gs = self._data.setdefault("global_state", {})
        for k, v in kwargs.items():
            if k in _MUTABLE_GLOBAL_STATE:
                gs[k] = v
            elif k in _MUTABLE_TOP:
                self._data[k] = v
            else:
                raise KeyError(
                    f"mutate() does not accept '{k}'. "
                    f"Mutable fields: {_MUTABLE_GLOBAL_STATE | _MUTABLE_TOP}"
                )

        self._touch()
        self._write()

    def mark_done(self) -> None:
        """Mark pipeline as DONE and write to disk."""
        self._data["phase"] = "DONE"
        self._data["turn_type"] = "done"
        self._data["phases_completed"] = list(PHASE_ORDER)
        self._data["global_state"]["docker_container_running"] = False
        self._touch()
        self._write()

    # -- internals ----------------------------------------------------

    def _touch(self) -> None:
        self._data["timestamps"]["last_phase_change"] = (
            datetime.now(timezone.utc).isoformat()
        )

    def _write(self) -> None:
        tmp = self._path.with_suffix(".tmp")
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self._data, f, indent=2, ensure_ascii=False)
        os.replace(tmp, self._path)

    # -- dict-like access (backward compat if needed) -----------------

    def to_dict(self) -> dict:
        """Return a deep copy of the raw state dict."""
        return json.loads(json.dumps(self._data))


# ── Helpers ───────────────────────────────────────────────────

def _make_session_id(target: str, version: str) -> str:
    """Generate a sanitised session_id: {target}-{version_short}-r{N}.

    Matches the convention from mine.md Step 7 / orchestrator.md Step 7.
    """
    v = version.lstrip("v").replace(".", "")
    return f"{target}-{v}-r1"


# ── CLI ───────────────────────────────────────────────────────

def _cli_init(args):
    state = PipelineState.create(
        target=args.target,
        version=args.version,
        max_rounds=args.max_rounds,
        min_defects=args.min_defects,
        session_dir=args.session_dir,
        project_root=args.project_root or "",
    )
    print(json.dumps(state.summary(), ensure_ascii=False))


def _cli_advance(args):
    state = PipelineState.load(args.session_dir)
    phase_data = None
    if args.phase_data:
        try:
            phase_data = json.loads(args.phase_data)
        except json.JSONDecodeError:
            print(f"ERROR: invalid JSON for --phase-data: {args.phase_data}", file=sys.stderr)
            sys.exit(2)
    state.advance(args.phase, phase_data=phase_data)
    print(json.dumps(state.summary(), ensure_ascii=False))


def _cli_mutate(args):
    state = PipelineState.load(args.session_dir)
    kwargs = {}
    if args.current_round is not None:
        kwargs["current_round"] = args.current_round
    if args.total_defects is not None:
        kwargs["total_defects_confirmed"] = args.total_defects
    if args.coverage is not None:
        kwargs["overall_coverage_pct"] = args.coverage
    if args.consecutive_no_defect is not None:
        kwargs["consecutive_no_defect_rounds"] = args.consecutive_no_defect
    if args.docker_running is not None:
        kwargs["docker_container_running"] = args.docker_running
    if args.project_root:
        kwargs["project_root"] = args.project_root
    state.mutate(**kwargs)
    print(json.dumps(state.summary(), ensure_ascii=False))


def _cli_status(args):
    state = PipelineState.load(args.session_dir)
    print(json.dumps(state.summary(), ensure_ascii=False))
    sys.exit(0 if state.is_running else 1)


def main():
    parser = argparse.ArgumentParser(description="TestVDB PipelineState (ADR-0004)")
    sub = parser.add_subparsers(dest="command", required=True)

    # init
    p_init = sub.add_parser("init", help="Create fresh pipeline_state.json")
    p_init.add_argument("--target", required=True)
    p_init.add_argument("--version", required=True)
    p_init.add_argument("--max-rounds", type=int, default=5)
    p_init.add_argument("--min-defects", type=int, default=1)
    p_init.add_argument("--session-dir", required=True)
    p_init.add_argument("--project-root", default="")

    # advance
    p_adv = sub.add_parser("advance", help="Transition to next phase")
    p_adv.add_argument("--session-dir", required=True)
    p_adv.add_argument("--phase", required=True)
    p_adv.add_argument("--phase-data", default=None)

    # mutate
    p_mut = sub.add_parser("mutate", help="Update counters / metadata")
    p_mut.add_argument("--session-dir", required=True)
    p_mut.add_argument("--current-round", type=int, default=None)
    p_mut.add_argument("--total-defects", type=int, default=None)
    p_mut.add_argument("--coverage", type=float, default=None)
    p_mut.add_argument("--consecutive-no-defect", type=int, default=None)
    p_mut.add_argument("--docker-running", type=lambda x: x.lower() == "true", default=None)
    p_mut.add_argument("--project-root", default=None)

    # status
    p_stat = sub.add_parser("status", help="Print pipeline summary")
    p_stat.add_argument("--session-dir", required=True)

    args = parser.parse_args()

    handlers = {
        "init": _cli_init,
        "advance": _cli_advance,
        "mutate": _cli_mutate,
        "status": _cli_status,
    }
    handlers[args.command](args)


if __name__ == "__main__":
    main()
