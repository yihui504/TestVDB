#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: count_consistency
Strategy: CRUD after COUNT consistency — create, insert N, verify count, insert M more, verify cumulative count
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

COLLECTION_NAME = f"state_count_{uuid.uuid4().hex[:8]}"
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


def insert_entities(entities):
    return rest_post("entities/insert", {"collectionName": COLLECTION_NAME, "data": entities})


def test_count_consistency():
    """Test that insert count is consistently reflected in collection stats"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"
    print(f"  Created: {resp.status_code}")

    # Step 2: Insert N=10 entities
    print("\n[Step 2] Inserting 10 entities...")
    entities = []
    for i in range(10):
        entities.append({"id": i, "vector": [float(j % 100) * 0.01 for j in range(DIM)]})
    resp = insert_entities(entities)
    assert resp.status_code == 200, f"Insert failed: {resp.status_code} {resp.text}"
    insert_count = resp.json().get("data", {}).get("insertCount", 0)
    print(f"  Insert returned count: {insert_count}")

    # Step 3: Verify count via get_stats
    print("\n[Step 3] Verifying count via get_stats...")
    time.sleep(2)  # Allow eventual consistency
    resp = get_collection_stats()
    assert resp.status_code == 200, f"get_stats failed: {resp.status_code} {resp.text}"
    row_count = resp.json().get("data", {}).get("rowCount", 0)
    print(f"  rowCount from stats: {row_count}")
    assert row_count >= 10, f"StateLogicViolation: Expected rowCount >= 10, got {row_count}"

    # Step 4: Insert M=5 more entities
    print("\n[Step 4] Inserting 5 more entities...")
    entities2 = []
    for i in range(10, 15):
        entities2.append({"id": i, "vector": [float(j % 100) * 0.01 for j in range(DIM)]})
    resp = insert_entities(entities2)
    assert resp.status_code == 200, f"Second insert failed: {resp.status_code} {resp.text}"

    # Step 5: Verify cumulative count
    print("\n[Step 5] Verifying cumulative count...")
    time.sleep(2)
    resp = get_collection_stats()
    assert resp.status_code == 200, f"get_stats failed: {resp.status_code} {resp.text}"
    row_count2 = resp.json().get("data", {}).get("rowCount", 0)
    print(f"  rowCount after second insert: {row_count2}")
    assert row_count2 >= 15, (
        f"StateLogicViolation: Expected cumulative rowCount >= 15, got {row_count2}"
    )
    print("\n=== PASSED: count consistency verified ===")


if __name__ == "__main__":
    try:
        test_count_consistency()
    except AssertionError as e:
        print(f"\n=== FAILED: {e} ===")
        sys.exit(1)
    except Exception as e:
        print(f"\n=== ERROR: {e} ===")
        sys.exit(1)
    finally:
        # Cleanup
        try:
            drop_collection()
            print(f"  Cleaned up collection: {COLLECTION_NAME}")
        except Exception:
            pass
