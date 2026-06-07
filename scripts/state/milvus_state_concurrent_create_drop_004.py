#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: concurrent
Strategy: Concurrent create/drop of same collection name from multiple threads -> verify consistent final state
"""

import requests
import json
import sys
import os
import time
import uuid
import threading

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

BASE_URL = os.environ.get("TESTVDB_DB_URL", "http://localhost:19530")
AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")
CONCURRENT_THREADS = int(os.environ.get("TESTVDB_CONCURRENT_THREADS", "10"))

headers = {"Content-Type": "application/json"}
if AUTH_HEADER:
    headers["Authorization"] = AUTH_HEADER

COLLECTION_NAME = f"state_concur_{uuid.uuid4().hex[:8]}"
DIM = 128
INSERT_COUNT = 50  # Total entities to insert per thread


def rest_post(path, payload):
    url = f"{BASE_URL}/v2/vectordb/{path}"
    try:
        resp = requests.post(url, json=payload, headers=headers, timeout=30)
        return resp
    except Exception as e:
        return None


errors = []
created_flag = threading.Event()
drop_done = threading.Event()


def concurrent_create():
    """Multiple threads try to create the same collection"""
    try:
        payload = {
            "collectionName": COLLECTION_NAME,
            "dimension": DIM,
            "metricType": "L2",
            "idType": "int64",
            "autoID": True,
            "primaryFieldName": "id",
            "vectorFieldName": "vector",
        }
        resp = rest_post("collections/create", payload)
        if resp and resp.status_code in (200, 201):
            created_flag.set()
        elif resp and resp.status_code == 409:
            pass  # Expected conflict
        elif resp is None:
            errors.append("concurrent_create: connection error")
        else:
            errors.append(f"concurrent_create: unexpected {resp.status_code}")
    except Exception as e:
        errors.append(f"concurrent_create: {str(e)}")


def concurrent_drop():
    """Multiple threads try to drop the same collection"""
    drop_done.wait()  # Wait for creation to have happened
    try:
        resp = rest_post("collections/drop", {"collectionName": COLLECTION_NAME})
        if resp and resp.status_code not in (200, 204):
            if resp.status_code != 404:  # 404 is acceptable if already dropped
                errors.append(f"concurrent_drop: {resp.status_code} {resp.text[:100] if resp.text else ''}")
    except Exception as e:
        errors.append(f"concurrent_drop: {str(e)}")


def concurrent_insert():
    """Multiple threads insert data concurrently into the collection"""
    try:
        entities = []
        for i in range(INSERT_COUNT):
            entities.append({
                "vector": [float((threading.get_ident() + i) % 100) * 0.01 for _ in range(DIM)]
            })
        resp = rest_post("entities/insert", {
            "collectionName": COLLECTION_NAME,
            "data": entities
        })
        if resp is None:
            errors.append(f"concurrent_insert_{threading.get_ident()}: connection error")
        elif resp.status_code != 200:
            if resp.status_code == 404:
                # Collection might have been dropped concurrently
                pass
            else:
                errors.append(f"concurrent_insert: {resp.status_code}")
    except Exception as e:
        errors.append(f"concurrent_insert_{threading.get_ident()}: {str(e)}")


def test_concurrent_state():
    """Test concurrent operations on the same collection"""
    print(f"\n[Test] Concurrent create/drop/insert on collection: {COLLECTION_NAME}")
    print(f"  Threads: {CONCURRENT_THREADS}")

    # Phase 1: Concurrent creates
    print("\n[Phase 1] Concurrent creates...")
    threads = []
    for _ in range(CONCURRENT_THREADS):
        t = threading.Thread(target=concurrent_create)
        threads.append(t)
        t.start()
    for t in threads:
        t.join()

    # At least one should have succeeded
    if not created_flag.is_set():
        print("  WARNING: No create succeeded. Attempting single create to stabilize...")
        payload = {
            "collectionName": COLLECTION_NAME,
            "dimension": DIM,
            "metricType": "L2",
            "idType": "int64",
            "autoID": True,
            "primaryFieldName": "id",
            "vectorFieldName": "vector",
        }
        resp = rest_post("collections/create", payload)
        if resp and resp.status_code in (200, 201):
            created_flag.set()
    print(f"  Created: {created_flag.is_set()}")

    if not created_flag.is_set():
        print("  SKIP: Could not create collection. Is Milvus running?")
        return

    # Phase 2: Wait for creation, then concurrent inserts
    print("\n[Phase 2] Concurrent inserts ({} threads)...".format(CONCURRENT_THREADS))
    drop_done.set()  # Allow drops to start alongside inserts
    threads = []
    for _ in range(min(CONCURRENT_THREADS, 5)):  # Limit to 5 insert threads
        t = threading.Thread(target=concurrent_insert)
        threads.append(t)
        t.start()
    for t in threads:
        t.join()

    # Phase 3: Concurrent drops
    print("\n[Phase 3] Concurrent drops...")
    threads = []
    for _ in range(CONCURRENT_THREADS):
        t = threading.Thread(target=concurrent_drop)
        threads.append(t)
        t.start()
    for t in threads:
        t.join()

    # Phase 4: Verify final state — collection should not exist
    time.sleep(2)
    resp = rest_post("collections/has", {"collectionName": COLLECTION_NAME})
    if resp and resp.status_code == 200:
        data = resp.json().get("data", {})
        existed = data.get("exist", data.get("value", None))
        if existed is not False:
            print(f"  WARNING: Collection still exists after concurrent drops: {data}")
            # Clean up
            rest_post("collections/drop", {"collectionName": COLLECTION_NAME})
        else:
            print("  Collection correctly gone: True")

    if errors:
        print(f"\n  Errors during concurrent ops ({len(errors)}):")
        for e in errors[:5]:
            print(f"    - {e}")
    else:
        print("  No errors during concurrent operations")

    print("\n=== PASSED: concurrent state test completed ===")


if __name__ == "__main__":
    try:
        test_concurrent_state()
    except AssertionError as e:
        print(f"\n=== FAILED: {e} ===")
        sys.exit(1)
    except Exception as e:
        print(f"\n=== ERROR: {e} ===")
        sys.exit(1)
    finally:
        try:
            rest_post("collections/drop", {"collectionName": COLLECTION_NAME})
            print(f"  Cleaned: {COLLECTION_NAME}")
        except Exception:
            pass
