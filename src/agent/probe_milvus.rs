use crate::target::SafetyNet;

const MILVUS_AUTH_HEADER: &str = "'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'";

fn milvus_create_collection_script(name_var: &str, dim: &str, metric: &str, index_type: &str) -> String {
    format!(
        r#"r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers={{{MILVUS_AUTH_HEADER}}}, json={{"collectionName":{name_var},"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":{dim}}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"{metric}","indexType":"{index_type}"}}]}})"#,
    )
}

pub fn milvus_create_collection_default(name_var: &str) -> String {
    milvus_create_collection_script(name_var, "4", "COSINE", "AUTOINDEX")
}

fn milvus_create_collection_no_index(name_var: &str) -> String {
    format!(
        r#"r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers={{{MILVUS_AUTH_HEADER}}}, json={{"collectionName":{name_var},"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":4}}}}]}}}})"#,
    )
}

pub fn milvus_search_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_search_params_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3,"searchParams":{{"params":{{"{param}":{value}}}}}}}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_create_probe(param: &str, value: &str, label: &str) -> String {
    let (py_dim, py_metric, py_index) = match param {
        "dim" | "elementTypeParams.dim" => (value.to_string(), "\"COSINE\"".to_string(), "\"AUTOINDEX\"".to_string()),
        "metricType" => ("4".to_string(), format!("\"{}\"", value), "\"AUTOINDEX\"".to_string()),
        "indexType" => ("4".to_string(), "\"COSINE\"".to_string(), format!("\"{}\"", value)),
        _ => ("4".to_string(), "\"COSINE\"".to_string(), "\"AUTOINDEX\"".to_string()),
    };

    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
body = {{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":{py_dim}}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":{py_metric},"indexType":{py_index}}}]}}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        py_dim = py_dim,
        py_metric = py_metric,
        py_index = py_index,
        label = label,
    )
}

pub fn milvus_insert_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_query_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"filter":"id > 0","limit":10}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/query', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_index_probe(param: &str, value: &str, label: &str) -> String {
    let (py_metric, py_index, py_params) = match param {
        "metricType" => (format!("\"{}\"", value), "\"AUTOINDEX\"".to_string(), "{}".to_string()),
        "indexType" => ("\"COSINE\"".to_string(), format!("\"{}\"", value), "{}".to_string()),
        "nlist" => ("\"COSINE\"".to_string(), "\"IVF_FLAT\"".to_string(), format!("{{\"nlist\":{}}}", value)),
        "M" => ("\"COSINE\"".to_string(), "\"HNSW\"".to_string(), format!("{{\"M\":{}}}", value)),
        "efConstruction" => ("\"COSINE\"".to_string(), "\"HNSW\"".to_string(), format!("{{\"efConstruction\":{}}}", value)),
        _ => ("\"COSINE\"".to_string(), "\"AUTOINDEX\"".to_string(), "{}".to_string()),
    };
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_idx_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/indexes/create', headers=HEADERS, json={{"collectionName":c,"indexParams":[{{"fieldName":"vector","metricType":{py_metric},"indexType":{py_index},"params":{py_params}}}]}})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_no_index("c"),
        py_metric = py_metric,
        py_index = py_index,
        py_params = py_params,
        label = label,
    )
}

