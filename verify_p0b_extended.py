import requests, json, uuid, time

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    return r.json()

print("=" * 60)
print("P0-B Extended: Dimension upper bound validation")
print("=" * 60)

for dim in [32768, 32769, 65536, 100000]:
    c = f'verify_dim_{uuid.uuid4().hex[:8]}'
    r = api('/v2/vectordb/collections/create', {
        "collectionName": c,
        "schema": {"autoID": False, "enableDynamicField": True, "fields": [
            {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
            {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": dim}}
        ]}
    })
    status = "ACCEPTED" if r.get('code') == 0 else f"REJECTED (code={r.get('code')}, msg={r.get('message', '')[:80]})"
    print(f"  dim={dim}: {status}")
    if r.get('code') == 0:
        api('/v2/vectordb/collections/drop', {"collectionName": c})

print()
print("=" * 60)
print("P0-B Extended: OOM risk test (insert into 32768-dim)")
print("=" * 60)
c = 'verify_oom_' + uuid.uuid4().hex[:8]
r = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 32768}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
print(f"  Create dim=32768: code={r.get('code')}")
if r.get('code') == 0:
    time.sleep(2)
    vec = [0.01] * 32768
    r2 = api('/v2/vectordb/entities/insert', {
        "collectionName": c,
        "data": [{"id": 1, "vector": vec}]
    })
    print(f"  Insert 1 row dim=32768: code={r2.get('code')}, msg={r2.get('message', 'N/A')[:100]}")
    api('/v2/vectordb/collections/drop', {"collectionName": c})

print()
print("=" * 60)
print("P0-B Extended: Negative dimension")
print("=" * 60)
c = 'verify_negdim_' + uuid.uuid4().hex[:8]
r = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": -1}}
    ]}
})
print(f"  dim=-1: code={r.get('code')}, msg={r.get('message', 'N/A')[:120]}")

print()
print("=" * 60)
print("P0-B Extended: Zero dimension")
print("=" * 60)
c = 'verify_zerodim_' + uuid.uuid4().hex[:8]
r = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 0}}
    ]}
})
print(f"  dim=0: code={r.get('code')}, msg={r.get('message', 'N/A')[:120]}")
