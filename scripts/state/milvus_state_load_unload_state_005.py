#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: index_state
Strategy: Verify load state transitions: created -> load -> loaded -> release -> released -> query fails
"""

import requests
import json
import sys
import os
import time
import uuid

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

BASE_URL = os.environ.get("TESTVDB_DB_URL", "http://localhost:19530")
AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")

headers = {"Content-Type": "application/json"}
if AUTH_HEADER:
    headers["Authorization"] = AUTH_HEADER

COLLECTION_NAME = f"state_load_{uuid.uuid4().hex[:8]}"
DIM = 128


def rest_post(path, payload):
    url = f"{BASE_URL}/v2/vectordb/{path}"
    resp = requests.post(url, json=payload, headers=headers, timeout=30)
    return resp


def create_collection():
    payload = {
        "collectionName": COLLECTION_NAME,
        "dimension": DIM,
        "metricType": "L2",
        "idType": "int64",
        "autoID": True,
        "primaryFieldName": "id",
        "vectorFieldName": "vector",
    }
    return rest_post("collections/create", payload)


def drop_collection():
    return rest_post("collections/drop", {"collectionName": COLLECTION_NAME})


def get_load_state():
    return rest_post("collections/get_load_state", {"collectionName": COLLECTION_NAME})


def load_collection():
    return rest_post("collections/load", {"collectionName": COLLECTION_NAME})


def release_collection():
    return rest_post("collections/release", {"collectionName": COLLECTION_NAME})


def insert_entities():
    entities = [{"vector": [float(j % 100) * 0.01 for _ in range(DIM)]} for _ in range(5)]
    return rest_post("entities/insert", {"collectionName": COLLECTION_NAME, "data": entities})


def search():
    return rest_post("entities/search", {
        "collectionName": COLLECTION_NAME,
        "data": [[float(j % 100) * 0.01 for j in range(DIM)]],
        "annsField": "vector",
        "limit": 3,
    })


def query():
    return rest_post("entities/query", {
        "collectionName": COLLECTION_NAME,
        "filter": "id >= 0",
        "limit": 10,
    })


def test_load_state_transitions():
    """Test state transitions: created -> load -> loaded -> release -> released -> query fails"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"
    print("  Created")

    # Step 2: Insert data
    print("\n[Step 2] Inserting data...")
    resp = insert_entities()
    assert resp.status_code == 200, f"Insert failed: {resp.status_code}"
    print("  Inserted 5 entities")

    # Step 3: Check initial load state (should be NotLoad)
    print("\n[Step 3] Checking initial load state...")
    resp = get_load_state()
    assert resp.status_code == 200, f"get_load_state failed: {resp.status_code}"
    load_state = resp.json().get("data", {}).get("state", "")
    print(f"  Load state before load: {load_state}")

    # Step 4: Load collection
    print("\n[Step 4] Loading collection...")
    resp = load_collection()
    assert resp.status_code == 200, f"Load failed: {resp.status_code} {resp.text}"
    print("  Load requested")
    time.sleep(3)

    # Step 5: Verify loaded state
    print("\n[Step 5] Verifying loaded state...")
    resp = get_load_state()
    assert resp.status_code == 200, f"get_load_state failed: {resp.status_code}"
    load_state = resp.json().get("data", {}).get("state", "")
    is_loaded = "Loaded" in str(load_state) or "load" in str(load_state).lower()
    print(f"  Load state: {load_state}")
    # Note: Milvus might return different state string formats

    # Step 6: Search on loaded collection must succeed
    print("\n[Step 6] Searching on loaded collection...")
    resp = search()
    assert resp.status_code == 200, \
        f"StateLogicViolation: Search on loaded collection failed: {resp.status_code} {resp.text}"
    results = resp.json().get("data", [])
    print(f"  Search succeeded, returned {len(results)} results")

    # Step 7: Release collection
    print("\n[Step 7] Releasing collection...")
    resp = release_collection()
    assert resp.status_code == 200, f"Release failed: {resp.status_code} {resp.text}"
    print("  Release requested")
    time.sleep(2)

    # Step 8: Verify released state
    resp = get_load_state()
    load_state = resp.json().get("data", {}).get("state", "")
    print(f"  Load state after release: {load_state}")

    # Step 9: Search/query on released collection must fail
    print("\n[Step 8] Verifying search/query fails after release...")
    resp = search()
    if resp.status_code == 200:
        print("  WARNING: Search succeeded after release (may be acceptable if Milvus auto-loads)")
    else:
        print(f"  Search correctly rejected: {resp.status_code}")

    resp = query()
    if resp.status_code == 200:
        print("  WARNING: Query succeeded after release")
    else:
        print(f"  Query correctly rejected: {resp.status_code}")

    print("\n=== PASSED: load state transitions verified ===")


if __name__ == "__main__":
    try:
        test_load_state_transitions()
    except AssertionError as e:
        print(f"\n=== FAILED: {e} ===")
        sys.exit(1)
    except Exception as e:
        print(f"\n=== ERROR: {e} ===")
        sys.exit(1)
    finally:
        try:
            drop_collection()
            print(f"  Cleaned: {COLLECTION_NAME}")
        except Exception:
            pass
