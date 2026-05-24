import requests, time, json, uuid

BASE = 'http://localhost:19530'
H = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'v_dup_' + uuid.uuid4().hex[:8]

# create
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=H, json={
    'collectionName': c, 'dbName': 'default',
    'schema': {'autoID': False, 'enableDynamicField': True, 'fields': [
        {'fieldName': 'id', 'dataType': 'Int64', 'isPrimary': True},
        {'fieldName': 'vector', 'dataType': 'FloatVector', 'elementTypeParams': {'dim': 4}}
    ]},
    'indexParams': [{'fieldName': 'vector', 'metricType': 'COSINE', 'indexType': 'AUTOINDEX'}]
})
print(f'CREATE: code={r.json().get("code")}')
time.sleep(2)

# first insert
r1 = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=H, json={
    'collectionName': c, 'dbName': 'default',
    'data': [{'id': 1, 'vector': [0.1,0.2,0.3,0.4]}]
})
d1 = r1.json()
print(f'INSERT 1: code={d1.get("code")}, insertCount={d1.get("data",{}).get("insertCount","N/A")}')
time.sleep(1)

# second insert (duplicate id)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=H, json={
    'collectionName': c, 'dbName': 'default',
    'data': [{'id': 1, 'vector': [0.9,0.8,0.7,0.6]}]
})
d2 = r2.json()
print(f'INSERT 2 (dup id): code={d2.get("code")}, insertCount={d2.get("data",{}).get("insertCount","N/A")}')
print(f'  Full: {json.dumps(d2)}')
time.sleep(1)

# describe
rd = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=H, json={
    'collectionName': c, 'dbName': 'default'
})
dd = rd.json()
print(f'DESCRIBE: code={dd.get("code")}, rowCount={dd.get("data",{}).get("rowCount","N/A")}')
print(f'  Full: {json.dumps(dd)}')

# cleanup
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=H, json={
    'collectionName': c, 'dbName': 'default'
})
print('DONE - cleaned up')
