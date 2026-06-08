#!/usr/bin/env python3
"""TestVDB Docker Cleanup on Stop.

Cleans up TestVDB Docker containers when the session stops.
Uses TESTVDB_SESSION_ID to target session-specific containers,
or falls back to cleaning all TestVDB containers.
Respects .session.lock to avoid cleaning active sessions.
"""
import json
import os
import subprocess
from _session_utils import find_session_id, is_session_locked, _plugin_root


BASE_CONTAINER_NAMES = [
    "testvdb-milvus-standalone",
    "testvdb-milvus-etcd",
    "testvdb-milvus-minio",
    "testvdb-qdrant",
    "testvdb-weaviate",
    "testvdb-pgvector",
]


def main():
    print("[TestVDB] Stop: Cleaning session Docker containers...")

    session_id = find_session_id()

    # Check session lock before cleaning
    if session_id:
        # Try to find and check session directory
        plugin_root = _plugin_root()
        results_dir = os.environ.get("TESTVDB_RESULTS_DIR", os.path.join(plugin_root, "results"))
        if os.path.isdir(results_dir):
            for target_dir in os.listdir(results_dir):
                target_path = os.path.join(results_dir, target_dir)
                if not os.path.isdir(target_path):
                    continue
                for ver_dir in os.listdir(target_path):
                    ver_path = os.path.join(target_path, ver_dir)
                    if not os.path.isdir(ver_path):
                        continue
                    for ts_dir in os.listdir(ver_path):
                        ts_path = os.path.join(ver_path, ts_dir)
                        if os.path.isdir(ts_path) and is_session_locked(ts_path):
                            print(f"[TestVDB] Stop: Active session lock at {ts_path}. "
                                  "Skipping cleanup — session may still be running.")
                            return

    if session_id:
        targets = [f"{base}-{session_id}" for base in BASE_CONTAINER_NAMES]
    else:
        targets = list(BASE_CONTAINER_NAMES)

    for container in targets:
        subprocess.run(
            ["docker", "rm", "-f", container],
            capture_output=True,
        )

    # Clean up Docker networks
    if session_id:
        subprocess.run(
            ["docker", "network", "rm", f"testvdb-net-{session_id}"],
            capture_output=True,
        )

    session_label = f" (session:{session_id})" if session_id else " (all TestVDB)"
    print(f"[TestVDB] Cleaned {len(targets)} containers.{session_label}")


if __name__ == "__main__":
    main()
