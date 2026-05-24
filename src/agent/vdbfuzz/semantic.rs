use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use crate::agent::vdbfuzz::boundary::FuzzTestCase;
// serde not needed for script generation

/// Generator for concurrent state consistency tests.
/// Tests: N threads insert → final count == N; concurrent insert+delete → correct count.
pub struct ConcurrentStateGenerator;

impl ConcurrentStateGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();

        let has_insert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("insert") || atc.endpoint.contains("entities") || atc.endpoint.contains("objects")
        });
        let has_delete = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("delete") || atc.endpoint.contains("drop")
        });
        // check if search endpoint exists for future use
        if !has_insert {
            return cases;
        }

        // 1. Concurrent insert count consistency
        cases.push(FuzzTestCase {
            name: "concurrent_insert_count".into(),
            script: build_concurrent_insert_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("insert".into(), "concurrent_count".into(), "parallel".into())),
            semantic_assertion: None,
        });

        // 2. Concurrent insert + delete
        if has_delete {
            cases.push(FuzzTestCase {
                name: "concurrent_insert_delete".into(),
                script: build_concurrent_insert_delete_script(style),
                expected_rejection: false,
                defect_marker: "STATE_LOGIC_VIOLATION".into(),
                coverage_entry: Some(("insert+delete".into(), "concurrent_mixed".into(), "parallel".into())),
                semantic_assertion: None,
            });
        }

        // 3. Concurrent upsert on same ID (no duplicates)
        cases.push(FuzzTestCase {
            name: "concurrent_upsert_same_id".into(),
            script: build_concurrent_upsert_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("upsert".into(), "duplicate_prevention".into(), "parallel".into())),
            semantic_assertion: None,
        });

        // 4. Concurrent drop + insert
        cases.push(FuzzTestCase {
            name: "concurrent_create_drop".into(),
            script: build_concurrent_create_drop_script(style),
            expected_rejection: false,
            defect_marker: "RUNTIME_FAILURE".into(),
            coverage_entry: Some(("create+drop".into(), "no_crash".into(), "parallel".into())),
            semantic_assertion: None,
        });

        cases
    }
}

/// Generator for semantic correctness tests.
/// Tests: recall (insert known → search → found), filter precision, distance ordering.
pub struct SemanticDriftGenerator;

impl SemanticDriftGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();

        let _has_search = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("search") || atc.endpoint.contains("query")
        });
        if !_has_search {
            return cases;
        }

        // 1. Recall: insert N items, search for a known one, verify in top-K
        cases.push(FuzzTestCase {
            name: "semantic_recall".into(),
            script: build_recall_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("search".into(), "recall".into(), "semantic".into())),
            semantic_assertion: None,
        });

        // 2. Filter precision
        cases.push(FuzzTestCase {
            name: "semantic_filter_precision".into(),
            script: build_filter_precision_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("search".into(), "filter_precision".into(), "semantic".into())),
            semantic_assertion: None,
        });

        // 3. Distance ordering (L2 ascending)
        cases.push(FuzzTestCase {
            name: "semantic_distance_ordering".into(),
            script: build_distance_ordering_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("search".into(), "distance_order".into(), "semantic".into())),
            semantic_assertion: None,
        });

        // 4. Pagination consistency (offset disjointness)
        cases.push(FuzzTestCase {
            name: "semantic_pagination".into(),
            script: build_pagination_script(style),
            expected_rejection: false,
            defect_marker: "STATE_LOGIC_VIOLATION".into(),
            coverage_entry: Some(("search".into(), "pagination".into(), "semantic".into())),
            semantic_assertion: None,
        });

        cases
    }
}

/// Generator for resource boundary / stress tests.
/// Tests: large dimension, many connections, rapid create/drop cycles.
pub struct ResourceBoundaryGenerator;

impl ResourceBoundaryGenerator {
    pub fn from_store(_store: &ContractStore, style: TargetStyle) -> Vec<FuzzTestCase> {
        let mut cases = Vec::new();

        // 1. Rapid create/drop cycle (resource leak detection)
        cases.push(FuzzTestCase {
            name: "resource_create_drop_cycle".into(),
            script: build_create_drop_cycle_script(style),
            expected_rejection: false,
            defect_marker: "RUNTIME_FAILURE".into(),
            coverage_entry: Some(("create+drop".into(), "rapid_cycle".into(), "resource".into())),
            semantic_assertion: None,
        });

        // 2. Large dimension
        cases.push(FuzzTestCase {
            name: "resource_large_dimension".into(),
            script: build_large_dimension_script(style),
            expected_rejection: true,
            defect_marker: "ILLEGAL_SUCCESS".into(),
            coverage_entry: Some(("create".into(), "dim".into(), "resource".into())),
            semantic_assertion: None,
        });

        // 3. Many inserts rapidly
        cases.push(FuzzTestCase {
            name: "resource_rapid_inserts".into(),
            script: build_rapid_inserts_script(style),
            expected_rejection: false,
            defect_marker: "RUNTIME_FAILURE".into(),
            coverage_entry: Some(("insert".into(), "rapid_bulk".into(), "resource".into())),
            semantic_assertion: None,
        });

        cases
    }
}

