#!/usr/bin/env python3
"""Cross-platform hook runner for TestVDB plugin.

Resolves the correct Python interpreter on Windows (py -3) vs Linux/Mac (python3),
then executes the target hook script passed as the first argument.
"""
import subprocess
import sys
import os


def find_python():
    """Find the best available Python 3 interpreter."""
    # If current Python is >= 3.9, use it directly
    if sys.version_info >= (3, 9):
        return [sys.executable]

    # On Windows, try py -3 (Python Launcher)
    if sys.platform == "win32":
        try:
            result = subprocess.run(
                ["py", "-3", "--version"],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                return ["py", "-3"]
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass

    # Try python3
    try:
        result = subprocess.run(
            ["python3", "--version"],
            capture_output=True, text=True, timeout=5
        )
        if result.returncode == 0:
            return ["python3"]
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass

    # Fallback to current interpreter
    return [sys.executable]


def main():
    if len(sys.argv) < 2:
        print("[TestVDB hook_runner] ERROR: No target script specified", file=sys.stderr)
        sys.exit(1)

    target_script = sys.argv[1]
    python_cmd = find_python()

    cmd = python_cmd + [target_script] + sys.argv[2:]
    try:
        result = subprocess.run(cmd, timeout=30)
        sys.exit(result.returncode)
    except subprocess.TimeoutExpired:
        print(f"[TestVDB hook_runner] TIMEOUT: {target_script}", file=sys.stderr)
        sys.exit(2)
    except FileNotFoundError:
        print(f"[TestVDB hook_runner] ERROR: Script not found: {target_script}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
