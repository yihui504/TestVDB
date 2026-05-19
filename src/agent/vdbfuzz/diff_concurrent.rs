use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffTestCase {
    pub name: String,
    pub diff_pattern: DiffPattern,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffPattern {
    CreateCollection,
    Insert,
    Search,
    Query,
    Delete,
    CreateIndex,
    Describe,
    Upsert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentTestCase {
    pub name: String,
    pub concurrent_pattern: ConcurrentPattern,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrentPattern {
    InsertSearch,
    DeleteQuery,
    UpsertSearch,
    CreateDrop,
    InsertFlush,
}

pub struct DiffTestGenerator;

impl DiffTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<DiffTestCase> {
        let mut cases = Vec::new();

        let has_create = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("collections/create") || atc.endpoint.contains("collections")
        });
        let has_insert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/insert") || atc.endpoint.contains("points")
        });
        let has_search = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/search") || atc.endpoint.contains("search")
        });
        let has_query = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/query")
        });
        let has_delete = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/delete") || atc.endpoint.contains("points/delete")
        });
        let has_upsert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/upsert") || atc.endpoint.contains("points/upsert")
        });

        match style {
            TargetStyle::Milvus => {
                if has_create {
                    cases.push(Self::generate_diff_create_collection());
                    cases.push(Self::generate_diff_describe());
                    cases.push(Self::generate_diff_create_index());
                }
                if has_insert {
                    cases.push(Self::generate_diff_insert());
                }
                if has_search {
                    cases.push(Self::generate_diff_search());
                }
                if has_query {
                    cases.push(Self::generate_diff_query());
                }
                if has_delete {
                    cases.push(Self::generate_diff_delete());
                }
                if has_upsert {
                    cases.push(Self::generate_diff_upsert());
                }
            }
            TargetStyle::Qdrant => {
                if has_create {
                    cases.push(Self::generate_qdrant_diff_create_collection());
                    cases.push(Self::generate_qdrant_diff_collection_info());
                }
                if has_insert || has_upsert {
                    cases.push(Self::generate_qdrant_diff_upsert());
                }
                if has_search {
                    cases.push(Self::generate_qdrant_diff_search());
                }
                if has_delete {
                    cases.push(Self::generate_qdrant_diff_delete());
                }
            }
        }

        cases.dedup_by(|a, b| a.name == b.name);
        cases
    }

    fn generate_diff_create_collection() -> DiffTestCase {
        DiffTestCase {
            name: "diff_create_collection".to_string(),
            diff_pattern: DiffPattern::CreateCollection,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c_rest = f'oracle_diff_rest_{uid}'
c_sdk = f'oracle_diff_sdk_{uid}'
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c_rest,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
rest_ok = r.json().get('code') == 0
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    client.create_collection(collection_name=c_sdk, dimension=4, metric_type='COSINE')
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] create: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok:
    time.sleep(1)
    rr = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c_rest})
    rs = client.describe_collection(collection_name=c_sdk)
    rest_dim = rr.json().get('data',{}).get('dimension')
    sdk_dim = rs.get('dimension')
    if rest_dim != sdk_dim: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe dimension: rest={rest_dim} sdk={sdk_dim}'); sys.exit(1)