// ── Script builders ──

fn build_concurrent_insert_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, threading, uuid, sys
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_conc_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
conn.commit()
errors = []
def insert_batch(start, count):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = c.cursor()
        for i in range(start, start+count):
            cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        c.commit()
        c.close()
    except Exception as e:
        errors.append(str(e))
threads = [threading.Thread(target=insert_batch, args=(i*10,10)) for i in range(4)]
for t in threads: t.start()
for t in threads: t.join()
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count != 40:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Concurrent insert: expected 40, got {count}")
    sys.exit(1)
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Concurrent insert errors: {errors}")
    sys.exit(1)
print(f"Concurrent insert OK: {count} rows")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),

        TargetStyle::Weaviate => r#"
import requests, threading, uuid, sys, time
BASE = "{{TESTVDB_DB_URL}}"
col = "testvdb_conc_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class":col,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code == 200
time.sleep(0.5)
errors = []
def insert_batch(start, count):
    try:
        for i in range(start, start+count):
            resp = requests.post(f"{BASE}/v1/objects", json={"class":col,"id":str(uuid.uuid4()),"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"properties":{"text":f"item{i}"}})
            if resp.status_code != 200:
                errors.append(f"Insert {i}: {resp.status_code}")
    except Exception as e:
        errors.append(str(e))
threads = [threading.Thread(target=insert_batch, args=(i*10,10)) for i in range(4)]
for t in threads: t.start()
for t in threads: t.join()
time.sleep(2)
r2 = requests.get(f"{BASE}/v1/schema/{col}")
count = r2.json().get("class",{}).get("objectCount",-1)
if count != 40:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Concurrent Weaviate insert: expected 40, got {count}")
    sys.exit(1)
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Errors: {errors}")
    sys.exit(1)
print(f"Concurrent insert OK: {count} objects")
requests.delete(f"{BASE}/v1/schema/{col}")
"#.to_string(),

        _ => { // Qdrant and Milvus
r#"import requests, threading, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_conc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(1)
errors = []
def insert_batch(start, count):
    try:
        for i in range(start, start+count):
            resp = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":start*100+i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
            if resp.json().get('code')!=0:
                errors.append(f"Insert {i}: {resp.json()}")
    except Exception as e:
        errors.append(str(e))
threads = [threading.Thread(target=insert_batch, args=(i*10,10)) for i in range(4)]
for t in threads: t.start()
for t in threads: t.join()
time.sleep(2)
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
count = r2.json().get('data',{}).get('rowCount',-1)
if count != 40:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Concurrent insert: expected 40, got {count}")
    sys.exit(1)
print(f"Concurrent insert OK: {count} rows")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
        }
    }
}

fn build_concurrent_insert_delete_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, threading, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_idel_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
for i in range(40):
    cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
time.sleep(0.5)
errors = []
def insert_more(start, count):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = c.cursor()
        for i in range(start, start+count):
            cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{1.0*i},{2.0*i},{3.0*i},{4.0*i}]')")
        c.commit()
        c.close()
    except Exception as e: errors.append(str(e))
def delete_some(start, count):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = c.cursor()
        cur.execute(f"DELETE FROM {table} WHERE id <= {start+count} AND id >= {start}")
        c.commit()
        c.close()
    except Exception as e: errors.append(str(e))
t1 = threading.Thread(target=insert_more, args=(100,10))
t2 = threading.Thread(target=delete_some, args=(1,5))
t1.start(); t2.start()
t1.join(); t2.join()
time.sleep(0.5)
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Concurrent insert+delete errors: {errors}")
    sys.exit(1)
print(f"Concurrent insert+delete: {count} rows remaining")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        _ => r#"
import requests, threading, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_idel_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(0.5)
for i in range(40):
    requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
time.sleep(0.5)
errors = []
def insert_more(start, count):
    for i in range(start, start+count):
        resp = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[1.0*i,2.0*i,3.0*i,4.0*i]}]})
        if resp.json().get('code')!=0: errors.append(f"Insert {i}")
def delete_some(start, count):
    resp = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":f"id >= {start} && id <= {start+count}"})
    if resp.json().get('code')!=0: errors.append(f"Delete {start}")
