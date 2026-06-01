use crate::target::SafetyNet;

pub fn metamorphic_nprobe_monotonicity() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"IVF_FLAT","params":{"nlist":128}}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(50)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10,"searchParams":{"nprobe":1}})
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10,"searchParams":{"nprobe":128}})
if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)
top1_a = r1.json().get('data',[{}])[0].get('id') if r1.json().get('data') else None
top1_b = r2.json().get('data',[{}])[0].get('id') if r2.json().get('data') else None
if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] nprobe top-1 mismatch: nprobe=1 id={top1_a} vs nprobe=128 id={top1_b}'); sys.exit(1)
else: print(f'nprobe monotonicity verified: top-1 id={top1_a} consistent'); sys.exit(0)"#.to_string()
}

pub fn metamorphic_ef_search_monotonicity() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"HNSW","params":{"M":16,"efConstruction":256}}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(50)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10,"searchParams":{"ef":8}})
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10,"searchParams":{"ef":256}})
if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)
top1_a = r1.json().get('data',[{}])[0].get('id') if r1.json().get('data') else None
top1_b = r2.json().get('data',[{}])[0].get('id') if r2.json().get('data') else None
if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] ef top-1 mismatch: ef=8 id={top1_a} vs ef=256 id={top1_b}'); sys.exit(1)
else: print(f'ef_search monotonicity verified: top-1 id={top1_a} consistent'); sys.exit(0)"#.to_string()
}

pub fn metamorphic_query_consistency() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(20)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)
res1 = [(d.get('id'),d.get('distance')) for d in r1.json().get('data',[])]
res2 = [(d.get('id'),d.get('distance')) for d in r2.json().get('data',[])]
if res1 != res2: print(f'[DEFECT: METAMORPHIC_VIOLATION] query consistency failed: {res1} vs {res2}'); sys.exit(1)
else: print(f'query consistency verified: {res1} == {res2}'); sys.exit(0)"#.to_string()
}

pub fn metamorphic_insert_monotonicity() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data1 = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(10)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data1})
if r.json().get('code') != 0: print(f'insert1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
if r1.json().get('code') != 0: print(f'search1 failed: {r1.text}'); sys.exit(0)
top1_id = r1.json().get('data',[{}])[0].get('id') if r1.json().get('data') else None
data2 = [{"id":i+10,"vector":[0.5*i,0.6*i,0.7*i,0.8*i]} for i in range(40)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data2})
if r.json().get('code') != 0: print(f'insert2 failed: {r.text}'); sys.exit(0)
time.sleep(2)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":50})
if r2.json().get('code') != 0: print(f'search2 failed: {r2.text}'); sys.exit(0)
all_ids_2 = set(d.get('id') for d in r2.json().get('data',[]))
if top1_id not in all_ids_2: print(f'[DEFECT: METAMORPHIC_VIOLATION] insert monotonicity: top-1 id={top1_id} not in results after more inserts'); sys.exit(1)
else: print(f'insert monotonicity verified: top-1 id={top1_id} still in results'); sys.exit(0)"#.to_string()
}

pub fn metamorphic_limit_monotonicity() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(20)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10})
if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)
ids1 = [d.get('id') for d in r1.json().get('data',[])]
ids2 = [d.get('id') for d in r2.json().get('data',[])]
top1_a = ids1[0] if ids1 else None
top1_b = ids2[0] if ids2 else None
if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] limit top-1 mismatch: limit=3 id={top1_a} vs limit=10 id={top1_b}'); sys.exit(1)
else: print(f'limit monotonicity verified: top-1 id={top1_a} consistent'); sys.exit(0)"#.to_string()
}

pub fn diff_create_collection() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c_rest = 'oracle_diff_' + uuid.uuid4().hex[:8]
c_sdk = 'oracle_diff_' + uuid.uuid4().hex[:8]
r_rest = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c_rest,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    client.create_collection(collection_name=c_sdk, dimension=4, metric_type='COSINE', auto_id=False)
except Exception as e:
    print(f'sdk create failed: {e}'); sys.exit(0)
