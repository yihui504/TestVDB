#!/usr/bin/env python3
"""TestVDB Session End Emergency Cleanup.

Performs emergency cleanup of Docker containers and saves a checkpoint
when the session ends unexpectedly. Respects .session.lock to avoid
cleaning active sessions.
"""
import json
import os
import subprocess
import time
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
    print("[TestVDB] SessionEnd: Emergency cleanup...")

    session_id = find_session_id()
    plugin_root = _plugin_root()

    # Check session lock before cleaning
    if session_id:
        results_dir = os.environ.get("TESTVDB_RESULTS_DIR", os.path.join(plugin_root, "results"))
        # Try to find session directory
        if os.path.isdir(results_dir):
            for candidate in [os.path.join(results_dir, d, d2, d3)
                              for d in os.listdir(results_dir) if os.path.isdir(os.path.join(results_dir, d))
                              for d2 in os.listdir(os.path.join(results_dir, d)) if os.path.isdir(os.path.join(results_dir, d, d2))
                              for d3 in os.listdir(os.path.join(results_dir, d, d2)) if os.path.isdir(os.path.join(results_dir, d, d2, d3))]:
                if is_session_locked(candidate):
                    print(f"[TestVDB] SessionEnd: Active session lock found at {candidate}. "
                          "Skipping cleanup — session may still be running.")
                    return

    # Clean up containers
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
    else:
        # Try to remove any testvdb-net networks
        result = subprocess.run(
            ["docker", "network", "ls", "--filter", "name=testvdb-net", "--format", "{{.Name}}"],
            capture_output=True, text=True,
        )
        for net in result.stdout.strip().split("\n"):
            if net:
                subprocess.run(["docker", "network", "rm", net], capture_output=True)

    # Save emergency checkpoint
    results_dir = os.environ.get("TESTVDB_RESULTS_DIR", os.path.join(plugin_root, "results"))
    ckpt_dir = os.path.join(results_dir, ".checkpoints")
    os.makedirs(ckpt_dir, exist_ok=True)

    state = {
        "pipeline_state": "interrupted",
        "interrupted_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "session_id": session_id,
        "note": "Emergency checkpoint by SessionEnd",
    }

    ckpt_path = os.path.join(ckpt_dir, f"mine_state_{int(time.time())}.json")
    with open(ckpt_path, "w", encoding="utf-8") as f:
        json.dump(state, f, indent=2)

    print(f"[TestVDB] Checkpoint saved: {ckpt_path}")
    print(f"[TestVDB] Cleaned {len(targets)} containers.")


if __name__ == "__main__":
    main()
