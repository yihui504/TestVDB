#!/usr/bin/env python3
"""TestVDB Post-Write Notification Check.

Reads the notification settings and reports the current webhook configuration
status after every Write tool invocation.
"""
import json
import os


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
    plugin_root = os.environ.get("CLAUDE_PLUGIN_ROOT", _plugin_root())
    settings_path = os.path.join(plugin_root, "settings.json")

    with open(settings_path, encoding="utf-8") as f:
        settings = json.load(f)

    notification = settings.get("notification", {})
    webhook_url = notification.get("webhook_url", "")
    severity = notification.get("on_severity", "critical")
    validate_on_start = notification.get("validate_on_start", True)

    if webhook_url:
        # Validate webhook URL format
        if not webhook_url.startswith(("http://", "https://")):
            print(f"[TestVDB] Write event. Severity={severity}, Webhook=INVALID (must start with http:// or https://)")
        elif webhook_url.endswith("/"):
            print(f"[TestVDB] Write event. Severity={severity}, Webhook=WARNING (trailing slash may cause 404)")
        elif "localhost" in webhook_url or "127.0.0.1" in webhook_url:
            print(f"[TestVDB] Write event. Severity={severity}, Webhook=WARNING (localhost URL - verify accessibility from container)")
        else:
            print(f"[TestVDB] Write event. Severity={severity}, Webhook=configured")
    else:
        print(f"[TestVDB] Write event. Severity={severity}, Webhook=none (notifications disabled)")


if __name__ == "__main__":
    main()