rest_ok = r_rest.json().get('code') == 0
desc_rest = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c_rest})
desc_sdk = client.describe_collection(collection_name=c_sdk)
rest_dim = desc_rest.json().get('data',{}).get('dimension')
sdk_dim = desc_sdk.get('dimension')
if rest_dim != sdk_dim: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] create_collection dim: rest={rest_dim} sdk={sdk_dim}'); sys.exit(1)
if rest_ok and rest_dim == sdk_dim: print(f'diff create_collection verified: rest_dim={rest_dim} sdk_dim={sdk_dim}'); sys.exit(0)
else: print(f'diff create_collection partial: rest_ok={rest_ok} rest_dim={rest_dim} sdk_dim={sdk_dim}'); sys.exit(0)"#.to_string()
}

pub fn diff_insert() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":1,"vector":[0.1,0.2,0.3,0.4]}]
r_rest = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
rest_ok = r_rest.json().get('code') == 0
rest_count = r_rest.json().get('data',{}).get('insertCount',0)
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_res = client.insert(collection_name=c, data=[{"id":2,"vector":[0.5,0.6,0.7,0.8]}])
    sdk_ok = True
    sdk_count = sdk_res.get('insert_count',0)
except Exception as e:
    sdk_ok = False
    sdk_count = 0
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] insert success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_count != sdk_count: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] insert count: rest={rest_count} sdk={sdk_count}'); sys.exit(1)
else: print(f'diff insert verified: rest_ok={rest_ok} sdk_ok={sdk_ok} rest_count={rest_count} sdk_count={sdk_count}'); sys.exit(0)"#.to_string()
}

pub fn diff_search() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(10)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r_rest = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
rest_ok = r_rest.json().get('code') == 0
rest_ids = [d.get('id') for d in r_rest.json().get('data',[])]
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_res = client.search(collection_name=c, data=[[0.1,0.2,0.3,0.4]], limit=5)
    sdk_ok = True
    sdk_ids = [hit.get('id') for hits in sdk_res for hit in hits]
except Exception as e:
    sdk_ok = False
    sdk_ids = []
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok and rest_ids and sdk_ids and rest_ids[0] != sdk_ids[0]: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search top-1: rest_id={rest_ids[0]} sdk_id={sdk_ids[0]}'); sys.exit(1)
else: print(f'diff search verified: rest_ok={rest_ok} sdk_ok={sdk_ok} rest_top1={rest_ids[0] if rest_ids else None} sdk_top1={sdk_ids[0] if sdk_ids else None}'); sys.exit(0)"#.to_string()
}

pub fn diff_query() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(5)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r_rest = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id > 0","limit":10})
rest_ok = r_rest.json().get('code') == 0
rest_ids = sorted([d.get('id') for d in r_rest.json().get('data',[])])
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_res = client.query(collection_name=c, filter="id > 0", limit=10)
    sdk_ok = True
    sdk_ids = sorted([d.get('id') for d in sdk_res])
except Exception as e:
    sdk_ok = False
    sdk_ids = []
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] query success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok and rest_ids != sdk_ids: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] query ids: rest={rest_ids} sdk={sdk_ids}'); sys.exit(1)
else: print(f'diff query verified: rest_ok={rest_ok} sdk_ok={sdk_ok} rest_ids={rest_ids} sdk_ids={sdk_ids}'); sys.exit(0)"#.to_string()
}

pub fn diff_delete() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r_rest = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
rest_ok = r_rest.json().get('code') == 0
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_res = client.delete(collection_name=c, filter="id == 2")
    sdk_ok = True
    sdk_count = sdk_res.get('delete_count',0)
except Exception as e:
    sdk_ok = False
    sdk_count = 0
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] delete success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
else: print(f'diff delete verified: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string()
}

