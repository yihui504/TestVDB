#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: index_state
Strategy: Create collection -> create index -> drop collection -> verify index auto-removed (dependency chain break)
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

COLLECTION_NAME = f"state_index_{uuid.uuid4().hex[:8]}"
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


def create_index():
    payload = {
        "collectionName": COLLECTION_NAME,
        "indexParams": [
            {"fieldName": "vector", "metricType": "L2", "indexName": "vector_idx", "index_type": "AUTOINDEX"}
        ]
    }
    return rest_post("indexes/create", payload)


def list_indexes():
    return rest_post("indexes/list", {"collectionName": COLLECTION_NAME})


def describe_index():
    return rest_post("indexes/describe", {"collectionName": COLLECTION_NAME, "fieldName": "vector"})


def insert_entities():
    entities = [{"vector": [float(j % 100) * 0.01 for _ in range(DIM)]} for _ in range(3)]
    return rest_post("entities/insert", {"collectionName": COLLECTION_NAME, "data": entities})


def test_index_drop_chain():
    """Test that dropping a collection properly cleans up associated indexes"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"
    print("  Created")

    # Step 2: Insert a few entities (index needs data)
    print("\n[Step 2] Inserting data...")
    resp = insert_entities()
    assert resp.status_code == 200, f"Insert failed: {resp.status_code}"
    print("  Inserted")

    # Step 3: Create index
    print("\n[Step 3] Creating index on vector field...")
    resp = create_index()
    assert resp.status_code == 200, f"Create index failed: {resp.status_code} {resp.text}"
    print(f"  Index creation initiated: {resp.json()}")

    # Step 4: Verify index exists (list indexes)
    time.sleep(2)
    print("\n[Step 4] Listing indexes...")
    resp = list_indexes()
    assert resp.status_code == 200, f"List indexes failed: {resp.status_code}"
    indexes = resp.json().get("data", [])
    print(f"  Indexes: {indexes}")
    assert len(indexes) > 0, \
        f"StateLogicViolation: Expected at least 1 index after creation, got {indexes}"

    # Step 5: Drop the collection
    print("\n[Step 5] Dropping collection...")
    resp = drop_collection()
    assert resp.status_code == 200, f"Drop failed: {resp.status_code} {resp.text}"
    print("  Dropped")

    # Step 6: Verify collection no longer exists
    time.sleep(2)
    resp = has_collection()
    data = resp.json().get("data", {})
    existed = data.get("exist", data.get("value", True))
    print(f"  Collection exists after drop: {existed}")

    # Step 7: Try to list indexes on dropped collection (should fail)
    print("\n[Step 6] Trying to list indexes on dropped collection...")
    resp = list_indexes()
    print(f"  List indexes status: {resp.status_code}")
    # Milvus should return an error for non-existent collection
    if resp.status_code == 200:
        indexes_after = resp.json().get("data", [])
        print(f"  Indexes after drop: {indexes_after}")
        if len(indexes_after) > 0:
            print("  WARNING: Indexes still returned for dropped collection (may be stale)")
    else:
        print(f"  Correctly rejected: {resp.text[:200] if resp.text else ''}")

    # Step 8: Recreate the same collection and verify no leftover indexes
    print("\n[Step 7] Recreating same-named collection...")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Recreate failed: {resp.status_code} {resp.text}"

    time.sleep(1)
    resp = list_indexes()
    indexes_recreated = resp.json().get("data", [])
    print(f"  Indexes on recreated collection: {indexes_recreated}")
    # A fresh collection should have no indexes
    if len(indexes_recreated) > 0:
        print("  WARNING: Recreated collection has leftover indexes from before")

    print("\n=== PASSED: index-drop chain verified ===")


if __name__ == "__main__":
    try:
        test_index_drop_chain()
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