pub fn milvus_partition_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_part_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"partitionName":"test_partition"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/partitions/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_alias_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_alias_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"aliasName":"test_alias","collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/aliases/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_database_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
db = 'oracle_db_' + uuid.uuid4().hex[:8]
body = {{"dbName":db}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/databases/create', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_hybrid_search_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_hybrid_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"searchParams":[{{"data":[[0.1,0.2,0.3,0.4]],"limit":3}}],"rerank":{{"strategy":"rrf","params":{{"k":60}}}}}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/hybrid_search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_collection_mgmt_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_mgmt_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_partition_drop_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_pdrop_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/partitions/create', headers=HEADERS, json={{"collectionName":c,"partitionName":"test_part"}})
if r.json().get('code') != 0: print(f'partition create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"partitionName":"test_part"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/partitions/drop', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_collection_rename_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_rename_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"newCollectionName":c+"_new"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/rename', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_alter_properties_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_alter_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"properties":{{"collection.ttl.seconds":3600}}}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/alter_properties', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_add_field_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_addfield_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"fieldName":"extra_field","dataType":"Int64"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/fields/add', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_get_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_get_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"id":[1]}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/entities/get', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_drop_nonexistent_partition() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_droppart_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/partitions/drop', headers=HEADERS, json={"collectionName":c,"partitionName":"nonexistent_partition"})
if r.json().get('code') == 0: print('[DEFECT: IDEMPOTENT_SUCCESS] drop nonexistent partition accepted'); sys.exit(1)
else: print(f'properly rejected drop nonexistent partition: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_add_vector_field_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_addvec_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/fields/add', headers=HEADERS, json={"collectionName":c,"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] adding duplicate vector field accepted'); sys.exit(1)
else: print(f'properly rejected adding duplicate vector field: {r.json()}'); sys.exit(0)"#.to_string()
}

fn milvus_read_probe(endpoint: &str, base_body: &str, param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
{base_body}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}{endpoint}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        base_body = base_body,
        endpoint = endpoint,
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_index_list_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/indexes/list", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_partition_list_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/partitions/list", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_alias_list_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/aliases/list", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_collection_list_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/collections/list", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_collection_has_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/collections/has", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_collection_stats_probe(param: &str, value: &str, label: &str) -> String {
    milvus_read_probe("/v2/vectordb/collections/get_stats", r#"body = {"collectionName":c}"#, param, value, label)
}

pub fn milvus_collection_release_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_rel_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/release', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_alias_alter_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_alter_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/aliases/create', headers=HEADERS, json={{"aliasName":"test_alias","collectionName":c}})
if r.json().get('code') != 0: print(f'alias create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"aliasName":"test_alias","collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/aliases/alter', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_alias_drop_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_adrop_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/aliases/create', headers=HEADERS, json={{"aliasName":"test_alias","collectionName":c}})
if r.json().get('code') != 0: print(f'alias create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"aliasName":"test_alias"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/aliases/drop', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_database_drop_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
db = 'oracle_dbdrop_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/databases/create', headers=HEADERS, json={{"dbName":db}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
body = {{"dbName":db}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/databases/drop', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_drop_nonexistent_database() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
r = requests.post(f'{BASE}/v2/vectordb/databases/drop', headers=HEADERS, json={"dbName":"nonexistent_db_" + uuid.uuid4().hex[:8]})
if r.json().get('code') == 0: print('[DEFECT: IDEMPOTENT_SUCCESS] drop nonexistent database accepted'); sys.exit(1)
else: print(f'properly rejected drop nonexistent database: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_database_list_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
body = {{}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/databases/list', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_partition_has_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_phass_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/partitions/create', headers=HEADERS, json={{"collectionName":c,"partitionName":"test_part"}})
if r.json().get('code') != 0: print(f'partition create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"partitionName":"test_part"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/partitions/has', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_collection_flush_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_flush_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":[{{"id":1,"vector":[0.1,0.2,0.3,0.4]}}]}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/flush', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_collection_compact_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_compact_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/collections/compact', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_index_describe_probe(param: &str, value: &str, label: &str) -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_idxdesc_' + uuid.uuid4().hex[:8]
{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
body = {{"collectionName":c,"indexName":"vector"}}
body["{param}"] = {value}
r = requests.post(f'{{BASE}}/v2/vectordb/indexes/describe', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        create = milvus_create_collection_default("c"),
        param = param,
        value = value,
        label = label,
    )
}

pub fn milvus_drop_nonexistent_index() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_dropidx_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/indexes/drop', headers=HEADERS, json={"collectionName":c,"indexName":"nonexistent_index"})
if r.json().get('code') == 0: print('[DEFECT: IDEMPOTENT_SUCCESS] drop nonexistent index accepted'); sys.exit(1)
else: print(f'properly rejected drop nonexistent index: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_describe_nonexistent_index() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_descidx_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/indexes/describe', headers=HEADERS, json={"collectionName":c,"indexName":"nonexistent_index"})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] describe nonexistent index accepted'); sys.exit(1)
else: print(f'properly rejected describe nonexistent index: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_search_nonexistent() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'nonexistent_' + uuid.uuid4().hex
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection returned code=0'); sys.exit(1)
else: print(f'properly rejected search on nonexistent collection: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_count_consistency_check() -> String {
    r#"import requests, sys, json, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_count_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
N = 5
data = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(N)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id >= 0","limit":100})
count = len(r.json().get('data', []))
if count != N:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] count mismatch: expected {N}, got {count}'); sys.exit(1)
else:
    print(f'count consistent: {count} == {N}'); sys.exit(0)"#.to_string()
}

pub fn milvus_nan_vector_check() -> String {
    r#"import requests, sys, json, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_nan_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = '{"collectionName":"' + c + '","data":[[NaN,0.2,0.3,0.4]],"limit":3}'
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, data=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] NaN vector accepted'); sys.exit(1)
else: print(f'properly rejected NaN vector: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_inf_vector_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_inf_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = '{"collectionName":"' + c + '","data":[[Infinity,0.2,0.3,0.4]],"limit":3}'
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, data=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] Infinity vector accepted'); sys.exit(1)
else: print(f'Infinity vector properly rejected: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_empty_vector_search_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_empty_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[]],"limit":3})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] empty vector accepted'); sys.exit(1)
else: print(f'empty vector properly rejected: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_duplicate_collection_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_dup_' + uuid.uuid4().hex[:8]
r1 = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r1.json().get('code') != 0: print(f'setup failed: {r1.text}'); sys.exit(0)
time.sleep(0.5)
r2 = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r2.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] duplicate collection name accepted (code=0)'); sys.exit(1)
else: print(f'properly rejected duplicate collection: {r2.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_invalid_metric_check() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_dist_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"InvalidMetric","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] invalid metricType accepted'); sys.exit(1)
else: print(f'properly rejected invalid metricType: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_invalid_index_type_check() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_idx_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"InvalidIndex"}]})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] invalid indexType accepted'); sys.exit(1)
else: print(f'properly rejected invalid indexType: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_upsert_nan_vector_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_upnan_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = '{"collectionName":"' + c + '","data":[{"id":1,"vector":[NaN,0.2,0.3,0.4]}]}'
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, data=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] NaN vector insert accepted'); sys.exit(1)
else: print(f'NaN vector properly rejected: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_upsert_inf_vector_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_upinf_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = '{"collectionName":"' + c + '","data":[{"id":1,"vector":[Infinity,0.2,0.3,0.4]}]}'
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, data=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] Infinity vector insert accepted'); sys.exit(1)
else: print(f'Infinity vector properly rejected: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_wrong_dimension_insert_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_wdim_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3]}]})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] wrong dimension vector insert accepted'); sys.exit(1)
else: print(f'wrong dimension properly rejected: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_delete_count_consistency_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_delcnt_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
N = 5
data = [{"id": i+1, "vector": [0.1*(i+1), 0.2*(i+1), 0.3*(i+1), 0.4*(i+1)]} for i in range(N)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
M = 2
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id <= 2"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id >= 0","limit":100})
count = len(r.json().get('data', []))
if count != N - M:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] count mismatch after delete: expected {N-M}, got {count}'); sys.exit(1)
else:
    print(f'count consistent after delete: {count} == {N-M}'); sys.exit(0)"#.to_string()
}

pub fn milvus_upsert_idempotency_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_upidem_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'first upsert failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.5,0.6,0.7,0.8]}]})
if r.json().get('code') != 0: print(f'second upsert failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id >= 0","limit":100})
count = len(r.json().get('data', []))
if count != 1:
    print(f'[DEFECT: STATE_LOGIC_VIOLATION] upsert same id twice: expected 1, got {count}'); sys.exit(1)
else:
    print(f'upsert idempotent: {count} == 1'); sys.exit(0)"#.to_string()
}

pub fn milvus_delete_empty_filter_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_delempty_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":""})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] empty filter delete accepted'); sys.exit(1)
else: print(f'properly rejected empty filter delete: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_delete_null_filter_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_delnull_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = {"collectionName":c}
body["filter"] = None
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json=body)
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] null filter delete accepted'); sys.exit(1)
else: print(f'properly rejected null filter delete: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_delete_nonexistent_id_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_delnonexist_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 999999"})
if r.json().get('code') != 0: print(f'delete nonexistent id returned error: {r.json()}'); sys.exit(0)
print(f'delete nonexistent id returned success (idempotent): {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_delete_then_query_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_delquery_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]},{"id":2,"vector":[0.5,0.6,0.7,0.8]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
if r.json().get('code') != 0: print(f'delete failed: {r.text}'); sys.exit(0)
r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(3)
r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id >= 0","limit":100})
ids = [d.get('id') for d in r.json().get('data',[])]
if 1 in ids: print(f'[DEFECT: STATE_LOGIC_VIOLATION] deleted id=1 still in query results: {ids}'); sys.exit(1)
if 2 not in ids: print(f'[DEFECT: STATE_LOGIC_VIOLATION] non-deleted id=2 missing from query results: {ids}'); sys.exit(1)
print(f'delete-then-query verified: deleted id gone, other ids present'); sys.exit(0)"#.to_string()
}

pub fn milvus_drop_nonexistent_collection() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'nonexistent_coll_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') == 0: print('[DEFECT: IDEMPOTENT_SUCCESS] drop nonexistent collection accepted'); sys.exit(1)
else: print(f'properly rejected drop nonexistent collection: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_drop_then_describe_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_dropdesc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'drop failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') == 0: print('[DEFECT: STATE_LOGIC_VIOLATION] describe dropped collection returned success'); sys.exit(1)
else: print(f'properly rejected describe dropped collection: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_describe_nonexistent_collection() -> String {
    r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'nonexistent_desc_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') == 0: print('[DEFECT: ILLEGAL_SUCCESS] describe nonexistent collection returned success'); sys.exit(1)
else: print(f'properly rejected describe nonexistent collection: {r.json()}'); sys.exit(0)"#.to_string()
}

pub fn milvus_search_score_ordering_check() -> String {
    r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_score_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
data = [{"id":1,"vector":[1.0,0.0,0.0,0.0]},{"id":2,"vector":[0.9,0.1,0.0,0.0]},{"id":3,"vector":[0.5,0.5,0.0,0.0]}]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[1.0,0.0,0.0,0.0]],"limit":3})
if r.json().get('code') != 0: print(f'search failed: {r.text}'); sys.exit(0)
results = r.json().get('data', [])
distances = [d.get('distance', 0) for d in results]
for i in range(len(distances)-1):
    if distances[i] < distances[i+1]:
        print(f'[DEFECT: STATE_LOGIC_VIOLATION] COSINE distances not descending: {distances}'); sys.exit(1)
print(f'score ordering correct: {distances}'); sys.exit(0)"#.to_string()
}