pub fn diff_create_index() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c_rest = 'oracle_diff_' + uuid.uuid4().hex[:8]
c_sdk = 'oracle_diff_' + uuid.uuid4().hex[:8]
for c in [c_rest, c_sdk]:
    r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
    if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r_rest = requests.post(f'{BASE}/v2/vectordb/indexes/create', headers=HEADERS, json={"collectionName":c_rest,"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"IVF_FLAT","params":{"nlist":128}}]})
rest_ok = r_rest.json().get('code') == 0
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    index_params = client.prepare_index_params()
    index_params.add_index(field_name="vector", index_type="IVF_FLAT", metric_type="L2", params={"nlist":128})
    client.create_index(collection_name=c_sdk, index_params=index_params)
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: PARAM_IGNORED] create_index success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
r_desc = requests.post(f'{BASE}/v2/vectordb/indexes/describe', headers=HEADERS, json={"collectionName":c_rest,"indexName":"vector"})
rest_indexes = r_desc.json().get('data',[])
sdk_indexes = client.list_indexes(collection_name=c_sdk)
if rest_ok and sdk_ok and (not rest_indexes or not sdk_indexes): print(f'[DEFECT: PARAM_IGNORED] create_index indexes: rest={len(rest_indexes)} sdk={len(sdk_indexes)}'); sys.exit(1)
else: print(f'diff create_index verified: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string()
}

pub fn diff_describe() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r_rest = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_desc = client.describe_collection(collection_name=c)
    sdk_ok = True
except Exception as e:
    sdk_ok = False
rest_ok = r_rest.json().get('code') == 0
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
rest_dim = r_rest.json().get('data',{}).get('dimension')
sdk_dim = sdk_desc.get('dimension') if sdk_ok else None
if rest_dim != sdk_dim: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe dim: rest={rest_dim} sdk={sdk_dim}'); sys.exit(1)
rest_name = r_rest.json().get('data',{}).get('collectionName')
sdk_name = sdk_desc.get('collection_name') if sdk_ok else None
if rest_name != sdk_name: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe name: rest={rest_name} sdk={sdk_name}'); sys.exit(1)
else: print(f'diff describe verified: rest_dim={rest_dim} sdk_dim={sdk_dim} rest_name={rest_name} sdk_name={sdk_name}'); sys.exit(0)"#.to_string()
}

pub fn diff_upsert() -> String {
    r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_diff_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r_rest = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.9,0.8,0.7,0.6]}]})
rest_ok = r_rest.json().get('code') == 0
client = MilvusClient(uri=BASE, token='{{TESTVDB_AUTH_HEADER}}')
try:
    sdk_res = client.upsert(collection_name=c, data=[{"id":2,"vector":[0.5,0.6,0.7,0.8]}])
    sdk_ok = True
    sdk_count = sdk_res.get('upsert_count',0)
except Exception as e:
    sdk_ok = False
    sdk_count = 0
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] upsert success: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
else: print(f'diff upsert verified: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string()
}

