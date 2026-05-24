import requests, time, json, uuid

BASE = 'http://localhost:19530'
H = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'v_cdc_' + uuid.uuid4().hex[:8]

def get_dim(describe_data):
    """Extract dimension from describe response"""
    for f in describe_data.get('fields', []):
        if f.get('name') == 'vector':
            for p in f.get('params', []):
                if p.get('key') == 'dim':
                    return p.get('value')
    return None

print(f"=== CDC dim=None test on v2.6.16 ===\n")

# Step 1: Create
r1 = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=H, json={
    'collectionName': c, 'dbName': 'default',
    'schema': {'autoID': False, 'enableDynamicField': True, 'fields': [
        {'fieldName': 'id', 'dataType': 'Int64', 'isPrimary': True},
        {'fieldName': 'vector', 'dataType': 'FloatVector', 'elementTypeParams': {'dim': 8}}
    ]},
    'indexParams': [{'fieldName': 'vector', 'metricType': 'COSINE', 'indexType': 'AUTOINDEX'}]
})
print(f'1. CREATE dim=8: code={r1.json().get("code")}')
dim1 = get_dim(r1.json().get('data',{}))
print(f'   dim after create: {dim1}')
time.sleep(2)

# Step 2: Drop
r2 = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=H, json={
    'collectionName': c, 'dbName': 'default'
})
print(f'2. DROP: code={r2.json().get("code")}')
time.sleep(1)

# Step 3: Recreate same name with dim=8
r3 = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=H, json={
    'collectionName': c, 'dbName': 'default',
    'schema': {'autoID': False, 'enableDynamicField': True, 'fields': [
        {'fieldName': 'id', 'dataType': 'Int64', 'isPrimary': True},
        {'fieldName': 'vector', 'dataType': 'FloatVector', 'elementTypeParams': {'dim': 8}}
    ]},
    'indexParams': [{'fieldName': 'vector', 'metricType': 'COSINE', 'indexType': 'AUTOINDEX'}]
})
print(f'3. RECREATE dim=8: code={r3.json().get("code")}')
time.sleep(2)

# Step 4: Describe
r4 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=H, json={
    'collectionName': c, 'dbName': 'default'
})
d4 = r4.json()
dim4 = get_dim(d4.get('data',{}))
print(f'4. DESCRIBE: code={d4.get("code")}')
print(f'   dim after CDC: {dim4}')
print(f'   fields: {json.dumps([(f.get("name"), [p.get("value") for p in f.get("params",[]) if p.get("key")=="dim"]) for f in d4.get("data",{}).get("fields",[])])}')

# Step 5: Also create a fresh collection to compare
c2 = 'v_fresh_' + uuid.uuid4().hex[:8]
r5 = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=H, json={
    'collectionName': c2, 'dbName': 'default',
    'schema': {'autoID': False, 'enableDynamicField': True, 'fields': [
        {'fieldName': 'id', 'dataType': 'Int64', 'isPrimary': True},
        {'fieldName': 'vector', 'dataType': 'FloatVector', 'elementTypeParams': {'dim': 8}}
    ]},
    'indexParams': [{'fieldName': 'vector', 'metricType': 'COSINE', 'indexType': 'AUTOINDEX'}]
})
time.sleep(2)
r6 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=H, json={
    'collectionName': c2, 'dbName': 'default'
})
dim_fresh = get_dim(r6.json().get('data',{}))
print(f'5. FRESH collection dim: {dim_fresh}')

# Verdict
print(f'\n=== VERDICT ===')
if dim4 != dim_fresh:
    print(f'CONFIRMED: CDC dim={dim4} vs fresh dim={dim_fresh} -> BUG')
elif dim4 is None and dim_fresh is not None:
    print(f'CONFIRMED: CDC dim=None (BUG)')
elif dim4 == dim_fresh:
    print(f'NOT REPRODUCED: both return dim={dim4} -> fixed?')
else:
    print(f'UNEXPECTED: CDC={dim4}, fresh={dim_fresh}')

# cleanup
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=H, json={'collectionName': c, 'dbName': 'default'})
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=H, json={'collectionName': c2, 'dbName': 'default'})
print('DONE')
