import requests, sys, uuid, time, json

BASE = 'http://localhost:19530'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

def api(path, body):
    r = requests.post(f'{BASE}{path}', headers=HEADERS, json=body)
    return r.json()

def get_dim(describe_data):
    for f in describe_data.get('fields', []):
        if f.get('fieldName') == 'vector' or f.get('name') == 'vector':
            params = f.get('params', [])
            if isinstance(params, list):
                for p in params:
                    if p.get('key') == 'dim':
                        return p.get('value')
            elif isinstance(params, dict):
                return params.get('dim')
            etp = f.get('elementTypeParams', {})
            if isinstance(etp, dict):
                return etp.get('dim')
    return None

print("=" * 60)
print("DEFECT 4 VERIFICATION: Create-Drop-Create dimension loss")
print("=" * 60)

c = 'verify_cdc_' + uuid.uuid4().hex[:8]
print(f"\n[1] Creating collection with dim=8: {c}")
r1 = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 8}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
print(f"  Create: {json.dumps(r1)}")

time.sleep(2)

print(f"\n[2] Describe after first create")
d1 = api('/v2/vectordb/collections/describe', {"collectionName": c})
dim1 = get_dim(d1.get('data', {})) if 'data' in d1 else None
print(f"  Dimension after first create: {dim1}")

print(f"\n[3] Dropping collection")
api('/v2/vectordb/collections/drop', {"collectionName": c})
print(f"  Dropped")

time.sleep(2)

print(f"\n[4] Re-creating collection with same name, dim=8")
r2 = api('/v2/vectordb/collections/create', {
    "collectionName": c,
    "schema": {"autoID": False, "enableDynamicField": True, "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": True},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 8}}
    ]},
    "indexParams": [{"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}]
})
print(f"  Create: {json.dumps(r2)}")

time.sleep(2)

print(f"\n[5] Describe after second create (checking dimension)")
d2 = api('/v2/vectordb/collections/describe', {"collectionName": c})
dim2 = get_dim(d2.get('data', {})) if 'data' in d2 else None
print(f"  Dimension after second create: {dim2}")

if 'data' in d2:
    print(f"\n[6] Full vector field info:")
    for f in d2['data'].get('fields', []):
        if f.get('fieldName') == 'vector' or f.get('name') == 'vector':
            print(f"  {json.dumps(f, indent=2)}")

print(f"\n[7] Cleanup")
api('/v2/vectordb/collections/drop', {"collectionName": c})
print(f"  Dropped")

print("\n" + "=" * 60)
print("DEFECT 4 VERDICT:")
if dim2 is None or str(dim2) == 'None':
    print("  CONFIRMED: Dimension is None/missing after create-drop-create (BUG)")
elif str(dim2) == '8':
    print("  NOT REPRODUCED: Dimension correctly returns 8 after create-drop-create")
else:
    print(f"  UNEXPECTED: Dimension = {dim2}")
print("=" * 60)