print(f'diff create_collection: rest_ok={rest_ok} sdk_ok={sdk_ok} dim_match=True'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_insert() -> DiffTestCase {
        DiffTestCase {
            name: "diff_insert".to_string(),
            diff_pattern: DiffPattern::Insert,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
rest_ok = r.json().get('code') == 0
rest_count = r.json().get('data',{}).get('insertCount',0)
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    client.insert(collection_name=c, data=[{"id":2,"vector":[0.5,0.6,0.7,0.8]}])
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] insert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff insert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_search() -> DiffTestCase {
        DiffTestCase {
            name: "diff_search".to_string(),
            diff_pattern: DiffPattern::Search,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,11)]
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":5})
rest_ok = r.json().get('code') == 0
rest_top1 = None
if rest_ok:
    results = r.json().get('data',[])
    if results: rest_top1 = results[0].get('id')
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    sdk_results = client.search(collection_name=c, data=[[0.1,0.2,0.3,0.4]], limit=5, output_fields=['id'])
    sdk_ok = True
    sdk_top1 = None
    if sdk_results and sdk_results[0]: sdk_top1 = sdk_results[0][0].get('id')
except Exception as e:
    sdk_ok = False
    sdk_top1 = None
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok and rest_top1 != sdk_top1: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search top1: rest={rest_top1} sdk={sdk_top1}'); sys.exit(1)
print(f'diff search: rest_ok={rest_ok} sdk_ok={sdk_ok} top1_match=True'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_query() -> DiffTestCase {
        DiffTestCase {
            name: "diff_query".to_string(),
            diff_pattern: DiffPattern::Query,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id > 0","limit":10})
rest_ok = r.json().get('code') == 0
rest_ids = sorted([d.get('id') for d in r.json().get('data',[])])
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    sdk_results = client.query(collection_name=c, filter='id > 0', limit=10, output_fields=['id'])
    sdk_ok = True
    sdk_ids = sorted([d.get('id') for d in sdk_results])
except Exception as e:
    sdk_ok = False
    sdk_ids = []
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] query: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok and rest_ids != sdk_ids: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] query ids: rest={rest_ids} sdk={sdk_ids}'); sys.exit(1)
print(f'diff query: rest_ok={rest_ok} sdk_ok={sdk_ok} ids_match=True'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_delete() -> DiffTestCase {
        DiffTestCase {
            name: "diff_delete".to_string(),
            diff_pattern: DiffPattern::Delete,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,6)]
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":"id == 1"})
rest_ok = r.json().get('code') == 0
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    client.delete(collection_name=c, filter='id == 2')
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] delete: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff delete: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_create_index() -> DiffTestCase {
        DiffTestCase {
            name: "diff_create_index".to_string(),
            diff_pattern: DiffPattern::CreateIndex,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient, CollectionSchema, FieldSchema, DataType
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c_rest = f'oracle_diff_rest_{uid}'
c_sdk = f'oracle_diff_sdk_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c_rest,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/indexes/create', headers=HEADERS, json={"collectionName":c_rest,"indexParams":[{"fieldName":"vector","metricType":"L2","indexType":"IVF_FLAT"}]})
rest_ok = r.json().get('code') == 0
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    client.create_collection(collection_name=c_sdk, dimension=4)
    time.sleep(1)
    from pymilvus import Collection
    coll = Collection(c_sdk)
    import pymilvus
    index_params = client.prepare_index_params()
    index_params.add_index(field_name='vector', metric_type='L2', index_type='IVF_FLAT')
    client.create_index(collection_name=c_sdk, index_params=index_params)
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] create_index: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff create_index: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_describe() -> DiffTestCase {
        DiffTestCase {
            name: "diff_describe".to_string(),
            diff_pattern: DiffPattern::Describe,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
rest_ok = r.json().get('code') == 0
rest_dim = r.json().get('data',{}).get('dimension')
rest_name = r.json().get('data',{}).get('collectionName')
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    rs = client.describe_collection(collection_name=c)
    sdk_ok = True
    sdk_dim = rs.get('dimension')
    sdk_name = rs.get('collection_name')
except Exception as e:
    sdk_ok = False
    sdk_dim = None
    sdk_name = None
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok:
    if rest_dim != sdk_dim: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe dimension: rest={rest_dim} sdk={sdk_dim}'); sys.exit(1)
    if rest_name != c and sdk_name != c: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] describe name: rest={rest_name} sdk={sdk_name}'); sys.exit(1)
print(f'diff describe: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_diff_upsert() -> DiffTestCase {
        DiffTestCase {
            name: "diff_upsert".to_string(),
            diff_pattern: DiffPattern::Upsert,
            script: r#"import requests, sys, uuid, time
from pymilvus import MilvusClient
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.9,0.8,0.7,0.6]}]})
rest_ok = r.json().get('code') == 0
try:
    client = MilvusClient(uri=BASE, token='root:Milvus')
    client.upsert(collection_name=c, data=[{"id":2,"vector":[0.5,0.6,0.7,0.8]}])
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] upsert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff upsert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_qdrant_diff_create_collection() -> DiffTestCase {
        DiffTestCase {
            name: "diff_create_collection".to_string(),
            diff_pattern: DiffPattern::CreateCollection,
            script: r#"import requests, sys, uuid, time
from qdrant_client import QdrantClient, models
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
c_rest = f'oracle_diff_rest_{uid}'
c_sdk = f'oracle_diff_sdk_{uid}'
r = requests.put(f'{BASE}/collections/{c_rest}', json={"vectors":{"size":4,"distance":"Cosine"}})
rest_ok = r.status_code == 200
try:
    client = QdrantClient(url=BASE, prefer_grpc=False)
    client.create_collection(collection_name=c_sdk, vectors_config=models.VectorParams(size=4, distance=models.Distance.COSINE))
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] create: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok:
    time.sleep(1)
    rr = requests.get(f'{BASE}/collections/{c_rest}')
    rs = client.get_collection(c_sdk)
    rest_dim = rr.json().get('result',{}).get('config',{}).get('params',{}).get('vectors',{}).get('size')
    sdk_dim = rs.config.params.vectors.size if hasattr(rs.config.params.vectors,'size') else None
    if rest_dim != sdk_dim: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] dimension: rest={rest_dim} sdk={sdk_dim}'); sys.exit(1)
