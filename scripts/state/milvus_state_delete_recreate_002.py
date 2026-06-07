#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: delete_consistency
Strategy: Create collection -> insert data -> drop collection -> verify data inaccessible -> recreate same name -> verify clean state
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

COLLECTION_NAME = f"state_delete_{uuid.uuid4().hex[:8]}"
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


def has_collection():
    return rest_post("collections/has", {"collectionName": COLLECTION_NAME})


def list_collections():
    return rest_post("collections/list", {})


def insert_entities():
    entities = [{"vector": [float(j % 100) * 0.01 for j in range(DIM)]} for _ in range(5)]
    return rest_post("entities/insert", {"collectionName": COLLECTION_NAME, "data": entities})


def test_delete_recreate_consistency():
    """Test that a dropped collection is fully removed and a new one with the same name starts clean"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"
    print(f"  Created: {resp.status_code}")

    # Step 2: Verify collection exists
    resp = has_collection()
    assert resp.status_code == 200, f"has failed: {resp.status_code}"
    data = resp.json().get("data", {})
    assert data.get("exist") is True or data.get("exist") == True or data.get("value") is True, \
        f"StateLogicViolation: Expected collection to exist after create, got: {data}"
    print("  Collection exists: True")

    # Step 3: Insert data
    print("\n[Step 2] Inserting 5 entities...")
    resp = insert_entities()
    assert resp.status_code == 200, f"Insert failed: {resp.status_code} {resp.text}"
    print(f"  Insert success: {resp.json()}")

    # Step 4: Verify collection appears in list
    resp = list_collections()
    assert resp.status_code == 200, f"list failed: {resp.status_code}"
    collections = resp.json().get("data", [])
    assert COLLECTION_NAME in collections, \
        f"StateLogicViolation: Collection should be in list after create, got: {collections}"
    print(f"  Collection in list: True")

    # Step 5: Drop collection
    print("\n[Step 3] Dropping collection...")
    resp = drop_collection()
    assert resp.status_code == 200, f"Drop failed: {resp.status_code} {resp.text}"
    print(f"  Dropped: {resp.status_code}")

    # Step 6: Verify collection no longer exists
    resp = has_collection()
    data = resp.json().get("data", {})
    # has might still return 200 but with exist=False
    existed = data.get("exist", data.get("value", True))
    assert existed is False or existed == "false", \
        f"StateLogicViolation: Collection should not exist after drop, got: {data}"
    print("  Collection exists after drop: False")

    # Step 7: Verify collection not in list
    resp = list_collections()
    collections = resp.json().get("data", [])
    assert COLLECTION_NAME not in collections, \
        f"StateLogicViolation: Collection should not be in list after drop, got: {collections}"
    print("  Collection not in list: confirmed")

    # Step 8: Recreate same-named collection
    print("\n[Step 4] Recreating same-named collection...")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Recreate failed: {resp.status_code} {resp.text}"
    print(f"  Recreated: {resp.status_code}")

    # Step 9: Verify the new collection is clean (stats = 0)
    time.sleep(2)
    resp = rest_post("collections/get_stats", {"collectionName": COLLECTION_NAME})
    assert resp.status_code == 200, f"get_stats on recreated failed: {resp.status_code} {resp.text}"
    row_count = resp.json().get("data", {}).get("rowCount", -1)
    assert row_count == 0 or row_count is None, \
        f"StateLogicViolation: Recreated collection should be empty, got rowCount={row_count}"
    print(f"  New collection rowCount: {row_count} (expected 0 or None)")
    print("\n=== PASSED: delete-recreate consistency verified ===")


if __name__ == "__main__":
    try:
        test_delete_recreate_consistency()
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
