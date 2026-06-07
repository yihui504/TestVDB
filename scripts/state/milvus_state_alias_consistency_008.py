#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
TestVDB State Attack Script
Target: milvus v2.6.17
Attack: alias_consistency
Strategy: Create alias -> describe alias -> alter alias to new collection -> verify alias points to new collection
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

COLLECTION_A = f"state_alias_a_{uuid.uuid4().hex[:6]}"
COLLECTION_B = f"state_alias_b_{uuid.uuid4().hex[:6]}"
ALIAS_NAME = f"alias_{uuid.uuid4().hex[:6]}"
DIM = 128


def rest_post(path, payload):
    url = f"{BASE_URL}/v2/vectordb/{path}"
    resp = requests.post(url, json=payload, headers=headers, timeout=30)
    return resp


def create_collection(name):
    payload = {
        "collectionName": name,
        "dimension": DIM,
        "metricType": "L2",
        "idType": "int64",
        "autoID": True,
        "primaryFieldName": "id",
        "vectorFieldName": "vector",
    }
    return rest_post("collections/create", payload)


def drop_collection(name):
    return rest_post("collections/drop", {"collectionName": name})


def create_alias():
    return rest_post("aliases/create", {
        "aliasName": ALIAS_NAME,
        "collectionName": COLLECTION_A
    })


def describe_alias():
    return rest_post("aliases/describe", {"aliasName": ALIAS_NAME})


def list_aliases():
    return rest_post("aliases/list", {})


def alter_alias():
    return rest_post("aliases/alter", {
        "aliasName": ALIAS_NAME,
        "collectionName": COLLECTION_B
    })


def drop_alias():
    return rest_post("aliases/drop", {"aliasName": ALIAS_NAME})


def test_alias_consistency():
    """Test alias lifecycle: create -> describe -> alter -> list -> drop -> verify gone"""
    # Step 1: Create two collections
    print(f"\n[Step 1] Creating collections A={COLLECTION_A} and B={COLLECTION_B}")
    resp = create_collection(COLLECTION_A)
    assert resp.status_code in (200, 201), f"Create A failed: {resp.status_code}"
    resp = create_collection(COLLECTION_B)
    assert resp.status_code in (200, 201), f"Create B failed: {resp.status_code}"
    print("  Both created")

    # Step 2: Create alias pointing to A
    print(f"\n[Step 2] Creating alias '{ALIAS_NAME}' -> {COLLECTION_A}")
    resp = create_alias()
    assert resp.status_code == 200, f"Create alias failed: {resp.status_code} {resp.text}"
    print("  Alias created")

    # Step 3: Describe alias and verify it points to A
    resp = describe_alias()
    assert resp.status_code == 200, f"Describe alias failed: {resp.status_code}"
    data = resp.json().get("data", {})
    collection_in_alias = data.get("collectionName", "")
    print(f"  Alias points to: {collection_in_alias}")
    assert collection_in_alias == COLLECTION_A, \
        f"StateLogicViolation: Alias should point to {COLLECTION_A}, points to {collection_in_alias}"

    # Step 4: List aliases and verify our alias is present
    resp = list_aliases()
    assert resp.status_code == 200, f"List aliases failed: {resp.status_code}"
    aliases = resp.json().get("data", [])
    alias_names = [a.get("aliasName", a) if isinstance(a, dict) else a for a in aliases]
    assert ALIAS_NAME in alias_names, \
        f"StateLogicViolation: Alias not found in list: {alias_names}"
    print(f"  Aliases ({len(alias_names)}): {alias_names}")

    # Step 5: Alter alias to point to B
    print(f"\n[Step 3] Altering alias -> {COLLECTION_B}")
    resp = alter_alias()
    assert resp.status_code == 200, f"Alter alias failed: {resp.status_code} {resp.text}"
    print("  Alias altered")

    # Step 6: Verify alias now points to B
    resp = describe_alias()
    data = resp.json().get("data", {})
    collection_in_alias = data.get("collectionName", "")
    assert collection_in_alias == COLLECTION_B, \
        f"StateLogicViolation: After alter, alias should point to {COLLECTION_B}, got {collection_in_alias}"
    print(f"  Alias now points to: {collection_in_alias}")

    # Step 7: Drop alias
    print("\n[Step 4] Dropping alias...")
    resp = drop_alias()
    assert resp.status_code == 200, f"Drop alias failed: {resp.status_code} {resp.text}"
    print("  Alias dropped")

    # Step 8: Verify alias not in list
    resp = list_aliases()
    aliases_after = resp.json().get("data", [])
    names_after = [a.get("aliasName", a) if isinstance(a, dict) else a for a in aliases_after]
    assert ALIAS_NAME not in names_after, \
        f"StateLogicViolation: Alias still in list after drop: {names_after}"
    print("  Alias removed from list: confirmed")

    print("\n=== PASSED: alias consistency verified ===")


if __name__ == "__main__":
    try:
        test_alias_consistency()
    except AssertionError as e:
        print(f"\n=== FAILED: {e} ===")
        sys.exit(1)
    except Exception as e:
        print(f"\n=== ERROR: {e} ===")
        sys.exit(1)
    finally:
        try:
            drop_collection(COLLECTION_A)
            drop_collection(COLLECTION_B)
            print(f"  Cleaned: {COLLECTION_A}, {COLLECTION_B}")
        except Exception:
            pass