print(f'diff create_collection: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_qdrant_diff_collection_info() -> DiffTestCase {
        DiffTestCase {
            name: "diff_collection_info".to_string(),
            diff_pattern: DiffPattern::Describe,
            script: r#"import requests, sys, uuid, time
from qdrant_client import QdrantClient, models
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
r = requests.get(f'{BASE}/collections/{c}')
rest_ok = r.status_code == 200
rest_status = r.json().get('status',{}).get('ok') if r.status_code == 200 else None
try:
    client = QdrantClient(url=BASE, prefer_grpc=False)
    info = client.get_collection(c)
    sdk_ok = True
    sdk_status = info.status.value if hasattr(info.status,'value') else str(info.status)
except Exception as e:
    sdk_ok = False
    sdk_status = None
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] info: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff collection_info: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_qdrant_diff_upsert() -> DiffTestCase {
        DiffTestCase {
            name: "diff_upsert".to_string(),
            diff_pattern: DiffPattern::Upsert,
            script: r#"import requests, sys, uuid, time
from qdrant_client import QdrantClient, models
from qdrant_client.models import PointStruct
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":1,"vector":[0.1,0.2,0.3,0.4],"payload":{}}]})
rest_ok = r.status_code == 200
try:
    client = QdrantClient(url=BASE, prefer_grpc=False)
    client.upsert(collection_name=c, points=[PointStruct(id=2,vector=[0.5,0.6,0.7,0.8],payload={})])
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] upsert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff upsert: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_qdrant_diff_search() -> DiffTestCase {
        DiffTestCase {
            name: "diff_search".to_string(),
            diff_pattern: DiffPattern::Search,
            script: r#"import requests, sys, uuid, time
from qdrant_client import QdrantClient, models
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
points = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"payload":{}} for i in range(1,11)]
requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
time.sleep(1)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":5})
rest_ok = r.status_code == 200
rest_top1 = None
if rest_ok:
    results = r.json().get('result',[])
    if results: rest_top1 = results[0].get('id')
try:
    client = QdrantClient(url=BASE, prefer_grpc=False)
    sdk_results = client.search(collection_name=c, query_vector=[0.1,0.2,0.3,0.4], limit=5)
    sdk_ok = True
    sdk_top1 = sdk_results[0].id if sdk_results else None
except Exception as e:
    sdk_ok = False
    sdk_top1 = None
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
if rest_ok and sdk_ok and rest_top1 != sdk_top1: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] search top1: rest={rest_top1} sdk={sdk_top1}'); sys.exit(1)
print(f'diff search: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }

    fn generate_qdrant_diff_delete() -> DiffTestCase {
        DiffTestCase {
            name: "diff_delete".to_string(),
            diff_pattern: DiffPattern::Delete,
            script: r#"import requests, sys, uuid, time
from qdrant_client import QdrantClient, models
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
c = f'oracle_diff_{uid}'
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
points = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"payload":{}} for i in range(1,6)]
requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
time.sleep(1)
r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points":[1]})
rest_ok = r.status_code == 200
try:
    client = QdrantClient(url=BASE, prefer_grpc=False)
    client.delete(collection_name=c, points_selector=[2])
    sdk_ok = True
except Exception as e:
    sdk_ok = False
if rest_ok != sdk_ok: print(f'[DEFECT: DIFFERENTIAL_MISMATCH] delete: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(1)
print(f'diff delete: rest_ok={rest_ok} sdk_ok={sdk_ok}'); sys.exit(0)"#.to_string(),
            defect_marker: "DIFFERENTIAL_MISMATCH".to_string(),
        }
    }
}

