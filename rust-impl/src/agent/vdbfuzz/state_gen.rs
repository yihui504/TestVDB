use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTestCase {
    pub name: String,
    pub state_pattern: StatePattern,
    pub endpoint: String,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatePattern {
    CrudLifecycle,
    UniquenessConstraint,
    ResourceNotExist,
    StateTransition,
    UpsertSemantic,
}

pub struct StateTestGenerator;

impl StateTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<StateTestCase> {
        let mut cases = Vec::new();

        let has_insert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/insert")) || atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/upsert"))
        });
        let has_search = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/search")) || atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/query"))
        });
        let has_delete = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/delete"))
        });
        let has_create = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/create"))
        });
        let has_drop = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("collections/drop"))
        });
        let has_upsert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("entities/upsert"))
        });
        let has_partition = store.type_constraints.iter().any(|atc| {
            atc.endpoint.as_deref().map_or(false, |e| e.contains("partitions/create")) || atc.endpoint.as_deref().map_or(false, |e| e.contains("partitions/drop"))
        });

        if has_insert && has_search && has_delete {
            cases.push(Self::generate_insert_search_delete_search(style));
            cases.push(Self::generate_insert_delete_insert_search(style));
        }

        if has_insert && has_upsert && has_search {
            cases.push(Self::generate_upsert_changes_vector(style));
        }

        if has_create && has_drop {
            cases.push(Self::generate_create_drop_create_dim(style));
            if style != TargetStyle::Milvus {
                cases.push(Self::generate_duplicate_collection(style));
            }
        }

        if has_create && has_drop && has_search {
            cases.push(Self::generate_drop_then_search(style));
        }

        if has_partition {
            cases.push(Self::generate_partition_data_isolation(style));
        }

        for asc in &store.state_constraints {
            let desc = asc.constraint.description.to_lowercase();
            if desc.contains("must exist") && desc.contains("before") {
                if desc.contains("search") || desc.contains("query") {
                    cases.push(Self::generate_search_without_collection(style));
                }
                if desc.contains("insert") || desc.contains("upsert") {
                    cases.push(Self::generate_insert_without_collection(style));
                }
            }
            if desc.contains("unique") || desc.contains("duplicate") || desc.contains("already exist") {
                if desc.contains("collection") && style != TargetStyle::Milvus {
                    cases.push(Self::generate_duplicate_collection(style));
                }
            }
        }

        cases.dedup_by(|a, b| a.name == b.name);

        cases
    }

    fn generate_insert_search_delete_search(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_insert_search_delete_search".to_string(),
            state_pattern: StatePattern::CrudLifecycle,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_state_script(style, StateScriptKind::InsertSearchDeleteSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_insert_delete_insert_search(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_insert_delete_insert_search".to_string(),
            state_pattern: StatePattern::StateTransition,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_state_script(style, StateScriptKind::InsertDeleteInsertSearch),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_upsert_changes_vector(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_upsert_changes_vector".to_string(),
            state_pattern: StatePattern::UpsertSemantic,
            endpoint: "/v2/vectordb/entities/upsert".to_string(),
            script: build_state_script(style, StateScriptKind::UpsertChangesVector),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_create_drop_create_dim(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_create_drop_create_different_dim".to_string(),
            state_pattern: StatePattern::StateTransition,
            endpoint: "/v2/vectordb/collections/create".to_string(),
            script: build_state_script(style, StateScriptKind::CreateDropCreateDim),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_duplicate_collection(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_duplicate_collection".to_string(),
            state_pattern: StatePattern::UniquenessConstraint,
            endpoint: "/v2/vectordb/collections/create".to_string(),
            script: build_state_script(style, StateScriptKind::DuplicateCollection),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_drop_then_search(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_drop_then_search".to_string(),
            state_pattern: StatePattern::ResourceNotExist,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_state_script(style, StateScriptKind::DropThenSearch),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_partition_data_isolation(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_partition_data_isolation".to_string(),
            state_pattern: StatePattern::CrudLifecycle,
            endpoint: "/v2/vectordb/partitions/drop".to_string(),
            script: build_state_script(style, StateScriptKind::PartitionDataIsolation),
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        }
    }

    fn generate_search_without_collection(style: TargetStyle) -> StateTestCase {
        StateTestCase {
            name: "state_search_without_collection".to_string(),
            state_pattern: StatePattern::ResourceNotExist,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_state_script(style, StateScriptKind::SearchWithoutCollection),
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_insert_without_collection(style: TargetStyle) -> StateTestCase {
        let marker = match style {
            TargetStyle::Weaviate => "PARAM_IGNORED",
            _ => "ILLEGAL_SUCCESS",
        };
        let script = build_state_script(style, StateScriptKind::InsertWithoutCollection);
        let script = script.replace("[DEFECT: ILLEGAL_SUCCESS]", &format!("[DEFECT: {}]", marker));
        StateTestCase {
            name: "state_insert_without_collection".to_string(),
            state_pattern: StatePattern::ResourceNotExist,
            endpoint: "/v2/vectordb/entities/insert".to_string(),
            script,
            defect_marker: marker.to_string(),
        }
    }
}

enum StateScriptKind {
    InsertSearchDeleteSearch,
    InsertDeleteInsertSearch,
    UpsertChangesVector,
    CreateDropCreateDim,
    DuplicateCollection,
    DropThenSearch,
    PartitionDataIsolation,
    SearchWithoutCollection,
    InsertWithoutCollection,
}

fn build_state_script(style: TargetStyle, kind: StateScriptKind) -> String {
    match style {
        TargetStyle::Milvus => build_milvus_state_script(kind),
        TargetStyle::Qdrant => build_qdrant_state_script(kind),
        TargetStyle::Weaviate => build_weaviate_state_script(kind),
        TargetStyle::PgVector => String::new(),
    }
}

fn build_milvus_state_script(kind: StateScriptKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}
c = 'state_' + uuid.uuid4().hex[:8]
"#;

    let create = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)"#;

    let load = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)"#;

    let flush = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/flush', headers=HEADERS, json={"collectionName":c})
time.sleep(2)"#;

    let release = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/release', headers=HEADERS, json={"collectionName":c})
time.sleep(1)"#;

    match kind {
        StateScriptKind::InsertSearchDeleteSearch => format!(
            "{setup}{create}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             if r.json().get('code') != 0: print(f'search1 failed: {{r.text}}'); sys.exit(0)\n\
             ids1 = [d.get('id') for d in r.json().get('data',[])]\n\
             if 1 not in ids1: print(f'[DEFECT: SEQUENCE_VIOLATION] insert+search: id=1 not found after insert'); sys.exit(1)\n\
             {flush}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/delete', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"id == 1\"}})\n\
             if r.json().get('code') != 0: print(f'delete failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             if r.json().get('code') != 0: print(f'search2 failed: {{r.text}}'); sys.exit(0)\n\
             ids2 = [d.get('id') for d in r.json().get('data',[])]\n\
             if 1 in ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] delete+search: id=1 still found after delete'); sys.exit(1)\n\
             else: print(f'state insert_search_delete_search verified'); sys.exit(0)",
            setup=setup, create=create, load=load, flush=flush,
        ),

        StateScriptKind::InsertDeleteInsertSearch => format!(
            "{setup}{create}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.json().get('code') != 0: print(f'insert1 failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {flush}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/delete', headers=HEADERS, json={{\"collectionName\":c,\"filter\":\"id == 1\"}})\n\
             if r.json().get('code') != 0: print(f'delete failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.5,0.6,0.7,0.8]}}]}})\n\
             if r.json().get('code') != 0: print(f'insert2 failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.5,0.6,0.7,0.8]],\"limit\":3}})\n\
             if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)\n\
             ids = [d.get('id') for d in r.json().get('data',[])]\n\
             if 1 not in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] re-insert+search: id=1 not found after re-insert'); sys.exit(1)\n\
             else: print(f'state insert_delete_insert_search verified'); sys.exit(0)",
            setup=setup, create=create, flush=flush, load=load,
        ),

        StateScriptKind::UpsertChangesVector => format!(
            "{setup}{create}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":1}})\n\
             if r1.json().get('code') != 0: print(f'search1 failed: {{r1.text}}'); sys.exit(0)\n\
             dist1 = r1.json().get('data',[{{}}])[0].get('distance') if r1.json().get('data') else None\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/upsert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.9,0.8,0.7,0.6]}}]}})\n\
             if r.json().get('code') != 0: print(f'upsert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":1}})\n\
             if r2.json().get('code') != 0: print(f'search2 failed: {{r2.text}}'); sys.exit(0)\n\
             dist2 = r2.json().get('data',[{{}}])[0].get('distance') if r2.json().get('data') else None\n\
             if dist1 is not None and dist2 is not None and dist1 == dist2: print(f'[DEFECT: METAMORPHIC_VIOLATION] upsert did not change distance: before={{dist1}} after={{dist2}}'); sys.exit(1)\n\
             else: print(f'state upsert_changes_vector verified: dist1={{dist1}} dist2={{dist2}}'); sys.exit(0)",
            setup=setup, create=create, load=load,
        ),

        StateScriptKind::CreateDropCreateDim => format!(
            "{setup}{create}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/drop', headers=HEADERS, json={{\"collectionName\":c}})\n\
             if r.json().get('code') != 0: print(f'drop failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(3)\n\
             recreate_ok = False\n\
             for attempt in range(3):\n\
                 r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":8}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\n\
                 if r.json().get('code') == 0:\n\
                     recreate_ok = True\n\
                     break\n\
                 time.sleep(2)\n\
             if not recreate_ok: print(f'recreate failed after 3 attempts: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/describe', headers=HEADERS, json={{\"collectionName\":c}})\n\
             dim = r.json().get('data',{{}}).get('dimension')\n\
             if dim != 8: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate dim: expected 8 got {{dim}}'); sys.exit(1)\n\
             else: print(f'state create_drop_create_different_dim verified: dim={{dim}}'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DuplicateCollection => format!(
            "{setup}{create}\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"AUTOINDEX\"}}]}})\n\
             if r2.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] duplicate collection name accepted'); sys.exit(1)\n\
             else: print(f'duplicate collection properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DropThenSearch => format!(
            "{setup}{create}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/drop', headers=HEADERS, json={{\"collectionName\":c}})\n\
             if r.json().get('code') != 0: print(f'drop failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] search on dropped collection succeeded'); sys.exit(1)\n\
             else: print(f'search on dropped collection properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::PartitionDataIsolation => format!(
            "{setup}{create}\n\
             p = 'part_' + uuid.uuid4().hex[:8]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/partitions/create', headers=HEADERS, json={{\"collectionName\":c,\"partitionName\":p}})\n\
             if r.json().get('code') != 0: print(f'partition create failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}],\"partitionName\":p}})\n\
             if r.json().get('code') != 0: print(f'insert partition failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3,\"partitionNames\":[p]}})\n\
             if r.json().get('code') != 0: print(f'search partition failed: {{r.text}}'); sys.exit(0)\n\
             ids_before = [d.get('id') for d in r.json().get('data',[])]\n\
             if 1 not in ids_before: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data not found before drop'); sys.exit(1)\n\
             {release}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/partitions/drop', headers=HEADERS, json={{\"collectionName\":c,\"partitionName\":p}})\n\
             if r.json().get('code') != 0: print(f'drop partition failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             ids_after = [d.get('id') for d in r.json().get('data',[])]\n\
             if 1 in ids_after: print(f'[DEFECT: SEQUENCE_VIOLATION] partition data still found after drop partition: {{ids_after}}'); sys.exit(1)\n\
             else: print(f'state partition_data_isolation verified'); sys.exit(0)",
            setup=setup, create=create, load=load, release=release,
        ),

        StateScriptKind::SearchWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'nonexistent_' + uuid.uuid4().hex[:8]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":fake_c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection succeeded'); sys.exit(1)\n\
             else: print(f'search on nonexistent collection properly rejected'); sys.exit(0)",
            setup=setup,
        ),

        StateScriptKind::InsertWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'nonexistent_' + uuid.uuid4().hex[:8]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":fake_c,\"data\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] insert on nonexistent collection succeeded'); sys.exit(1)\n\
             else: print(f'insert on nonexistent collection properly rejected'); sys.exit(0)",
            setup=setup,
        ),
    }
}

