#!/usr/bin/env python3
"""TestVDB Session Start Pre-flight Checks.

Verifies Docker, Python version, disk space, GitHub token, and network
connectivity before starting a mining session.
"""
import subprocess
import sys
import shutil
import os


def check_docker():
    r = subprocess.run(["docker", "ps"], capture_output=True)
    status = "OK" if not r.returncode else "WARNING - not running"
    print(f"[TestVDB] Docker: {status}")

    # Check docker compose availability
    r2 = subprocess.run(["docker", "compose", "version"], capture_output=True)
    compose_status = "OK" if not r2.returncode else "WARNING - docker compose not available"
    print(f"[TestVDB] Docker Compose: {compose_status}")


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


def check_network():
    # Cross-platform network check using Python urllib (avoids curl dependency on Windows)
    try:
        import urllib.request
        req = urllib.request.Request("https://pypi.org", method="HEAD")
        urllib.request.urlopen(req, timeout=5)
        print("[TestVDB] Network: OK")
    except Exception:
        print("[TestVDB] Network: WARNING - pypi.org unreachable, WebSearch may fail")


def main():
    print("[TestVDB] Pre-flight checks...")
    check_docker()
    check_python()
    check_disk()
    check_github_token()
    check_network()
    print("[TestVDB] Checks done. Python<3.9 is fatal per Orchestrator spec.")


if __name__ == "__main__":
    main()