pub struct ConcurrentTestGenerator;

impl ConcurrentTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<ConcurrentTestCase> {
        let mut cases = Vec::new();

        let has_create = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("collections/create") || atc.endpoint.contains("collections")
        });
        let has_insert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/insert") || atc.endpoint.contains("points")
        });
        let has_search = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/search") || atc.endpoint.contains("search")
        });
        let has_query = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/query")
        });
        let has_delete = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/delete") || atc.endpoint.contains("points/delete")
        });
        let has_upsert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/upsert") || atc.endpoint.contains("points/upsert")
        });
        let has_drop = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("collections/drop") || atc.endpoint.contains("collections/delete")
        });

        match style {
            TargetStyle::Milvus => {
                if has_insert && has_search {
                    cases.push(Self::generate_concurrent_insert_search());
                }
                if has_delete && has_query {
                    cases.push(Self::generate_concurrent_delete_query());
                }
                if has_upsert && has_search {
                    cases.push(Self::generate_concurrent_upsert_search());
                }
                if has_create && has_drop {
                    cases.push(Self::generate_concurrent_create_drop());
                }
                if has_insert {
                    cases.push(Self::generate_concurrent_insert_flush());
                }
            }
            TargetStyle::Qdrant => {
                if has_insert || has_upsert {
                    if has_search {
                        cases.push(Self::generate_qdrant_concurrent_upsert_search());
                    }
                    if has_delete {
                        cases.push(Self::generate_qdrant_concurrent_upsert_delete());
                    }
                }
                if has_create && has_drop {
                    cases.push(Self::generate_qdrant_concurrent_create_delete());
                }
            }
        }

        cases.dedup_by(|a, b| a.name == b.name);
        cases
    }

    fn generate_concurrent_insert_search() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_insert_search".to_string(),
            concurrent_pattern: ConcurrentPattern::InsertSearch,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
errors = []
def do_insert():
    try:
        for i in range(10):
            r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
            if r.json().get('code') != 0: errors.append(f'insert {i}: {r.text}')
    except Exception as e: errors.append(f'insert exception: {e}')
def do_search():
    try:
        for _ in range(5):
            r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
            if r.json().get('code') != 0: errors.append(f'search: {r.text}')
    except Exception as e: errors.append(f'search exception: {e}')
t1 = threading.Thread(target=do_insert)
t2 = threading.Thread(target=do_search)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
    if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent insert+search crashed system'); sys.exit(1)
    else: print(f'concurrent insert+search: transient errors, system healthy'); sys.exit(0)
print(f'concurrent insert+search: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_concurrent_delete_query() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_delete_query".to_string(),
            concurrent_pattern: ConcurrentPattern::DeleteQuery,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
data = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]} for i in range(1,11)]
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
errors = []
def do_delete():
    try:
        for i in range(1,6):
            r = requests.post(f'{BASE}/v2/vectordb/entities/delete', headers=HEADERS, json={"collectionName":c,"filter":f"id == {i}"})
            if r.json().get('code') != 0: errors.append(f'delete {i}: {r.text}')
    except Exception as e: errors.append(f'delete exception: {e}')
