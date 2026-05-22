use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetamorphicTestCase {
    pub name: String,
    pub metamorphic_pattern: MetamorphicPattern,
    pub endpoint: String,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetamorphicPattern {
    NprobeMonotonicity,
    EfSearchMonotonicity,
    QueryConsistency,
    InsertMonotonicity,
    LimitMonotonicity,
    FlatL2Ordering,
    FlatCosineOrdering,
}

pub struct MetamorphicTestGenerator;

impl MetamorphicTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<MetamorphicTestCase> {
        let mut cases = Vec::new();

        let has_search = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/search")
        });
        let has_insert = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("entities/insert")
        });
        let has_ivf = store.type_constraints.iter().any(|atc| {
            atc.constraint.param_name == "indexType"
                && atc.constraint.violation_examples.iter().any(|v| v.contains("IVF"))
        });
        let has_hnsw = store.type_constraints.iter().any(|atc| {
            atc.constraint.param_name == "indexType"
                && atc.constraint.violation_examples.iter().any(|v| v.contains("HNSW"))
        });

        if has_search {
            cases.push(Self::generate_query_consistency(style));
            cases.push(Self::generate_limit_monotonicity(style));
        }

        if has_search && has_insert {
            cases.push(Self::generate_insert_monotonicity(style));
        }

        if has_ivf && has_search {
            cases.push(Self::generate_nprobe_monotonicity(style));
        }

        if has_hnsw && has_search {
            cases.push(Self::generate_ef_search_monotonicity(style));
        }

        cases.push(Self::generate_flat_l2_ordering(style));
        cases.push(Self::generate_flat_cosine_ordering(style));

        for abc in &store.behavioral_contracts {
            let name = abc.contract.name.to_lowercase();
            let outcome = abc.contract.expected_outcome.to_lowercase();
            let combined = format!("{} {}", name, outcome);
            if combined.contains("monotonic") || combined.contains("ordering") || combined.contains("sorted") {
                if combined.contains("l2") || combined.contains("euclidean") {
                    cases.push(Self::generate_flat_l2_ordering(style));
                }
                if combined.contains("cosine") {
                    cases.push(Self::generate_flat_cosine_ordering(style));
                }
            }
        }

        cases.dedup_by(|a, b| a.name == b.name);

        cases
    }

    fn generate_nprobe_monotonicity(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_nprobe_monotonicity".to_string(),
            metamorphic_pattern: MetamorphicPattern::NprobeMonotonicity,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::NprobeMonotonicity),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_ef_search_monotonicity(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_ef_search_monotonicity".to_string(),
            metamorphic_pattern: MetamorphicPattern::EfSearchMonotonicity,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::EfSearchMonotonicity),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_query_consistency(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_query_consistency".to_string(),
            metamorphic_pattern: MetamorphicPattern::QueryConsistency,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::QueryConsistency),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_insert_monotonicity(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_insert_monotonicity".to_string(),
            metamorphic_pattern: MetamorphicPattern::InsertMonotonicity,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::InsertMonotonicity),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_limit_monotonicity(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_limit_monotonicity".to_string(),
            metamorphic_pattern: MetamorphicPattern::LimitMonotonicity,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::LimitMonotonicity),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_flat_l2_ordering(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_flat_l2_distance_ordering".to_string(),
            metamorphic_pattern: MetamorphicPattern::FlatL2Ordering,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::FlatL2Ordering),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }

    fn generate_flat_cosine_ordering(style: TargetStyle) -> MetamorphicTestCase {
        MetamorphicTestCase {
            name: "metamorphic_flat_cosine_distance_ordering".to_string(),
            metamorphic_pattern: MetamorphicPattern::FlatCosineOrdering,
            endpoint: "/v2/vectordb/entities/search".to_string(),
            script: build_metamorphic_script(style, MetamorphicScriptKind::FlatCosineOrdering),
            defect_marker: "METAMORPHIC_VIOLATION".to_string(),
        }
    }
}

