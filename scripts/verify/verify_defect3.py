import requests, sys, uuid, time, json

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    return r.json()

print("=" * 60)
print("DEFECT 3 VERIFICATION: Duplicate ID insert count")
print("=" * 60)

c = 'verify_dup_' + uuid.uuid4().hex[:8]
print(f"\n[1] Creating collection: {c}")
r = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
print(f"  Create: {json.dumps(r)}")

time.sleep(2)

print(f"\n[2] First insert (id=1, vector=[0.1,0.2,0.3,0.4])")
r1 = api('/v2/vectordb/entities/insert', {
    "collectionName": c,
    "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
print(f"  Response: {json.dumps(r1)}")
if 'data' in r1:
    print(f"  insertCount = {r1['data'].get('insertCount', 'N/A')}")

time.sleep(1)

print(f"\n[3] Second insert (same id=1, different vector=[0.9,0.8,0.7,0.6])")
r2 = api('/v2/vectordb/entities/insert', {
    "collectionName": c,
    "data": [{"id": 1, "vector": [0.9, 0.8, 0.7, 0.6]}]
})
print(f"  Response: {json.dumps(r2)}")
if 'data' in r2:
    print(f"  insertCount = {r2['data'].get('insertCount', 'N/A')}")

print(f"\n[4] Query to verify actual data")
r3 = api('/v2/vectordb/entities/query', {
    "collectionName": c,
    "filter": "id == 1",
    "outputFields": ["id", "vector"]
})
print(f"  Query result: {json.dumps(r3)}")

print(f"\n[5] Get collection stats")
r4 = api('/v2/vectordb/collections/describe', {"collectionName": c})
print(f"  Describe: {json.dumps(r4)}")

print(f"\n[6] Cleanup")
api('/v2/vectordb/collections/drop', {"collectionName": c})
print(f"  Dropped")

print("\n" + "=" * 60)
print("DEFECT 3 VERDICT:")
if 'data' in r2 and r2['data'].get('insertCount') == -1:
    print("  CONFIRMED: insertCount=-1 for duplicate ID (BUG)")
elif 'data' in r2 and r2['data'].get('insertCount') == 1:
    print("  NOT A BUG: insertCount=1 (upsert semantics, by design per #49849)")
else:
    print(f"  UNEXPECTED: insertCount={r2.get('data', {}).get('insertCount', 'N/A')}")
print("=" * 60)