fn build_qdrant_state_script(kind: StateScriptKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
c = 'state_' + uuid.uuid4().hex[:8]
"#;

    let create = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)"#;

    match kind {
        StateScriptKind::InsertSearchDeleteSearch => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\n\
             if r.status_code != 200: print(f'search1 failed: {{r.status_code}}'); sys.exit(0)\n\
             ids1 = [p.get('id') for p in r.json().get('result',[])]\n\
             if 1 not in ids1: print(f'[DEFECT: SEQUENCE_VIOLATION] insert+search: id=1 not found'); sys.exit(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/delete', json={{\"points\":[1]}})\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\n\
             if r.status_code != 200: print(f'search2 failed: {{r.status_code}}'); sys.exit(0)\n\
             ids2 = [p.get('id') for p in r.json().get('result',[])]\n\
             if 1 in ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] delete+search: id=1 still found'); sys.exit(1)\n\
             else: print(f'state insert_search_delete_search verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DuplicateCollection => format!(
            "{setup}{create}\n\
             r2 = requests.put(f'{{BASE}}/collections/{{c}}', json={{\"vectors\":{{\"size\":4,\"distance\":\"Cosine\"}}}})\n\
             if r2.status_code in (200, 201): print(f'[DEFECT: ILLEGAL_SUCCESS] duplicate collection accepted'); sys.exit(1)\n\
             else: print(f'duplicate collection properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::SearchWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'nonexistent_' + uuid.uuid4().hex[:8]\n\
             r = requests.post(f'{{BASE}}/collections/{{fake_c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\n\
             if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] search on nonexistent collection succeeded'); sys.exit(1)\n\
             else: print(f'search on nonexistent collection properly rejected'); sys.exit(0)",
            setup=setup,
        ),

        StateScriptKind::InsertDeleteInsertSearch => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert1 failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/delete', json={{\"points\":[1]}})\n\
             time.sleep(1)\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.5,0.6,0.7,0.8]}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert2 failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.5,0.6,0.7,0.8],\"limit\":3}})\n\
             if r.status_code != 200: print(f'search failed: {{r.status_code}}'); sys.exit(0)\n\
             ids = [p.get('id') for p in r.json().get('result',[])]\n\
             if 1 not in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] re-insert+search: id=1 not found after re-insert'); sys.exit(1)\n\
             else: print(f'state insert_delete_insert_search verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::UpsertChangesVector => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":1}})\n\
             if r1.status_code != 200: print(f'search1 failed: {{r1.status_code}}'); sys.exit(0)\n\
             score1 = r1.json().get('result',[{{}}])[0].get('score') if r1.json().get('result') else None\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.9,0.8,0.7,0.6]}}]}})\n\
             if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":1}})\n\
             if r2.status_code != 200: print(f'search2 failed: {{r2.status_code}}'); sys.exit(0)\n\
             score2 = r2.json().get('result',[{{}}])[0].get('score') if r2.json().get('result') else None\n\
             if score1 is not None and score2 is not None and score1 == score2: print(f'[DEFECT: METAMORPHIC_VIOLATION] upsert did not change distance: before={{score1}} after={{score2}}'); sys.exit(1)\n\
             else: print(f'state upsert_changes_vector verified: score1={{score1}} score2={{score2}}'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::CreateDropCreateDim => format!(
            "{setup}{create}\n\
             r = requests.delete(f'{{BASE}}/collections/{{c}}')\n\
             if r.status_code not in (200, 201): print(f'drop failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             recreate_ok = False\n\
             for attempt in range(3):\n\
                 r = requests.put(f'{{BASE}}/collections/{{c}}', json={{\"vectors\":{{\"size\":8,\"distance\":\"Cosine\"}}}})\n\
                 if r.status_code in (200, 201):\n\
                     recreate_ok = True\n\
                     break\n\
                 time.sleep(2)\n\
             if not recreate_ok: print(f'recreate failed after 3 attempts: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.get(f'{{BASE}}/collections/{{c}}')\n\
             dim = r.json().get('result',{{}}).get('config',{{}}).get('params',{{}}).get('vectors',{{}}).get('size')\n\
             if dim != 8: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate dim: expected 8 got {{dim}}'); sys.exit(1)\n\
             else: print(f'state create_drop_create_different_dim verified: dim={{dim}}'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DropThenSearch => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.delete(f'{{BASE}}/collections/{{c}}')\n\
             if r.status_code not in (200, 201): print(f'drop failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\n\
             if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] search on dropped collection succeeded'); sys.exit(1)\n\
             else: print(f'search on dropped collection properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::PartitionDataIsolation => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4],\"payload\":{{\"group\":\"A\"}}}},{{\"id\":2,\"vector\":[0.5,0.6,0.7,0.8],\"payload\":{{\"group\":\"B\"}}}}]}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3,\"filter\":{{\"must\":[{{\"key\":\"group\",\"match\":{{\"value\":\"A\"}}}}]}}}})\n\
             if r.status_code != 200: print(f'search A failed: {{r.status_code}}'); sys.exit(0)\n\
             ids_a = [p.get('id') for p in r.json().get('result',[])]\n\
             if 2 in ids_a: print(f'[DEFECT: SEQUENCE_VIOLATION] payload filter isolation: group A search returned group B point id=2'); sys.exit(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.5,0.6,0.7,0.8],\"limit\":3,\"filter\":{{\"must\":[{{\"key\":\"group\",\"match\":{{\"value\":\"B\"}}}}]}}}})\n\
             if r.status_code != 200: print(f'search B failed: {{r.status_code}}'); sys.exit(0)\n\
             ids_b = [p.get('id') for p in r.json().get('result',[])]\n\
             if 1 in ids_b: print(f'[DEFECT: SEQUENCE_VIOLATION] payload filter isolation: group B search returned group A point id=1'); sys.exit(1)\n\
             else: print(f'state partition_data_isolation verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::InsertWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'nonexistent_' + uuid.uuid4().hex[:8]\n\
             r = requests.put(f'{{BASE}}/collections/{{fake_c}}/points', json={{\"points\":[{{\"id\":1,\"vector\":[0.1,0.2,0.3,0.4]}}]}})\n\
             if r.status_code in (200, 201): print(f'[DEFECT: ILLEGAL_SUCCESS] upsert on nonexistent collection succeeded'); sys.exit(1)\n\
             else: print(f'upsert on nonexistent collection properly rejected'); sys.exit(0)",
            setup=setup,
        ),
    }
}