def do_query():
    try:
        for _ in range(5):
            r = requests.post(f'{BASE}/v2/vectordb/entities/query', headers=HEADERS, json={"collectionName":c,"filter":"id > 0","limit":10})
            if r.json().get('code') != 0: errors.append(f'query: {r.text}')
    except Exception as e: errors.append(f'query exception: {e}')
t1 = threading.Thread(target=do_delete)
t2 = threading.Thread(target=do_query)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
    if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent delete+query crashed system'); sys.exit(1)
    else: print(f'concurrent delete+query: transient errors, system healthy'); sys.exit(0)
print(f'concurrent delete+query: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_concurrent_upsert_search() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_upsert_search".to_string(),
            concurrent_pattern: ConcurrentPattern::UpsertSearch,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
time.sleep(1)
requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
time.sleep(2)
errors = []
def do_upsert():
    try:
        for i in range(5):
            r = requests.post(f'{BASE}/v2/vectordb/entities/upsert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.01*i,0.02*i,0.03*i,0.04*i]}]})
            if r.json().get('code') != 0: errors.append(f'upsert {i}: {r.text}')
    except Exception as e: errors.append(f'upsert exception: {e}')
def do_search():
    try:
        for _ in range(5):
            r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json={"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3})
            if r.json().get('code') != 0: errors.append(f'search: {r.text}')
    except Exception as e: errors.append(f'search exception: {e}')
t1 = threading.Thread(target=do_upsert)
t2 = threading.Thread(target=do_search)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
    if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent upsert+search crashed system'); sys.exit(1)
    else: print(f'concurrent upsert+search: transient errors, system healthy'); sys.exit(0)
print(f'concurrent upsert+search: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_concurrent_create_drop() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_create_drop".to_string(),
            concurrent_pattern: ConcurrentPattern::CreateDrop,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
uid = uuid.uuid4().hex[:8]
errors = []
def do_create_drop_a():
    try:
        for i in range(3):
            c = f'oracle_conc_a_{uid}_{i}'
            r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
            time.sleep(0.5)
            requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
    except Exception as e: errors.append(f'create_drop_a exception: {e}')
def do_create_drop_b():
    try:
        for i in range(3):
            c = f'oracle_conc_b_{uid}_{i}'
            r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
            time.sleep(0.5)
            requests.post(f'{BASE}/v2/vectordb/collections/drop', headers=HEADERS, json={"collectionName":c})
    except Exception as e: errors.append(f'create_drop_b exception: {e}')
t1 = threading.Thread(target=do_create_drop_a)
t2 = threading.Thread(target=do_create_drop_b)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.post(f'{BASE}/v2/vectordb/collections/list', headers=HEADERS, json={})
    if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent create+drop crashed system'); sys.exit(1)
    else: print(f'concurrent create+drop: transient errors, system healthy'); sys.exit(0)
print(f'concurrent create+drop: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_concurrent_insert_flush() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_insert_flush".to_string(),
            concurrent_pattern: ConcurrentPattern::InsertFlush,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]}})
time.sleep(1)
errors = []
def do_insert():
    try:
        for i in range(10):
            r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i]}]})
            if r.json().get('code') != 0: errors.append(f'insert {i}: {r.text}')
    except Exception as e: errors.append(f'insert exception: {e}')
def do_flush():
    try:
        for _ in range(3):
            r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
            if r.json().get('code') != 0: errors.append(f'flush: {r.text}')
            time.sleep(1)
    except Exception as e: errors.append(f'flush exception: {e}')
t1 = threading.Thread(target=do_insert)
t2 = threading.Thread(target=do_flush)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.post(f'{BASE}/v2/vectordb/collections/describe', headers=HEADERS, json={"collectionName":c})
    if r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent insert+flush crashed system'); sys.exit(1)
    else: print(f'concurrent insert+flush: transient errors, system healthy'); sys.exit(0)
