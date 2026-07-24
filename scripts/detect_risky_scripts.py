#!/usr/bin/env python3
"""Stage 1 script error heuristic detector — scans attack scripts for risky patterns.

Detects Python error patterns (TypeError, AttributeError, JSONDecodeError, etc.)
that appear in script source code without corresponding error handling (safe_request
or try/except blocks). Scripts with these patterns are marked as RISKY_SCRIPT for
priority review in execution logs.

Usage:
  python scripts/detect_risky_scripts.py <session_dir>
"""
from __future__ import annotations
import glob
import json
import os
import sys

from _pipeline_utils import setup_encoding

setup_encoding()

ERROR_PATTERNS = [
    "'str' object has no attribute 'get'",
    "TypeError",
    "AttributeError",
    "json.decoder.JSONDecodeError",
    "KeyError:",
    "IndexError:",
]


def detect_risky_scripts(session_dir: str) -> list[dict]:
    """Scan all .py files in session_dir for risky patterns without error handling.

    Returns list of dicts with 'file' and 'risk' fields.
    """
    risky = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f.replace("\\", "/"):
            continue
        try:
            with open(f, encoding="utf-8", errors="replace") as fh:
                content = fh.read()
        except OSError:
            continue

        for pat in ERROR_PATTERNS:
            if pat.lower() in content.lower():
                # Check if the script has corresponding robust handling
                if "safe_request" not in content and "try:" not in content:
                    rel = os.path.relpath(f, session_dir)
                    risky.append(
                        {
                            "file": rel,
                            "risk": f"contains '{pat}' without error handling",
                        }
                    )
                    break  # One risk flag per script

    return risky


def _self_check() -> int:
    """Self-check: safe_request/try: 排除 + mre/ 跳过 + break 单 script 单 flag + pattern 成员守卫."""
    import tempfile
    from pathlib import Path

    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    with tempfile.TemporaryDirectory() as td:
        # risky: TypeError 无 safe_request/try
        (Path(td) / "risky1.py").write_text("raise TypeError('x')\n", encoding="utf-8")
        # robust: TypeError + try
        (Path(td) / "robust_try.py").write_text(
            "try:\n    raise TypeError()\nexcept Exception:\n    pass\n", encoding="utf-8")
        # robust: TypeError + safe_request
        (Path(td) / "robust_safe.py").write_text(
            "def safe_request():\n    pass\nsafe_request()\n# TypeError mentioned\n", encoding="utf-8")
        # clean: 无 risky pattern
        (Path(td) / "clean.py").write_text("print('hello')\n", encoding="utf-8")
        # mre/ 应跳过（Windows 路径 \mre\ 也要正确跳过）
        mre_dir = Path(td) / "sub" / "mre"
        mre_dir.mkdir(parents=True)
        (mre_dir / "should_skip.py").write_text("raise TypeError('x')\n", encoding="utf-8")

        findings = detect_risky_scripts(td)
        rel_files = {f["file"].replace("\\", "/") for f in findings}

        expect("risky1.py" in rel_files, f"risky1 should be flagged: {rel_files}")
        expect("robust_try.py" not in rel_files, f"robust_try should be skipped (try:): {rel_files}")
        expect("robust_safe.py" not in rel_files, f"robust_safe should be skipped (safe_request): {rel_files}")
        expect("clean.py" not in rel_files, f"clean should not be flagged: {rel_files}")
        mre_flagged = any("/mre/" in f for f in rel_files)
        expect(not mre_flagged, f"mre/ should be skipped (Windows-safe): {rel_files}")

        # break 单 script 单 flag
        risky1_findings = [f for f in findings if "risky1" in f["file"]]
        expect(len(risky1_findings) == 1, f"risky1 single flag (break): got {len(risky1_findings)}")

        # case-insensitive
        (Path(td) / "lower.py").write_text("got typeerror from server\n", encoding="utf-8")
        findings2 = detect_risky_scripts(td)
        expect(any("lower.py" in f["file"] for f in findings2), "case-insensitive TypeError detection")

        # ERROR_PATTERNS 当前成员守卫（防意外 drift）
        expected = [
            "'str' object has no attribute 'get'",
            "TypeError", "AttributeError",
            "json.decoder.JSONDecodeError",
            "KeyError:", "IndexError:",
        ]
        expect(ERROR_PATTERNS == expected, f"ERROR_PATTERNS drift: {ERROR_PATTERNS}")

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("self-check OK")
    return 0


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "--self-check":
        sys.exit(_self_check())
    if len(sys.argv) < 2:
        print("Usage: python scripts/detect_risky_scripts.py <session_dir>", file=sys.stderr)
        sys.exit(1)

    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        sys.exit(2)

    findings = detect_risky_scripts(session_dir)

    if findings:
        print(f"[Stage 1] Script Error Heuristic: {len(findings)} RISKY_SCRIPT(s) detected")
        for f in findings:
            print(f"  RISKY_SCRIPT: {f['file']} — {f['risk']}")
        print(json.dumps({"risky_scripts": findings}, indent=2, ensure_ascii=False))
    else:
        print("[Stage 1] Script Error Heuristic: all scripts pass")

    sys.exit(0)


if __name__ == "__main__":
    main()
