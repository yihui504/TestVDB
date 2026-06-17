#!/usr/bin/env python3
"""
TestVDB pipeline_gate.py — Stop hook gate.

Prevents the orchestrator from silently ending the turn before the pipeline
reaches phase=DONE, and enforces two quality contracts once it does:

  - Symptom ③ step completeness: phase != DONE and rounds remain  → exit 2
  - Symptom ① doc coverage    : attack agents' "## Analyzed Documents" union
                                 must cover raw_knowledge.md Document Sources
  - Symptom ② fallback reason  : every FALLBACK_TRIGGERED in output_*.log must be
                                 paired with a [FALLBACK_JUSTIFIED: <reason>]

Wired onto the Claude Code Stop hook. On every turn end:
  - No active pipeline_state.json nearby  → exit 0 (normal chat, don't interfere)
  - Active pipeline, rounds exhausted      → exit 0 (anti-loop release, skip ① ②)
  - Active pipeline, phase != "DONE"       → exit 2 + stderr (force Claude to continue)
  - Active pipeline, phase == "DONE"       → run ① ② quality gates, exit 2 on failure

The ① ② checks never block on *missing* data sources (no analyzed_documents yet,
no fallback markers): they only block when evidence of *incomplete* work exists.
This keeps the gate from misfiring on legacy sessions or partially-run rounds.

Usage (via Stop hook): python scripts/hooks/pipeline_gate.py

Audit log: every run appends one line to scripts/hooks/_gate_audit.log
(exit code, phase, round, session, reason) — hard evidence that the Stop hook
actually dispatched the gate, used for MVP end-to-end verification.
"""
from __future__ import annotations

import json
import logging
import os
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

logging.basicConfig(level=logging.INFO, format="[gate] %(levelname)s %(message)s")
log = logging.getLogger("pipeline_gate")

FALLBACK_TRIGGER_RE = re.compile(r"FALLBACK_TRIGGERED:\s*(?P<what>.+)")
FALLBACK_JUSTIFIED_RE = re.compile(r"\[FALLBACK_JUSTIFIED:\s*(?P<reason>.+?)\]")
_URL_RE = re.compile(r"https?://[^\s\)|\]]+")


@dataclass(frozen=True)
class GateConfig:
    """Immutable gate configuration — no hidden state, safe to reuse."""

    project_root: Path
    results_dir: Path
    active_threshold_seconds: int  # state file modified within N seconds → active
    terminal_phase: str = "DONE"
    doc_coverage_threshold: float = 0.6  # min fraction of Document Sources analyzed


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _drain_stdin() -> str:
    """Drain hook input from stdin. Disk state is authoritative; stdin is advisory only."""
    try:
        return sys.stdin.read()
    except Exception:
        return ""


