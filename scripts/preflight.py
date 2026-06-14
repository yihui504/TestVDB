#!/usr/bin/env python3
"""TestVDB Session Start Pre-flight Checks.

Verifies Docker, Python version, disk space, GitHub token, and network
connectivity before starting a mining session.
"""
import subprocess
import sys
import shutil
import os
import re


def check_docker():
    r = subprocess.run(["docker", "ps"], capture_output=True)
    status = "OK" if not r.returncode else "WARNING - not running"
    print(f"[TestVDB] Docker: {status}")

    # Check docker compose availability
    r2 = subprocess.run(["docker", "compose", "version"], capture_output=True)
    compose_status = "OK" if not r2.returncode else "WARNING - docker compose not available"
    print(f"[TestVDB] Docker Compose: {compose_status}")


def _parse_python_version(output):
    """Extract (major, minor, patch) from version string output."""
    m = re.search(r"(\d+)\.(\d+)(?:\.(\d+))?", output)
    if m:
        return (int(m.group(1)), int(m.group(2)), int(m.group(3) or 0))
    return None


def _scan_python_candidates():
    """Scan system PATH for Python >= 3.9 installations."""
    candidates = []
    if sys.platform == "win32":
        commands = ["py -3", "py -3.12", "py -3.11", "py -3.10", "py -3.9",
                     "python3", "python3.12", "python3.11", "python3.10", "python3.9"]
    else:
        commands = ["python3", "python3.12", "python3.11", "python3.10", "python3.9"]
    for cmd in commands:
        try:
            r = subprocess.run(cmd.split() + ["--version"], capture_output=True, text=True, timeout=5)
            if r.returncode == 0:
                ver = _parse_python_version(r.stdout + r.stderr)
                if ver and ver >= (3, 9, 0):
                    candidates.append((cmd, f"{ver[0]}.{ver[1]}.{ver[2]}"))
        except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
            continue
    return candidates


def check_python():
    vi = sys.version_info
    py_ok = vi >= (3, 9)
    msg = f"[TestVDB] Python: {vi.major}.{vi.minor}"
    if not py_ok:
        msg += " (FATAL: <3.9, Orchestrator will terminate)"
    print(msg)

    # Windows: check py launcher versions
    if sys.platform == "win32":
        r2 = subprocess.run(["py", "-0"], capture_output=True, text=True)
        if not r2.returncode:
            best = [
                line.strip()
                for line in r2.stdout.strip().split("\n")
                if line.strip() and line.strip()[0].isdigit()
            ]
            if best:
                print(f"[TestVDB] Windows py launcher versions: {best}")

    # If current Python < 3.9, scan for alternatives
    if not py_ok:
        print("[TestVDB] Scanning for Python >= 3.9 in PATH...")
        candidates = _scan_python_candidates()
        if candidates:
            for cmd, ver in candidates:
                print(f"[TestVDB]   Found: {cmd} ({ver})")
            print(f"[TestVDB] RECOMMEND: Use '{candidates[0][0]}' instead of 'python'")
        else:
            print("[TestVDB] No Python >= 3.9 found in PATH. Install Python 3.9+.")


def _plugin_root():
    """Determine plugin root from script location."""
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def check_session_env():
    """Ensure TESTVDB_SESSION_ID is set; persist to plugin_root/.env."""
    session_id = os.environ.get("TESTVDB_SESSION_ID", "")
    if session_id:
        print(f"[TestVDB] Session ID: {session_id}")
        return

    # Generate a session ID
    import time
    new_id = f"sess-{int(time.time())}"

    # Try CLAUDE_ENV_FILE first (if available and non-empty)
    env_file = os.environ.get("CLAUDE_ENV_FILE", "")
    if env_file:
        try:
            with open(env_file, "a", encoding="utf-8") as f:
                f.write(f"TESTVDB_SESSION_ID={new_id}\n")
            print(f"[TestVDB] Session ID: {new_id} (persisted to CLAUDE_ENV_FILE)")
            return
        except OSError:
            pass

    # Fallback: write to plugin_root/.env (readable by all hook scripts)
    plugin_root = _plugin_root()
    dot_env = os.path.join(plugin_root, ".env")
    try:
        with open(dot_env, "a", encoding="utf-8") as f:
            f.write(f"TESTVDB_SESSION_ID={new_id}\n")
        print(f"[TestVDB] Session ID: {new_id} (persisted to {dot_env})")
    except OSError as e:
        print(f"[TestVDB] WARNING: Failed to write .env: {e}")
        print(f"[TestVDB] Set TESTVDB_SESSION_ID={new_id} manually")


def check_disk():
    gb = shutil.disk_usage(".").free / 1e9
    msg = f"[TestVDB] Disk: {gb:.1f}GB"
    if gb < 10:
        msg += " (WARNING: <10GB)"
    print(msg)


def check_github_token():
    gh = os.environ.get("GITHUB_TOKEN", "") or os.environ.get("GH_TOKEN", "")
    status = "configured" if gh else "WARNING - not set, Novelty Judge will use WebSearch only"
    print(f"[TestVDB] GitHub Token: {status}")


def check_docker_hub_token():
    dh = os.environ.get("DOCKER_HUB_TOKEN", "")
    if dh:
        print("[TestVDB] Docker Hub Token: configured (higher API rate limits)")
    else:
        print("[TestVDB] Docker Hub Token: WARNING - not set. Docker CLI commands (pull/manifest) work without token. Only Docker Hub REST API queries for tag listing may be rate-limited.")


def check_network():
    # Cross-platform network check using Python urllib (avoids curl dependency on Windows)
    try:
        import urllib.request
        req = urllib.request.Request("https://pypi.org", method="HEAD")
        urllib.request.urlopen(req, timeout=5)
        print("[TestVDB] Network: OK")
    except Exception:
        print("[TestVDB] Network: WARNING - pypi.org unreachable, WebSearch may fail")


def check_settings_schema():
    """Validate settings.json against contracts/settings_schema.json."""
    plugin_root = _plugin_root()
    settings_path = os.path.join(plugin_root, "settings.json")
    schema_path = os.path.join(plugin_root, "contracts", "settings_schema.json")
    if not os.path.exists(schema_path):
        print("[TestVDB] Settings schema: SKIP (contracts/settings_schema.json not found)")
        return
    try:
        import json
        with open(settings_path, encoding="utf-8") as f:
            settings = json.load(f)
        with open(schema_path, encoding="utf-8") as f:
            schema = json.load(f)
    except (OSError, json.JSONDecodeError) as e:
        print(f"[TestVDB] Settings schema: WARNING - {e}")
        return
    # 优先 jsonschema 完整验证
    try:
        import jsonschema
        jsonschema.validate(settings, schema)
        print("[TestVDB] Settings schema: OK (jsonschema)")
    except ImportError:
        # jsonschema 未装 → 轻量自检 required 顶层 keys
        required = schema.get("required", [])
        missing = [k for k in required if k not in settings]
        if missing:
            print(f"[TestVDB] Settings schema: WARNING - missing required keys: {missing}")
        else:
            print(f"[TestVDB] Settings schema: OK (lightweight, {len(required)} required keys)")
    except Exception as e:
        print(f"[TestVDB] Settings schema: WARNING - {e}")


def main():
    print("[TestVDB] Pre-flight checks...")
    check_docker()
    check_python()
    check_session_env()
    check_disk()
    check_github_token()
    check_docker_hub_token()
    check_network()
    check_settings_schema()
    print("[TestVDB] Checks done. Python<3.9 is fatal per Orchestrator spec.")


if __name__ == "__main__":
    main()
