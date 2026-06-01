use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceTestCase {
    pub name: String,
    pub sequence_pattern: SequencePattern,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequencePattern {
    DropRecreate,
    DeleteSearch,
    ReleaseLoad,
    DropIndexSearch,
    UpsertSemantic,
    DuplicateId,
    Rename,
    Alias,
    FlushSearch,
    CompactSearch,
    PartitionDrop,
    AlterProperties,
    DynamicField,
    DatabaseCrud,
    SearchQueryMixed,
    DeleteAllReinsert,
    LoadReleaseCycle,
    HybridSearch,
    MultiBatchInsert,
    RecreateDataIsolation,
}

pub struct SequenceTestGenerator;

impl SequenceTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<SequenceTestCase> {
        let mut cases = Vec::new();

        let has_create = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/create")));
        let has_drop = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/drop")));
        let has_insert = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/insert")));
        let has_search = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/search")));
        let has_delete = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/delete")));
        let has_upsert = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/upsert")));
        let has_load = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/load")));
        let has_release = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/release")));
        let has_partition = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("partitions")));
        let has_rename = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/rename")));
        let has_alias = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("aliases")));
        let has_flush = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/flush")));
        let has_compact = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/compact")));
        let has_alter = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/alter")));
        let has_dynamic = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("fields/add")));
        let has_database = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("databases")));
        let has_query = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/query")));
        let has_hybrid = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/hybrid_search")));
        let has_index = store.type_constraints.iter().any(|atc| atc.endpoint.as_deref().map_or(false, |e| e.contains("indexes")));

        if has_create && has_drop && has_search {
            cases.push(Self::seq_drop_recreate(style));
        }
        if has_delete && has_search {
            cases.push(Self::seq_delete_search(style));
        }
        if has_load && has_release {
            cases.push(Self::seq_release_load(style));
        }
        if has_index && has_search {
            cases.push(Self::seq_drop_index_search(style));
        }
        if has_upsert && has_search {
            cases.push(Self::seq_upsert_semantic(style));
        }
        if has_insert {
            cases.push(Self::seq_duplicate_id(style));
        }
        if has_rename {
            cases.push(Self::seq_rename(style));
        }
        if has_alias {
            cases.push(Self::seq_alias(style));
        }
        if has_flush {
            cases.push(Self::seq_flush_search(style));
        }
        if has_compact {
            cases.push(Self::seq_compact_search(style));
        }
        if has_partition {
            cases.push(Self::seq_partition_drop(style));
        }
        if has_alter {
            cases.push(Self::seq_alter_properties(style));
        }
        if has_dynamic {
            cases.push(Self::seq_dynamic_field(style));
        }
        if has_database {
            cases.push(Self::seq_database_crud(style));
        }
        if has_search && has_query {
            cases.push(Self::seq_search_query_mixed(style));
        }
        if has_delete {
            cases.push(Self::seq_delete_all_reinsert(style));
        }
        if has_load && has_release {
            cases.push(Self::seq_load_release_cycle(style));
        }
        if has_hybrid {
            cases.push(Self::seq_hybrid_search(style));
        }
        if has_insert {
            cases.push(Self::seq_multi_batch_insert(style));
        }
        if has_create && has_drop {
            cases.push(Self::seq_recreate_data_isolation(style));
        }

        cases
    }

    fn seq_drop_recreate(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_drop_recreate_search".to_string(),
            sequence_pattern: SequencePattern::DropRecreate,
            script: build_seq_script(style, SeqKind::DropRecreate),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_delete_search(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_delete_then_search".to_string(),
            sequence_pattern: SequencePattern::DeleteSearch,
            script: build_seq_script(style, SeqKind::DeleteSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_release_load(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_release_load_consistency".to_string(),
            sequence_pattern: SequencePattern::ReleaseLoad,
            script: build_seq_script(style, SeqKind::ReleaseLoad),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_drop_index_search(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_drop_index_search".to_string(),
            sequence_pattern: SequencePattern::DropIndexSearch,
            script: build_seq_script(style, SeqKind::DropIndexSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_upsert_semantic(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_upsert_semantic".to_string(),
            sequence_pattern: SequencePattern::UpsertSemantic,
            script: build_seq_script(style, SeqKind::UpsertSemantic),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_duplicate_id(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_duplicate_id_count".to_string(),
            sequence_pattern: SequencePattern::DuplicateId,
            script: build_seq_script(style, SeqKind::DuplicateId),
            defect_marker: "PARAM_IGNORED".to_string(),
        }
    }
    fn seq_rename(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_rename_insert".to_string(),
            sequence_pattern: SequencePattern::Rename,
            script: build_seq_script(style, SeqKind::Rename),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_alias(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_alias_operations".to_string(),
            sequence_pattern: SequencePattern::Alias,
            script: build_seq_script(style, SeqKind::Alias),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_flush_search(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_flush_search".to_string(),
            sequence_pattern: SequencePattern::FlushSearch,
            script: build_seq_script(style, SeqKind::FlushSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_compact_search(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_compact_search".to_string(),
            sequence_pattern: SequencePattern::CompactSearch,
            script: build_seq_script(style, SeqKind::CompactSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_partition_drop(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_partition_drop_data".to_string(),
            sequence_pattern: SequencePattern::PartitionDrop,
            script: build_seq_script(style, SeqKind::PartitionDrop),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_alter_properties(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_alter_properties".to_string(),
            sequence_pattern: SequencePattern::AlterProperties,
            script: build_seq_script(style, SeqKind::AlterProperties),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_dynamic_field(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_dynamic_field_query".to_string(),
            sequence_pattern: SequencePattern::DynamicField,
            script: build_seq_script(style, SeqKind::DynamicField),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_database_crud(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_database_crud".to_string(),
            sequence_pattern: SequencePattern::DatabaseCrud,
            script: build_seq_script(style, SeqKind::DatabaseCrud),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_search_query_mixed(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_search_query_mixed".to_string(),
            sequence_pattern: SequencePattern::SearchQueryMixed,
            script: build_seq_script(style, SeqKind::SearchQueryMixed),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_delete_all_reinsert(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_delete_all_reinsert".to_string(),
            sequence_pattern: SequencePattern::DeleteAllReinsert,
            script: build_seq_script(style, SeqKind::DeleteAllReinsert),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_load_release_cycle(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_load_release_cycle".to_string(),
            sequence_pattern: SequencePattern::LoadReleaseCycle,
            script: build_seq_script(style, SeqKind::LoadReleaseCycle),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_hybrid_search(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_hybrid_then_search".to_string(),
            sequence_pattern: SequencePattern::HybridSearch,
            script: build_seq_script(style, SeqKind::HybridSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_multi_batch_insert(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_multi_batch_insert".to_string(),
            sequence_pattern: SequencePattern::MultiBatchInsert,
            script: build_seq_script(style, SeqKind::MultiBatchInsert),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
    fn seq_recreate_data_isolation(style: TargetStyle) -> SequenceTestCase {
        SequenceTestCase {
            name: "seq_recreate_data_isolation".to_string(),
            sequence_pattern: SequencePattern::RecreateDataIsolation,
            script: build_seq_script(style, SeqKind::RecreateDataIsolation),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }
}

enum SeqKind {
    DropRecreate, DeleteSearch, ReleaseLoad, DropIndexSearch, UpsertSemantic,
    DuplicateId, Rename, Alias, FlushSearch, CompactSearch, PartitionDrop,
    AlterProperties, DynamicField, DatabaseCrud, SearchQueryMixed, DeleteAllReinsert,
    LoadReleaseCycle, HybridSearch, MultiBatchInsert, RecreateDataIsolation,
}

fn build_seq_script(style: TargetStyle, kind: SeqKind) -> String {
    match style {
        TargetStyle::Milvus => build_milvus_seq_script(kind),
        TargetStyle::Qdrant => build_qdrant_seq_script(kind),
        TargetStyle::Weaviate => build_weaviate_seq_script(kind),
        TargetStyle::PgVector => String::new(),
    }
}

fn milvus_setup() -> &'static str {
    r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'seq_' + uuid.uuid4().hex[:8]
"#
}

fn milvus_create() -> &'static str {
    r#"r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)"#
}

fn milvus_load() -> &'static str {
    r#"r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)"#
}

fn milvus_release() -> &'static str {
    r#"r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'release failed: {r.text}'); sys.exit(0)
time.sleep(1)"#
}

fn milvus_flush() -> &'static str {
    r#"r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(2)"#
}

fn build_milvus_seq_script(kind: SeqKind) -> String {
    let s = milvus_setup();
    let cr = milvus_create();
    let ld = milvus_load();
    let rl = milvus_release();
    let fl = milvus_flush();

    match kind {
        SeqKind::DropRecreate => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/drop', headers=HEADERS, json={{\"collectionName\":c}})\nif r.json().get('code') != 0: print(f'drop failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\nif r.json().get('code') != 0: print(f'recreate failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') == 0 and len(r.json().get('data',[])) > 0: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate search returned data after drop'); sys.exit(1)\nelse: print(f'seq drop_recreate verified'); sys.exit(0)"),

        SeqKind::DeleteSearch => format!("{s}{cr}\ndata = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/delete', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"id == 1\"}})\nif r.json().get('code') != 0: print(f'delete failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10}})\nids = [d.get('id') for d in r.json().get('data',[])]\nif 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] deleted id=1 still in search results: {{ids}}'); sys.exit(1)\nelse: print(f'seq delete_search verified'); sys.exit(0)"),

        SeqKind::ReleaseLoad => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r1.json().get('code') != 0: print(f'search1 failed: {{r1.text}}'); sys.exit(0)\n{rl}\n{ld}\nr2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r2.json().get('code') != 0: print(f'search2 failed: {{r2.text}}'); sys.exit(0)\nids1 = set(d.get('id') for d in r1.json().get('data',[]))\nids2 = set(d.get('id') for d in r2.json().get('data',[]))\nif ids1 != ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] release+load changed results: {{ids1}} vs {{ids2}}'); sys.exit(1)\nelse: print(f'seq release_load verified'); sys.exit(0)"),

        SeqKind::DropIndexSearch => format!("{s}r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}}}})\nif r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/indexes/create', headers=HEADERS, json={{\"collectionName\":c,\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\nif r.json().get('code') != 0: print(f'create index failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/indexes/drop', headers=HEADERS, json={{\"collectionName\":c,\"indexName\":\"vector\"}})\nif r.json().get('code') != 0: print(f'drop index failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed after drop_index: {{r.text}}'); sys.exit(1)\nelse: print(f'seq drop_index_search verified'); sys.exit(0)"),

        SeqKind::UpsertSemantic => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/upsert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.9,0.8,0.7,0.6]}}]}})\nif r.json().get('code') != 0: print(f'upsert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.9,0.8,0.7,0.6]],\"limit\":1}})\nif r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)\ntop = r.json().get('data',[])\nif top and top[0].get('id') == 1: print(f'seq upsert_semantic verified'); sys.exit(0)\nelse: print(f'[DEFECT: SEQUENCE_VIOLATION] upsert did not update: {{top}}'); sys.exit(1)"),

        SeqKind::DuplicateId => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert1 failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.5,0.6,0.7,0.8]}}]}})\ntime.sleep(1)\n{fl}\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/get_stats', headers=HEADERS, json={{\"collectionName\":c}})\ncount = r.json().get('data',{{}}).get('rowCount',-1)\nif count != 1: print(f'[DEFECT: PARAM_IGNORED] duplicate id insert count: expected 1 got {{count}}'); sys.exit(1)\nelse: print(f'seq duplicate_id count={{count}}'); sys.exit(0)"),

        SeqKind::Rename => format!("{s}{cr}\nc_new = c + '_new'\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/rename', headers=HEADERS, json={{\"collectionName\":c,\"newCollectionName\":c_new}})\nif r.json().get('code') != 0: print(f'rename failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c_new,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] insert after rename failed: {{r.text}}'); sys.exit(1)\nelse: print(f'seq rename verified'); sys.exit(0)"),

        SeqKind::Alias => format!("{s}{cr}\nalias = 'alias_' + uuid.uuid4().hex[:8]\nr = requests.post(f'{{BASE}}/v2/vectordb/aliases/create', headers=HEADERS, json={{\"aliasName\":alias,\"collectionName\":c}})\nif r.json().get('code') != 0: print(f'alias create failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":alias,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] insert via alias failed: {{r.text}}'); sys.exit(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":alias,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search via alias failed: {{r.text}}'); sys.exit(1)\nelse: print(f'seq alias verified'); sys.exit(0)"),

        SeqKind::FlushSearch => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{fl}\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after flush failed: {{r.text}}'); sys.exit(1)\nelse: print(f'seq flush_search verified'); sys.exit(0)"),

        SeqKind::CompactSearch => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/compact', headers=HEADERS, json={{\"collectionName\":c}})\ntime.sleep(2)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after compact failed: {{r.text}}'); sys.exit(1)\nelse: print(f'seq compact_search verified'); sys.exit(0)"),

        SeqKind::PartitionDrop => format!("{s}{cr}\np = 'part_' + uuid.uuid4().hex[:8]\nr = requests.post(f'{{BASE}}/v2/vectordb/partitions/create', headers=HEADERS, json={{\"collectionName\":c,\"partitionName\":p}})\nif r.json().get('code') != 0: print(f'partition create failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}],\"partitionName\":p}})\nif r.json().get('code') != 0: print(f'insert partition failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3,\"partitionNames\":[p]}})\nif r.json().get('code') != 0: print(f'search partition failed: {{r.text}}'); sys.exit(0)\n{rl}\nr = requests.post(f'{{BASE}}/v2/vectordb/partitions/drop', headers=HEADERS, json={{\"collectionName\":c,\"partitionName\":p}})\nif r.json().get('code') != 0: print(f'drop partition failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nids = [d.get('id') for d in r.json().get('data',[])]\nif 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data still found after drop: {{ids}}'); sys.exit(1)\nelse: print(f'seq partition_drop verified'); sys.exit(0)"),

        SeqKind::AlterProperties => format!("{s}{cr}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/alter_properties', headers=HEADERS, json={{\"collectionName\":c,\"properties\":{{\"collection.ttl.seconds\":86400}}}})\nif r.json().get('code') != 0: print(f'alter failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/describe', headers=HEADERS, json={{\"collectionName\":c}})\nprops = r.json().get('data',{{}}).get('properties',{{}})\nif not props: print(f'[DEFECT: SEQUENCE_VIOLATION] properties not reflected in describe'); sys.exit(1)\nelse: print(f'seq alter_properties verified'); sys.exit(0)"),

        SeqKind::DynamicField => format!("{s}r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}}}})\nif r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/fields/add', headers=HEADERS, json={{\"collectionName\":c,\"fieldName\":\"extra\",\"dataType\":\"Int64\"}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":2,\"vector\":[0.5,0.6,0.7,0.8],\"extra\":42}}]}})\nif r.json().get('code') != 0: print(f'insert with new field failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/query', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"extra == 42\",\"limit\":10}})\nif r.json().get('code') == 0 and len(r.json().get('data',[])) > 0: print(f'seq dynamic_field verified'); sys.exit(0)\nelse: print(f'[DEFECT: SEQUENCE_VIOLATION] dynamic field query returned no data'); sys.exit(1)"),

        SeqKind::DatabaseCrud => format!("{s}db = 'db_' + uuid.uuid4().hex[:8]\nr = requests.post(f'{{BASE}}/v2/vectordb/databases/create', headers=HEADERS, json={{\"dbName\":db}})\nif r.json().get('code') != 0: print(f'db create failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"dbName\":db,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\nif r.json().get('code') != 0: print(f'collection in db create failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/list', headers=HEADERS, json={{\"dbName\":db}})\nif r.json().get('code') != 0: print(f'list in db failed: {{r.text}}'); sys.exit(0)\nnames = r.json().get('data',[])\nif isinstance(names, list) and len(names) > 0 and isinstance(names[0], dict):\n    names = [d.get('collectionName','') for d in names]\nif c not in names: print(f'[DEFECT: SEQUENCE_VIOLATION] collection not found in db: {{names}}'); sys.exit(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/drop', headers=HEADERS, json={{\"collectionName\":c,\"dbName\":db}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/databases/drop', headers=HEADERS, json={{\"dbName\":db}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] db drop failed: {{r.text}}'); sys.exit(1)\nprint(f'seq database_crud verified'); sys.exit(0)"),

        SeqKind::SearchQueryMixed => format!("{s}{cr}\ndata = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r1.json().get('code') != 0: print(f'search failed: {{r1.text}}'); sys.exit(0)\nr2 = requests.post(f'{{BASE}}/v2/vectordb/entities/query', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"id > 0\",\"limit\":10}})\nif r2.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] query failed: {{r2.text}}'); sys.exit(1)\nsearch_ids = set(d.get('id') for d in r1.json().get('data',[]))\nquery_ids = set(d.get('id') for d in r2.json().get('data',[]))\nif not search_ids or not query_ids: print(f'[DEFECT: SEQUENCE_VIOLATION] search+query returned empty'); sys.exit(1)\nprint(f'seq search_query_mixed verified'); sys.exit(0)"),

        SeqKind::DeleteAllReinsert => format!("{s}{cr}\ndata = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/delete', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"id > 0\"}})\nif r.json().get('code') != 0: print(f'delete failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\ndata2 = [{{\"id\":i,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(1,4)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data2}})\nif r.json().get('code') != 0: print(f'reinsert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.5,0.6,0.7,0.8]],\"limit\":5}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after reinsert failed: {{r.text}}'); sys.exit(1)\nids = [d.get('id') for d in r.json().get('data',[])]\nif len(ids) == 0: print(f'[DEFECT: SEQUENCE_VIOLATION] no results after reinsert'); sys.exit(1)\nelse: print(f'seq delete_all_reinsert verified'); sys.exit(0)"),

        SeqKind::LoadReleaseCycle => format!("{s}{cr}\n{ld}\n{rl}\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after repeated load/release failed'); sys.exit(1)\nelse: print(f'seq load_release_cycle verified'); sys.exit(0)"),

        SeqKind::HybridSearch => format!("{s}{cr}\ndata = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\nif r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/hybrid_search', headers=HEADERS, json={{\"collectionName\":c,\"searchParams\":[{{\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}}],\"rerank\":{{\"strategy\":\"rrf\",\"params\":{{\"k\":60}}}}}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\nif r.json().get('code') != 0: print(f'[DEFECT: SEQUENCE_VIOLATION] search after hybrid failed: {{r.text}}'); sys.exit(1)\nelse: print(f'seq hybrid_search verified'); sys.exit(0)"),

        SeqKind::MultiBatchInsert => format!("{s}{cr}\nfor batch in range(3):\n    data = [{{\"id\":batch*10+i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(10)]\n    r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\n    if r.json().get('code') != 0: print(f'insert batch {{batch}} failed: {{r.text}}'); sys.exit(0)\n    time.sleep(1)\n{ld}\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":30}})\nif r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)\nids = [d.get('id') for d in r.json().get('data',[])]\nif len(ids) < 10: print(f'[DEFECT: SEQUENCE_VIOLATION] multi-batch search returned too few: {{len(ids)}}'); sys.exit(1)\nelse: print(f'seq multi_batch_insert verified, found {{len(ids)}} results'); sys.exit(0)"),

        SeqKind::RecreateDataIsolation => format!("{s}{cr}\ndata1 = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data1}})\nif r.json().get('code') != 0: print(f'insert1 failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":5}})\nif r1.json().get('code') != 0: print(f'search1 failed: {{r1.text}}'); sys.exit(0)\nids1 = set(d.get('id') for d in r1.json().get('data',[]))\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/drop', headers=HEADERS, json={{\"collectionName\":c}})\nif r.json().get('code') != 0: print(f'drop failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\nif r.json().get('code') != 0: print(f'recreate failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\ndata2 = [{{\"id\":i+10,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(1,6)]\nr = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data2}})\nif r.json().get('code') != 0: print(f'insert2 failed: {{r.text}}'); sys.exit(0)\ntime.sleep(1)\n{ld}\nr2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.5,0.6,0.7,0.8]],\"limit\":10}})\nif r2.json().get('code') != 0: print(f'search2 failed: {{r2.text}}'); sys.exit(0)\nids2 = set(d.get('id') for d in r2.json().get('data',[]))\nif ids1 & ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] data not isolated after recreate: old={{ids1}} new={{ids2}} overlap={{ids1&ids2}}'); sys.exit(1)\nelse: print(f'seq recreate_data_isolation verified'); sys.exit(0)"),
    }
}

