#!/usr/bin/env python3
"""
Checks — L1 VerificationPipeline check protocol (ADR-0006).

Defines the Check protocol and CheckContext so all 11 mechanical checks
in verify_live_l1.py share a uniform interface.

Usage:
  from checks import Check, CheckContext, Verdict
  class MyCheck:
      def check(self, candidate, log_path, ctx): ...
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal, Optional, Protocol

from _pipeline_utils import find_log as _find


@dataclass
class Verdict:
    """Result of a single check.  None return means 'not applicable / skipped'."""

    result: Literal["REFUTED", "UNCERTAIN"]
    reason: str
    check_name: str


@dataclass
class CheckContext:
    """Optional dependencies for checks.  Checks ignore fields they don't need."""

    contract: dict | None = None   # structured_contract.json
    db_url: str | None = None      # reserved for future use
    target: str = ""               # target DB identifier


class Check(Protocol):
    """Single-method interface for L1 mechanical checks (ADR-0006).

    Returns:
      - Verdict:        REFUTED or UNCERTAIN
      - None:           check is not applicable (e.g. missing optional dependency)

    All 11 checks in verify_live_l1.py should implement this protocol.
    """

    def check(
        self,
        candidate: dict,
        log_path: str,
        ctx: CheckContext,
    ) -> Optional[Verdict]:
        ...


def run_checks(
    checks: list[Check],
    candidate: dict,
    log_path: str,
    ctx: CheckContext,
) -> list[Verdict]:
    """Run a list of checks against a single candidate + log.

    Returns only non-None verdicts.  Short-circuits on first REFUTED? No —
    we collect ALL verdicts so the caller can report every reason a candidate
    was refuted, not just the first.
    """
    results: list[Verdict] = []
    for check in checks:
        verdict = check.check(candidate, log_path, ctx)
        if verdict is not None:
            results.append(verdict)
    return results


def refuted_candidates(
    checks: list[Check],
    candidates: list[dict],
    session_dir: str,
    ctx: CheckContext,
) -> dict[str, list[Verdict]]:
    """Run checks against all candidates.  Returns {candidate_key: [verdicts]}

    Only includes candidates that had at least one REFUTED verdict.
    UNCERTAIN-only candidates are not included (they go to L2).
    """
    refuted: dict[str, list[Verdict]] = {}

    for candidate in candidates:
        script = candidate.get("script", "") or candidate.get("candidate", "")
        if not script:
            continue

        log_path = _find(session_dir, script)
        if not log_path:
            continue

        verdicts = run_checks(checks, candidate, str(log_path), ctx)
        refuted_verdicts = [v for v in verdicts if v.result == "REFUTED"]
        if refuted_verdicts:
            refuted[script] = refuted_verdicts

    return refuted
