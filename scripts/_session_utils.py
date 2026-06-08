#!/usr/bin/env python3
"""TestVDB Session Utilities — shared across hook and maintenance scripts.

Provides find_session_id() and is_session_locked() used by:
  - precompact_save.py
  - postcompact_verify.py
  - emergency_cleanup.py
  - cleanup_stop.py
  - log_execution.py
  - notify_check.py
  - retry_policy.py
"""

import json
import os


def _plugin_root():
    """Determine plugin root directory.

    Priority: TESTVDB_PLUGIN_ROOT env var > script location inference.
    The env var approach is preferred because it doesn't depend on file location.
    """
    root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if root and os.path.isdir(root):
        return root
    # Fallback: infer from script location (assumes scripts/_session_utils.py
    # is exactly 2 levels below plugin root)
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
                        val = line.split("=", 1)[1].strip()
                        # Strip surrounding quotes (single or double)
                        if len(val) >= 2 and val[0] == val[-1] and val[0] in ('"', "'"):
                            val = val[1:-1]
                        return val
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


def is_session_locked(session_dir):
    """Check if a session has an active .session.lock file."""
    lock_path = os.path.join(session_dir, ".session.lock")
    if not os.path.exists(lock_path):
        return False
    try:
        with open(lock_path, encoding="utf-8") as f:
            lock_data = json.load(f)
        return lock_data.get("status") == "active"
    except (json.JSONDecodeError, OSError):
        return False


def find_sessions_dir(base_dir=None):
    """Find the results/ directory for TestVDB sessions."""
    if base_dir is None:
        base_dir = _plugin_root()
    results_dir = os.path.join(base_dir, "results")
    if os.path.isdir(results_dir):
        return results_dir
    return None
