import requests, json, uuid, time, math

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    try:
        return r.json()
    except Exception:
        return {"code": -1, "raw": r.text[:200]}

def api_raw(path, body_str):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, data=body_str)
    try:
        return r.json()
    except Exception:
        return {"code": -1, "raw": r.text[:200]}

results = {}

print("=" * 60)
print("TEST 1: Negative TTL via alter endpoint")
print("=" * 60)
c1 = 'verify_ttl_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c1,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
r1 = api('/v2/vectordb/collections/alter', {
    "collectionName": c1,
    "properties": {"collection.ttl.seconds": -1}
})
print(f"  Alter TTL=-1: {json.dumps(r1)}")
results['negative_ttl'] = r1.get('code', -1)

r1b = api('/v2/vectordb/collections/alter', {
    "collectionName": c1,
    "properties": {"collection.ttl.seconds": 0}
})
print(f"  Alter TTL=0: {json.dumps(r1b)}")
results['zero_ttl'] = r1b.get('code', -1)

api('/v2/vectordb/collections/drop', {"collectionName": c1})

print()
print("=" * 60)
print("TEST 2: NaN/Inf in vector insert (raw JSON)")
print("=" * 60)
c2 = 'verify_nan_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c2,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)

r2a = api_raw('/v2/vectordb/entities/insert',
    json.dumps({"collectionName": c2, "data": [{"id": 1, "vector": [float('nan'), 0.2, 0.3, 0.4]}]}, allow_nan=True))
print(f"  Insert NaN: {json.dumps(r2a)}")
results['insert_nan'] = r2a.get('code', -1)

r2b = api_raw('/v2/vectordb/entities/insert',
    json.dumps({"collectionName": c2, "data": [{"id": 2, "vector": [float('inf'), 0.2, 0.3, 0.4]}]}, allow_nan=True))
print(f"  Insert Inf: {json.dumps(r2b)}")
results['insert_inf'] = r2b.get('code', -1)

r2c = api_raw('/v2/vectordb/entities/insert',
    json.dumps({"collectionName": c2, "data": [{"id": 3, "vector": [float('-inf'), 0.2, 0.3, 0.4]}]}, allow_nan=True))
print(f"  Insert -Inf: {json.dumps(r2c)}")
results['insert_neg_inf'] = r2c.get('code', -1)

api('/v2/vectordb/collections/drop', {"collectionName": c2})

print()
print("=" * 60)
print("TEST 3: Empty collectionName in aliases/list")
print("=" * 60)
r3 = api('/v2/vectordb/aliases/list', {"collectionName": ""})
print(f"  Aliases list with empty name: {json.dumps(r3)}")
results['alias_empty_name'] = r3.get('code', -1)

print()
print("=" * 60)
print("TEST 4: Empty ID array in entities/get")
print("=" * 60)
c4 = 'verify_get_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c4,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
api('/v2/vectordb/entities/insert', {
    "collectionName": c4,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
time.sleep(1)
r4 = api('/v2/vectordb/entities/get', {
    "collectionName": c4,
    "id": []
})
print(f"  Get with empty id array: {json.dumps(r4)}")
results['get_empty_ids'] = r4.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c4})

print()
print("=" * 60)
print("TEST 5: Negative limit in search")
print("=" * 60)
c5 = 'verify_neglimit_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c5,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
api('/v2/vectordb/entities/insert', {
    "collectionName": c5,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
time.sleep(1)
r5 = api('/v2/vectordb/entities/search', {
    "collectionName": c5,
    "data": [[0.1, 0.2, 0.3, 0.4]],
    "limit": -1
})
print(f"  Search limit=-1: {json.dumps(r5)}")
results['search_neg_limit'] = r5.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c5})

print()
print("=" * 60)
print("TEST 6: Negative offset in search")
print("=" * 60)
c6 = 'verify_negoffset_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c6,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
api('/v2/vectordb/entities/insert', {
    "collectionName": c6,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
time.sleep(1)
r6 = api('/v2/vectordb/entities/search', {
    "collectionName": c6,
    "data": [[0.1, 0.2, 0.3, 0.4]],
    "limit": 5,
    "offset": -1
})
print(f"  Search offset=-1: {json.dumps(r6)}")
results['search_neg_offset'] = r6.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c6})

print()
print("=" * 60)
print("TEST 7: Very large limit in search")
print("=" * 60)
c7 = 'verify_largelimit_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c7,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
api('/v2/vectordb/entities/insert', {
    "collectionName": c7,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
time.sleep(1)
r7 = api('/v2/vectordb/entities/search', {
    "collectionName": c7,
    "data": [[0.1, 0.2, 0.3, 0.4]],
    "limit": 999999
})
print(f"  Search limit=999999: {json.dumps(r7)}")
results['search_large_limit'] = r7.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c7})

print()
print("=" * 60)
print("SUMMARY")
print("=" * 60)
for k, v in results.items():
    status = "BUG (accepted invalid)" if v == 0 else f"OK (rejected, code={v})"
    print(f"  {k}: {status}")
