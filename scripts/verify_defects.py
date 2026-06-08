import requests, sys, time, json, uuid

BASE = "http://localhost:19530"
HEADERS = {"Content-Type": "application/json"}

def create_collection(name, extra_props=None, index_type="IVF_FLAT"):
    body = {
        "collectionName": name,
        "schema": {
            "autoID": False,
            "enableDynamicField": True,
            "fields": [
                {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
                {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
            ]
        },
        "indexParams": [
            {"fieldName": "vector", "metricType": "COSINE", "indexType": index_type,
             "params": {"nlist": 128}}
        ]
    }
    if extra_props:
        body["properties"] = extra_props
    r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS, json=body)
    return r.json()

def insert_data(name, count=10):
    data = [{"id": i, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(1, count+1)]
    r = requests.post(f"{BASE}/v2/vectordb/entities/insert", headers=HEADERS,
                      json={"collectionName": name, "data": data})
    return r.json()

def search(name, vector, limit=3, search_params=None):
    body = {"collectionName": name, "data": [vector], "limit": limit}
    if search_params:
        body["searchParams"] = search_params
    r = requests.post(f"{BASE}/v2/vectordb/entities/search", headers=HEADERS, json=body)
    return r.json()

def drop_collection(name):
    requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
                  json={"collectionName": name})

def get_collection_details(name):
    r = requests.post(f"{BASE}/v2/vectordb/collections/describe", headers=HEADERS,
                      json={"collectionName": name})
    return r.json()

print("=" * 70)
print("TestVDB Defect Verification Script")
target = os.environ.get("TESTVDB_TARGET", "unknown")
version = os.environ.get("TESTVDB_VERSION", "unknown")
print(f"Target: {target} v{version}")
print("=" * 70)

results = {}

# ============================================================
# CLUSTER 1: nprobe validation (IVF_FLAT)
# ============================================================
print("\n" + "=" * 70)
print("CLUSTER 1: nprobe parameter validation")
print("=" * 70)

c1 = "verify_nprobe_" + uuid.uuid4().hex[:8]
drop_collection(c1)

print(f"\n[Setup] Creating collection '{c1}' with IVF_FLAT index...")
r = create_collection(c1, index_type="IVF_FLAT")
print(f"  Create: {json.dumps(r)}")
time.sleep(2)

print("[Setup] Inserting 10 vectors...")
r = insert_data(c1, 10)
print(f"  Insert: code={r.get('code')}")
time.sleep(2)

# Test 1a: nprobe=0
print("\n[Test 1a] search with nprobe=0 (should be rejected, valid range [1, nlist])...")
r = search(c1, [0.1, 0.2, 0.3, 0.4], search_params={"nprobe": 0})
nprobe_0_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}, data_count={len(r.get('data', []))}")
print(f"  Result: {'ILLEGAL_SUCCESS - nprobe=0 ACCEPTED!' if nprobe_0_accepted else 'Properly rejected'}")
results["nprobe=0"] = nprobe_0_accepted

# Test 1b: nprobe=-1
print("\n[Test 1b] search with nprobe=-1 (should be rejected)...")
r = search(c1, [0.1, 0.2, 0.3, 0.4], search_params={"nprobe": -1})
nprobe_neg1_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}, data_count={len(r.get('data', []))}")
print(f"  Result: {'ILLEGAL_SUCCESS - nprobe=-1 ACCEPTED!' if nprobe_neg1_accepted else 'Properly rejected'}")
results["nprobe=-1"] = nprobe_neg1_accepted

# Test 1c: nprobe=1 (valid, should work)
print("\n[Test 1c] search with nprobe=1 (valid, should succeed)...")
r = search(c1, [0.1, 0.2, 0.3, 0.4], search_params={"nprobe": 1})
print(f"  Response: code={r.get('code')}, data_count={len(r.get('data', []))}")
results["nprobe=1_valid"] = r.get("code") == 0