enum MetamorphicScriptKind {
    NprobeMonotonicity,
    EfSearchMonotonicity,
    QueryConsistency,
    InsertMonotonicity,
    LimitMonotonicity,
    FlatL2Ordering,
    FlatCosineOrdering,
}

fn build_metamorphic_script(style: TargetStyle, kind: MetamorphicScriptKind) -> String {
    match style {
        TargetStyle::Milvus => build_milvus_metamorphic_script(kind),
        TargetStyle::Qdrant | TargetStyle::Weaviate => build_qdrant_metamorphic_script(kind),
        TargetStyle::PgVector => String::new(),
    }
}

fn build_milvus_metamorphic_script(kind: MetamorphicScriptKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'meta_' + uuid.uuid4().hex[:8]
"#;

    let create_cosine = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)"#;

    // Perturb vectors to break collinearity and avoid tie-breaking false positives (all Cosine=1.0).
    let insert_20 = r#"data = [{"id":i,"vector":[0.1*i+0.001*(i%3),0.2*i+0.001*((i+1)%3),0.3*i+0.001*((i+2)%3),0.4*i]} for i in range(20)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)"#;

    let load = r#"r = requests.post(f'{BASE}/v2/vectordb/collections/load', headers=HEADERS, json={"collectionName":c})
if r.json().get('code') != 0: print(f'load failed: {r.text}'); sys.exit(0)
time.sleep(2)"#;

    // Perturbed data for ordering tests — breaks collinearity to avoid tie-breaking false positives.
    let insert_perturbed_10_milvus = r#"data = [{"id":i,"vector":[0.1*i+0.001*(i%3),0.2*i+0.001*((i+1)%3),0.3*i+0.001*((i+2)%3),0.4*i]} for i in range(1,11)]
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":data})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)"#;

    match kind {
        MetamorphicScriptKind::NprobeMonotonicity => format!(
            "{setup}\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"L2\",\"indexType\":\"IVF_FLAT\",\"params\":{{\"nlist\":128}}}}]}})\n\
             if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             data = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(50)]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\n\
             if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10,\"searchParams\":{{\"nprobe\":1}}}})\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10,\"searchParams\":{{\"nprobe\":128}}}})\n\
             if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)\n\
             top1_a = r1.json().get('data',[{{}}])[0].get('id') if r1.json().get('data') else None\n\
             top1_b = r2.json().get('data',[{{}}])[0].get('id') if r2.json().get('data') else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] nprobe top-1 mismatch: nprobe=1 id={{top1_a}} vs nprobe=128 id={{top1_b}}'); sys.exit(1)\n\
             else: print(f'nprobe monotonicity verified: top-1 id={{top1_a}} consistent'); sys.exit(0)",
            setup=setup, load=load,
        ),

        MetamorphicScriptKind::EfSearchMonotonicity => format!(
            "{setup}\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"L2\",\"indexType\":\"HNSW\",\"params\":{{\"M\":16,\"efConstruction\":256}}}}]}})\n\
             if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             data = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(50)]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data}})\n\
             if r.json().get('code') != 0: print(f'insert failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10,\"searchParams\":{{\"ef\":8}}}})\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10,\"searchParams\":{{\"ef\":256}}}})\n\
             if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)\n\
             top1_a = r1.json().get('data',[{{}}])[0].get('id') if r1.json().get('data') else None\n\
             top1_b = r2.json().get('data',[{{}}])[0].get('id') if r2.json().get('data') else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] ef top-1 mismatch: ef=8 id={{top1_a}} vs ef=256 id={{top1_b}}'); sys.exit(1)\n\
             else: print(f'ef_search monotonicity verified: top-1 id={{top1_a}} consistent'); sys.exit(0)",
            setup=setup, load=load,
        ),

        MetamorphicScriptKind::QueryConsistency => format!(
            "{setup}{create_cosine}\n{insert_20}\n{load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":5}})\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":5}})\n\
             if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)\n\
             res1 = [(d.get('id'),d.get('distance')) for d in r1.json().get('data',[])]\n\
             res2 = [(d.get('id'),d.get('distance')) for d in r2.json().get('data',[])]\n\
             if res1 != res2: print(f'[DEFECT: METAMORPHIC_VIOLATION] query consistency failed: {{res1}} vs {{res2}}'); sys.exit(1)\n\
             else: print(f'query consistency verified: {{res1}} == {{res2}}'); sys.exit(0)",
            setup=setup, create_cosine=create_cosine, insert_20=insert_20, load=load,
        ),

        MetamorphicScriptKind::InsertMonotonicity => format!(
            "{setup}{create_cosine}\n\
             data1 = [{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(10)]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data1}})\n\
             if r.json().get('code') != 0: print(f'insert1 failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":5}})\n\
             if r1.json().get('code') != 0: print(f'search1 failed: {{r1.text}}'); sys.exit(0)\n\
             top1_id = r1.json().get('data',[{{}}])[0].get('id') if r1.json().get('data') else None\n\
             data2 = [{{\"id\":i+10,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(40)]\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/insert', headers=HEADERS, json={{\"collectionName\":c,\"data\":data2}})\n\
             if r.json().get('code') != 0: print(f'insert2 failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(2)\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":50}})\n\
             if r2.json().get('code') != 0: print(f'search2 failed: {{r2.text}}'); sys.exit(0)\n\
             all_ids_2 = set(d.get('id') for d in r2.json().get('data',[]))\n\
             if top1_id not in all_ids_2: print(f'[DEFECT: METAMORPHIC_VIOLATION] insert monotonicity: top-1 id={{top1_id}} not in results after more inserts'); sys.exit(1)\n\
             else: print(f'insert monotonicity verified: top-1 id={{top1_id}} still in results'); sys.exit(0)",
            setup=setup, create_cosine=create_cosine, load=load,
        ),

        MetamorphicScriptKind::LimitMonotonicity => format!(
            "{setup}{create_cosine}\n{insert_20}\n{load}\n\
             r1 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":3}})\n\
             r2 = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10}})\n\
             if r1.json().get('code') != 0 or r2.json().get('code') != 0: print('search failed'); sys.exit(0)\n\
             ids1 = [d.get('id') for d in r1.json().get('data',[])]\n\
             ids2 = [d.get('id') for d in r2.json().get('data',[])]\n\
             top1_a = ids1[0] if ids1 else None\n\
             top1_b = ids2[0] if ids2 else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] limit top-1 mismatch: limit=3 id={{top1_a}} vs limit=10 id={{top1_b}}'); sys.exit(1)\n\
             else: print(f'limit monotonicity verified: top-1 id={{top1_a}} consistent'); sys.exit(0)",
            setup=setup, create_cosine=create_cosine, insert_20=insert_20, load=load,
        ),

        MetamorphicScriptKind::FlatL2Ordering => format!(
            "{setup}\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"L2\",\"indexType\":\"FLAT\"}}]}})\n\
             if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {insert_perturbed_10_milvus}\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10}})\n\
             if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)\n\
             dists = [d.get('distance') for d in r.json().get('data',[])]\n\
             if not dists: print('no results'); sys.exit(0)\n\
             if any(dists[i] > dists[i+1] for i in range(len(dists)-1)): print(f'[DEFECT: METAMORPHIC_VIOLATION] L2 not ascending: dists={{dists}}'); sys.exit(1)\n\
             print(f'FLAT L2 ordering verified: dists={{dists}}'); sys.exit(0)",
            setup=setup, load=load, insert_perturbed_10_milvus=insert_perturbed_10_milvus,
        ),

        MetamorphicScriptKind::FlatCosineOrdering => format!(
            "{setup}\
             r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{\"collectionName\":c,\"schema\":{{\"autoID\":False,\"enableDynamicField\":True,\"fields\":[{{\"fieldName\":\"id\",\"dataType\":\"Int64\",\"isPrimary\":True}},{{\"fieldName\":\"vector\",\"dataType\":\"FloatVector\",\"elementTypeParams\":{{\"dim\":4}}}}]}},\"indexParams\":[{{\"fieldName\":\"vector\",\"metricType\":\"COSINE\",\"indexType\":\"FLAT\"}}]}})\n\
             if r.json().get('code') != 0: print(f'setup failed: {{r.text}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {insert_perturbed_10_milvus}\n\
             {load}\n\
             r = requests.post(f'{{BASE}}/v2/vectordb/entities/search', headers=HEADERS, json={{\"collectionName\":c,\"data\":[[0.1,0.2,0.3,0.4]],\"limit\":10}})\n\
             if r.json().get('code') != 0: print(f'search failed: {{r.text}}'); sys.exit(0)\n\
             dists = [d.get('distance') for d in r.json().get('data',[])]\n\
             if not dists: print('no results'); sys.exit(0)\n\
             if any(dists[i] < dists[i+1] for i in range(len(dists)-1)): print(f'[DEFECT: METAMORPHIC_VIOLATION] COSINE not descending: dists={{dists}}'); sys.exit(1)\n\
             print(f'FLAT COSINE ordering verified: dists={{dists}}'); sys.exit(0)",
            setup=setup, load=load, insert_perturbed_10_milvus=insert_perturbed_10_milvus,
        ),
    }
}

