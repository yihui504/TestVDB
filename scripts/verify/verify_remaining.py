import requests, json, uuid, time

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    return r.json()

results = {}

print("=" * 60)
print("P0-B: 32768-dim collection creation")
print("=" * 60)
c1 = 'verify_dim_' + uuid.uuid4().hex[:8]
r = api('/v2/vectordb/collections/create', {
    "collectionName": c1,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 32768}}
    ]}
})
print(f"  Create dim=32768: {json.dumps(r)}")
results['P0-B_32768dim'] = r.get('code', -1)
if r.get('code') == 0:
    api('/v2/vectordb/collections/drop', {"collectionName": c1})
    print("  CONFIRMED: 32768-dim collection created successfully (should be rejected)")
else:
    print("  NOT REPRODUCED: 32768-dim rejected as expected")

print()
print("=" * 60)
print("P0-A: REST create_index without prior index in collection")
print("=" * 60)
c2 = 'verify_idx_' + uuid.uuid4().hex[:8]
r_create = api('/v2/vectordb/collections/create', {
    "collectionName": c2,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]}
})
print(f"  Create collection (no indexParams): {json.dumps(r_create)}")
time.sleep(1)
r_idx = api('/v2/vectordb/indexes/create', {
    "collectionName": c2,
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "IVF_FLAT", "params": {"nlist": 128}}]
})
print(f"  Create index via REST: {json.dumps(r_idx)}")
results['P0-A_REST_create_index'] = r_idx.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c2})

print()
print("=" * 60)
print("P1-A: nprobe=-1 in search")
print("=" * 60)
c3 = 'verify_nprobe_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c3,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(2)
api('/v2/vectordb/entities/insert', {
    "collectionName": c3,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
time.sleep(1)
r_search = api('/v2/vectordb/entities/search', {
    "collectionName": c3,
    "data": [[0.1, 0.2, 0.3, 0.4]],
    "limit": 5,
    "searchParams": {"nprobe": -1}
})
print(f"  Search with nprobe=-1: {json.dumps(r_search)}")
results['P1-A_nprobe_neg1'] = r_search.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c3})

print()
print("=" * 60)
print("P1-B: collectionName='' in create")
print("=" * 60)
r_empty = api('/v2/vectordb/collections/create', {
    "collectionName": "",
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]}
})
print(f"  Create with empty name: {json.dumps(r_empty)}")
results['P1-B_empty_name'] = r_empty.get('code', -1)

print()
print("=" * 60)
print("SUMMARY")
print("=" * 60)
for k, v in results.items():
    status = "CONFIRMED (bug exists)" if v == 0 else "NOT REPRODUCED"
    print(f"  {k}: code={v} -> {status}")