def _load_json(path: Path) -> dict[str, Any] | None:
    try:
        with path.open(encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else None
    except (FileNotFoundError, json.JSONDecodeError, PermissionError, OSError):
        return None


def _read_text(path: Path) -> str:
    try:
        with path.open(encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def find_active_pipeline_state(cfg: GateConfig) -> Path | None:
    """Most-recently-modified pipeline_state.json under results/ within the active window."""
    if not cfg.results_dir.is_dir():
        return None
    candidates = list(cfg.results_dir.rglob("pipeline_state.json"))
    if not candidates:
        return None
    now = _now()
    recent: list[tuple[float, Path]] = []
    for path in candidates:
        try:
            mtime = datetime.fromtimestamp(path.stat().st_mtime, tz=timezone.utc)
        except OSError:
            continue
        age = (now - mtime).total_seconds()
        if age <= cfg.active_threshold_seconds:
            recent.append((age, path))
    if not recent:
        return None
    recent.sort(key=lambda item: item[0])
    return recent[0][1]


def _resolve_session_dir(state: dict[str, Any], cfg: GateConfig) -> Path | None:
    """Version-level session dir (holds raw_knowledge.md). session_dir may be relative."""
    raw = str(state.get("session_dir", "")).strip()
    if not raw:
        return None
    session_dir = Path(raw)
    if not session_dir.is_absolute():
        session_dir = cfg.project_root / session_dir
    return session_dir if session_dir.is_dir() else None


def _resolve_round_dir(state: dict[str, Any], cfg: GateConfig) -> Path | None:
    """Per-round output dir (timestamp subdir holding scripts/logs). Falls back to session dir.

    timestamp_dir can be:
    - project-root-relative (e.g. results/target/ver/2026-01-01T00-00-00Z)
    - session_dir-relative (e.g. 20260612T100000) — legacy / test convention

    We try project_root-relative first, then session_dir-relative, then fall back.
    """
    session_dir = _resolve_session_dir(state, cfg)
    if session_dir is None:
        return None
    ts = str(state.get("timestamp_dir", "")).strip()
    if not ts:
        return session_dir
    # Try project-root-relative first (pipeline v3 convention)
    round_dir = cfg.project_root / ts
    if round_dir.is_dir():
        return round_dir
    # Legacy / test convention: timestamp_dir is relative to session_dir
    round_dir = session_dir / ts
    return round_dir if round_dir.is_dir() else session_dir


def check_step_completeness(
    state: dict[str, Any], cfg: GateConfig
) -> tuple[bool, str]:
    """Symptom ③ — has the pipeline legitimately finished? Returns (allow_stop, reason)."""
    phase = str(state.get("phase", "")).upper()
    current = state.get("current_round")
    max_rounds = state.get("max_rounds")
    session_id = state.get("session_id", "?")
    target = state.get("target", "?")
    version = state.get("version_target", state.get("version", "?"))
    rounds_note = (
        f"round {current}/{max_rounds}"
        if current is not None and max_rounds is not None
        else "round ?"
    )

    if phase == cfg.terminal_phase:
        return True, f"pipeline finished (phase=DONE, session={session_id})"

    # Anti-loop guard: rounds EXCEEDED but phase never reached DONE — release
    # the brake. Strict ">" (not ">=") so a budgeted round like 1/1 that simply
    # hasn't finished yet is still forced to continue (symptom ③). Only release
    # when current has overshot max (e.g. 2/1), proving the loop is stuck.
    # max_rounds=0 means UNLIMITED (per mine.md --max-rounds 0) — never exhausted.
    if (
        current is not None
        and max_rounds is not None
        and max_rounds > 0  # 0 = unlimited, anti-loop guard must not fire
        and current > max_rounds
    ):
        log.warning(
            "rounds exhausted (%s/%s) but phase=%s — allowing stop to avoid loop",
            current,
            max_rounds,
            phase,
        )
        return True, f"anti-loop release: rounds exhausted {rounds_note}, phase={phase}"

    return False, (
        f"TestVDB pipeline NOT finished — session={session_id} "
        f"target={target} {version} phase={phase} {rounds_note}. "
        f"You are trying to end the turn, but the pipeline has not reached phase=DONE. "
        f"Continue executing the remaining orchestrator steps "
        f"(see agents/orchestrator.md '强制执行步骤 Checklist')."
    )


def _parse_document_sources(raw_knowledge_path: Path) -> set[str]:
    """Extract the URL column from raw_knowledge.md '## Document Sources' table."""
    if not raw_knowledge_path.is_file():
        return set()
    urls: set[str] = set()
    in_sources = False
    for line in _read_text(raw_knowledge_path).splitlines():
        stripped = line.strip()
        if stripped.startswith("## "):
            in_sources = stripped.startswith("## Document Sources")
            continue
        if not in_sources or not stripped.startswith("|"):
            continue
        cells = [c.strip() for c in stripped.strip("|").split("|")]
        if len(cells) < 2:
            continue
        # skip separator rows (|---|---|…) and anything whose first cell is only -/:/space
        if set(cells[0]) <= set("-: "):
            continue
        for url in _URL_RE.findall(cells[1]):
            urls.add(url.rstrip(".,;)\"'"))
    return urls


def _parse_analyzed_docs(round_dir: Path) -> set[str]:
    """Union of URLs declared in every analyzed_documents*.md under the round dir.

    Uses recursive glob because agents may write analyzed_documents*.md directly
    in the round dir or in subdirectories like debate_logs/.
    """
    urls: set[str] = set()
    for md in round_dir.rglob("analyzed_documents*.md"):
        for url in _URL_RE.findall(_read_text(md)):
            urls.add(url.rstrip(".,;)\"'"))
    return urls


def check_doc_coverage(
    state: dict[str, Any], cfg: GateConfig
) -> tuple[bool, str]:
    """Symptom ① — partial document analysis by attack agents."""
    session_dir = _resolve_session_dir(state, cfg)
    round_dir = _resolve_round_dir(state, cfg)
    if session_dir is None or round_dir is None:
        return True, "skipped (no session dir)"

    all_docs = _parse_document_sources(session_dir / "raw_knowledge.md")
    if not all_docs:
        log.info("doc-coverage: raw_knowledge.md has no Document Sources, skip")
        return True, "skipped (no Document Sources)"

    analyzed = _parse_analyzed_docs(round_dir)
    if not analyzed:
        # 区分"agent 还没跑"(放行) vs "agent 跑了但没写"(拦截空声明绕过)。
        # 判据: phase==DONE 且 ATTACK_GEN 已完成 → attack agent 跑过，analyzed 不应为空。
        attack_ran = (
            str(state.get("phase", "")).upper() == "DONE"
            and "ATTACK_GEN" in state.get("phases_completed", [])
        )
        if attack_ran:
            return False, (
                "Symptom ① — ATTACK_GEN completed but NO analyzed_documents written "
                "(空声明绕过). Attack agents must each emit analyzed_documents_*.md "
                "listing raw_knowledge.md Document Source URLs (see agents/attack-*.md)."
            )
        # agents 尚未跑或 round 早于合约 → 放行，避免误伤 legacy
        log.warning(
            "doc-coverage: no analyzed_documents*.md in %s — cannot verify", round_dir
        )
        return True, "skipped (no analyzed_documents yet — ATTACK_GEN not completed)"

    covered = all_docs & analyzed
    coverage = len(covered) / len(all_docs)
    if coverage >= cfg.doc_coverage_threshold:
        return True, f"doc coverage {coverage:.0%} (>= {cfg.doc_coverage_threshold:.0%})"

    missing = sorted(all_docs - analyzed)
    return False, (
        f"Symptom ① — doc coverage {coverage:.0%} below {cfg.doc_coverage_threshold:.0%} "
        f"threshold ({len(covered)}/{len(all_docs)}). "
        f"Attack agents did not analyze: {', '.join(missing[:8])}"
        f"{' …' if len(missing) > 8 else ''}. "
        f"Each attack agent must list its analyzed docs in ## Analyzed Documents "
        f"(see agents/attack-*.md contract)."
    )


def check_fallback_justification(
    state: dict[str, Any], cfg: GateConfig
) -> tuple[bool, str]:
    """Symptom ② — every FALLBACK_TRIGGERED needs a [FALLBACK_JUSTIFIED: reason]."""
    round_dir = _resolve_round_dir(state, cfg)
    if round_dir is None:
        return True, "skipped (no round dir)"

    total_triggers = 0
    total_justified = 0
    unjustified_files: list[str] = []
    for log_file in round_dir.glob("output_*.log"):
        text = _read_text(log_file)
        triggers = len(FALLBACK_TRIGGER_RE.findall(text))
        if triggers == 0:
            continue
        justified = len(FALLBACK_JUSTIFIED_RE.findall(text))
        total_triggers += triggers
        total_justified += justified
        if justified < triggers:
            unjustified_files.append(log_file.name)

    if total_triggers == 0:
        return True, "no fallback triggered"
    if total_justified >= total_triggers:
        return True, f"{total_triggers} fallback(s) all justified"

    return False, (
        f"Symptom ② — unjustified fallback: {total_triggers} FALLBACK_TRIGGERED but only "
        f"{total_justified} [FALLBACK_JUSTIFIED: …]. "
        f"Unjustified in: {', '.join(unjustified_files[:5])}. "
        f"Every degradation must print both markers (see agents/attack-*.md contract)."
    )


def _audit(cfg: GateConfig, rc: int, state: dict[str, Any] | None, reason: str) -> None:
    """Append one line of hard evidence per gate run (MVP verification trail)."""
    audit_path = cfg.project_root / "scripts" / "hooks" / "_gate_audit.log"
    ts = _now().isoformat(timespec="seconds")
    if state:
        phase = state.get("phase", "-")
        rnd = f"{state.get('current_round')}/{state.get('max_rounds')}"
        sid = state.get("session_id", "-")
    else:
        phase, rnd, sid = "-", "-/-", "-"
    short = reason.replace("\n", " ")[:160]
    try:
        with audit_path.open("a", encoding="utf-8") as fh:
            fh.write(f"[{ts}] exit={rc} phase={phase} round={rnd} session={sid} | {short}\n")
    except OSError:
        pass


def _evaluate(cfg: GateConfig) -> tuple[int, str, dict[str, Any] | None]:
    """Pure decision logic. Returns (exit_code, reason, state_or_None)."""
    active = find_active_pipeline_state(cfg)
    if active is None:
        # Normal chat — no active TestVDB pipeline nearby. Do not interfere.
        return 0, "no active pipeline", None

    state = _load_json(active)
    if state is None:
        log.warning("found %s but could not parse JSON; allowing stop", active)
        return 0, f"unparseable state: {active.name}", None

    log.info(
        "active pipeline: %s | phase=%s round=%s",
        active,
        state.get("phase"),
        state.get("current_round"),
    )

    allow, reason = check_step_completeness(state, cfg)
    if not allow:
        return 2, reason, state  # block stop, force Claude to continue

    if reason.startswith("anti-loop"):
        # Rounds exceeded: never追加 quality gates here — that would re-introduce
        # the very loop the anti-loop guard is meant to break.
        return 0, reason, state

    # phase == DONE: final quality enforcement for symptoms ① and ②.
    for checker in (check_doc_coverage, check_fallback_justification):
        ok, msg = checker(state, cfg)
        log.info("%s: %s", checker.__name__, msg)
        if not ok:
            return 2, msg, state

    return 0, "all gates passed", state


def _plugin_root() -> Path:
    """Authoritative plugin root — does NOT depend on cwd.

    cwd drifts when claude is launched from a parent dir (e.g. ``mftui/``
    instead of ``TestVDB/``); deriving project_root from cwd made the gate scan
    the wrong ``results/`` tree and silently never fire. The script location is
    stable: pipeline_gate.py lives at ``<plugin_root>/scripts/hooks/``.

    Priority: ``TESTVDB_PLUGIN_ROOT`` env var > script location.
    """
    env_root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if env_root and os.path.isdir(env_root):
        return Path(env_root)
    return Path(__file__).resolve().parents[2]


def main() -> int:
    _drain_stdin()

    project_root = _plugin_root()
    # CLAUDE_PROJECT_DIR is honoured ONLY if it genuinely points at a TestVDB
    # plugin root (contains commands/mine.md). A bare cwd-derived value that
    # happens to be a parent dir must not override the stable script-location
    # root — that override was the root cause of "gate never fires".
    cpd = os.environ.get("CLAUDE_PROJECT_DIR", "")
    if cpd and (Path(cpd) / "commands" / "mine.md").is_file():
        project_root = Path(cpd)
    cfg = GateConfig(
        project_root=project_root,
        results_dir=project_root / "results",
        active_threshold_seconds=int(
            # 24h: covers multi-round pipelines (each round 10-30min × N rounds).
            # Legacy state > 24h treated as stale (won't force-continue a dead session).
            os.environ.get("TESTVDB_GATE_ACTIVE_THRESHOLD", "86400")
        ),
        doc_coverage_threshold=float(
            os.environ.get("TESTVDB_DOC_COVERAGE_THRESHOLD", "0.6")
        ),
    )

    rc, reason, state = _evaluate(cfg)
    if rc == 2:
        print(reason, file=sys.stderr)
    _audit(cfg, rc, state, reason)
    return rc


if __name__ == "__main__":
    sys.exit(main())