pub struct MilvusSimpleSafetyNet {
    pub name: String,
    pub param: String,
    pub value: String,
    pub label: String,
    pub redundant_with_mutation: bool,
}

impl MilvusSimpleSafetyNet {
    pub fn to_search_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_search_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_search_params_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_search_params_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_create_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_create_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_index_create_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_index_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_partition_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_partition_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_alias_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_alias_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_database_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_database_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_hybrid_search_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_hybrid_search_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_mgmt_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_mgmt_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_index_describe_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_index_describe_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_partition_drop_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_partition_drop_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_rename_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_rename_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_alter_properties_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_alter_properties_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_add_field_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_add_field_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_get_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_get_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_index_list_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_index_list_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_partition_list_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_partition_list_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_alias_list_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_alias_list_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_list_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_list_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_has_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_has_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_stats_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_stats_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_release_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_release_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_alias_alter_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_alias_alter_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_alias_drop_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_alias_drop_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_database_drop_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_database_drop_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_database_list_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_database_list_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_partition_has_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_partition_has_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_flush_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_flush_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }

    pub fn to_collection_compact_safety_net(&self) -> SafetyNet {
        SafetyNet {
            name: self.name.clone(),
            script: milvus_collection_compact_probe(&self.param, &self.value, &self.label),
            redundant_with_mutation: self.redundant_with_mutation,
        }
    }
}