# Test 1d: limit=0 (should be rejected - contrast test)
print("\n[Test 1d] search with limit=0 (should be rejected - contrast test)...")
r = search(c1, [0.1, 0.2, 0.3, 0.4], limit=0)
limit_0_rejected = r.get("code") != 0
print(f"  Response: code={r.get('code')}, message={r.get('message', 'N/A')[:80]}")
print(f"  Result: {'Properly rejected' if limit_0_rejected else 'UNEXPECTED - limit=0 accepted!'}")
results["limit=0_rejected"] = limit_0_rejected

# Test 1e: Behavior comparison - nprobe=0 vs nprobe=128
print("\n[Test 1e] Behavior comparison: nprobe=0 vs nprobe=128...")
r0 = search(c1, [0.1, 0.2, 0.3, 0.4], search_params={"nprobe": 0})
r128 = search(c1, [0.1, 0.2, 0.3, 0.4], search_params={"nprobe": 128})
same_results = (r0.get("data") == r128.get("data"))
print(f"  nprobe=0:  code={r0.get('code')}, results={len(r0.get('data', []))}")
print(f"  nprobe=128: code={r128.get('code')}, results={len(r128.get('data', []))}")
print(f"  Results identical: {same_results}")
results["nprobe_0_vs_128_identical"] = same_results

drop_collection(c1)

# ============================================================
# CLUSTER 2: nlist validation in searchParams
# ============================================================
print("\n" + "=" * 70)
print("CLUSTER 2: nlist parameter validation in searchParams")
print("=" * 70)

c2 = "verify_nlist_" + uuid.uuid4().hex[:8]
drop_collection(c2)

print(f"\n[Setup] Creating collection '{c2}' with IVF_FLAT index (nlist=128)...")
r = create_collection(c2, index_type="IVF_FLAT")
print(f"  Create: {json.dumps(r)}")
time.sleep(2)

print("[Setup] Inserting 10 vectors...")
r = insert_data(c2, 10)
print(f"  Insert: code={r.get('code')}")
time.sleep(2)

# Test 2a: nlist=0 in searchParams
print("\n[Test 2a] search with searchParams.nlist=0 (should be rejected)...")
r = search(c2, [0.1, 0.2, 0.3, 0.4], search_params={"nlist": 0})
nlist_0_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}, data_count={len(r.get('data', []))}")
print(f"  Result: {'ILLEGAL_SUCCESS - nlist=0 ACCEPTED!' if nlist_0_accepted else 'Properly rejected'}")
results["nlist=0"] = nlist_0_accepted

# Test 2b: nlist=-1 in searchParams
print("\n[Test 2b] search with searchParams.nlist=-1 (should be rejected)...")
r = search(c2, [0.1, 0.2, 0.3, 0.4], search_params={"nlist": -1})
nlist_neg1_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}, data_count={len(r.get('data', []))}")
print(f"  Result: {'ILLEGAL_SUCCESS - nlist=-1 ACCEPTED!' if nlist_neg1_accepted else 'Properly rejected'}")
results["nlist=-1"] = nlist_neg1_accepted

# Test 2c: CRITICAL - Does nlist in searchParams actually affect behavior?
print("\n[Test 2c] CRITICAL: Does nlist in searchParams affect search behavior?")
print("  Comparing: searchParams.nlist=1 vs searchParams.nlist=999999...")
r1 = search(c2, [0.1, 0.2, 0.3, 0.4], search_params={"nlist": 1})
r999 = search(c2, [0.1, 0.2, 0.3, 0.4], search_params={"nlist": 999999})
nlist_affects_behavior = (r1.get("data") != r999.get("data"))
print(f"  nlist=1:     code={r1.get('code')}, results={len(r1.get('data', []))}")
print(f"  nlist=999999: code={r999.get('code')}, results={len(r999.get('data', []))}")
print(f"  Results differ: {nlist_affects_behavior}")
if nlist_affects_behavior:
    print("  >>> nlist in searchParams DOES affect search behavior - this is a REAL bug")