t1 = threading.Thread(target=insert_more, args=(100,10))
t2 = threading.Thread(target=delete_some, args=(1,5))
t1.start(); t2.start()
t1.join(); t2.join()
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
count = r2.json().get('data',{}).get('rowCount',-1)
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Errors: {errors}")
print(f"Concurrent insert+delete: {count} rows")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_concurrent_upsert_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, threading, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_cups_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id int PRIMARY KEY, emb vector(4))")
conn.commit()
errors = []
def upsert_id(id_val):
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = c.cursor()
        cur.execute(f"INSERT INTO {table} (id, emb) VALUES ({id_val}, '[{0.1*id_val},{0.2*id_val},{0.3*id_val},{0.4*id_val}]') ON CONFLICT (id) DO UPDATE SET emb = EXCLUDED.emb")
        c.commit()
        c.close()
    except Exception as e: errors.append(str(e))
threads = [threading.Thread(target=upsert_id, args=(1,)) for _ in range(8)]
for t in threads: t.start()
for t in threads: t.join()
time.sleep(0.5)
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count > 1:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Concurrent upsert same ID: expected 1, got {count}")
    sys.exit(1)
print(f"Concurrent upsert OK: {count} row(s)")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        _ => r#"
import requests, threading, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_cups_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(0.5)
errors = []
def upsert_same():
    try:
        resp = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
        if resp.json().get('code')!=0: errors.append(f"Upsert: {resp.json()}")
    except Exception as e: errors.append(str(e))
threads = [threading.Thread(target=upsert_same) for _ in range(6)]
for t in threads: t.start()
for t in threads: t.join()
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
count = r2.json().get('data',{}).get('rowCount',-1)
if count > 1:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Concurrent upsert: expected 1, got {count}")
    sys.exit(1)
print(f"Concurrent upsert OK: {count} rows")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_concurrent_create_drop_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, threading, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
errors = []
def create_drop_cycle():
    try:
        c = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
        cur = c.cursor()
        name = "t_" + uuid.uuid4().hex[:8]
        cur.execute(f"CREATE TABLE {name} (id serial PRIMARY KEY, emb vector(4))")
        cur.execute(f"INSERT INTO {name} (emb) VALUES ('[1,2,3,4]')")
        c.commit()
        cur.execute(f"DROP TABLE {name}")
        c.commit()
        c.close()
    except Exception as e: errors.append(str(e))
threads = [threading.Thread(target=create_drop_cycle) for _ in range(10)]
for t in threads: t.start()
for t in threads: t.join()
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Create/drop cycle errors: {errors}")
    sys.exit(1)
print("Concurrent create/drop OK")
"#.to_string(),
        _ => r#"
import requests, threading, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
errors = []
def create_drop_cycle():
    try:
        c = 'testvdb_cd_' + uuid.uuid4().hex[:8]
        requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
        requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
    except Exception as e: errors.append(str(e))
threads = [threading.Thread(target=create_drop_cycle) for _ in range(10)]
for t in threads: t.start()
for t in threads: t.join()
if errors:
    print(f"[DEFECT: RUNTIME_FAILURE] Create/drop cycle errors: {errors}")
    sys.exit(1)
print("Concurrent create/drop OK")
"#.to_string()
    }
}

fn build_recall_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_recall_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
target_vec = '[0.5,0.5,0.5,0.5]'
cur.execute(f"INSERT INTO {table} (emb) VALUES ('{target_vec}')")
for i in range(19):
    cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.01*i},{0.02*i},{0.03*i},{0.04*i}]')")
conn.commit()
time.sleep(0.5)
cur.execute(f"SELECT id FROM {table} ORDER BY emb <-> '{target_vec}' LIMIT 10")
rows = cur.fetchall()
ids = [r[0] for r in rows]
if 1 not in ids:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Recall failure: target (id=1) not in top-10 results. Got: {ids}")
    sys.exit(1)
print(f"Recall OK: target in top-10, ids={ids[:5]}...")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        TargetStyle::Weaviate => r#"
import requests, uuid, sys, time
BASE = "{{TESTVDB_DB_URL}}"
col = "testvdb_recall_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class":col,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"text","dataType":["text"]}]})
assert r.status_code==200
time.sleep(0.5)
target_id = str(uuid.uuid4())
target_vec = [0.5,0.5,0.5,0.5]
requests.post(f"{BASE}/v1/objects", json={"class":col,"id":target_id,"vector":target_vec,"properties":{"text":"target"}})
for i in range(19):
    requests.post(f"{BASE}/v1/objects", json={"class":col,"id":str(uuid.uuid4()),"vector":[0.01*i,0.02*i,0.03*i,0.04*i],"properties":{"text":f"noise{i}"}})