pub fn milvus_l2_distance_ordering_check() -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":4}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"L2","indexType":"AUTOINDEX"}}]}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
data = [{{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(20)]
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":data}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5}})
if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance',0) for d in results]
if distances != sorted(distances): print(f'[DEFECT: METAMORPHIC_VIOLATION] L2 distances not ascending: {{distances}}'); sys.exit(1)
else: print(f'L2 ordering verified: {{distances}}'); sys.exit(0)"#
    )
}

pub fn milvus_ip_distance_ordering_check() -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":4}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"IP","indexType":"AUTOINDEX"}}]}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
data = [{{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(20)]
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":data}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5}})
if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance',0) for d in results]
if distances != sorted(distances, reverse=True): print(f'[DEFECT: METAMORPHIC_VIOLATION] IP distances not descending: {{distances}}'); sys.exit(1)
else: print(f'IP ordering verified: {{distances}}'); sys.exit(0)"#
    )
}

pub fn milvus_hamming_search_check() -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"BinaryVector","elementTypeParams":{{"dim":32}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"HAMMING","indexType":"BIN_IVF_FLAT"}}]}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
data = [{{"id":i,"vector":[1 if j%2==i%2 else 0 for j in range(32)]}} for i in range(10)]
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":data}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{"collectionName":c,"data":[[1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0]],"limit":5}})
if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance',0) for d in results]
if distances != sorted(distances): print(f'[DEFECT: METAMORPHIC_VIOLATION] HAMMING distances not ascending: {{distances}}'); sys.exit(1)
else: print(f'HAMMING ordering verified: {{distances}}'); sys.exit(0)"#
    )
}

pub fn milvus_jaccard_search_check() -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"BinaryVector","elementTypeParams":{{"dim":32}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"JACCARD","indexType":"BIN_IVF_FLAT"}}]}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
data = [{{"id":i,"vector":[1 if j%2==i%2 else 0 for j in range(32)]}} for i in range(10)]
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":data}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/v2/vectordb/collections/load', headers=HEADERS, json={{"collectionName":c}})
if r.json().get('code') != 0: print(f'load failed: {{r.text}}'); sys.exit(0)
time.sleep(2)
r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{"collectionName":c,"data":[[1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0,1,0]],"limit":5}})
if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)
results = r.json().get('data',[])
distances = [d.get('distance',0) for d in results]
if distances != sorted(distances): print(f'[DEFECT: METAMORPHIC_VIOLATION] JACCARD distances not ascending: {{distances}}'); sys.exit(1)
else: print(f'JACCARD ordering verified: {{distances}}'); sys.exit(0)"#
    )
}

pub fn milvus_auto_id_check() -> String {
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":True,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True,"autoID":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":4}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}}]}})
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
data = [{{"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(5)]
r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{"collectionName":c,"data":data}})
if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)
insert_ids = r.json().get('data',{{}}).get('insertIds',[])
if len(insert_ids) != 5: print(f'[DEFECT: METAMORPHIC_VIOLATION] autoID insert returned {{len(insert_ids)}} ids, expected 5'); sys.exit(1)
if any(i is None for i in insert_ids): print(f'[DEFECT: METAMORPHIC_VIOLATION] autoID returned None ids: {{insert_ids}}'); sys.exit(1)
print(f'autoID verified: {{insert_ids}}'); sys.exit(0)"#
    )
}

pub fn generate_mutation_probe(
    endpoint: &str,
    base_body: &str,
    mutation_line: &str,
    label: &str,
    needs_setup: bool,
) -> String {
    generate_mutation_probe_with_marker(endpoint, base_body, mutation_line, label, needs_setup, "ILLEGAL_SUCCESS")
}

pub fn generate_mutation_probe_with_marker(
    endpoint: &str,
    base_body: &str,
    mutation_line: &str,
    label: &str,
    needs_setup: bool,
    defect_marker: &str,
) -> String {
    let setup_block = if needs_setup {
        format!(
            r#"{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
"#,
            create = milvus_create_collection_default("c"),
        )
    } else {
        String::new()
    };
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{setup_block}body = {base_body}
{mutation_line}
r = requests.post(f'{{BASE}}{endpoint}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: {defect_marker}] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        setup_block = setup_block,
        base_body = base_body,
        mutation_line = mutation_line,
        endpoint = endpoint,
        label = label,
        defect_marker = defect_marker,
    )
}

