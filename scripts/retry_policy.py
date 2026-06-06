#!/usr/bin/env python3
"""TestVDB Tool Failure Retry Policy Reporter.

Reports the configured retry policy when a tool invocation fails.
Reads failure context from stdin (JSON) to provide targeted retry advice.
"""
import json
import os
import sys


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def find_session_id():
    """Find TESTVDB_SESSION_ID from multiple sources.

    Priority: environment variable > .env file > settings.json
    """
    # 1. Environment variable
    sid = os.environ.get("TESTVDB_SESSION_ID", "")
    if sid:
        return sid

    # 2. .env file in plugin root
    plugin_root = _plugin_root()
    env_path = os.path.join(plugin_root, ".env")
    if os.path.exists(env_path):
        try:
            with open(env_path, encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line.startswith("TESTVDB_SESSION_ID="):
                        return line.split("=", 1)[1].strip()
        except OSError:
            pass

    # 3. settings.json in plugin root
    settings_path = os.path.join(plugin_root, "settings.json")
    if os.path.exists(settings_path):
        try:
            with open(settings_path, encoding="utf-8") as f:
                settings = json.load(f)
            sid = settings.get("session", {}).get("session_id", "")
            if sid:
                return sid
        except (json.JSONDecodeError, OSError):
            pass

    return ""


def main():
    # Read failure context from stdin (if available)
    failure_context = {}
    if not sys.stdin.isatty():
        try:
            raw = sys.stdin.read()
            if raw.strip():
                failure_context = json.loads(raw)
        except (json.JSONDecodeError, UnicodeDecodeError):
            failure_context = {"raw_input": "parse_error"}

    plugin_root = os.environ.get("CLAUDE_PLUGIN_ROOT", _plugin_root())
    settings_path = os.path.join(plugin_root, "settings.json")

    with open(settings_path, encoding="utf-8") as f:
        settings = json.load(f)

    retry = settings.get("retry", {})
    max_attempts = retry.get("max_attempts", 5)

    tool_name = failure_context.get("tool", "unknown")
    error_type = failure_context.get("error_type", "unknown")

    # Provide retry advice based on error type
    advice = ""
    if "docker" in str(failure_context).lower():
        docker_delay = retry.get("docker_startup_delay_seconds", 10)
        advice = f" Docker retry delay: {docker_delay}s."
    elif "script" in str(failure_context).lower():
        script_delay = retry.get("script_execution_delay_seconds", 3)
        advice = f" Script retry delay: {script_delay}s."
    elif "doc" in str(failure_context).lower() or "fetch" in str(failure_context).lower():
        doc_delay = retry.get("doc_fetch_delay_seconds", 5)
        advice = f" Doc fetch retry delay: {doc_delay}s."

    print(f"[TestVDB] Tool failure: {tool_name} ({error_type}). "
          f"Retry policy: max_attempts={max_attempts}.{advice}")


if __name__ == "__main__":
    main()
