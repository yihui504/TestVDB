import requests, json, uuid, time

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    try:
        return r.json()
    except Exception:
        return {"code": -1, "raw": r.text[:200]}

results = {}

print("=" * 60)
print("TEST 1: IVF_FLAT nprobe=0/-1 in search")
print("=" * 60)
c1 = 'verify_nprobe_ivf_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c1,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "L2", "indexType": "IVF_FLAT", "params": {"nlist": 128}}]
})
time.sleep(2)
api('/v2/vectordb/entities/insert', {"collectionName": c1, "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}, {"id": 2, "vector": [0.5, 0.6, 0.7, 0.8]}]})
time.sleep(1)
r1 = api('/v2/vectordb/entities/search', {"collectionName": c1, "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 5, "searchParams": {"nprobe": 0}})
print(f"  IVF_FLAT nprobe=0: {json.dumps(r1)}")
results['ivf_nprobe_0'] = r1.get('code', -1)

r2 = api('/v2/vectordb/entities/search', {"collectionName": c1, "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 5, "searchParams": {"nprobe": -1}})
print(f"  IVF_FLAT nprobe=-1: {json.dumps(r2)}")
results['ivf_nprobe_neg1'] = r2.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c1})

print()
print("=" * 60)
print("TEST 2: HNSW ef=0/-1 in search")
print("=" * 60)
c2 = 'verify_ef_search_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c2,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "L2", "indexType": "HNSW", "params": {"efConstruction": 128, "M": 16}}]
})
time.sleep(2)
api('/v2/vectordb/entities/insert', {"collectionName": c2, "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]})
time.sleep(1)
r3 = api('/v2/vectordb/entities/search', {"collectionName": c2, "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 5, "searchParams": {"ef": -1}})
print(f"  HNSW ef=-1: {json.dumps(r3)}")
results['hnsw_ef_neg1'] = r3.get('code', -1)

r4 = api('/v2/vectordb/entities/search', {"collectionName": c2, "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 5, "searchParams": {"ef": 0}})
print(f"  HNSW ef=0: {json.dumps(r4)}")
results['hnsw_ef_0'] = r4.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c2})

print()
print("=" * 60)
print("TEST 3: drop_index with empty indexName")
print("=" * 60)
c3 = 'verify_dropidx_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c3,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
r5 = api('/v2/vectordb/indexes/drop', {"collectionName": c3, "indexName": ""})
print(f"  drop_index empty name: {json.dumps(r5)}")
results['drop_idx_empty'] = r5.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c3})

print()
print("=" * 60)
print("TEST 4: create_partition with empty partitionName")
print("=" * 60)
c4 = 'verify_part_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c4,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
r6 = api('/v2/vectordb/partitions/create', {"collectionName": c4, "partitionName": ""})
print(f"  create_partition empty name: {json.dumps(r6)}")
results['create_part_empty'] = r6.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c4})

print()
print("=" * 60)
print("TEST 5: search with nonexistent collectionName")
print("=" * 60)
r7 = api('/v2/vectordb/entities/search', {"collectionName": "nonexistent_xyz_12345", "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 5})
print(f"  search nonexistent: {json.dumps(r7)}")
results['search_nonexistent'] = r7.get('code', -1)

print()
print("=" * 60)
print("TEST 6: insert with wrong vector dimension")
print("=" * 60)
c5 = 'verify_wrongdim_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c5,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
r8 = api('/v2/vectordb/entities/insert', {"collectionName": c5, "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4, 0.5]}]})
print(f"  insert dim=5 into dim=4: {json.dumps(r8)}")
results['insert_wrong_dim'] = r8.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c5})

print()
print("=" * 60)
print("TEST 7: search with mismatched vector dimension")
print("=" * 60)
c6 = 'verify_searchdim_' + uuid.uuid4().hex[:8]
api('/v2/vectordb/collections/create', {
    "collectionName": c6,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
time.sleep(1)
api('/v2/vectordb/entities/insert', {"collectionName": c6, "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]})
time.sleep(1)
r9 = api('/v2/vectordb/entities/search', {"collectionName": c6, "data": [[0.1, 0.2, 0.3, 0.4, 0.5]], "limit": 5})
print(f"  search dim=5 into dim=4: {json.dumps(r9)}")
results['search_wrong_dim'] = r9.get('code', -1)
api('/v2/vectordb/collections/drop', {"collectionName": c6})

print()
print("=" * 60)
print("SUMMARY")
print("=" * 60)
for k, v in results.items():
    status = "BUG (accepted invalid)" if v == 0 else f"OK (rejected, code={v})"
    print(f"  {k}: {status}")
