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
import subprocess
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

# Transition gates (设计 §3.1, §3.4 — 挂 CLI 层 _cli_advance)。
# ponytail: 只挂已实现 + 独立可 block 的 gate。
# 待加：verify_defects（DEFECT_REVIEW→STATE_SAVE，exit 1=NEEDS_IMPROVEMENT 触发 retry，
#       非简单 block，需 1b.5 retry 回退机制一起挂）。
TRANSITION_GATES: dict[tuple[str, str], list[str]] = {
    # aggregate_votes 先跑（转换器：重写 stage2_aggregation.json 为代码版，规则 1-6），
    # gate_severity_coverage 后跑（读新 aggregation）。顺序 = 列表顺序（设计 §5 + §3.1）。
    ("DEBATE_S2", "VERIFY_LIVE"):    ["aggregate_votes.py", "gate_severity_coverage.py"],
    ("CLEANUP", "DONE"):             ["gate_summary_consistency.py"],
}

# 回退边（设计 §3.2 retry 状态机）。verify_defects 三态路由（advance/delete/rollback），
# 在 _cli_advance 特殊分支处理，不进 TRANSITION_GATES（非二态 blocker）。
# ponytail: 仅此一条回退边 — YAGNI，未来需要时再加
_ROLLBACK_ALLOWED: dict[str, set[str]] = {
    "DEFECT_REVIEW": {"REPORTING"},
}
MAX_RETRY = 3

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

    def rollback(self, to_phase: str) -> None:
        """回退到 to_phase（仅 _ROLLBACK_ALLOWED 允许的边）。

        从 phases_completed 移除 to_phase 及其后阶段，让 reconstruct_context 的
        _get_next_phase 在 PostCompact 恢复时正确回到 to_phase（设计 §3.2 CRITICAL：
        否则线性前进逻辑会跳过 retry 直接推到 STATE_SAVE）。
        ponytail: 独立于 advance() — forward 流程零回归；不碰 defect_retry（由调用方维护）。
        """
        current = self.phase
        if to_phase not in _ROLLBACK_ALLOWED.get(current, set()):
            raise InvalidTransition(current, to_phase)
        idx = PHASE_ORDER.index(to_phase)
        completed = self._data.get("phases_completed", [])
        self._data["phases_completed"] = [
            p for p in completed
            if p in PHASE_ORDER and PHASE_ORDER.index(p) < idx
        ]
        self._data["phase"] = to_phase
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


# ── Transition gates (设计 §3.4 — 挂 CLI 层) ──────────────────

def _run_transition_gates(from_phase: str, to_phase: str, session_dir: str, target: str) -> list[dict]:
    """跑 transition 前置 gate 脚本，返回每 gate 结果。
    ponytail: subprocess 隔离跑独立 gate（复用 §3.3 CLI 契约）。
    TESTVDB_ENFORCE_STRICT=1 时 fail 抛 InvalidTransition；否则 advisory warn 继续。"""
    gates = TRANSITION_GATES.get((from_phase, to_phase), [])
    if not gates:
        return []
    strict = os.environ.get("TESTVDB_ENFORCE_STRICT") == "1"
    scripts_dir = Path(__file__).parent
    results = []
    for g in gates:
        script = scripts_dir / g
        if not script.exists():
            # ponytail: gate 脚本未实现 → 跳过（不阻塞流水线，Phase 1b 渐进填充）
            results.append({"gate": g, "status": "skip", "reason": "script not found"})
            continue
        cmd = [sys.executable, str(script), session_dir, "--target", target or "unknown"]
        if strict:
            cmd.append("--strict")
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace")
            rc = proc.returncode
            out = (proc.stdout or "").strip()
        except Exception as e:
            rc, out = 1, str(e)
        # 解析 gate JSON status（advisory 模式 gate exit 0 但 status 可能 fail — 看 JSON 才准）
        gate_status, gate_reason = "", ""
        try:
            data = json.loads(out)
            gate_status = data.get("status", "")
            gate_reason = data.get("reason", "")
        except (json.JSONDecodeError, ValueError):
            pass
        detail = gate_reason or (out.splitlines()[-1] if out else "")
        failed = (rc != 0) or (gate_status == "fail")
        results.append({"gate": g, "status": "fail" if failed else "pass", "rc": rc, "detail": detail[:200]})
        if failed:
            if strict:
                raise InvalidTransition(from_phase, to_phase)
            print(f"[GATE advisory] {from_phase}→{to_phase}: gate {g} failed"
                  + (f": {detail[:140]}" if detail else ""), file=sys.stderr)
    return results


# ── Defect review routing（设计 §3.2 retry 状态机）─────────────