fn build_qdrant_seq_script(kind: SeqKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
c = 'seq_' + uuid.uuid4().hex[:8]
"#;
    let create = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)"#;

    match kind {
        SeqKind::DropRecreate => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.delete(f'{{BASE}}/collections/{{c}}')\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{c}}', json={{\"vectors\":{{\"size\":4,\"distance\":\"Cosine\"}}}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code == 200 and len(r.json().get('result',[])) > 0: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate search returned data'); sys.exit(1)\nelse: print(f'seq drop_recreate verified'); sys.exit(0)"),

        SeqKind::DeleteSearch => format!("{setup}{create}\npoints = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/delete', json={{\"points\":[1]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10}})\nids = [p.get('id') for p in r.json().get('result',[])]\nif 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] deleted id=1 still in results: {{ids}}'); sys.exit(1)\nelse: print(f'seq delete_search verified'); sys.exit(0)"),

        SeqKind::ReleaseLoad => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r1.status_code != 200: print(f'search1 failed'); sys.exit(0)\nids1 = set(p.get('id') for p in r1.json().get('result',[]))\nr = requests.get(f'{{BASE}}/collections/{{c}}')\ntime.sleep(1)\nr2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r2.status_code != 200: print(f'search2 failed'); sys.exit(0)\nids2 = set(p.get('id') for p in r2.json().get('result',[]))\nif ids1 != ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] results changed after get+search: {{ids1}} vs {{ids2}}'); sys.exit(1)\nelse: print(f'seq release_load verified'); sys.exit(0)"),

        SeqKind::DropIndexSearch => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.patch(f'{{BASE}}/collections/{{c}}', json={{\"optimizers_config\":{{\"indexing_threshold\":10000}}}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/index', json={{}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed after index opt: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq drop_index_search verified'); sys.exit(0)"),

        SeqKind::UpsertSemantic => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.9,0.8,0.7,0.6]}}]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.9,0.8,0.7,0.6],\"limit\":1}})\nif r.status_code != 200: print(f'search failed'); sys.exit(0)\ntop = r.json().get('result',[])\nif top and top[0].get('id') == 1: print(f'seq upsert_semantic verified'); sys.exit(0)\nelse: print(f'[DEFECT: SEQUENCE_VIOLATION] upsert did not update: {{top}}'); sys.exit(1)"),

        SeqKind::DuplicateId => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.5,0.6,0.7,0.8]}}]}})\ntime.sleep(1)\ninfo = requests.get(f'{{BASE}}/collections/{{c}}').json()\ncount = info.get('result',{{}}).get('points_count',-1)\nif count != 1: print(f'[DEFECT: SEQUENCE_VIOLATION] duplicate id count: expected 1 got {{count}}'); sys.exit(1)\nelse: print(f'seq duplicate_id count={{count}}'); sys.exit(0)"),

        SeqKind::Rename => format!("{setup}{create}\nalias = 'seq_alias_' + uuid.uuid4().hex[:8]\nr = requests.post(f'{{BASE}}/collections/aliases', json={{\"actions\":[{{\"create_alias\":{{\"alias_name\":alias,\"collection_name\":c}}}}]}})\nif r.status_code not in (200, 201): print(f'alias create failed: {{r.status_code}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{alias}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\nif r.status_code not in (200, 201): print(f'[DEFECT: SEQUENCE_VIOLATION] upsert via alias failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq rename verified'); sys.exit(0)"),

        SeqKind::Alias => format!("{setup}{create}\nalias = 'seq_alias_' + uuid.uuid4().hex[:8]\nr = requests.post(f'{{BASE}}/collections/aliases', json={{\"actions\":[{{\"create_alias\":{{\"alias_name\":alias,\"collection_name\":c}}}}]}})\nif r.status_code not in (200, 201): print(f'alias create failed: {{r.status_code}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{alias}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{alias}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search via alias failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq alias verified'); sys.exit(0)"),

        SeqKind::FlushSearch => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points?wait=true', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after wait upsert failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq flush_search verified'); sys.exit(0)"),

        SeqKind::CompactSearch => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/index', json={{}})\ntime.sleep(2)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after compact failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq compact_search verified'); sys.exit(0)"),

        SeqKind::PartitionDrop => format!("{setup}{create}\npoints = [{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4],\"payload\":{{\"group\":\"A\"}}}},{{\"id\":2,\"vector\":[0.5,0.6,0.7,0.8],\"payload\":{{\"group\":\"B\"}}}}]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5,\"filter\":{{\"must\":[{{\"key\":\"group\",\"match\":{{\"value\":\"A\"}}}}]}}}})\nif r.status_code != 200: print(f'search A failed'); sys.exit(0)\nids_a = [p.get('id') for p in r.json().get('result',[])]\nif 2 in ids_a: print(f'[DEFECT: SEQUENCE_VIOLATION] group=A returned id=2'); sys.exit(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/delete', json={{\"filter\":{{\"must\":[{{\"key\":\"group\",\"match\":{{\"value\":\"A\"}}}}]}}}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5}})\nids = [p.get('id') for p in r.json().get('result',[])]\nif 1 in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] group=A data still found after delete: {{ids}}'); sys.exit(1)\nelse: print(f'seq partition_drop verified'); sys.exit(0)"),

        SeqKind::AlterProperties => format!("{setup}{create}\nr = requests.patch(f'{{BASE}}/collections/{{c}}', json={{\"optimizers_config\":{{\"indexing_threshold\":10000}}}})\nif r.status_code not in (200, 201): print(f'alter failed: {{r.status_code}}'); sys.exit(0)\ntime.sleep(1)\ninfo = requests.get(f'{{BASE}}/collections/{{c}}').json()\nthreshold = info.get('result',{{}}).get('config',{{}}).get('optimizer_config',{{}}).get('indexing_threshold',None)\nif threshold != 10000: print(f'[DEFECT: SEQUENCE_VIOLATION] properties not reflected: threshold={{threshold}}'); sys.exit(1)\nelse: print(f'seq alter_properties verified'); sys.exit(0)"),

        SeqKind::DynamicField => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4],\"payload\":{{\"color\":\"red\"}}}},{{\"id\":2,\"vector\":[0.5,0.6,0.7,0.8],\"payload\":{{\"color\":\"blue\"}}}}]}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5,\"filter\":{{\"must\":[{{\"key\":\"color\",\"match\":{{\"value\":\"red\"}}}}]}}}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] dynamic field query failed: {{r.status_code}}'); sys.exit(1)\nresults = r.json().get('result',[])\nfor h in results:\n    if h.get('payload',{{}}).get('color') != 'red': print(f'[DEFECT: SEQUENCE_VIOLATION] filter color=red returned wrong: {{h}}'); sys.exit(1)\nprint(f'seq dynamic_field verified'); sys.exit(0)"),

        SeqKind::DatabaseCrud => format!("{setup}\nprint(f'seq database_crud not applicable for Qdrant'); sys.exit(0)"),

        SeqKind::SearchQueryMixed => format!("{setup}{create}\npoints = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\ntime.sleep(1)\nr1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r1.status_code != 200: print(f'search failed'); sys.exit(0)\nr2 = requests.post(f'{{BASE}}/collections/{{c}}/points/scroll', json={{\"limit\":10}})\nif r2.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] scroll failed: {{r2.status_code}}'); sys.exit(1)\nsearch_ids = set(p.get('id') for p in r1.json().get('result',[]))\nscroll_ids = set(p.get('id') for p in r2.json().get('result',{{}}).get('points',[]))\nif not search_ids or not scroll_ids: print(f'[DEFECT: SEQUENCE_VIOLATION] search+scroll returned empty'); sys.exit(1)\nprint(f'seq search_query_mixed verified'); sys.exit(0)"),

        SeqKind::DeleteAllReinsert => format!("{setup}{create}\npoints = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/clear', json={{}})\ntime.sleep(1)\npoints2 = [{{\"id\":i,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(1,4)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points2}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.5,0.6,0.7,0.8],\"limit\":5}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after reinsert failed: {{r.status_code}}'); sys.exit(1)\nids = [p.get('id') for p in r.json().get('result',[])]\nif len(ids) == 0: print(f'[DEFECT: SEQUENCE_VIOLATION] no results after reinsert'); sys.exit(1)\nelse: print(f'seq delete_all_reinsert verified'); sys.exit(0)"),

        SeqKind::LoadReleaseCycle => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\ntime.sleep(1)\nr = requests.get(f'{{BASE}}/collections/{{c}}')\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after get cycle failed'); sys.exit(1)\nelse: print(f'seq load_release_cycle verified'); sys.exit(0)"),

        SeqKind::HybridSearch => format!("{setup}{create}\npoints = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/recommend', json={{\"positive\":[1],\"limit\":5}})\nif r.status_code != 200: print(f'recommend failed: {{r.status_code}}'); sys.exit(0)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after recommend failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq hybrid_search verified'); sys.exit(0)"),

        SeqKind::MultiBatchInsert => format!("{setup}{create}\nfor batch in range(3):\n    points = [{{\"id\":batch*10+i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(10)]\n    r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points}})\n    if r.status_code not in (200, 201): print(f'batch {{batch}} failed: {{r.status_code}}'); sys.exit(0)\n    time.sleep(1)\nr = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":30}})\nif r.status_code != 200: print(f'search failed: {{r.status_code}}'); sys.exit(0)\nids = [p.get('id') for p in r.json().get('result',[])]\nif len(ids) < 10: print(f'[DEFECT: SEQUENCE_VIOLATION] multi-batch search too few: {{len(ids)}}'); sys.exit(1)\nelse: print(f'seq multi_batch_insert verified, found {{len(ids)}} results'); sys.exit(0)"),

        SeqKind::RecreateDataIsolation => format!("{setup}{create}\npoints1 = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points1}})\ntime.sleep(1)\nr1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5}})\nif r1.status_code != 200: print(f'search1 failed'); sys.exit(0)\nids1 = set(p.get('id') for p in r1.json().get('result',[]))\nr = requests.delete(f'{{BASE}}/collections/{{c}}')\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/collections/{{c}}', json={{\"vectors\":{{\"size\":4,\"distance\":\"Cosine\"}}}})\ntime.sleep(1)\npoints2 = [{{\"id\":i+10,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(1,6)]\nr = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":points2}})\ntime.sleep(1)\nr2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.5,0.6,0.7,0.8],\"limit\":10}})\nif r2.status_code != 200: print(f'search2 failed'); sys.exit(0)\nids2 = set(p.get('id') for p in r2.json().get('result',[]))\noverlap = ids1 & ids2\nif overlap: print(f'[DEFECT: SEQUENCE_VIOLATION] stale data after recreate: overlap={{overlap}}'); sys.exit(1)\nelse: print(f'seq recreate_data_isolation verified'); sys.exit(0)"),
    }
}