fn build_qdrant_metamorphic_script(kind: MetamorphicScriptKind) -> String {
    let setup = r#"import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
c = 'meta_' + uuid.uuid4().hex[:8]
"#;

    let create = r#"r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(1)"#;

    // Perturb vectors to break collinearity — avoids tie-breaking false positives (Cosine=1.0 for all).
    let insert_perturbed = r#"r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":i,"vector":[0.1*i+0.001*(i%3),0.2*i+0.001*((i+1)%3),0.3*i+0.001*((i+2)%3),0.4*i]} for i in range(20)]})"#;
    let insert_perturbed_50 = r#"r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":i,"vector":[0.1*i+0.001*(i%3),0.2*i+0.001*((i+1)%3),0.3*i+0.001*((i+2)%3),0.4*i]} for i in range(50)]})"#;
    let insert_perturbed_10 = r#"r = requests.put(f'{BASE}/collections/{c}/points', json={"points":[{"id":i,"vector":[0.1*i+0.001*(i%3),0.2*i+0.001*((i+1)%3),0.3*i+0.001*((i+2)%3),0.4*i]} for i in range(1,11)]})"#;

    match kind {
        MetamorphicScriptKind::QueryConsistency => format!(
            "{setup}{create}\n\
             {insert_perturbed}\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5}})\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5}})\n\
             res1 = [(p.get('id'),p.get('score')) for p in r1.json().get('result',[])]\n\
             res2 = [(p.get('id'),p.get('score')) for p in r2.json().get('result',[])]\n\
             if res1 != res2: print(f'[DEFECT: METAMORPHIC_VIOLATION] query consistency: {{res1}} vs {{res2}}'); sys.exit(1)\n\
             else: print(f'query consistency verified'); sys.exit(0)",
            setup=setup, create=create, insert_perturbed=insert_perturbed,
        ),

        MetamorphicScriptKind::LimitMonotonicity => format!(
            "{setup}{create}\n\
             {insert_perturbed}\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":3}})\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10}})\n\
             top1_a = r1.json().get('result',[{{}}])[0].get('id') if r1.json().get('result') else None\n\
             top1_b = r2.json().get('result',[{{}}])[0].get('id') if r2.json().get('result') else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] limit top-1 mismatch: {{top1_a}} vs {{top1_b}}'); sys.exit(1)\n\
             else: print(f'limit monotonicity verified'); sys.exit(0)",
            setup=setup, create=create, insert_perturbed=insert_perturbed,
        ),

        MetamorphicScriptKind::NprobeMonotonicity => format!(
            "{setup}{create}\n\
             {insert_perturbed_50}\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10,\"params\":{{\"exact\":true}}}})\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10,\"params\":{{\"exact\":false}}}})\n\
             top1_a = r1.json().get('result',[{{}}])[0].get('id') if r1.json().get('result') else None\n\
             top1_b = r2.json().get('result',[{{}}])[0].get('id') if r2.json().get('result') else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] exact vs approx top-1 mismatch: exact id={{top1_a}} vs approx id={{top1_b}}'); sys.exit(1)\n\
             else: print(f'nprobe monotonicity verified: top-1 id={{top1_a}} consistent'); sys.exit(0)",
            setup=setup, create=create, insert_perturbed_50=insert_perturbed_50,
        ),

        MetamorphicScriptKind::EfSearchMonotonicity => format!(
            "{setup}{create}\n\
             {insert_perturbed_50}\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10,\"params\":{{\"hnsw_ef\":8}}}})\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10,\"params\":{{\"hnsw_ef\":256}}}})\n\
             top1_a = r1.json().get('result',[{{}}])[0].get('id') if r1.json().get('result') else None\n\
             top1_b = r2.json().get('result',[{{}}])[0].get('id') if r2.json().get('result') else None\n\
             if top1_a != top1_b: print(f'[DEFECT: METAMORPHIC_VIOLATION] hnsw_ef top-1 mismatch: ef=8 id={{top1_a}} vs ef=256 id={{top1_b}}'); sys.exit(1)\n\
             else: print(f'ef_search monotonicity verified: top-1 id={{top1_a}} consistent'); sys.exit(0)",
            setup=setup, create=create, insert_perturbed_50=insert_perturbed_50,
        ),

        MetamorphicScriptKind::InsertMonotonicity => format!(
            "{setup}{create}\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":i,\"vector\":[0.1*i,0.2*i,0.3*i,0.4*i]}} for i in range(10)]}})\n\
             time.sleep(1)\n\
             r1 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":5}})\n\
             top1_id = r1.json().get('result',[{{}}])[0].get('id') if r1.json().get('result') else None\n\
             r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{\"points\":[{{\"id\":i+10,\"vector\":[0.5*i,0.6*i,0.7*i,0.8*i]}} for i in range(40)]}})\n\
             time.sleep(2)\n\
             r2 = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":50}})\n\
             all_ids_2 = set(p.get('id') for p in r2.json().get('result',[]))\n\
             if top1_id not in all_ids_2: print(f'[DEFECT: METAMORPHIC_VIOLATION] insert monotonicity: top-1 id={{top1_id}} not in results after more inserts'); sys.exit(1)\n\
             else: print(f'insert monotonicity verified: top-1 id={{top1_id}} still in results'); sys.exit(0)",
            setup=setup, create=create,
        ),

        MetamorphicScriptKind::FlatL2Ordering => format!(
            "{setup}\
             r = requests.put(f'{{BASE}}/collections/{{c}}', json={{\"vectors\":{{\"size\":4,\"distance\":\"Euclid\"}}}})\n\
             if r.status_code not in (200, 201): print(f'setup failed: {{r.status_code}}'); sys.exit(0)\n\
             time.sleep(1)\n\
             {insert_perturbed_10}\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10}})\n\
             scores = [p.get('score') for p in r.json().get('result',[])]\n\
             if not scores: print('no results'); sys.exit(0)\n\
             if any(scores[i] < scores[i+1] for i in range(len(scores)-1)): print(f'[DEFECT: METAMORPHIC_VIOLATION] L2 not descending (score): scores={{scores}}'); sys.exit(1)\n\
             print(f'FLAT L2 ordering verified: scores={{scores}}'); sys.exit(0)",
            setup=setup, insert_perturbed_10=insert_perturbed_10,
        ),

        MetamorphicScriptKind::FlatCosineOrdering => format!(
            "{setup}{create}\n\
             {insert_perturbed_10}\n\
             time.sleep(1)\n\
             r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{\"vector\":[0.1,0.2,0.3,0.4],\"limit\":10}})\n\
             scores = [p.get('score') for p in r.json().get('result',[])]\n\
             if not scores: print('no results'); sys.exit(0)\n\
             if any(scores[i] < scores[i+1] for i in range(len(scores)-1)): print(f'[DEFECT: METAMORPHIC_VIOLATION] COSINE not descending: scores={{scores}}'); sys.exit(1)\n\
             print(f'FLAT COSINE ordering verified: scores={{scores}}'); sys.exit(0)",
            setup=setup, create=create, insert_perturbed_10=insert_perturbed_10,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::TypeConstraint;
    use crate::contract::store::{AnnotatedTypeConstraint, Confidence, ConstraintSource};

    fn make_test_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "2.4");

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/search".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "data".to_string(),
                expected_type: "array".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/entities/insert".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store.type_constraints.push(AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "indexType".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec!["IVF_FLAT".to_string(), "HNSW".to_string()],
            },
            endpoint: "/v2/vectordb/indexes/create".to_string(),
            source: ConstraintSource::OpenapiDerived,
            confidence: Confidence::High,
        });

        store
    }

    #[test]
    fn test_from_store_generates_metamorphic_cases() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        assert!(cases.len() >= 5, "Should generate at least 5 metamorphic test cases, got {}", cases.len());

        let names: Vec<_> = cases.iter().map(|c| c.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("nprobe")), "Missing nprobe monotonicity");
        assert!(names.iter().any(|n| n.contains("ef_search")), "Missing ef_search monotonicity");
        assert!(names.iter().any(|n| n.contains("query_consistency")), "Missing query consistency");
        assert!(names.iter().any(|n| n.contains("insert_monotonicity")), "Missing insert monotonicity");
        assert!(names.iter().any(|n| n.contains("limit_monotonicity")), "Missing limit monotonicity");
    }

    #[test]
    fn test_flat_ordering_always_generated() {
        let store = ContractStore::new("milvus", "2.4");
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        let l2: Vec<_> = cases.iter().filter(|c| c.name.contains("l2")).collect();
        let cosine: Vec<_> = cases.iter().filter(|c| c.name.contains("cosine")).collect();
        assert!(!l2.is_empty(), "Should always generate FLAT L2 ordering");
        assert!(!cosine.is_empty(), "Should always generate FLAT COSINE ordering");
    }

    #[test]
    fn test_milvus_scripts_contain_auth() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        for case in &cases {
            assert!(case.script.contains("Bearer root:Milvus"), "Milvus script missing auth: {}", case.name);
        }
    }

    #[test]
    fn test_scripts_contain_defect_markers() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        for case in &cases {
            assert!(case.script.contains("[DEFECT:"), "Script missing DEFECT marker: {}", case.name);
        }
    }

    #[test]
    fn test_l2_ordering_checks_ascending() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        let l2_case = cases.iter().find(|c| c.name.contains("l2")).unwrap();
        assert!(l2_case.script.contains("ascending"), "L2 should check ascending order");
    }

    #[test]
    fn test_cosine_ordering_checks_descending() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        let cosine_case = cases.iter().find(|c| c.name.contains("cosine")).unwrap();
        assert!(cosine_case.script.contains("descending"), "COSINE should check descending order");
    }

    #[test]
    fn test_no_duplicate_cases() {
        let store = make_test_store();
        let cases = MetamorphicTestGenerator::from_store(&store, TargetStyle::Milvus);

        let names: Vec<_> = cases.iter().map(|c| &c.name).collect();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(names.len(), unique.len(), "Should have no duplicate case names");
    }
}