fn build_weaviate_state_script(kind: StateScriptKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
c = 'State_' + uuid.uuid4().hex[:8]
"#;

    let create = r#"r = requests.post(f'{BASE}/v1/schema', json={"class":c,"vectorIndexConfig":{"distance":"cosine","efConstruction":128,"maxConnections":64},"properties":[{"name":"title","dataType":["string"]}]})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)"#;

    match kind {
        StateScriptKind::InsertSearchDeleteSearch => format!(
            "{setup}{create}\n\
             uid = str(uuid.uuid4())\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             q1 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\n\
             if r.status_code != 200: print(f'search1 failed: {{r.status_code}}'); sys.exit(0)\n\
             ids1 = [h.get('_additional',{{}}).get('id') for h in r.json().get('data',{{}}).get('Get',{{}}).get(c,[])]\n\
             if uid not in ids1: print(f'[DEFECT: SEQUENCE_VIOLATION] insert+search: uid not found after insert'); sys.exit(1)\n\
             r = requests.delete(f'{{BASE}}/v1/objects/{{c}}/{{uid}}')\n\
             time.sleep(1)\n\
             q2 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q2}})\n\
             if r.status_code != 200: print(f'search2 failed: {{r.status_code}}'); sys.exit(0)\n\
             ids2 = [h.get('_additional',{{}}).get('id') for h in r.json().get('data',{{}}).get('Get',{{}}).get(c,[])]\n\
             if uid in ids2: print(f'[DEFECT: SEQUENCE_VIOLATION] delete+search: uid still found after delete'); sys.exit(1)\n\
             else: print(f'state insert_search_delete_search verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::InsertDeleteInsertSearch => format!(
            "{setup}{create}\n\
             uid = str(uuid.uuid4())\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\n\
             if r.status_code not in (200, 201): print(f'insert1 failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.delete(f'{{BASE}}/v1/objects/{{c}}/{{uid}}')\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test2\"}},\"vector\":[0.5,0.6,0.7,0.8],\"id\":uid}})\n\
             if r.status_code not in (200, 201): print(f'insert2 failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             q = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.5,0.6,0.7,0.8]}} limit: 3) {{ _additional {{ id distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\n\
             if r.status_code != 200: print(f'search failed: {{r.status_code}}'); sys.exit(0)\n\
             ids = [h.get('_additional',{{}}).get('id') for h in r.json().get('data',{{}}).get('Get',{{}}).get(c,[])]\n\
             if uid not in ids: print(f'[DEFECT: SEQUENCE_VIOLATION] re-insert+search: uid not found after re-insert'); sys.exit(1)\n\
             else: print(f'state insert_delete_insert_search verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::UpsertChangesVector => format!(
            "{setup}{create}\n\
             uid = str(uuid.uuid4())\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             q1 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 1) {{ _additional {{ id distance }} }} }} }}'\n\
             r1 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q1}})\n\
             if r1.status_code != 200: print(f'search1 failed: {{r1.status_code}}'); sys.exit(0)\n\
             dist1 = r1.json().get('data',{{}}).get('Get',{{}}).get(c,[{{}}])[0].get('_additional',{{}}).get('distance') if r1.json().get('data',{{}}).get('Get',{{}}).get(c) else None\n\
             r = requests.put(f'{{BASE}}/v1/objects/{{c}}/{{uid}}', json={{\"properties\":{{\"title\":\"updated\"}},\"vector\":[0.9,0.8,0.7,0.6]}})\n\
             if r.status_code not in (200, 201): print(f'upsert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             q2 = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 1) {{ _additional {{ id distance }} }} }} }}'\n\
             r2 = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q2}})\n\
             if r2.status_code != 200: print(f'search2 failed: {{r2.status_code}}'); sys.exit(0)\n\
             dist2 = r2.json().get('data',{{}}).get('Get',{{}}).get(c,[{{}}])[0].get('_additional',{{}}).get('distance') if r2.json().get('data',{{}}).get('Get',{{}}).get(c) else None\n\
             if dist1 is not None and dist2 is not None and dist1 == dist2: print(f'[DEFECT: METAMORPHIC_VIOLATION] upsert did not change distance: before={{dist1}} after={{dist2}}'); sys.exit(1)\n\
             else: print(f'state upsert_changes_vector verified: dist1={{dist1}} dist2={{dist2}}'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::CreateDropCreateDim => format!(
            "{setup}{create}\n\
             r = requests.delete(f'{{BASE}}/v1/schema/{{c}}')\n\
             if r.status_code not in (200, 201): print(f'drop failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             recreate_ok = False\n\
             for attempt in range(3):\n\
                 r = requests.post(f'{{BASE}}/v1/schema', json={{\"class\":c,\"vectorIndexConfig\":{{\"distance\":\"cosine\",\"efConstruction\":256,\"maxConnections\":32}},\"properties\":[{{\"name\":\"title\",\"dataType\":[\"string\"]}}]}})\n\
                 if r.status_code in (200, 201):\n\
                     recreate_ok = True\n\
                     break\n\
                 time.sleep(2)\n\
             if not recreate_ok: print(f'recreate failed after 3 attempts: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.get(f'{{BASE}}/v1/schema/{{c}}')\n\
             efc = r.json().get('vectorIndexConfig',{{}}).get('efConstruction')\n\
             if efc != 256: print(f'[DEFECT: SEQUENCE_VIOLATION] recreate config: expected efConstruction=256 got {{efc}}'); sys.exit(1)\n\
             else: print(f'state create_drop_create_different_dim verified: efConstruction={{efc}}'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DuplicateCollection => format!(
            "{setup}{create}\n\
             r2 = requests.post(f'{{BASE}}/v1/schema', json={{\"class\":c,\"vectorIndexConfig\":{{\"distance\":\"cosine\",\"efConstruction\":128,\"maxConnections\":64}},\"properties\":[{{\"name\":\"title\",\"dataType\":[\"string\"]}}]}})\n\
             if r2.status_code in (200, 201): print(f'[DEFECT: ILLEGAL_SUCCESS] duplicate class accepted'); sys.exit(1)\n\
             else: print(f'duplicate class properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::DropThenSearch => format!(
            "{setup}{create}\n\
             uid = str(uuid.uuid4())\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             r = requests.delete(f'{{BASE}}/v1/schema/{{c}}')\n\
             if r.status_code not in (200, 201): print(f'drop failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             q = '{{ Get {{ ' + c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\n\
             if r.status_code == 200 and r.json().get('data',{{}}).get('Get',{{}}).get(c) is not None: print(f'[DEFECT: ILLEGAL_SUCCESS] search on dropped class succeeded'); sys.exit(1)\n\
             else: print(f'search on dropped class properly rejected'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::PartitionDataIsolation => format!(
            "{setup}{create}\n\
             uid_a = str(uuid.uuid4())\n\
             uid_b = str(uuid.uuid4())\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"A\"}},\"vector\":[0.1,0.2,0.3,0.4],\"id\":uid_a}})\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":c,\"properties\":{{\"title\":\"B\"}},\"vector\":[0.5,0.6,0.7,0.8],\"id\":uid_b}})\n\
             if r.status_code not in (200, 201): print(f'insert failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             qa = '{{ Get {{ ' + c + '(where: {{path: [\"title\"], operator: Equal, valueString: \"A\"}} nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ title _additional {{ id distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":qa}})\n\
             if r.status_code != 200: print(f'search A failed: {{r.status_code}}'); sys.exit(0)\n\
             results_a = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\n\
             for h in results_a:\n\
                 if h.get('title') != 'A': print(f'[DEFECT: SEQUENCE_VIOLATION] filter title=A returned wrong: {{h}}'); sys.exit(1)\n\
             qb = '{{ Get {{ ' + c + '(where: {{path: [\"title\"], operator: Equal, valueString: \"B\"}} nearVector: {{vector: [0.5,0.6,0.7,0.8]}} limit: 3) {{ title _additional {{ id distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":qb}})\n\
             if r.status_code != 200: print(f'search B failed: {{r.status_code}}'); sys.exit(0)\n\
             results_b = r.json().get('data',{{}}).get('Get',{{}}).get(c,[])\n\
             for h in results_b:\n\
                 if h.get('title') != 'B': print(f'[DEFECT: SEQUENCE_VIOLATION] filter title=B returned wrong: {{h}}'); sys.exit(1)\n\
             else: print(f'state partition_data_isolation verified'); sys.exit(0)",
            setup=setup, create=create,
        ),

        StateScriptKind::SearchWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'Nonexistent_' + uuid.uuid4().hex[:8]\n\
             q = '{{ Get {{ ' + fake_c + '(nearVector: {{vector: [0.1,0.2,0.3,0.4]}} limit: 3) {{ _additional {{ distance }} }} }} }}'\n\
             r = requests.post(f'{{BASE}}/v1/graphql', json={{\"query\":q}})\n\
             data = r.json().get('data',{{}}).get('Get',{{}}).get(fake_c)\n\
             if data is not None and len(data) > 0: print(f'[DEFECT: ILLEGAL_SUCCESS] search on nonexistent class succeeded'); sys.exit(1)\n\
             else: print(f'search on nonexistent class properly rejected'); sys.exit(0)",
            setup=setup,
        ),

        StateScriptKind::InsertWithoutCollection => format!(
            "{setup}\n\
             fake_c = 'Nonexistent_' + uuid.uuid4().hex[:8]\n\
             r = requests.post(f'{{BASE}}/v1/objects', json={{\"class\":fake_c,\"properties\":{{\"title\":\"test\"}},\"vector\":[0.1,0.2,0.3,0.4]}})\n\
             if r.status_code in (200, 201): print(f'[DEFECT: ILLEGAL_SUCCESS] insert on nonexistent class succeeded'); sys.exit(1)\n\
             else: print(f'insert on nonexistent class properly rejected'); sys.exit(0)",
            setup=setup,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{RejectionPolicy, TypeConstraint};
    use crate::contract::store::{AnnotatedTypeConstraint, Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "2.4");

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/search".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "data".to_string(),
                expected_type: "array".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/insert".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "data".to_string(),
                expected_type: "array".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/upsert".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "filter".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/entities/delete".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "collectionName".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/collections/create".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "collectionName".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/collections/drop".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "partitionName".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: Some("/v2/vectordb/partitions/create".to_string()),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
            rejection_policy: Some(RejectionPolicy::Reject),
        });

        store
    }

    #[test]
    fn test_from_store_generates_state_cases() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Milvus);

        assert!(cases.len() >= 5, "Should generate at least 5 state test cases, got {}", cases.len());

        let names: Vec<_> = cases.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"state_insert_search_delete_search"), "Missing insert_search_delete_search");
        assert!(names.contains(&"state_insert_delete_insert_search"), "Missing insert_delete_insert_search");
        assert!(names.contains(&"state_upsert_changes_vector"), "Missing upsert_changes_vector");
        assert!(names.contains(&"state_create_drop_create_different_dim"), "Missing create_drop_create_dim");
        assert!(!names.contains(&"state_duplicate_collection"), "Milvus should not generate duplicate_collection (by-design idempotent)");
    }

    #[test]
    fn test_milvus_scripts_contain_auth() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Milvus);

        assert!(!cases.is_empty());
        for case in &cases {
            assert!(case.script.contains("{{TESTVDB_AUTH_HEADER}}"), "Milvus state script missing auth: {}", case.name);
        }
    }

    #[test]
    fn test_milvus_scripts_contain_defect_markers() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Milvus);

        for case in &cases {
            assert!(case.script.contains("[DEFECT:"), "State script missing DEFECT marker: {}", case.name);
        }
    }

    #[test]
    fn test_qdrant_scripts_no_auth() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Qdrant);

        assert!(!cases.is_empty());
        for case in &cases {
            assert!(!case.script.contains("{{TESTVDB_AUTH_HEADER}}"), "Qdrant state script should not have auth: {}", case.name);
        }
    }

    #[test]
    fn test_partition_data_isolation_generated() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Milvus);

        let partition_cases: Vec<_> = cases.iter()
            .filter(|c| c.name.contains("partition"))
            .collect();
        assert!(!partition_cases.is_empty(), "Should have partition data isolation test");
    }

    #[test]
    fn test_no_duplicate_cases() {
        let store = make_test_store();
        let cases = StateTestGenerator::from_store(&store, TargetStyle::Milvus);

        let names: Vec<_> = cases.iter().map(|c| &c.name).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "Should have no duplicate case names");
    }
}