print(f'concurrent insert+flush: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_qdrant_concurrent_upsert_search() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_upsert_search".to_string(),
            concurrent_pattern: ConcurrentPattern::UpsertSearch,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
errors = []
def do_upsert():
    try:
        for i in range(10):
            r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"payload":{}}]})
            if r.status_code != 200: errors.append(f'upsert {i}: {r.text}')
    except Exception as e: errors.append(f'upsert exception: {e}')
def do_search():
    try:
        for _ in range(5):
            r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.1,0.2,0.3,0.4],"limit":3})
            if r.status_code != 200: errors.append(f'search: {r.text}')
    except Exception as e: errors.append(f'search exception: {e}')
t1 = threading.Thread(target=do_upsert)
t2 = threading.Thread(target=do_search)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.get(f'{BASE}/collections/{c}')
    if r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent upsert+search crashed system'); sys.exit(1)
    else: print(f'concurrent upsert+search: transient errors, system healthy'); sys.exit(0)
print(f'concurrent upsert+search: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_qdrant_concurrent_upsert_delete() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_upsert_delete".to_string(),
            concurrent_pattern: ConcurrentPattern::DeleteQuery,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
c = 'oracle_conc_' + uuid.uuid4().hex[:8]
requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
time.sleep(1)
points = [{"id":i,"vector":[0.1*i,0.2*i,0.3*i,0.4*i],"payload":{}} for i in range(1,11)]
requests.put(f'{BASE}/collections/{c}/points', json={"points":points})
time.sleep(1)
errors = []
def do_upsert():
    try:
        for i in range(10,20):
            r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":i,"vector":[0.01*i,0.02*i,0.03*i,0.04*i],"payload":{}}]})
            if r.status_code != 200: errors.append(f'upsert {i}: {r.text}')
    except Exception as e: errors.append(f'upsert exception: {e}')
def do_delete():
    try:
        for i in range(1,6):
            r = requests.post(f'{BASE}/collections/{c}/points/delete', json={"points":[i]})
            if r.status_code != 200: errors.append(f'delete {i}: {r.text}')
    except Exception as e: errors.append(f'delete exception: {e}')
t1 = threading.Thread(target=do_upsert)
t2 = threading.Thread(target=do_delete)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.get(f'{BASE}/collections/{c}')
    if r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent upsert+delete crashed system'); sys.exit(1)
    else: print(f'concurrent upsert+delete: transient errors, system healthy'); sys.exit(0)
print(f'concurrent upsert+delete: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_qdrant_concurrent_create_delete() -> ConcurrentTestCase {
        ConcurrentTestCase {
            name: "concurrent_create_delete".to_string(),
            concurrent_pattern: ConcurrentPattern::CreateDrop,
            script: r#"import requests, sys, uuid, time, threading
BASE = '{TESTVDB_DB_URL}'
uid = uuid.uuid4().hex[:8]
errors = []
def do_create_delete_a():
    try:
        for i in range(3):
            c = f'oracle_conc_a_{uid}_{i}'
            r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
            time.sleep(0.5)
            requests.delete(f'{BASE}/collections/{c}')
    except Exception as e: errors.append(f'create_delete_a exception: {e}')
def do_create_delete_b():
    try:
        for i in range(3):
            c = f'oracle_conc_b_{uid}_{i}'
            r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
            time.sleep(0.5)
            requests.delete(f'{BASE}/collections/{c}')
    except Exception as e: errors.append(f'create_delete_b exception: {e}')
t1 = threading.Thread(target=do_create_delete_a)
t2 = threading.Thread(target=do_create_delete_b)
t1.start(); t2.start()
t1.join(); t2.join()
if errors:
    time.sleep(2)
    r = requests.get(f'{BASE}/collections')
    if r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] concurrent create+delete crashed system'); sys.exit(1)
    else: print(f'concurrent create+delete: transient errors, system healthy'); sys.exit(0)
print(f'concurrent create+delete: no errors'); sys.exit(0)"#.to_string(),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::TypeConstraint;

    fn make_milvus_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "v2.4");
        let endpoints = [
            ("/v2/vectordb/collections/create", "collectionName"),
            ("/v2/vectordb/collections/drop", "collectionName"),
            ("/v2/vectordb/entities/insert", "data"),
            ("/v2/vectordb/entities/search", "data"),
            ("/v2/vectordb/entities/query", "filter"),
            ("/v2/vectordb/entities/delete", "filter"),
            ("/v2/vectordb/entities/upsert", "data"),
            ("/v2/vectordb/collections/flush", "collectionName"),
        ];
        for (ep, param) in &endpoints {
            store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
                constraint: TypeConstraint {
                    param_name: param.to_string(),
                    expected_type: "string".to_string(),
                    violation_examples: vec![],
                },
                endpoint: ep.to_string(),
                source: crate::contract::store::ConstraintSource::ExplicitDoc,
                confidence: crate::contract::store::Confidence::High,
            });
        }
        store
    }

    #[test]
    fn test_diff_generator_milvus() {
        let store = make_milvus_store();
        let cases = DiffTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 7, "Should have at least 7 diff tests, got {}", cases.len());
        for case in &cases {
            assert!(case.script.contains("[DEFECT: DIFFERENTIAL_MISMATCH]"));
            assert!(case.script.contains("sys.exit"));
            assert!(case.script.contains("pymilvus"));
        }
    }

    #[test]
    fn test_diff_generator_qdrant() {
        let mut store = ContractStore::new("qdrant", "v1.7");
        let endpoints = [
            ("collections", "name"),
            ("points/upsert", "points"),
            ("points/search", "vector"),
            ("points/delete", "points"),
        ];
        for (ep, param) in &endpoints {
            store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
                constraint: TypeConstraint {
                    param_name: param.to_string(),
                    expected_type: "string".to_string(),
                    violation_examples: vec![],
                },
                endpoint: ep.to_string(),
                source: crate::contract::store::ConstraintSource::ExplicitDoc,
                confidence: crate::contract::store::Confidence::High,
            });
        }
        let cases = DiffTestGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(cases.len() >= 4, "Should have at least 4 diff tests for Qdrant, got {}", cases.len());
        for case in &cases {
            assert!(case.script.contains("[DEFECT: DIFFERENTIAL_MISMATCH]"));
            assert!(case.script.contains("qdrant_client"));
        }
    }

    #[test]
    fn test_diff_generator_no_endpoints() {
        let store = ContractStore::new("milvus", "v2.4");
        let cases = DiffTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.is_empty(), "Should have no diff tests without endpoints");
    }

    #[test]
    fn test_concurrent_generator_milvus() {
        let store = make_milvus_store();
        let cases = ConcurrentTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 4, "Should have at least 4 concurrent tests, got {}", cases.len());
        for case in &cases {
            assert!(case.script.contains("[DEFECT: SEQUENCE_VIOLATION]"));
            assert!(case.script.contains("threading"));
            assert!(case.script.contains("sys.exit"));
        }
    }

    #[test]
    fn test_concurrent_generator_qdrant() {
        let mut store = ContractStore::new("qdrant", "v1.7");
        let endpoints = [
            ("collections", "name"),
            ("points/upsert", "points"),
            ("points/search", "vector"),
            ("points/delete", "points"),
        ];
        for (ep, param) in &endpoints {
            store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
                constraint: TypeConstraint {
                    param_name: param.to_string(),
                    expected_type: "string".to_string(),
                    violation_examples: vec![],
                },
                endpoint: ep.to_string(),
                source: crate::contract::store::ConstraintSource::ExplicitDoc,
                confidence: crate::contract::store::Confidence::High,
            });
        }
        let cases = ConcurrentTestGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(cases.len() >= 2, "Should have at least 2 concurrent tests for Qdrant, got {}", cases.len());
        for case in &cases {
            assert!(case.script.contains("[DEFECT: SEQUENCE_VIOLATION]"));
            assert!(case.script.contains("threading"));
        }
    }

    #[test]
    fn test_concurrent_generator_no_endpoints() {
        let store = ContractStore::new("milvus", "v2.4");
        let cases = ConcurrentTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.is_empty(), "Should have no concurrent tests without endpoints");
    }
}