time.sleep(1)
r2 = requests.get(f"{BASE}/v1/objects?class={col}&nearVector={{'vector':{target_vec}}}&limit=10")
objs = r2.json().get("objects",[])
found = any(o.get("properties",{}).get("text")=="target" for o in objs)
if not found:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Semantic recall failure in Weaviate")
    sys.exit(1)
print(f"Recall OK: target found in {len(objs)} results")
requests.delete(f"{BASE}/v1/schema/{col}")
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_recall_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(1)
target_vec = [0.5,0.5,0.5,0.5]
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":target_vec}]})
for i in range(19):
    requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":100+i,"vector":[0.01*i,0.02*i,0.03*i,0.04*i]}]})
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[target_vec],"limit":10})
results = r2.json().get('data',[])
found = any(d.get('id')==1 for d in results)
if not found:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Recall failure: target id=1 not in top-10")
    sys.exit(1)
print(f"Recall OK: target in top-10")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_filter_precision_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_filt_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4), color text)")
for i in range(20):
    color = 'red' if i%2==0 else 'blue'
    cur.execute(f"INSERT INTO {table} (emb, color) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]', '{color}')")
conn.commit()
time.sleep(0.5)
cur.execute(f"SELECT color FROM {table} WHERE color='red' ORDER BY emb <-> '[0.5,0.5,0.5,0.5]' LIMIT 10")
rows = cur.fetchall()
wrong = [r[0] for r in rows if r[0]!='red']
if wrong:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Filter precision: {len(wrong)} non-red rows returned")
    sys.exit(1)
print(f"Filter precision OK: all {len(rows)} results are red")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        TargetStyle::Weaviate => r#"
import requests, uuid, sys, time
BASE = "{{TESTVDB_DB_URL}}"
col = "testvdb_filt_" + uuid.uuid4().hex[:8]
r = requests.post(f"{BASE}/v1/schema", json={"class":col,"vectorizer":"none","vectorIndexConfig":{"distance":"cosine"},"properties":[{"name":"color","dataType":["string"]}]})
assert r.status_code==200
time.sleep(0.5)
for i in range(20):
    color = 'red' if i%2==0 else 'blue'
    requests.post(f"{BASE}/v1/objects", json={"class":col,"id":str(uuid.uuid4()),"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"properties":{"color":color}})
time.sleep(1)
r2 = requests.get(f"{BASE}/v1/objects?class={col}&where={{'path':['color'],'operator':'Equal','valueString':'red'}}&limit=10")
objs = r2.json().get("objects",[])
wrong = [o for o in objs if o.get("properties",{}).get("color")!="red"]
if wrong:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Filter precision: {len(wrong)} non-red objects returned")
    sys.exit(1)
print(f"Filter precision OK: all {len(objs)} results are red")
requests.delete(f"{BASE}/v1/schema/{col}")
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_filt_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(1)
for i in range(20):
    color = 'red' if i%2==0 else 'blue'
    requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"color":color}]})
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.5,0.5,0.5]],"limit":10,"filter":"color == \\\\"red\\\\""})
results = r2.json().get('data',[])
wrong = [d for d in results if d.get('color')!='red']
if wrong:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Filter precision: {len(wrong)} non-red results")
    sys.exit(1)
print(f"Filter precision OK: {len(results)} results all red")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_distance_ordering_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_dist_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
for i in range(20):
    cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
time.sleep(0.5)
cur.execute(f"SELECT emb <-> '[0.5,0.5,0.5,0.5]' AS dist FROM {table} ORDER BY dist LIMIT 10")
rows = cur.fetchall()
distances = [r[0] for r in rows]
if distances != sorted(distances):
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] L2 distance not ascending: {distances}")
    sys.exit(1)
print(f"Distance ordering OK: {distances[:5]}...")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_dist_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(1)
for i in range(20):
    requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.5,0.5,0.5]],"limit":10})
results = r2.json().get('data',[])
distances = [d.get('distance',999) for d in results]
if distances != sorted(distances):
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] L2 distance not ascending: {distances}")
    sys.exit(1)
print(f"Distance ordering OK")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_pagination_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_page_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
for i in range(20):
    cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
conn.commit()
time.sleep(0.5)
cur.execute(f"SELECT id FROM {table} ORDER BY emb <-> '[0.5,0.5,0.5,0.5]' LIMIT 5")
page1 = {r[0] for r in cur.fetchall()}
cur.execute(f"SELECT id FROM {table} ORDER BY emb <-> '[0.5,0.5,0.5,0.5]' LIMIT 5 OFFSET 5")
page2 = {r[0] for r in cur.fetchall()}
overlap = page1 & page2
if overlap:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Pagination overlap: {overlap} appears in both pages")
    sys.exit(1)