pub fn generate_mutation_probe_with_marker_no_index(
    endpoint: &str,
    base_body: &str,
    mutation_line: &str,
    label: &str,
    defect_marker: &str,
) -> String {
    let setup_block = format!(
        r#"{create}
if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
"#,
        create = milvus_create_collection_no_index("c"),
    );
    format!(
        r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_' + uuid.uuid4().hex[:8]
{setup_block}body = {base_body}
{mutation_line}
r = requests.post(f'{{BASE}}{endpoint}', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: {defect_marker}] {label} accepted'); sys.exit(1)
else: print(f'properly rejected {label}: {{r.json()}}'); sys.exit(0)"#,
        setup_block = setup_block,
        base_body = base_body,
        mutation_line = mutation_line,
        endpoint = endpoint,
        label = label,
        defect_marker = defect_marker,
    )
}

pub fn generate_missing_field_probe(
    endpoint: &str,
    base_body: &str,
    target_field: &str,
    label: &str,
    needs_setup: bool,
) -> String {
    generate_mutation_probe(
        endpoint,
        base_body,
        &format!(r#"body.pop("{target_field}", None)"#),
        label,
        needs_setup,
    )
}

pub fn generate_oversized_data_probe(
    endpoint: &str,
    label: &str,
) -> String {
    generate_mutation_probe(
        endpoint,
        r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#,
        r#"body["data"] = [{"id":i,"vector":[0.1]*4} for i in range(10000)]"#,
        label,
        true,
    )
}

pub fn milvus_create_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["schema"]["fields"][1]["elementTypeParams"]["dim"] = "not_a_number""#, "type_confusion_string_dimension", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = None"#, "null_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["schema"]["fields"][1]["elementTypeParams"]["dim"] = 32768"#, "oversized_dimension", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["schema"]["fields"][1]["elementTypeParams"]["dim"] = 3.4e38"#, "boundary_float_dimension", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("create_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/collections/create", base_body, mutation_line, label, false, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_insert_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["data"] = "not_an_array""#, "type_confusion_string_data", "ILLEGAL_SUCCESS"),
        (r#"body["data"] = None"#, "null_injection_data", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["data"] = [{"id":i,"vector":[0.1]*4} for i in range(10000)]"#, "oversized_data", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["data"] = [{"id":1,"vector":[3.4e38,0.2,0.3,0.4]}]"#, "boundary_float_data", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("insert_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/entities/insert", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_search_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["limit"] = True"#, "type_confusion_bool_limit", "ILLEGAL_SUCCESS"),
        (r#"body["data"] = None"#, "null_injection_data", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["limit"] = 999999"#, "oversized_limit", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["data"] = [[3.4e38,0.2,0.3,0.4]]"#, "boundary_float_data", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("search_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/entities/search", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_query_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"filter":"id > 0","limit":3}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["limit"] = "not_a_number""#, "type_confusion_string_limit", "ILLEGAL_SUCCESS"),
        (r#"body["filter"] = None"#, "null_injection_filter", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["limit"] = 999999"#, "oversized_limit", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["limit"] = 3.4e38"#, "boundary_float_limit", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("query_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/entities/query", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_upsert_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["data"] = "not_an_array""#, "type_confusion_string_data", "ILLEGAL_SUCCESS"),
        (r#"body["data"] = None"#, "null_injection_data", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["data"] = [{"id":i,"vector":[0.1]*4} for i in range(10000)]"#, "oversized_data", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["data"] = [{"id":1,"vector":[3.4e38,0.2,0.3,0.4]}]"#, "boundary_float_data", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("upsert_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/entities/upsert", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_index_create_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["indexParams"][0]["indexType"] = "not_an_index""#, "type_confusion_string_indexType", "ILLEGAL_SUCCESS"),
        (r#"body["indexParams"] = None"#, "null_injection_indexParams", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["indexParams"][0]["metricType"] = "INVALID_METRIC""#, "invalid_metricType", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["indexParams"][0]["params"] = {"nlist": -1}"#, "negative_nlist", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("index_create_mutation_{label}"),
            script: generate_mutation_probe_with_marker_no_index("/v2/vectordb/indexes/create", base_body, mutation_line, label, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_delete_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"filter":"id > 0"}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["filter"] = "not_a_filter""#, "type_confusion_string_filter", "ILLEGAL_SUCCESS"),
        (r#"body["filter"] = None"#, "null_injection_filter", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["filter"] = "id > 0; DROP TABLE""#, "sql_injection_filter", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["filter"] = """#, "empty_filter", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("delete_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/entities/delete", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_partition_create_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"collectionName":c,"partitionName":"test_partition"}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["partitionName"] = 123"#, "type_confusion_int_partitionName", "ILLEGAL_SUCCESS"),
        (r#"body["partitionName"] = None"#, "null_injection_partitionName", "ILLEGAL_SUCCESS"),
        (r#"body.pop("collectionName", None)"#, "missing_required_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["collectionName"] = "test'; DROP TABLE--""#, "unicode_injection_collectionName", "ILLEGAL_SUCCESS"),
        (r#"body["partitionName"] = """#, "empty_partitionName", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["partitionName"] = "test'; DROP TABLE--""#, "unicode_injection_partitionName", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("partition_create_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/partitions/create", base_body, mutation_line, label, true, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

pub fn milvus_database_create_mutation_probes() -> Vec<SafetyNet> {
    let base_body = r#"{"dbName":"test_db"}"#;
    let mutations: Vec<(&str, &str, &str)> = vec![
        (r#"body["dbName"] = 123"#, "type_confusion_int_dbName", "ILLEGAL_SUCCESS"),
        (r#"body["dbName"] = None"#, "null_injection_dbName", "ILLEGAL_SUCCESS"),
        (r#"body.pop("dbName", None)"#, "missing_required_dbName", "ILLEGAL_SUCCESS"),
        (r#"body["dbName"] = "test'; DROP TABLE--""#, "unicode_injection_dbName", "ILLEGAL_SUCCESS"),
        (r#"body["dbName"] = """#, "empty_dbName", "ILLEGAL_SUCCESS"),
        (r#"body["unknownParam"] = 123"#, "unknown_param", "PERMISSIVE_PARSING"),
        (r#"body["extraField"] = "unexpected""#, "extra_fields", "PERMISSIVE_PARSING"),
        (r#"body["dbName"] = "default""#, "duplicate_default_db", "ILLEGAL_SUCCESS"),
    ];
    mutations.into_iter().map(|(mutation_line, label, marker)| {
        SafetyNet {
            name: format!("database_create_mutation_{label}"),
            script: generate_mutation_probe_with_marker("/v2/vectordb/databases/create", base_body, mutation_line, label, false, marker),
            redundant_with_mutation: true,
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_milvus_search_probe_contains_auth() {
        let script = milvus_search_probe("limit", "0", "limit=0");
        assert!(script.contains("Authorization"));
        assert!(script.contains("root:Milvus"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_search_probe_contains_api_path() {
        let script = milvus_search_probe("limit", "0", "limit=0");
        assert!(script.contains("/v2/vectordb/entities/search"));
        assert!(script.contains("/v2/vectordb/collections/create"));
    }

    #[test]
    fn test_milvus_create_probe_dim() {
        let script = milvus_create_probe("dim", "0", "dim=0");
        assert!(script.contains("dim"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_create_probe_metric_type() {
        let script = milvus_create_probe("metricType", "INVALID", "metricType=INVALID");
        assert!(script.contains("INVALID"));
        assert!(script.contains("metricType"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_create_probe_index_type() {
        let script = milvus_create_probe("indexType", "InvalidIndex", "indexType=InvalidIndex");
        assert!(script.contains("InvalidIndex"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_nonexistent_check() {
        let script = milvus_search_nonexistent();
        assert!(script.contains("nonexistent_"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_simple_safety_net() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_limit_zero".to_string(),
            param: "limit".to_string(),
            value: "0".to_string(),
            label: "limit=0".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_search_safety_net();
        assert_eq!(sn.name, "test_limit_zero");
        assert!(sn.script.contains("limit"));
        assert!(sn.script.contains("/v2/vectordb/"));
    }

    #[test]
    fn test_milvus_index_probe_index_type() {
        let script = milvus_index_probe("indexType", "InvalidIndex", "invalid indexType");
        assert!(script.contains("/v2/vectordb/indexes/create"));
        assert!(script.contains("InvalidIndex"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_index_probe_metric_type() {
        let script = milvus_index_probe("metricType", "INVALID", "invalid metricType");
        assert!(script.contains("/v2/vectordb/indexes/create"));
        assert!(script.contains("INVALID"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_index_probe_nlist() {
        let script = milvus_index_probe("nlist", "0", "nlist=0");
        assert!(script.contains("/v2/vectordb/indexes/create"));
        assert!(script.contains("nlist"));
        assert!(script.contains("IVF_FLAT"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_partition_probe() {
        let script = milvus_partition_probe("partitionName", "''", "empty partition name");
        assert!(script.contains("/v2/vectordb/partitions/create"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_alias_probe() {
        let script = milvus_alias_probe("aliasName", "''", "empty alias name");
        assert!(script.contains("/v2/vectordb/aliases/create"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_database_probe() {
        let script = milvus_database_probe("dbName", "''", "empty db name");
        assert!(script.contains("/v2/vectordb/databases/create"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_hybrid_search_probe() {
        let script = milvus_hybrid_search_probe("limit", "0", "limit=0");
        assert!(script.contains("/v2/vectordb/entities/hybrid_search"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_mgmt_probe() {
        let script = milvus_collection_mgmt_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/collections/load"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_index_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_index".to_string(),
            param: "indexType".to_string(),
            value: "InvalidIndex".to_string(),
            label: "invalid indexType".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_index_create_safety_net();
        assert_eq!(sn.name, "test_index");
        assert!(sn.script.contains("/v2/vectordb/indexes/create"));
    }

    #[test]
    fn test_milvus_partition_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_partition".to_string(),
            param: "partitionName".to_string(),
            value: "''".to_string(),
            label: "empty partition".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_partition_safety_net();
        assert!(sn.script.contains("/v2/vectordb/partitions/create"));
    }

    #[test]
    fn test_milvus_alias_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_alias".to_string(),
            param: "aliasName".to_string(),
            value: "''".to_string(),
            label: "empty alias".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_alias_safety_net();
        assert!(sn.script.contains("/v2/vectordb/aliases/create"));
    }

    #[test]
    fn test_milvus_database_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_db".to_string(),
            param: "dbName".to_string(),
            value: "''".to_string(),
            label: "empty db name".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_database_safety_net();
        assert!(sn.script.contains("/v2/vectordb/databases/create"));
    }

    #[test]
    fn test_milvus_hybrid_search_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_hybrid".to_string(),
            param: "limit".to_string(),
            value: "0".to_string(),
            label: "limit=0".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_hybrid_search_safety_net();
        assert!(sn.script.contains("/v2/vectordb/entities/hybrid_search"));
    }

    #[test]
    fn test_milvus_collection_mgmt_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_mgmt".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_mgmt_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/load"));
    }

    #[test]
    fn test_milvus_index_describe_probe() {
        let script = milvus_index_describe_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/indexes/describe"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_drop_nonexistent_index() {
        let script = milvus_drop_nonexistent_index();
        assert!(script.contains("/v2/vectordb/indexes/drop"));
        assert!(script.contains("nonexistent_index"));
        assert!(script.contains("IDEMPOTENT_SUCCESS"));
    }

    #[test]
    fn test_milvus_describe_nonexistent_index() {
        let script = milvus_describe_nonexistent_index();
        assert!(script.contains("/v2/vectordb/indexes/describe"));
        assert!(script.contains("nonexistent_index"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_index_describe_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_idx_desc".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_index_describe_safety_net();
        assert!(sn.script.contains("/v2/vectordb/indexes/describe"));
    }

    #[test]
    fn test_milvus_partition_drop_probe() {
        let script = milvus_partition_drop_probe("partitionName", "''", "empty partition name");
        assert!(script.contains("/v2/vectordb/partitions/drop"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_rename_probe() {
        let script = milvus_collection_rename_probe("newCollectionName", "''", "empty new name");
        assert!(script.contains("/v2/vectordb/collections/rename"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_alter_properties_probe() {
        let script = milvus_alter_properties_probe("properties", "{}", "empty properties");
        assert!(script.contains("/v2/vectordb/collections/alter_properties"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_add_field_probe() {
        let script = milvus_add_field_probe("fieldName", "''", "empty field name");
        assert!(script.contains("/v2/vectordb/collections/fields/add"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_get_probe() {
        let script = milvus_get_probe("id", "[]", "empty id array");
        assert!(script.contains("/v2/vectordb/entities/get"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_drop_nonexistent_partition() {
        let script = milvus_drop_nonexistent_partition();
        assert!(script.contains("/v2/vectordb/partitions/drop"));
        assert!(script.contains("nonexistent_partition"));
        assert!(script.contains("IDEMPOTENT_SUCCESS"));
    }

    #[test]
    fn test_milvus_add_vector_field_check() {
        let script = milvus_add_vector_field_check();
        assert!(script.contains("/v2/vectordb/collections/fields/add"));
        assert!(script.contains("duplicate vector field"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_partition_drop_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_pdrop".to_string(),
            param: "partitionName".to_string(),
            value: "''".to_string(),
            label: "empty partition".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_partition_drop_safety_net();
        assert!(sn.script.contains("/v2/vectordb/partitions/drop"));
    }

    #[test]
    fn test_milvus_rename_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_rename".to_string(),
            param: "newCollectionName".to_string(),
            value: "''".to_string(),
            label: "empty new name".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_rename_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/rename"));
    }

    #[test]
    fn test_milvus_alter_properties_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_alter".to_string(),
            param: "properties".to_string(),
            value: "{}".to_string(),
            label: "empty properties".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_alter_properties_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/alter_properties"));
    }

    #[test]
    fn test_milvus_add_field_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_addfield".to_string(),
            param: "fieldName".to_string(),
            value: "''".to_string(),
            label: "empty field name".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_add_field_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/fields/add"));
    }

    #[test]
    fn test_milvus_get_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_get".to_string(),
            param: "id".to_string(),
            value: "[]".to_string(),
            label: "empty id array".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_get_safety_net();
        assert!(sn.script.contains("/v2/vectordb/entities/get"));
    }

    #[test]
    fn test_milvus_index_list_probe() {
        let script = milvus_index_list_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/indexes/list"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_partition_list_probe() {
        let script = milvus_partition_list_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/partitions/list"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_alias_list_probe() {
        let script = milvus_alias_list_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/aliases/list"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_list_probe() {
        let script = milvus_collection_list_probe("dbName", "''", "empty db name");
        assert!(script.contains("/v2/vectordb/collections/list"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_has_probe() {
        let script = milvus_collection_has_probe("collectionName", "'nonexistent'", "nonexistent collection");
        assert!(script.contains("/v2/vectordb/collections/has"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_stats_probe() {
        let script = milvus_collection_stats_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/collections/get_stats"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_release_probe() {
        let script = milvus_collection_release_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/collections/release"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_alias_alter_probe() {
        let script = milvus_alias_alter_probe("collectionName", "'nonexistent'", "nonexistent collection");
        assert!(script.contains("/v2/vectordb/aliases/alter"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_alias_drop_probe() {
        let script = milvus_alias_drop_probe("aliasName", "''", "empty alias name");
        assert!(script.contains("/v2/vectordb/aliases/drop"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_database_drop_probe() {
        let script = milvus_database_drop_probe("dbName", "'nonexistent'", "nonexistent database");
        assert!(script.contains("/v2/vectordb/databases/drop"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_database_list_probe() {
        let script = milvus_database_list_probe("invalidParam", "'test'", "invalid param");
        assert!(script.contains("/v2/vectordb/databases/list"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_partition_has_probe() {
        let script = milvus_partition_has_probe("partitionName", "'nonexistent'", "nonexistent partition");
        assert!(script.contains("/v2/vectordb/partitions/has"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_flush_probe() {
        let script = milvus_collection_flush_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/collections/flush"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_collection_compact_probe() {
        let script = milvus_collection_compact_probe("collectionName", "''", "empty collection name");
        assert!(script.contains("/v2/vectordb/collections/compact"));
        assert!(script.contains("ILLEGAL_SUCCESS"));
    }

    #[test]
    fn test_milvus_index_list_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_idx_list".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_index_list_safety_net();
        assert!(sn.script.contains("/v2/vectordb/indexes/list"));
    }

    #[test]
    fn test_milvus_partition_list_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_part_list".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_partition_list_safety_net();
        assert!(sn.script.contains("/v2/vectordb/partitions/list"));
    }

    #[test]
    fn test_milvus_alias_list_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_alias_list".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_alias_list_safety_net();
        assert!(sn.script.contains("/v2/vectordb/aliases/list"));
    }

    #[test]
    fn test_milvus_collection_list_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_coll_list".to_string(),
            param: "dbName".to_string(),
            value: "''".to_string(),
            label: "empty db name".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_list_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/list"));
    }

    #[test]
    fn test_milvus_collection_has_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_coll_has".to_string(),
            param: "collectionName".to_string(),
            value: "'nonexistent'".to_string(),
            label: "nonexistent collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_has_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/has"));
    }

    #[test]
    fn test_milvus_collection_stats_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_coll_stats".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_stats_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/get_stats"));
    }

    #[test]
    fn test_milvus_collection_release_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_release".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_release_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/release"));
    }

    #[test]
    fn test_milvus_alias_alter_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_alias_alter".to_string(),
            param: "collectionName".to_string(),
            value: "'nonexistent'".to_string(),
            label: "nonexistent collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_alias_alter_safety_net();
        assert!(sn.script.contains("/v2/vectordb/aliases/alter"));
    }

    #[test]
    fn test_milvus_alias_drop_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_alias_drop".to_string(),
            param: "aliasName".to_string(),
            value: "''".to_string(),
            label: "empty alias".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_alias_drop_safety_net();
        assert!(sn.script.contains("/v2/vectordb/aliases/drop"));
    }

    #[test]
    fn test_milvus_database_drop_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_db_drop".to_string(),
            param: "dbName".to_string(),
            value: "'nonexistent'".to_string(),
            label: "nonexistent db".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_database_drop_safety_net();
        assert!(sn.script.contains("/v2/vectordb/databases/drop"));
    }

    #[test]
    fn test_milvus_database_list_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_db_list".to_string(),
            param: "invalidParam".to_string(),
            value: "'test'".to_string(),
            label: "invalid param".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_database_list_safety_net();
        assert!(sn.script.contains("/v2/vectordb/databases/list"));
    }

    #[test]
    fn test_milvus_partition_has_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_part_has".to_string(),
            param: "partitionName".to_string(),
            value: "'nonexistent'".to_string(),
            label: "nonexistent partition".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_partition_has_safety_net();
        assert!(sn.script.contains("/v2/vectordb/partitions/has"));
    }

    #[test]
    fn test_milvus_collection_flush_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_flush".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_flush_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/flush"));
    }

    #[test]
    fn test_milvus_collection_compact_safety_net_adapter() {
        let spec = MilvusSimpleSafetyNet {
            name: "test_compact".to_string(),
            param: "collectionName".to_string(),
            value: "''".to_string(),
            label: "empty collection".to_string(),
            redundant_with_mutation: false,
        };
        let sn = spec.to_collection_compact_safety_net();
        assert!(sn.script.contains("/v2/vectordb/collections/compact"));
    }

    #[test]
    fn test_milvus_l2_distance_ordering_check() {
        let script = milvus_l2_distance_ordering_check();
        assert!(script.contains("L2"));
        assert!(script.contains("sorted(distances)"));
        assert!(script.contains("METAMORPHIC_VIOLATION"));
        assert!(script.contains("/v2/vectordb/entities/search"));
    }

    #[test]
    fn test_milvus_ip_distance_ordering_check() {
        let script = milvus_ip_distance_ordering_check();
        assert!(script.contains("IP"));
        assert!(script.contains("reverse=True"));
        assert!(script.contains("METAMORPHIC_VIOLATION"));
        assert!(script.contains("/v2/vectordb/entities/search"));
    }

    #[test]
    fn test_milvus_hamming_search_check() {
        let script = milvus_hamming_search_check();
        assert!(script.contains("HAMMING"));
        assert!(script.contains("BinaryVector"));
        assert!(script.contains("BIN_IVF_FLAT"));
        assert!(script.contains("sorted(distances)"));
    }

    #[test]
    fn test_milvus_jaccard_search_check() {
        let script = milvus_jaccard_search_check();
        assert!(script.contains("JACCARD"));
        assert!(script.contains("BinaryVector"));
        assert!(script.contains("BIN_IVF_FLAT"));
        assert!(script.contains("sorted(distances)"));
    }

    #[test]
    fn test_milvus_auto_id_check() {
        let script = milvus_auto_id_check();
        assert!(script.contains("autoID"));
        assert!(script.contains("insertIds"));
        assert!(script.contains("METAMORPHIC_VIOLATION"));
        assert!(script.contains("/v2/vectordb/entities/insert"));
    }
}
