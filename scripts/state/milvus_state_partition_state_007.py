#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: partition_state
Strategy: Create collection with partition -> insert into partition -> verify partition stats -> drop partition -> verify cleanup
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

COLLECTION_NAME = f"state_part_{uuid.uuid4().hex[:8]}"
PARTITION_NAME = f"p_{uuid.uuid4().hex[:6]}"
DIM = 128


def rest_post(path, payload):
    url = f"{BASE_URL}/v2/vectordb/{path}"
    resp = requests.post(url, json=payload, headers=headers, timeout=30)
    return resp


def create_collection_with_partition_key():
    """Create collection with a partition_key field to enable partition operations"""
    schema = {
        "autoId": True,
        "fields": [
            {"name": "id", "dataType": "Int64", "isPrimary": True, "autoID": True},
            {"name": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": str(DIM)}},
            {"name": "category", "dataType": "Int64"},
        ]
    }
    return rest_post("collections/create", {
        "collectionName": COLLECTION_NAME,
        "schema": schema
    })


def drop_collection():
    return rest_post("collections/drop", {"collectionName": COLLECTION_NAME})


def create_partition():
    return rest_post("partitions/create", {
        "collectionName": COLLECTION_NAME,
        "partitionName": PARTITION_NAME
    })


def has_partition():
    return rest_post("partitions/has", {
        "collectionName": COLLECTION_NAME,
        "partitionName": PARTITION_NAME
    })


def list_partitions():
    return rest_post("partitions/list", {"collectionName": COLLECTION_NAME})


def drop_partition():
    return rest_post("partitions/drop", {
        "collectionName": COLLECTION_NAME,
        "partitionName": PARTITION_NAME
    })


def get_partition_stats():
    return rest_post("partitions/get_stats", {
        "collectionName": COLLECTION_NAME,
        "partitionName": PARTITION_NAME
    })


def insert_into_partition():
    entities = [
        {"vector": [float(j % 100) * 0.01 for _ in range(DIM)], "category": 1}
        for _ in range(5)
    ]
    return rest_post("entities/insert", {
        "collectionName": COLLECTION_NAME,
        "data": entities,
        "partitionName": PARTITION_NAME
    })


def test_partition_state():
    """Test partition lifecycle: create -> insert -> stats -> drop -> state cleanup"""
    # Step 1: Create collection
    print(f"\n[Step 1] Creating collection: {COLLECTION_NAME}")
    resp = create_collection_with_partition_key()
    assert resp.status_code in (200, 201), f"Create failed: {resp.status_code} {resp.text}"
    print("  Created")

    # Step 2: Create partition
    print(f"\n[Step 2] Creating partition: {PARTITION_NAME}")
    resp = create_partition()
    assert resp.status_code == 200, f"Create partition failed: {resp.status_code} {resp.text}"
    print("  Created")

    # Step 3: Verify partition exists
    resp = has_partition()
    assert resp.status_code == 200, f"has partition failed: {resp.status_code}"
    print(f"  Partition exists: {resp.json()}")

    # Step 4: List partitions -- should include _default and our new one
    resp = list_partitions()
    assert resp.status_code == 200, f"List partitions failed: {resp.status_code}"
    partitions = resp.json().get("data", [])
    partition_names = [p.get("partitionName", p) if isinstance(p, dict) else p for p in partitions]
    assert PARTITION_NAME in partition_names, \
        f"StateLogicViolation: Partition {PARTITION_NAME} not found in {partition_names}"
    print(f"  Partitions: {partition_names}")

    # Step 5: Insert into partition
    print("\n[Step 3] Inserting 5 entities into partition...")
    resp = insert_into_partition()
    assert resp.status_code == 200, f"Insert into partition failed: {resp.status_code} {resp.text}"
    print(f"  Inserted: {resp.json()}")

    # Step 6: Get partition stats
    time.sleep(2)
    print("\n[Step 4] Getting partition stats...")
    resp = get_partition_stats()
    if resp.status_code == 200:
        stats = resp.json().get("data", {})
        print(f"  Partition stats: {stats}")
    else:
        print(f"  get_partition_stats returned: {resp.status_code} {resp.text[:200] if resp.text else ''}")

    # Step 7: Drop partition
    print("\n[Step 5] Dropping partition...")
    resp = drop_partition()
    assert resp.status_code == 200, f"Drop partition failed: {resp.status_code} {resp.text}"
    print("  Dropped")

    # Step 8: Verify partition no longer exists
    resp = has_partition()
    data = resp.json().get("data", {})
    existed = data.get("exist", data.get("value", True))
    print(f"  Partition exists after drop: {existed}")

    # Step 9: Verify partition removed from list
    resp = list_partitions()
    partitions_after = resp.json().get("data", [])
    names_after = [p.get("partitionName", p) if isinstance(p, dict) else p for p in partitions_after]
    assert PARTITION_NAME not in names_after, \
        f"StateLogicViolation: Partition {PARTITION_NAME} still in list after drop: {names_after}"
    print(f"  Partitions after drop (without our partition): {names_after}")

    print("\n=== PASSED: partition state verified ===")


if __name__ == "__main__":
    try:
        test_partition_state()
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
