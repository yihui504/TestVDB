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


def main():
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