else:
    print("  >>> nlist in searchParams does NOT affect behavior - may be by-design ignore")
results["nlist_affects_behavior"] = nlist_affects_behavior

# Test 2d: nlist=0 vs no nlist - does nlist=0 change results?
print("\n[Test 2d] nlist=0 vs no nlist param - does nlist=0 change results?")
r_no_nlist = search(c2, [0.1, 0.2, 0.3, 0.4])
r_nlist_0 = search(c2, [0.1, 0.2, 0.3, 0.4], search_params={"nlist": 0})
nlist_0_changes = (r_no_nlist.get("data") != r_nlist_0.get("data"))
print(f"  No nlist: code={r_no_nlist.get('code')}, results={len(r_no_nlist.get('data', []))}")
print(f"  nlist=0:  code={r_nlist_0.get('code')}, results={len(r_nlist_0.get('data', []))}")
print(f"  Results differ: {nlist_0_changes}")
results["nlist_0_changes_results"] = nlist_0_changes

drop_collection(c2)

# ============================================================
# CLUSTER 3: collection.ttl.seconds validation
# ============================================================
print("\n" + "=" * 70)
print("CLUSTER 3: collection.ttl.seconds parameter validation")
print("=" * 70)

# Test 3a: ttl.seconds=-1 (should be rejected per documentation range)
print("\n[Test 3a] create collection with collection.ttl.seconds=-1...")
c3a = "verify_ttl_neg1_" + uuid.uuid4().hex[:8]
drop_collection(c3a)
r = create_collection(c3a, extra_props={"collection.ttl.seconds": -1})
ttl_neg1_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}")
if ttl_neg1_accepted:
    print(f"  ILLEGAL_SUCCESS - ttl.seconds=-1 ACCEPTED!")
    details = get_collection_details(c3a)
    props = details.get("data", {}).get("properties", {})
    print(f"  Collection properties: {json.dumps(props)}")
    has_ttl = "collection.ttl.seconds" in props or any("ttl" in str(k).lower() for k in props)
    print(f"  TTL property present: {has_ttl}")
    results["ttl=-1_has_property"] = has_ttl
else:
    print(f"  Properly rejected: {r.get('message', 'N/A')[:100]}")
    results["ttl=-1_has_property"] = False
results["ttl=-1"] = ttl_neg1_accepted
drop_collection(c3a)

# Test 3b: ttl.seconds=-100 (clearly invalid negative value)
print("\n[Test 3b] create collection with collection.ttl.seconds=-100...")
c3b = "verify_ttl_neg100_" + uuid.uuid4().hex[:8]
drop_collection(c3b)
r = create_collection(c3b, extra_props={"collection.ttl.seconds": -100})
ttl_neg100_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}")
if ttl_neg100_accepted:
    print(f"  ILLEGAL_SUCCESS - ttl.seconds=-100 ACCEPTED!")
    details = get_collection_details(c3b)
    props = details.get("data", {}).get("properties", {})
    print(f"  Collection properties: {json.dumps(props)}")
    has_ttl = "collection.ttl.seconds" in props or any("ttl" in str(k).lower() for k in props)
    print(f"  TTL property present: {has_ttl}")
    results["ttl=-100_has_property"] = has_ttl
else:
    print(f"  Properly rejected: {r.get('message', 'N/A')[:100]}")
    results["ttl=-100_has_property"] = False
results["ttl=-100"] = ttl_neg100_accepted
drop_collection(c3b)

# Test 3c: ttl.seconds=0 (edge case)
print("\n[Test 3c] create collection with collection.ttl.seconds=0...")
c3c = "verify_ttl_0_" + uuid.uuid4().hex[:8]
drop_collection(c3c)
r = create_collection(c3c, extra_props={"collection.ttl.seconds": 0})
ttl_0_accepted = r.get("code") == 0
print(f"  Response: code={r.get('code')}")
if ttl_0_accepted:
    details = get_collection_details(c3c)
    props = details.get("data", {}).get("properties", {})
    print(f"  Collection properties: {json.dumps(props)}")
