import requests, json

BASE = 'http://localhost:19530'
H = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}

print("=== collectionName='' tests on Milvus v2.6.16 ===\n")

# Test 1: create with empty collectionName
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=H, json={
    'collectionName': '', 'dbName': 'default',
    'dimension': 4, 'metricType': 'L2'
})
print(f'1. CREATE collectionName="": code={r.json().get("code")}, msg={r.json().get("message","N/A")}')

# Test 2: describe with empty collectionName
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=H, json={
    'collectionName': '', 'dbName': 'default'
})
print(f'2. DESCRIBE collectionName="": code={r2.json().get("code")}, msg={r2.json().get("message","N/A")}')

# Test 3: aliases/list with empty collectionName
r3 = requests.post(f'{BASE}/v2/vectordb/aliases/list', headers=H, json={
    'collectionName': '', 'dbName': 'default'
})
d3 = r3.json()
print(f'3. ALIASES/LIST collectionName="": code={d3.get("code")}, msg={d3.get("message","N/A")}, data={d3.get("data")}')

# Test 4: drop with empty collectionName
r4 = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=H, json={
    'collectionName': '', 'dbName': 'default'
})
print(f'4. DROP collectionName="": code={r4.json().get("code")}, msg={r4.json().get("message","N/A")}')

# Test 5: load with empty collectionName
r5 = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=H, json={
    'collectionName': '', 'dbName': 'default'
})
print(f'5. LOAD collectionName="": code={r5.json().get("code")}, msg={r5.json().get("message","N/A")}')

print("\n=== DONE ===")