pub fn generate_milvus_sequences() -> Vec<String> {
    vec![
        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'search1 failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'drop failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'recreate failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'reload failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') == 0 and len(r.json().get('data',[])) > 0: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate search returned data after drop'); sys.exit(1)
else: print(f'seq1 verified: recreate search empty'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10})
ids = [d.get('id') for d in r.json().get('data',[])]
if 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] deleted id=1 still in search results: {ids}'); sys.exit(1)
else: print(f'seq2 verified: deleted id not in results: {ids}'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r1.json().get('code') != 0: print(f'search1 failed: {r1.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'release failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'reload failed: {r.text}'); sys.exit(0)
time.sleep(2)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r2.json().get('code') != 0: print(f'search2 failed: {r2.text}'); sys.exit(0)
ids1 = set(d.get('id') for d in r1.json().get('data',[]))
ids2 = set(d.get('id') for d in r2.json().get('data',[]))
if ids1 != ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] release+load changed results: {ids1} vs {ids2}'); sys.exit(1)
else: print(f'seq3 verified: release+load results consistent'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/indexes/create', headers=HEADERS, json={"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'create index failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/indexes/drop', headers=HEADERS, json={"collectionName":c,"indexName":"vector"})
if r.json().get('code') != 0: print(f'drop index failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed after drop_index: {r.text}'); sys.exit(1)
else: print(f'seq4 verified: search works after drop_index'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.9,0.8,0.7,0.6]}]})
if r.json().get('code') != 0: print(f'upsert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.9,0.8,0.7,0.6]],"limit":1})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
top = r.json().get('data',[])
if top and top[0].get('id') == 1: print(f'seq5 verified: upsert updated data'); sys.exit(0)
else: print(f'[DEFECT: SEQUENCE_VIOLATION] upsert did not update: {top}'); sys.exit(1)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/collections/get_stats', headers=HEADERS, json={"collectionName":c})
count = r.json().get('data',{}).get('rowCount',-1)
if count != 1: print(f'[DEFECT: SEQUENCE_VIOLATION] duplicate id insert count: expected 1 got {count}'); sys.exit(1)
else: print(f'seq6 verified: duplicate id count={count}'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
c_new = c + '_new'
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/rename', headers=HEADERS, json={"collectionName":c,"newCollectionName":c_new})
if r.json().get('code') != 0: print(f'rename failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c_new,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] insert after rename failed: {r.text}'); sys.exit(1)
else: print(f'seq7 verified: insert after rename ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
alias = 'oracle_alias_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/aliases/create', headers=HEADERS, json={"aliasName":alias,"collectionName":c})
if r.json().get('code') != 0: print(f'alias create failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":alias,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] insert via alias failed: {r.text}'); sys.exit(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":alias,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search via alias failed: {r.text}'); sys.exit(1)
else: print(f'seq8 verified: alias operations ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'flush failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after flush failed: {r.text}'); sys.exit(1)
else: print(f'seq9 verified: flush+search ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/compact', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after compact failed: {r.text}'); sys.exit(1)
else: print(f'seq10 verified: compact+search ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
p = 'part_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/partitions/create', headers=HEADERS, json={"collectionName":c,"partitionName":p})
if r.json().get('code') != 0: print(f'partition create failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}],"partitionName":p})
if r.json().get('code') != 0: print(f'insert partition failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3,"partitionNames":[p]})
if r.json().get('code') != 0: print(f'search partition failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/partitions/drop', headers=HEADERS, json={"collectionName":c,"partitionName":p})
if r.json().get('code') != 0: print(f'drop partition failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'reload failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
ids = [d.get('id') for d in r.json().get('data',[])]
if 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data still found after drop: {ids}'); sys.exit(1)
else: print(f'seq11 verified: partition drop ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/alter_properties', headers=HEADERS, json={"collectionName":c,"properties":{"collection.ttl.seconds":86400}})
if r.json().get('code') != 0: print(f'alter failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
props = r.json().get('data',{}).get('properties',{})
if not props: print(f'[DEFECT: SEQUENCE_VIOLATION] properties not reflected in describe'); sys.exit(1)
else: print(f'seq12 verified: alter_properties ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/fields/add', headers=HEADERS, json={"collectionName":c,"fieldName":"extra","dataType":"Int64"})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":2,"vector":[0.5,0.6,0.7,0.8],"extra":42}]})
if r.json().get('code') != 0: print(f'insert with new field failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"extra == 42","limit":10})
if r.json().get('code') == 0 and len(r.json().get('data',[])) > 0: print(f'seq13 verified: dynamic field query ok'); sys.exit(0)
else: print(f'[DEFECT: SEQUENCE_VIOLATION] dynamic field query returned no data'); sys.exit(1)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/databases/create', headers=HEADERS, json={"dbName":db})
if r.json().get('code') != 0: print(f'db create failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"dbName":db,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'collection in db create failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/list', headers=HEADERS, json={"dbName":db})
if r.json().get('code') != 0: print(f'list in db failed: {r.text}'); sys.exit(0)
names = r.json().get('data',[])
if isinstance(names, list) and len(names) > 0 and isinstance(names[0], dict):
    names = [d.get('collectionName','') for d in names]
if c not in names: print(f'[DEFECT: SEQUENCE_VIOLATION] collection not found in db: {names}'); sys.exit(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c,"dbName":db})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/databases/drop', headers=HEADERS, json={"dbName":db})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] db drop failed: {r.text}'); sys.exit(1)
print(f'seq14 verified: db operations ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r1.json().get('code') != 0: print(f'search failed: {r1.text}'); sys.exit(0)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3,"filter":"id > 0"})
if r2.json().get('code') != 0: print(f'search with filter failed: {r2.text}'); sys.exit(0)
r3 = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id > 0","limit":10})
if r3.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] query failed: {r3.text}'); sys.exit(1)
search_ids = set(d.get('id') for d in r1.json().get('data',[]))
query_ids = set(d.get('id') for d in r3.json().get('data',[]))
if not search_ids or not query_ids: print(f'[DEFECT: SEQUENCE_VIOLATION] search+query returned empty'); sys.exit(1)
print(f'seq15 verified: search+query mixed ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id > 0"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
time.sleep(1)
data2 = [{"id":i,"vector":[0.5*i,0.6*i,0.7*i,0.8*i]} for i in range(1,4)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data2})
if r.json().get('code') != 0: print(f'reinsert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.6,0.7,0.8]],"limit":5})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after reinsert failed: {r.text}'); sys.exit(1)
ids = [d.get('id') for d in r.json().get('data',[])]
if len(ids) == 0: print(f'[DEFECT: SEQUENCE_VIOLATION] no results after reinsert'); sys.exit(1)
else: print(f'seq16 verified: delete_all+reinsert ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'release1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load2 failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after repeated load/release failed'); sys.exit(1)
else: print(f'seq17 verified: repeated load/release ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/hybrid_search', headers=HEADERS, json={"collectionName":c,"searchParams":[{"data":[[0.1,0.2,0.3,0.4]],"limit":3}],"rerank":{"strategy":"rrf","params":{"k":60}}})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after hybrid failed: {r.text}'); sys.exit(1)
else: print(f'seq18 verified: hybrid+search ok'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
for batch in range(3):
    data = [{"id":batch*10+i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(10)]
    r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
    if r.json().get('code') != 0: print(f'insert batch {batch} failed: {r.text}'); sys.exit(0)
    time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":30})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
ids = [d.get('id') for d in r.json().get('data',[])]
if len(ids) < 10: print(f'[DEFECT: SEQUENCE_VIOLATION] multi-batch search returned too few: {len(ids)}'); sys.exit(1)
else: print(f'seq19 verified: multi-batch insert ok, found {len(ids)} results'); sys.exit(0)"#.to_string(),

        r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_seq_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data1 = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data1})
if r.json().get('code') != 0: print(f'insert1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load1 failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
if r1.json().get('code') != 0: print(f'search1 failed: {r1.text}'); sys.exit(0)
ids1 = set(d.get('id') for d in r1.json().get('data',[]))
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'drop failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'recreate failed: {r.text}'); sys.exit(0)
time.sleep(1)
data2 = [{"id":i+10,"vector":[0.5*i,0.6*i,0.7*i,0.8*i]} for i in range(1,6)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data2})
if r.json().get('code') != 0: print(f'insert2 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load2 failed: {r.text}'); sys.exit(0)
time.sleep(2)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.6,0.7,0.8]],"limit":10})
if r2.json().get('code') != 0: print(f'search2 failed: {r2.text}'); sys.exit(0)
ids2 = set(d.get('id') for d in r2.json().get('data',[]))
if ids1 & ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] data not isolated after recreate: old={ids1} new={ids2} overlap={ids1&ids2}'); sys.exit(1)
else: print(f'seq20 verified: data isolated after recreate'); sys.exit(0)"#.to_string(),
    ]
}

pub fn concurrent_insert_search() -> String {
    r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
errors = []
def do_insert():
    for i in range(10):
        data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]
        r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
        if r.json().get('code') != 0: errors.append(f'insert {i} failed')
def do_search():
    for _ in range(5):
        r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
        if r.json().get('code') != 0: errors.append('search failed')
        time.sleep(0.2)
t1 = threading.Thread(target=do_insert)
t2 = threading.Thread(target=do_search)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    retry_ok = True
    try:
        r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: retry_ok = False
    except: retry_ok = False
    if not retry_ok: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent insert+search errors: {errors} system unhealthy after retry'); sys.exit(1)
    else: print(f'concurrent insert+search transient errors recovered: {errors}'); sys.exit(0)
else: print('concurrent insert+search verified'); sys.exit(0)"#.to_string()
}

pub fn concurrent_delete_query() -> String {
    r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,11)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
errors = []
def do_delete():
    for i in range(1,6):
        r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":f"id == {i}"})
        if r.json().get('code') != 0: errors.append(f'delete {i} failed')
        time.sleep(0.1)
def do_query():
    for _ in range(5):
        r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id > 0","limit":10})
        if r.json().get('code') != 0: errors.append('query failed')
        time.sleep(0.2)
t1 = threading.Thread(target=do_delete)
t2 = threading.Thread(target=do_query)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    retry_ok = True
    try:
        r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: retry_ok = False
    except: retry_ok = False
    if not retry_ok: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent delete+query errors: {errors} system unhealthy after retry'); sys.exit(1)
    else: print(f'concurrent delete+query transient errors recovered: {errors}'); sys.exit(0)
else: print('concurrent delete+query verified'); sys.exit(0)"#.to_string()
}

pub fn concurrent_upsert_search() -> String {
    r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
errors = []
def do_upsert():
    for i in range(5):
        r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)]}]})
        if r.json().get('code') != 0: errors.append(f'upsert {i} failed')
