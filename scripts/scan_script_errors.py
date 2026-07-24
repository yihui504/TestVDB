#!/usr/bin/env python3
"""Post-execution script error scanner — scans output logs for Python/script errors.

Detects SCRIPT_ERROR patterns in executor output logs, extracting the last 10 lines
as error context for the reject-and-revise pipeline (Step 8d.5).

Usage:
  python scripts/scan_script_errors.py <session_dir>
"""
import glob
import json
import os
import sys

from _pipeline_utils import setup_encoding

setup_encoding()

SCRIPT_ERROR_MARKERS = [
    "'str' object has no attribute 'get'",
    "TypeError",
    "AttributeError",
    "SCRIPT_ERROR",
    "KeyError:",
    "json.decoder.JSONDecodeError",
]


def scan_execution_logs(session_dir: str) -> dict:
    """Scan all output_*.log files in session_dir for script errors.

    Returns dict with 'errored_count' and 'scripts' fields.
    """
    errored = []
    for log_path in sorted(glob.glob(os.path.join(session_dir, "output_*.log"))):
        try:
            with open(log_path, encoding="utf-8", errors="replace") as f:
                content = f.read()
        except OSError:
            continue

        is_se = any(x.lower() in content.lower() for x in SCRIPT_ERROR_MARKERS)
        if is_se:
            base = (
                os.path.basename(log_path)
                .replace("output_", "")
                .replace(".log", "")
            )
            # Extract the last 10 non-empty lines as error context
            raw_lines = content.split("\n")
            # Take last 15 raw lines first (to preserve tail), then filter blanks, then take last 10
            lines = [l.strip() for l in raw_lines[-15:] if l.strip()]
            error_context = "\n".join(lines[-10:])
            errored.append(
                {
                    "script_base": base,
                    "log": os.path.basename(log_path),
                    "error": error_context[:500],
                }
            )

    return {"errored_count": len(errored), "scripts": errored}


def _self_check() -> int:
    """Self-check: base 提取 + 最后 10 非空行 + 500 字符截断 + case-insensitive + pattern 守卫."""
    import tempfile
    from pathlib import Path

    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    with tempfile.TemporaryDirectory() as td:
        # errored log: 8 line + SCRIPT_ERROR + 11 tail
        log1 = Path(td) / "output_my_script.log"
        lines = [f"line {i}" for i in range(8)] + ["SCRIPT_ERROR"] + [f"tail {i}" for i in range(11)]
        log1.write_text("\n".join(lines), encoding="utf-8")

        # clean log
        (Path(td) / "output_clean.log").write_text("all good\nno problems\n", encoding="utf-8")

        # case-insensitive: lowercase typeerror
        (Path(td) / "output_lower.log").write_text("got typeerror from server\n", encoding="utf-8")

        # 长 error context（> 500 字符）
        (Path(td) / "output_long.log").write_text("SCRIPT_ERROR\n" + "x" * 600 + "\n", encoding="utf-8")

        result = scan_execution_logs(td)
        scripts = {s["script_base"]: s for s in result["scripts"]}

        # base 提取（output_<base>.log → <base>）
        expect("my_script" in scripts, f"base 'my_script' extracted: {list(scripts)}")
        expect("clean" not in scripts, f"clean should not be flagged: {list(scripts)}")
        expect("lower" in scripts, f"case-insensitive TypeError: {list(scripts)}")
        expect("long" in scripts, f"long flagged: {list(scripts)}")

        # error_context 最后 10 非空行（guard：expect 失败时不 crash）
        if "my_script" in scripts:
            ctx = scripts["my_script"]["error"]
            expect("tail 10" in ctx, f"last line should be 'tail 10': ...{ctx[-40:]}")
            expect("line 0" not in ctx, f"early line should be cut: {ctx[:40]}")

        # 500 字符截断
        if "long" in scripts:
            long_ctx = scripts["long"]["error"]
            expect(len(long_ctx) <= 500, f"long ctx truncated to 500: got {len(long_ctx)}")

        # SCRIPT_ERROR_MARKERS 当前成员守卫
        expected = [
            "'str' object has no attribute 'get'",
            "TypeError", "AttributeError", "SCRIPT_ERROR",
            "KeyError:", "json.decoder.JSONDecodeError",
        ]
        expect(SCRIPT_ERROR_MARKERS == expected, f"MARKERS drift: {SCRIPT_ERROR_MARKERS}")

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
        print("Usage: python scripts/scan_script_errors.py <session_dir>", file=sys.stderr)
        sys.exit(1)

    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        sys.exit(2)

    result = scan_execution_logs(session_dir)
    print(json.dumps(result, indent=2, ensure_ascii=False))
    sys.exit(0)


if __name__ == "__main__":
    main()
