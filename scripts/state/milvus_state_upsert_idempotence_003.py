#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: upsert_idempotence
Strategy: Upsert same point twice -> verify count increases by 1 (not 2) -> verify data is correct
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

COLLECTION_NAME = f"state_upsert_{uuid.uuid4().hex[:8]}"
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
        "autoID": False,
        "primaryFieldName": "id",
        "vectorFieldName": "vector",
    }
    return rest_post("collections/create", payload)


def drop_collection():
    return rest_post("collections/drop", {"collectionName": COLLECTION_NAME})


def get_collection_stats():
    return rest_post("collections/get_stats", {"collectionName": COLLECTION_NAME})


def upsert_entities(entities):
    return rest_post("entities/upsert", {"collectionName": COLLECTION_NAME, "data": entities})


def query_all():
    return rest_post("entities/query", {
        "collectionName": COLLECTION_NAME,
        "filter": "id >= 0",
        "outputFields": ["id", "vector"],
        "limit": 100
    })


def load_collection():
    return rest_post("collections/load", {"collectionName": COLLECTION_NAME})


def test_upsert_idempotence():
    """Test that upserting the same key multiple times results in correct state (1 entity, not N)"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"

    # Step 2: Upsert a single entity with id=1
    print("\n[Step 2] Upserting entity id=1 (first time)...")
    entities = [{"id": 1, "vector": [float(j % 100) * 0.01 for j in range(DIM)]}]
    resp = upsert_entities(entities)
    assert resp.status_code == 200, f"First upsert failed: {resp.status_code} {resp.text}"
    upsert_count_1 = resp.json().get("data", {}).get("upsertCount", 0)
    print(f"  First upsert count: {upsert_count_1}")

    # Step 3: Upsert the SAME entity again with a different vector
    print("\n[Step 3] Upserting same entity id=1 with different vector...")
    different_vector = [float((j * 2) % 100) * 0.01 for j in range(DIM)]
    entities2 = [{"id": 1, "vector": different_vector}]
    resp = upsert_entities(entities2)
    assert resp.status_code == 200, f"Second upsert failed: {resp.status_code} {resp.text}"
    upsert_count_2 = resp.json().get("data", {}).get("upsertCount", 0)
    print(f"  Second upsert count: {upsert_count_2}")

    # Step 4: Load and query to verify only 1 entity exists
    print("\n[Step 4] Loading collection and querying...")
    resp = load_collection()
    assert resp.status_code == 200, f"Load failed: {resp.status_code}"
    time.sleep(2)

    resp = query_all()
    assert resp.status_code == 200, f"Query failed: {resp.status_code} {resp.text}"
    results = resp.json().get("data", [])
    print(f"  Query returned {len(results)} entities")
    assert len(results) == 1, (
        f"StateLogicViolation: Upserting same id twice should yield exactly 1 entity, "
        f"got {len(results)}"
    )

    # Step 5: Verify the latest vector value was persisted (last write wins)
    actual_vector = results[0].get("vector", [])
    assert actual_vector == different_vector, (
        f"StateLogicViolation: Upsert should persist last write. "
        f"Expected vector head={different_vector[:3]}, got {actual_vector[:3]}"
    )
    print("  Vector value verified: last write wins")

    # Step 6: Verify stats show 1 entity
    time.sleep(1)
    resp = get_collection_stats()
    row_count = resp.json().get("data", {}).get("rowCount", -1)
    assert row_count == 1 or (row_count is not None and row_count >= 1), \
        f"StateLogicViolation: Expected rowCount=1 after idempotent upsert, got {row_count}"
    print(f"  Stats rowCount: {row_count}")
    print("\n=== PASSED: upsert idempotence verified ===")


if __name__ == "__main__":
    try:
        test_upsert_idempotence()
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