results["ttl=0"] = ttl_0_accepted
drop_collection(c3c)

# Test 3d: alter_properties with ttl.seconds=-100 (should be rejected - contrast)
print("\n[Test 3d] alter_properties with ttl.seconds=-100 (should be rejected)...")
c3d = "verify_ttl_alter_" + uuid.uuid4().hex[:8]
drop_collection(c3d)
r = create_collection(c3d)
time.sleep(1)
r = requests.post(f"{BASE}/v2/vectordb/collections/alter_properties", headers=HEADERS,
                  json={"collectionName": c3d, "properties": {"collection.ttl.seconds": -100}}).json()
alter_rejected = r.get("code") != 0
print(f"  Response: code={r.get('code')}, message={r.get('message', 'N/A')[:100]}")
print(f"  Result: {'Properly rejected' if alter_rejected else 'UNEXPECTED - alter also accepts -100!'}")
results["alter_ttl=-100_rejected"] = alter_rejected
drop_collection(c3d)

# ============================================================
# SUMMARY
# ============================================================
print("\n" + "=" * 70)
print("VERIFICATION SUMMARY")
print("=" * 70)

print("\n--- Cluster 1: nprobe ---")
print(f"  nprobe=0 accepted (ILLEGAL_SUCCESS): {results.get('nprobe=0', 'N/A')}")
print(f"  nprobe=-1 accepted (ILLEGAL_SUCCESS): {results.get('nprobe=-1', 'N/A')}")
print(f"  nprobe=1 valid (should succeed): {results.get('nprobe=1_valid', 'N/A')}")
print(f"  limit=0 rejected (contrast): {results.get('limit=0_rejected', 'N/A')}")
print(f"  nprobe=0 vs nprobe=128 identical: {results.get('nprobe_0_vs_128_identical', 'N/A')}")

print("\n--- Cluster 2: nlist ---")
print(f"  nlist=0 accepted (ILLEGAL_SUCCESS): {results.get('nlist=0', 'N/A')}")
print(f"  nlist=-1 accepted (ILLEGAL_SUCCESS): {results.get('nlist=-1', 'N/A')}")
print(f"  nlist AFFECTS search behavior: {results.get('nlist_affects_behavior', 'N/A')}")
print(f"  nlist=0 changes results vs no nlist: {results.get('nlist_0_changes_results', 'N/A')}")

print("\n--- Cluster 3: collection.ttl.seconds ---")
print(f"  ttl=-1 accepted (ILLEGAL_SUCCESS): {results.get('ttl=-1', 'N/A')}")
if results.get('ttl=-1'):
    print(f"  ttl=-1 property actually set: {results.get('ttl=-1_has_property', 'N/A')}")
print(f"  ttl=-100 accepted (ILLEGAL_SUCCESS): {results.get('ttl=-100', 'N/A')}")
if results.get('ttl=-100'):
    print(f"  ttl=-100 property actually set: {results.get('ttl=-100_has_property', 'N/A')}")
print(f"  ttl=0 accepted: {results.get('ttl=0', 'N/A')}")
print(f"  alter_properties ttl=-100 rejected: {results.get('alter_ttl=-100_rejected', 'N/A')}")

confirmed = sum(1 for k in ["nprobe=0", "nprobe=-1", "nlist=0", "nlist=-1", "ttl=-1"] if results.get(k))
print(f"\nTotal confirmed ILLEGAL_SUCCESS defects: {confirmed}/5")
print(f"nlist behavioral impact: {'REAL BUG (affects results)' if results.get('nlist_affects_behavior') else 'NEEDS ARGUMENT (no behavioral impact)'}")