def _handle_defect_review(state: "PipelineState", session_dir: str, target: str) -> str:
    """跑 verify_defects，按 exit code 三态路由。返回 'advance' 或 'rollback'。

    ponytail: retry/删文件是 mechanism（代码强制），不受 TESTVDB_ENFORCE_STRICT 影响 —
    strict 只控制 gate block；rollback 是改方向不是阻塞。通用：基于 verify_defects 的
    exit code + defect_review.json 数据契约，不引用任何 DB 名。
    """
    script = Path(__file__).parent / "verify_defects.py"
    if not script.exists():
        return "advance"  # verify_defects 缺失 — 不阻塞（advisory）
    cmd = [sys.executable, str(script), session_dir, "--target", target or "unknown"]
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True,
                              encoding="utf-8", errors="replace")
        rc = proc.returncode
    except Exception:
        return "advance"  # subprocess 故障 — 不阻塞

    # rc 3/4 = 启动错误（无数据/session_dir 不存在）— verify_defects 已警告
    if rc in (3, 4):
        return "advance"

    # 读 defect_review.json（verify_defects 落盘的结构化结果）
    review_path = Path(session_dir) / "defect_review.json"
    defects: list[dict] = []
    if review_path.exists():
        try:
            with open(review_path, encoding="utf-8") as f:
                defects = json.load(f).get("defects", [])
        except (json.JSONDecodeError, OSError):
            pass

    def _delete_with_status(status: str) -> None:
        for d in defects:
            if d.get("status") == status:
                fp = Path(session_dir) / "defects" / d.get("file", "")
                if fp.exists():
                    fp.unlink()

    # exit 2 (FALSE_POSITIVE) — 删文件，优先于 retry（设计 §3.2 混合规则）
    if rc == 2:
        _delete_with_status("FALSE_POSITIVE")
        if not any(d.get("status") == "NEEDS_IMPROVEMENT" for d in defects):
            return "advance"  # 仅假阳性，删完即可 advance

    # exit 1 (NEEDS_IMPROVEMENT) — per-defect retry，超限降级删文件
    if rc == 1:
        retry_map = state._data.setdefault("defect_retry", {})
        needs_rollback = False
        for d in defects:
            if d.get("status") != "NEEDS_IMPROVEMENT":
                continue
            did = d.get("id", "")
            if retry_map.get(did, 0) < MAX_RETRY:
                retry_map[did] = retry_map.get(did, 0) + 1
                needs_rollback = True
            else:
                # 超限降级：删文件（避免 summary 计数对不上）
                fp = Path(session_dir) / "defects" / d.get("file", "")
                if fp.exists():
                    fp.unlink()
        if needs_rollback:
            state.rollback("REPORTING")
            return "rollback"
    return "advance"


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
    from_phase = state.phase
    target = state._data.get("target", "")
    # DEFECT_REVIEW → STATE_SAVE: verify_defects 三态路由（设计 §3.2）
    # ponytail: 特殊分支而非塞进通用 _run_transition_gates — verify_defects 是
    # 三态路由器（advance/delete/rollback），不是二态 blocker
    if (from_phase, args.phase) == ("DEFECT_REVIEW", "STATE_SAVE"):
        action = _handle_defect_review(state, args.session_dir, target)
        if action == "rollback":
            print(f"DEFECT_REVIEW: NEEDS_IMPROVEMENT — rolled back to REPORTING "
                  f"(reporter 重写标记项，见 defect_review.json; retry 计数见 defect_retry)")
            return  # 不执行 advance STATE_SAVE
        # action == "advance": 假阳性已删 / retry 耗尽的已降级 / 全 CONFIRMED
    # 跑 transition 前置 gate（设计 §3.4，挂 CLI 层 — mine.md Bash 唯一入口，LLM 无法绕过）
    try:
        _run_transition_gates(from_phase, args.phase, args.session_dir, target)
    except InvalidTransition:
        # strict 模式 gate 拒绝：打印清晰错误，exit 3 区分其他错误
        print(f"GATE BLOCKED: transition {from_phase} → {args.phase} rejected by gate "
              f"(TESTVDB_ENFORCE_STRICT=1). 修好 gate 指出的问题后重试。", file=sys.stderr)
        sys.exit(3)
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
    if len(sys.argv) > 1 and sys.argv[1] in ("--self-check", "-s"):
        _self_check()
        return
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


def _self_check() -> None:
    """ponytail: rollback 正确清理 phases_completed + 不碰 defect_retry + 非法回退抛错。"""
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        sd = Path(td)
        state = PipelineState.create(
            target="t", version="v1", max_rounds=1, min_defects=0, session_dir=str(sd))
        # 推到 DEFECT_REVIEW
        for p in ["ATTACK_GEN", "DEBATE_S1", "EXECUTION", "DEBATE_S2",
                  "VERIFY_LIVE", "REPORTING", "DEFECT_REVIEW"]:
            state.advance(p)
        assert "REPORTING" in state._data["phases_completed"]
        assert "VERIFY_LIVE" in state._data["phases_completed"]

        # rollback REPORTING
        state._data.setdefault("defect_retry", {})["d1"] = 1
        state.rollback("REPORTING")
        assert state.phase == "REPORTING", f"phase={state.phase}"
        assert "REPORTING" not in state._data["phases_completed"], "REPORTING 应被移除"
        # phases_completed 里 REPORTING 之后的不存在（DEFECT_REVIEW 未 append，但逻辑等价）
        assert "VERIFY_LIVE" in state._data["phases_completed"], "VERIFY_LIVE（REPORTING 之前）应保留"
        assert state._data["defect_retry"] == {"d1": 1}, "rollback 不碰 retry 计数"

        # 非法回退 → InvalidTransition
        sd2 = str(sd) + "2"
        state2 = PipelineState.create(
            target="t", version="v1", max_rounds=1, min_defects=0, session_dir=sd2)
        state2.advance("ATTACK_GEN")
        try:
            state2.rollback("ROUND_START")  # ATTACK_GEN 不在 _ROLLBACK_ALLOWED
            assert False, "非法回退应抛 InvalidTransition"
        except InvalidTransition:
            pass
    print("self-check OK")


if __name__ == "__main__":
    main()