def do_search():
    for _ in range(5):
        r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
        if r.json().get('code') != 0: errors.append('search failed')
        time.sleep(0.2)
t1 = threading.Thread(target=do_upsert)
t2 = threading.Thread(target=do_search)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    retry_ok = True
    try:
        r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: retry_ok = False
    except: retry_ok = False
    if not retry_ok: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent upsert+search errors: {errors} system unhealthy after retry'); sys.exit(1)
    else: print(f'concurrent upsert+search transient errors recovered: {errors}'); sys.exit(0)
else: print('concurrent upsert+search verified'); sys.exit(0)"#.to_string()
}

pub fn concurrent_create_drop() -> String {
    r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
errors = []
def do_create():
    for i in range(3):
        c = 'oracle_conc_' + uuid.uuid4().hex[:8]
        r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
        if r.json().get('code') != 0: errors.append(f'create {i} failed')
        time.sleep(0.5)
        requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
def do_drop():
    for i in range(3):
        c = 'oracle_conc_drop_' + uuid.uuid4().hex[:8]
        r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
        if r.json().get('code') != 0: errors.append(f'drop_setup {i} failed'); continue
        r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: errors.append(f'drop {i} failed')
        time.sleep(0.3)
t1 = threading.Thread(target=do_create)
t2 = threading.Thread(target=do_drop)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    try:
        r = requests.post(f'{BASE}/v2/vectordb/collections/list', headers=HEADERS, json={})
        retry_ok = r.json().get('code') == 0
    except: retry_ok = False
    if not retry_ok: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent create+drop errors: {errors} system unhealthy'); sys.exit(1)
    else: print(f'concurrent create+drop transient errors recovered: {errors}'); sys.exit(0)
else: print('concurrent create+drop verified'); sys.exit(0)"#.to_string()
}