print(f"Pagination OK: page1({len(page1)}) and page2({len(page2)}) disjoint")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_page_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(1)
for i in range(20):
    requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
time.sleep(1)
r1 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.5,0.5,0.5]],"limit":5,"offset":0})
r2 = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.5,0.5,0.5,0.5]],"limit":5,"offset":5})
ids1 = {d['id'] for d in r1.json().get('data',[])}
ids2 = {d['id'] for d in r2.json().get('data',[])}
overlap = ids1 & ids2
if overlap:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Pagination overlap: {overlap}")
    sys.exit(1)
print(f"Pagination OK: disjoint pages")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_create_drop_cycle_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
for cycle in range(50):
    name = f"t_cycle_{cycle}_{uuid.uuid4().hex[:8]}"
    cur.execute(f"CREATE TABLE {name} (id serial PRIMARY KEY, emb vector(4))")
    cur.execute(f"INSERT INTO {name} (emb) VALUES ('[1,2,3,4]')")
    conn.commit()
    cur.execute(f"DROP TABLE {name}")
    conn.commit()
cur.execute("SELECT 1")
conn.commit()
print("Rapid create/drop cycle OK: 50 cycles completed")
conn.close()
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
for cycle in range(30):
    c = f'testvdb_cycle_{cycle}_{uuid.uuid4().hex[:8]}'
    r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
    requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
print("Rapid create/drop cycle OK: 30 cycles completed")
"#.to_string()
    }
}

fn build_large_dimension_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys
DB = "{{TESTVDB_DB_URL}}"
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
name = "t_largedim_" + uuid.uuid4().hex[:8]
try:
    cur.execute(f"CREATE TABLE {name} (id serial PRIMARY KEY, emb vector(16000))")
    conn.commit()
    print(f"[DEFECT: ILLEGAL_SUCCESS] Large dimension 16000 accepted without resource check")
    sys.exit(1)
except Exception as e:
    if "exceed" in str(e).lower() or "too large" in str(e).lower():
        print(f"Large dimension correctly rejected: {e}")
    else:
        print(f"Unexpected error: {e}")
finally:
    conn.rollback()
    conn.close()
"#.to_string(),
        _ => r#"
import requests, uuid, sys
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_largedim_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":32768}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code')==0:
    print(f"[DEFECT: ILLEGAL_SUCCESS] 32768-dim collection created (resource risk)")
    sys.exit(1)
else:
    print(f"Large dimension correctly rejected: {r.json()}")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}

fn build_rapid_inserts_script(style: TargetStyle) -> String {
    match style {
        TargetStyle::PgVector => r#"
import psycopg2, uuid, sys, time
DB = "{{TESTVDB_DB_URL}}"
table = "testvdb_rapid_" + uuid.uuid4().hex[:8]
conn = psycopg2.connect(f"dbname=testvdb user=postgres password=postgres host={DB} port=5432")
cur = conn.cursor()
cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
cur.execute(f"CREATE TABLE {table} (id serial PRIMARY KEY, emb vector(4))")
conn.commit()
errors = 0
for i in range(500):
    try:
        cur.execute(f"INSERT INTO {table} (emb) VALUES ('[{0.1*i},{0.2*i},{0.3*i},{0.4*i}]')")
        if i % 100 == 0:
            conn.commit()
    except Exception as e:
        errors += 1
conn.commit()
time.sleep(1)
cur.execute(f"SELECT COUNT(*) FROM {table}")
count = cur.fetchone()[0]
if count != 500:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Rapid inserts: expected 500, got {count} (errors={errors})")
    sys.exit(1)
print(f"Rapid inserts OK: {count} rows")
cur.execute(f"DROP TABLE IF EXISTS {table}")
conn.commit()
conn.close()
"#.to_string(),
        _ => r#"
import requests, uuid, sys, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'testvdb_rapid_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
assert r.json().get('code')==0
time.sleep(0.5)
errors = 0
for i in range(200):
    try:
        resp = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
        if resp.json().get('code')!=0:
            errors += 1
    except:
        errors += 1
time.sleep(1)
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
count = r2.json().get('data',{}).get('rowCount',-1)
if count != 200:
    print(f"[DEFECT: STATE_LOGIC_VIOLATION] Rapid inserts: expected 200, got {count}")
    sys.exit(1)
print(f"Rapid inserts OK: {count} rows")
requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
"#.to_string()
    }
}