fn build_weaviate_seq_script(kind: SeqKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
c = 'Seq_' + uuid.uuid4().hex[:8]
"#;
    let create = r#"r = requests.post(f'{BASE}/v1/schema', json={"class":c,"vectorIndexConfig":{"distance":"cosine","efConstruction":128,"maxConnections":64},"properties":[{"name":"title","dataType":["string"]}]})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)"#;

    match kind {
        SeqKind::DropRecreate => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nr = requests.delete(f'{{BASE}}/v1/schema/{{c}}')\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v1/schema', json={{\"class\":c,\"vectorIndexConfig\":{{\"distance\":\"cosine\",\"efConstruction\":128,\"maxConnections\":64}},\"properties\":[{{\"name\":\"title\",\"dataType\":[\"string\"]}}]}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ title _additional {{ distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code == 200 and len(r.json().get('data',{{}}).get('Get',{{}}).get(c,[])) > 0: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate search returned data'); sys.exit(1)\nelse: print(f'seq drop_recreate verified'); sys.exit(0)"),

        SeqKind::DeleteSearch => format!("{setup}{create}\nuids = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)],\"id\":uid}})\ntime.sleep(1)\nr = requests.delete(f'{{BASE}}/v1/objects/{{c}}/{{uids[0]}}')\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 10) {{ _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nids = [h.get('_additional',{{}}).get('id') for h in r.json().get('data',{{}}).get('Get',{{}}).get(c,[])]\nif uids[0] in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] deleted object still in search results'); sys.exit(1)\nelse: print(f'seq delete_search verified'); sys.exit(0)"),

        SeqKind::ReleaseLoad => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nq1 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\nr1 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\nids1 = set(h.get('_additional',{{}}).get('id') for h in r1.json().get('data',{{}}).get('Get',{{}}).get(c,[]))\nr = requests.get(f'{{BASE}}/v1/schema/{{c}}')\ntime.sleep(1)\nr2 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\nids2 = set(h.get('_additional',{{}}).get('id') for h in r2.json().get('data',{{}}).get('Get',{{}}).get(c,[]))\nif ids1 != ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] results changed after get+search: {{ids1}} vs {{ids2}}'); sys.exit(1)\nelse: print(f'seq release_load verified'); sys.exit(0)"),

        SeqKind::DropIndexSearch => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq drop_index_search verified'); sys.exit(0)"),

        SeqKind::UpsertSemantic => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nr = requests.put(f'{{BASE}}/v1/objects/{{c}}/{{uid}}', json={{\"properties\":{{\"title\":\"updated\"}},\"vector\":[0.9,0.8,0.7,0.6]}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.9,0.8,0.7,0.6]}} limit: 1) {{ title _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nresults = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\nif results and results[0].get('_additional',{{}}).get('id') == uid: print(f'seq upsert_semantic verified'); sys.exit(0)\nelse: print(f'[DEFECT: SEQUENCE_VIOLATION] upsert did not update: {{results}}'); sys.exit(1)"),

        SeqKind::DuplicateId => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nr2 = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test2\"}},\"vector\":[0.5,0.6,0.7,0.8],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Aggregate {{ ' + c + ' {{ meta {{ count }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\ncount = r.json().get('data',{{}}).get('Aggregate',{{}}).get(c,[{{}}])[0].get('meta',{{}}).get('count',-1)\nif count != 1: print(f'[DEFECT: PARAM_IGNORED] duplicate id count: expected 1 got {{count}}'); sys.exit(1)\nelse: print(f'seq duplicate_id count={{count}}'); sys.exit(0)"),

        SeqKind::Rename => format!("{setup}\nprint(f'seq rename not applicable for Weaviate'); sys.exit(0)"),

        SeqKind::Alias => format!("{setup}\nprint(f'seq alias not applicable for Weaviate'); sys.exit(0)"),

        SeqKind::FlushSearch => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ title _additional {{ distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after insert failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq flush_search verified'); sys.exit(0)"),

        SeqKind::CompactSearch => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ title _additional {{ distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after compact failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq compact_search verified'); sys.exit(0)"),

        SeqKind::PartitionDrop => format!("{setup}{create}\nuids = [str(uuid.uuid4()) for _ in range(2)]\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"A\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uids[0]}})\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"B\"}},\"vector\":[0.5,0.6,0.7,0.8],\"id\":uids[1]}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(where: {{path: [\"title\"], operator: Equal, valueString: \"A\"}} nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 5) {{ title _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nresults_a = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\nfor h in results_a:\n    if h.get('title') != 'A': print(f'[DEFECT: SEQUENCE_VIOLATION] filter title=A returned wrong: {{h}}'); sys.exit(1)\nr = requests.delete(f'{{BASE}}/v1/objects/{{c}}/{{uids[0]}}')\ntime.sleep(1)\nq2 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 5) {{ title _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q2}})\nids = [h.get('_additional',{{}}).get('id') for h in r.json().get('data',{{}}).get('Get',{{}}).get(c,[])]\nif uids[0] in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] deleted object still found: {{ids}}'); sys.exit(1)\nelse: print(f'seq partition_drop verified'); sys.exit(0)"),

        SeqKind::AlterProperties => format!("{setup}{create}\nr = requests.put(f'{{BASE}}/v1/schema/{{c}}', json={{\"vectorIndexConfig\":{{\"efConstruction\":256}}}})\nif r.status_code not in (200, 201): print(f'alter failed: {{r.status_code}}'); sys.exit(0)\ntime.sleep(1)\nr = requests.get(f'{{BASE}}/v1/schema/{{c}}')\nefc = r.json().get('vectorIndexConfig',{{}}).get('efConstruction')\nif efc != 256: print(f'[DEFECT: SEQUENCE_VIOLATION] properties not reflected: efConstruction={{efc}}'); sys.exit(1)\nelse: print(f'seq alter_properties verified'); sys.exit(0)"),

        SeqKind::DynamicField => format!("{setup}{create}\nuid1 = str(uuid.uuid4())\nuid2 = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"red\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid1}})\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"blue\"}},\"vector\":[0.5,0.6,0.7,0.8],\"id\":uid2}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(where: {{path: [\"title\"], operator: Equal, valueString: \"red\"}} limit: 5) {{ title _additional {{ distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nresults = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\nfor h in results:\n    if h.get('title') != 'red': print(f'[DEFECT: SEQUENCE_VIOLATION] filter title=red returned wrong: {{h}}'); sys.exit(1)\nprint(f'seq dynamic_field verified'); sys.exit(0)"),

        SeqKind::DatabaseCrud => format!("{setup}\nprint(f'seq database_crud not applicable for Weaviate'); sys.exit(0)"),

        SeqKind::SearchQueryMixed => format!("{setup}{create}\nuids = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)],\"id\":uid}})\ntime.sleep(1)\nq1 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\nr1 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\nif r1.status_code != 200: print(f'search failed'); sys.exit(0)\nq2 = '{{ Get {{ ' + c + '(limit: 10) {{ title _additional {{ id }} }} }} }}'\nr2 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q2}})\nif r2.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] query failed: {{r2.status_code}}'); sys.exit(1)\nprint(f'seq search_query_mixed verified'); sys.exit(0)"),

        SeqKind::DeleteAllReinsert => format!("{setup}{create}\nuids = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)],\"id\":uid}})\ntime.sleep(1)\nfor uid in uids:\n    r = requests.delete(f'{{BASE}}/v1/objects/{{c}}/{{uid}}')\ntime.sleep(1)\nuids2 = [str(uuid.uuid4()) for _ in range(3)]\nfor i, uid in enumerate(uids2):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test2\"}},\"vector\":[0.5*(i+1),0.6*(i+1),0.7*(i+1),0.8*(i+1)],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.5,0.6,0.7,0.8]}} limit: 5) {{ _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nresults = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\nif len(results) == 0: print(f'[DEFECT: SEQUENCE_VIOLATION] no results after reinsert'); sys.exit(1)\nelse: print(f'seq delete_all_reinsert verified'); sys.exit(0)"),

        SeqKind::LoadReleaseCycle => format!("{setup}{create}\nuid = str(uuid.uuid4())\nr = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search after cycle failed'); sys.exit(1)\nelse: print(f'seq load_release_cycle verified'); sys.exit(0)"),

        SeqKind::HybridSearch => format!("{setup}{create}\nuids = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)],\"id\":uid}})\ntime.sleep(1)\nq = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\nif r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed: {{r.status_code}}'); sys.exit(1)\nelse: print(f'seq hybrid_search verified'); sys.exit(0)"),

        SeqKind::MultiBatchInsert => format!("{setup}{create}\nfor batch in range(3):\n    objs = []\n    for i in range(10):\n        objs.append({{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i],\"id\":str(uuid.uuid4())}})\n    r = requests.post(f'{{BASE}}/v1/batch/objects', json={{\"objects\":objs}})\n    if r.status_code not in (200, 201): print(f'batch {{batch}} failed: {{r.status_code}}'); sys.exit(0)\n    time.sleep(1)\nq = '{{ Aggregate {{ ' + c + ' {{ meta {{ count }} }} }} }}'\nr = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\ncount = r.json().get('data',{{}}).get('Aggregate',{{}}).get(c,[{{}}])[0].get('meta',{{}}).get('count',-1)\nif count < 10: print(f'[DEFECT: SEQUENCE_VIOLATION] multi-batch count too few: {{count}}'); sys.exit(1)\nelse: print(f'seq multi_batch_insert verified, count={{count}}'); sys.exit(0)"),

        SeqKind::RecreateDataIsolation => format!("{setup}{create}\nuids1 = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids1):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1*(i+1),0.2*(i+1),0.3*(i+1),0.4*(i+1)],\"id\":uid}})\ntime.sleep(1)\nq1 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 5) {{ _additional {{ id distance }} }} }} }}'\nr1 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\nids1 = set(h.get('_additional',{{}}).get('id') for h in r1.json().get('data',{{}}).get('Get',{{}}).get(c,[]))\nr = requests.delete(f'{{BASE}}/v1/schema/{{c}}')\ntime.sleep(1)\nr = requests.post(f'{{BASE}}/v1/schema', json={{\"class\":c,\"vectorIndexConfig\":{{\"distance\":\"cosine\",\"efConstruction\":128,\"maxConnections\":64}},\"properties\":[{{\"name\":\"title\",\"dataType\":[\"string\"]}}]}})\ntime.sleep(1)\nuids2 = [str(uuid.uuid4()) for _ in range(5)]\nfor i, uid in enumerate(uids2):\n    r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test2\"}},\"vector\":[0.5*(i+1),0.6*(i+1),0.7*(i+1),0.8*(i+1)],\"id\":uid}})\ntime.sleep(1)\nq2 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.5,0.6,0.7,0.8]}} limit: 10) {{ _additional {{ id distance }} }} }} }}'\nr2 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q2}})\nids2 = set(h.get('_additional',{{}}).get('id') for h in r2.json().get('data',{{}}).get('Get',{{}}).get(c,[]))\noverlap = ids1 & ids2\nif overlap: print(f'[DEFECT: SEQUENCE_VIOLATION] stale data after recreate: overlap={{overlap}}'); sys.exit(1)\nelse: print(f'seq recreate_data_isolation verified'); sys.exit(0)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::TypeConstraint;
    use crate::contract::schema::RejectionPolicy;
    use crate::contract::store::{AnnotatedTypeConstraint, Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "2.4");
        for (ep, pn) in [
            ("/v2/vectordb/collections/create", "collectionName"),
            ("/v2/vectordb/collections/drop", "collectionName"),
            ("/v2/vectordb/collections/load", "collectionName"),
            ("/v2/vectordb/collections/release", "collectionName"),
            ("/v2/vectordb/collections/flush", "collectionName"),
            ("/v2/vectordb/collections/compact", "collectionName"),
            ("/v2/vectordb/collections/rename", "collectionName"),
            ("/v2/vectordb/collections/alter_properties", "properties"),
            ("/v2/vectordb/entities/insert", "data"),
            ("/v2/vectordb/entities/upsert", "data"),
            ("/v2/vectordb/entities/delete", "filter"),
            ("/v2/vectordb/entities/search", "limit"),
            ("/v2/vectordb/entities/query", "filter"),
            ("/v2/vectordb/entities/hybrid_search", "searchParams"),
            ("/v2/vectordb/indexes/create", "indexParams"),
            ("/v2/vectordb/aliases/create", "aliasName"),
            ("/v2/vectordb/partitions/create", "partitionName"),
            ("/v2/vectordb/partitions/drop", "partitionName"),
            ("/v2/vectordb/collections/fields/add", "fieldName"),
            ("/v2/vectordb/databases/create", "dbName"),
        ] {
            store.type_constraints.push(AnnotatedTypeConstraint {
                constraint: TypeConstraint {
                    param_name: pn.to_string(),
                    expected_type: "string".to_string(),
                    violation_examples: vec![],
                },
                endpoint: Some(ep.to_string()),
                source: ConstraintSource::OpenapiDerived,
                confidence: Confidence::High,
                rejection_policy: Some(RejectionPolicy::Reject),
            });
        }
        store
    }

    #[test]
    fn test_from_store_generates_all_sequences() {
        let store = make_test_store();
        let cases = SequenceTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 15, "Should generate at least 15 sequence test cases, got {}", cases.len());
    }

    #[test]
    fn test_milvus_scripts_contain_auth() {
        let store = make_test_store();
        let cases = SequenceTestGenerator::from_store(&store, TargetStyle::Milvus);
        for case in &cases {
            assert!(case.script.contains("{{TESTVDB_AUTH_HEADER}}"), "Missing auth: {}", case.name);
        }
    }

    #[test]
    fn test_scripts_contain_defect_markers() {
        let store = make_test_store();
        let cases = SequenceTestGenerator::from_store(&store, TargetStyle::Milvus);
        for case in &cases {
            assert!(case.script.contains("[DEFECT:"), "Missing DEFECT marker: {}", case.name);
        }
    }

    #[test]
    fn test_no_duplicate_names() {
        let store = make_test_store();
        let cases = SequenceTestGenerator::from_store(&store, TargetStyle::Milvus);
        let names: Vec<_> = cases.iter().map(|c| &c.name).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "No duplicate names");
    }

    #[test]
    fn test_minimal_store_generates_fewer() {
        let store = ContractStore::new("milvus", "2.4");
        let cases = SequenceTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() < 5, "Empty store should generate few sequences, got {}", cases.len());
    }
}