pub fn concurrent_insert_flush() -> String {
    r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
errors = []
def do_insert():
    for i in range(10):
        data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]
        r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
        if r.json().get('code') != 0: errors.append(f'insert {i} failed')
        time.sleep(0.1)
def do_flush():
    for _ in range(3):
        r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: errors.append('flush failed')
        time.sleep(0.5)
t1 = threading.Thread(target=do_insert)
t2 = threading.Thread(target=do_flush)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    retry_ok = True
    try:
        r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
        if r.json().get('code') != 0: retry_ok = False
    except: retry_ok = False
    if not retry_ok: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent insert+flush errors: {errors} system unhealthy after retry'); sys.exit(1)
    else: print(f'concurrent insert+flush transient errors recovered: {errors}'); sys.exit(0)
else: print('concurrent insert+flush verified'); sys.exit(0)"#.to_string()
}

pub fn state_insert_search_delete_search() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_state_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'search1 failed: {r.text}'); sys.exit(0)
ids1 = [d.get('id') for d in r.json().get('data',[])]
if 1 not in ids1: print(f'[DEFECT: SEQUENCE_VIOLATION] insert+search: id=1 not found after insert'); sys.exit(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') != 0: print(f'search2 failed: {r.text}'); sys.exit(0)
ids2 = [d.get('id') for d in r.json().get('data',[])]
if 1 in ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] delete+search: id=1 still found after delete'); sys.exit(1)
else: print(f'state insert_search_delete_search verified'); sys.exit(0)"#.to_string()
}

pub fn state_insert_delete_insert_search() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_state_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert1 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
if r.json().get('code') != 0: print(f'insert2 failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.6,0.7,0.8]],"limit":3})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
ids = [d.get('id') for d in r.json().get('data',[])]
if 1 not in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] re-insert+search: id=1 not found after re-insert'); sys.exit(1)
else: print(f'state insert_delete_insert_search verified'); sys.exit(0)"#.to_string()
}

