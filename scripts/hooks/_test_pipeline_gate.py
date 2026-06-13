#!/usr/bin/env python3
"""
TestVDB Step 4 — pipeline_gate.py logic tests (symptoms ① ② ③ + anti-loop).

Isolated per-case temp trees; runs the REAL gate via subprocess and asserts
exit codes. Covers the hooks-gate plan acceptance criteria:

  1. normal chat / no pipeline            → exit 0  (don't interfere)
  2. ③ phase != DONE, rounds remain       → exit 2
  3. anti-loop: rounds exhausted          → exit 0  (skips ① ②)
  4. ① DONE + no analyzed docs yet        → exit 0  (no false positive)
  5. ① DONE + doc coverage < threshold    → exit 2
  6. DONE + good coverage, no fallback    → exit 0
  7. ② DONE + unjustified fallback        → exit 2
  8. DONE + justified fallback            → exit 0
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

GATE = Path(__file__).resolve().parent / "pipeline_gate.py"

RAW_KNOWLEDGE = """# testdb v1.0 API Knowledge

## Document Sources
| # | URL | Doc Version | Fetched At | Version Match |
|---|-----|-------------|------------|---------------|
| 1 | https://example.test/docs/a | 1.0 | 2026-01-01 | matched |
| 2 | https://example.test/docs/b | 1.0 | 2026-01-01 | matched |
| 3 | https://example.test/docs/c | 1.0 | 2026-01-01 | matched |
| 4 | https://example.test/docs/d | 1.0 | 2026-01-01 | matched |
| 5 | https://example.test/docs/e | 1.0 | 2026-01-01 | matched |
"""

PASSED: list[str] = []
FAILED: list[str] = []


def _run_gate(root: Path) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, str(GATE)],
        cwd=str(root),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=30,
    )
    return proc.returncode, (proc.stdout + proc.stderr)


def _scaffold(phase: str, current: int, maxr: int) -> tuple[Path, Path]:
    root = Path(tempfile.mkdtemp(prefix="gate_"))
    ver_dir = root / "results" / "testdb" / "1.0"
    round_dir = ver_dir / "20260612T100000"
    round_dir.mkdir(parents=True)
    (ver_dir / "raw_knowledge.md").write_text(RAW_KNOWLEDGE, encoding="utf-8")
    state = {
        "version": 3,
        "session_id": "test-001",
        "target": "testdb",
        "version_target": "1.0",
        "current_round": current,
        "max_rounds": maxr,
        "phase": phase,
        "session_dir": "results/testdb/1.0",
        "timestamp_dir": "20260612T100000",
    }
    (ver_dir / "pipeline_state.json").write_text(json.dumps(state), encoding="utf-8")
    return root, round_dir


def _good_analyzed(rd: Path) -> None:
    (rd / "analyzed_documents_boundary.md").write_text(
        "## Analyzed Documents — boundary\n"
        "- https://example.test/docs/a\n"
        "- https://example.test/docs/b\n"
        "- https://example.test/docs/c\n"
        "- https://example.test/docs/d\n",
        encoding="utf-8",
    )


def _check(name: str, got: int, want: int, out: str) -> None:
    ok = got == want
    (PASSED if ok else FAILED).append(name)
    print(f"[{'PASS' if ok else 'FAIL'}] {name}: exit={got} (want {want})")
    if not ok:
        print(f"    --- gate output ---\n{out}    --------------------")


def main() -> int:
    # 1. No pipeline → exit 0
    root = Path(tempfile.mkdtemp(prefix="gate_empty_"))
    try:
        (root / "results").mkdir()
        rc, out = _run_gate(root)
        _check("1 no-pipeline → 0", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 2. Symptom ③: phase != DONE, round 1/1 not exhausted → exit 2
    root, _ = _scaffold("ROUND_START", 1, 1)
    try:
        rc, out = _run_gate(root)
        _check("2 ③ not-done → 2", rc, 2, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 3. Anti-loop: rounds exhausted (2 >= 1) → exit 0, skips ① ②
    root, _ = _scaffold("ROUND_START", 2, 1)
    try:
        rc, out = _run_gate(root)
        _check("3 anti-loop → 0", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 4. DONE + no analyzed docs yet → exit 0 (no false positive on missing data)
    root, _ = _scaffold("DONE", 1, 1)
    try:
        rc, out = _run_gate(root)
        _check("4 ① DONE no-analyzed → 0", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 5. Symptom ①: DONE + coverage 2/5 = 40% < 60% → exit 2
    root, rd = _scaffold("DONE", 1, 1)
    try:
        (rd / "analyzed_documents_boundary.md").write_text(
            "## Analyzed Documents — boundary\n"
            "- https://example.test/docs/a\n"
            "- https://example.test/docs/b\n",
            encoding="utf-8",
        )
        rc, out = _run_gate(root)
        _check("5 ① low-coverage → 2", rc, 2, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 6. DONE + good coverage (4/5=80%), no fallback → exit 0
    root, rd = _scaffold("DONE", 1, 1)
    try:
        _good_analyzed(rd)
        rc, out = _run_gate(root)
        _check("6 ①+② clean DONE → 0", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 7. Symptom ②: DONE + good coverage + unjustified fallback → exit 2
    root, rd = _scaffold("DONE", 1, 1)
    try:
        _good_analyzed(rd)
        (rd / "output_state_concurrent.log").write_text(
            "FALLBACK_TRIGGERED: SDK used instead of REST for bulk\n"
            "VERDICT: NO_DEFECT\n",
            encoding="utf-8",
        )
        rc, out = _run_gate(root)
        _check("7 ② unjustified-fallback → 2", rc, 2, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 8. DONE + good coverage + justified fallback → exit 0
    root, rd = _scaffold("DONE", 1, 1)
    try:
        _good_analyzed(rd)
        (rd / "output_state_concurrent.log").write_text(
            "FALLBACK_TRIGGERED: SDK used instead of REST for bulk\n"
            "[FALLBACK_JUSTIFIED: REST has no bulk endpoint per raw_knowledge]\n"
            "VERDICT: NO_DEFECT\n",
            encoding="utf-8",
        )
        rc, out = _run_gate(root)
        _check("8 ② justified-fallback → 0", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    print(f"\n{len(PASSED)}/{len(PASSED) + len(FAILED)} passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