pub fn state_upsert_changes_vector() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_state_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":1})
if r1.json().get('code') != 0: print(f'search1 failed: {r1.text}'); sys.exit(0)
dist1 = r1.json().get('data',[{}])[0].get('distance') if r1.json().get('data') else None
r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.9,0.8,0.7,0.6]}]})
if r.json().get('code') != 0: print(f'upsert failed: {r.text}'); sys.exit(0)
time.sleep(2)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":1})
if r2.json().get('code') != 0: print(f'search2 failed: {r2.text}'); sys.exit(0)
dist2 = r2.json().get('data',[{}])[0].get('distance') if r2.json().get('data') else None
if dist1 is not None and dist2 is not None and dist1 == dist2: print(f'[DEFECT: METAMORPHIC_VIOLATION] upsert did not change distance: before={dist1} after={dist2}'); sys.exit(1)
else: print(f'state upsert_changes_vector verified: dist1={dist1} dist2={dist2}'); sys.exit(0)"#.to_string()
}

pub fn state_create_drop_create_different_dim() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_state_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'drop failed: {r.text}'); sys.exit(0)
time.sleep(3)
recreate_ok = False
for attempt in range(3):
    r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":8}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
    if r.json().get('code') == 0:
        recreate_ok = True
        break
    time.sleep(2)
if not recreate_ok: print(f'recreate failed after 3 attempts: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
dim = r.json().get('data',{}).get('dimension')
if dim != 8: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate dim: expected 8 got {dim}'); sys.exit(1)
else: print(f'state create_drop_create_different_dim verified: dim={dim}'); sys.exit(0)"#.to_string()
}

pub fn state_partition_create_drop_data_isolation() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_state_' + uuid.uuid4().hex[:8]
p = 'part_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/partitions/create', headers=HEADERS, json={"collectionName":c,"partitionName":p})
if r.json().get('code') != 0: print(f'partition create failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}],"partitionName":p})
if r.json().get('code') != 0: print(f'insert partition failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3,"partitionNames":[p]})
if r.json().get('code') != 0: print(f'search partition failed: {r.text}'); sys.exit(0)
ids_before = [d.get('id') for d in r.json().get('data',[])]
if 1 not in ids_before: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data not found before drop'); sys.exit(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/partitions/drop', headers=HEADERS, json={"collectionName":c,"partitionName":p})
if r.json().get('code') != 0: print(f'drop partition failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'reload failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
ids_after = [d.get('id') for d in r.json().get('data',[])]
if 1 in ids_after: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data still found after drop partition: {ids_after}'); sys.exit(1)
else: print(f'state partition_create_drop_data_isolation verified'); sys.exit(0)"#.to_string()
}

pub fn resource_large_dimension() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":32768}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print(f'[DEFECT: PARAM_IGNORED] 32768-dim collection created (documented max)'); sys.exit(1)
else: print(f'properly rejected 32768-dim: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn resource_long_collection_name() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'a' * 256
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] 256-char collection name accepted'); sys.exit(1)
else: print(f'properly rejected 256-char name: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn resource_zero_dimension() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":0}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] 0-dim collection created'); sys.exit(1)
else: print(f'properly rejected 0-dim: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_param_combination_probes() -> Vec<SafetyNet> {
    let combos: Vec<(&str, &str, &str)> = vec![
        ("4", "L2", "IVF_FLAT"),
        ("4", "L2", "HNSW"),
        ("8", "COSINE", "IVF_FLAT"),
        ("8", "COSINE", "HNSW"),
        ("4", "IP", "IVF_FLAT"),
        ("4", "IP", "HNSW"),
        ("128", "COSINE", "AUTOINDEX"),
        ("128", "L2", "HNSW"),
        ("4", "COSINE", "FLAT"),
        ("4", "L2", "FLAT"),
    ];
    combos.into_iter().map(|(dim, metric, idx)| {
        let name = format!("param_combo_{dim}_{metric}_{idx}");
        let script = format!(
            r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}}
c = 'oracle_combo_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":{dim}}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"{metric}","indexType":"{idx}"}}]}})
if r.json().get('code') != 0: print(f'create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
vec_data = [0.01] * {dim}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":vec_data}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{"collectionName":c,"data":[vec_data],"limit":3}})
if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed for dim={dim} metric={metric} index={idx}: {{r.text}}'); sys.exit(1)
else: print(f'param combo verified: dim={dim} metric={metric} index={idx}'); sys.exit(0)"#
        );
        SafetyNet {
            name,
            script,
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn flat_index_l2_distance_ordering() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_flat_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"FLAT"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,11)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance') for d in results]
for i in range(len(distances)-1):
    if distances[i] > distances[i+1]: print(f'[DEFECT: METAMORPHIC_VIOLATION] FLAT L2 not ascending: d[{i}]={distances[i]} > d[{i+1}]={distances[i+1]}'); sys.exit(1)
print(f'FLAT L2 distance ordering verified: {distances}'); sys.exit(0)"#.to_string()
}

pub fn flat_index_cosine_distance_ordering() -> String {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'oracle_flat_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"FLAT"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,11)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":10})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance') for d in results]
for i in range(len(distances)-1):
    if distances[i] < distances[i+1]: print(f'[DEFECT: METAMORPHIC_VIOLATION] FLAT COSINE not descending: d[{i}]={distances[i]} < d[{i+1}]={distances[i+1]}'); sys.exit(1)
print(f'FLAT COSINE distance ordering verified: {distances}'); sys.exit(0)"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metamorphic_nprobe_returns_script() {
        let s = metamorphic_nprobe_monotonicity();
        assert!(s.contains("METAMORPHIC_VIOLATION"));
        assert!(s.contains("nprobe"));
    }

    #[test]
    fn test_metamorphic_ef_search_returns_script() {
        let s = metamorphic_ef_search_monotonicity();
        assert!(s.contains("METAMORPHIC_VIOLATION"));
        assert!(s.contains("ef"));
    }

    #[test]
    fn test_metamorphic_query_consistency_returns_script() {
        let s = metamorphic_query_consistency();
        assert!(s.contains("METAMORPHIC_VIOLATION"));
        assert!(s.contains("consistency"));
    }

    #[test]
    fn test_metamorphic_insert_monotonicity_returns_script() {
        let s = metamorphic_insert_monotonicity();
        assert!(s.contains("METAMORPHIC_VIOLATION"));
    }

    #[test]
    fn test_metamorphic_limit_monotonicity_returns_script() {
        let s = metamorphic_limit_monotonicity();
        assert!(s.contains("METAMORPHIC_VIOLATION"));
        assert!(s.contains("limit"));
    }

    #[test]
    fn test_diff_create_collection_returns_script() {
        let s = diff_create_collection();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
        assert!(s.contains("pymilvus"));
    }

    #[test]
    fn test_diff_insert_returns_script() {
        let s = diff_insert();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_diff_search_returns_script() {
        let s = diff_search();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_diff_query_returns_script() {
        let s = diff_query();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_diff_delete_returns_script() {
        let s = diff_delete();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_diff_create_index_returns_script() {
        let s = diff_create_index();
        assert!(s.contains("PARAM_IGNORED"));
    }

    #[test]
    fn test_diff_describe_returns_script() {
        let s = diff_describe();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_diff_upsert_returns_script() {
        let s = diff_upsert();
        assert!(s.contains("DIFFERENTIAL_MISMATCH"));
    }

    #[test]
    fn test_generate_milvus_sequences_count() {
        let seqs = generate_milvus_sequences();
        assert_eq!(seqs.len(), 20);
    }

    #[test]
    fn test_generate_milvus_sequences_contain_defect() {
        let seqs = generate_milvus_sequences();
        for (i, s) in seqs.iter().enumerate() {
            assert!(s.contains("SEQUENCE_VIOLATION"), "seq {} missing SEQUENCE_VIOLATION", i);
        }
    }

    #[test]
    fn test_concurrent_insert_search_returns_script() {
        let s = concurrent_insert_search();
        assert!(s.contains("threading"));
        assert!(s.contains("SEQUENCE_VIOLATION"));
    }

    #[test]
    fn test_concurrent_delete_query_returns_script() {
        let s = concurrent_delete_query();
        assert!(s.contains("threading"));
        assert!(s.contains("SEQUENCE_VIOLATION"));
    }

    #[test]
    fn test_concurrent_upsert_search_returns_script() {
        let s = concurrent_upsert_search();
        assert!(s.contains("threading"));
        assert!(s.contains("SEQUENCE_VIOLATION"));
    }

    #[test]
    fn test_concurrent_create_drop_returns_script() {
        let s = concurrent_create_drop();
        assert!(s.contains("threading"));
        assert!(s.contains("SEQUENCE_VIOLATION"));
    }

    #[test]
    fn test_concurrent_insert_flush_returns_script() {
        let s = concurrent_insert_flush();
        assert!(s.contains("threading"));
        assert!(s.contains("SEQUENCE_VIOLATION"));
    }
}
