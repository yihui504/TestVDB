use crate::contract::store::ContractStore;
use crate::target::TargetStyle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTestCase {
    pub name: String,
    pub resource_pattern: ResourcePattern,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePattern {
    LargeDimension,
    ZeroDimension,
    LongCollectionName,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComboTestCase {
    pub name: String,
    pub combo_pattern: ComboPattern,
    pub script: String,
    pub defect_marker: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComboPattern {
    DimMetricIndex,
}

pub struct ResourceTestGenerator;

impl ResourceTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<ResourceTestCase> {
        let mut cases = Vec::new();

        let has_create = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("collections/create")
        });
        if !has_create {
            return cases;
        }

        let dim_max = store.range_constraints.iter().find_map(|arc| {
            if arc.constraint.param_name == "dim" { arc.constraint.max } else { None }
        });
        let large_dim = dim_max.map_or(32768, |m| (m as usize) * 2);

        cases.push(Self::generate_large_dimension(large_dim, style));

        let dim_min = store.range_constraints.iter().find_map(|arc| {
            if arc.constraint.param_name == "dim" { arc.constraint.min } else { None }
        });
        if dim_min.map_or(true, |m| m >= 1.0) {
            cases.push(Self::generate_zero_dimension(style));
        }

        let has_collection_name = store.type_constraints.iter().any(|atc| {
            atc.constraint.param_name == "collectionName"
        });
        if has_collection_name {
            cases.push(Self::generate_long_collection_name(style));
        }

        cases.dedup_by(|a, b| a.name == b.name);
        cases
    }

    fn generate_large_dimension(large_dim: usize, style: TargetStyle) -> ResourceTestCase {
        let script = match style {
            TargetStyle::Milvus => format!(r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.post(f'{{BASE}}/v2/vectordb/collections/create', headers=HEADERS, json={{"collectionName":c,"schema":{{"autoID":False,"enableDynamicField":True,"fields":[{{"fieldName":"id","dataType":"Int64","isPrimary":True}},{{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{{"dim":{large_dim}}}}}]}},"indexParams":[{{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}}]}})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] {large_dim}-dim collection created (may cause OOM)'); sys.exit(1)
else: print(f'properly rejected {large_dim}-dim: {{r.json()}}'); sys.exit(0)"#),
            TargetStyle::Qdrant => format!(r#"import requests, sys, uuid
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":{large_dim},"distance":"Cosine"}}}})
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] {large_dim}-dim collection created (may cause OOM)'); sys.exit(1)
else: print(f'properly rejected {large_dim}-dim: {{r.status_code}}'); sys.exit(0)"#),
            TargetStyle::Weaviate | TargetStyle::PgVector => String::new(),
        };
        ResourceTestCase {
            name: format!("resource_large_dimension_{}", large_dim),
            resource_pattern: ResourcePattern::LargeDimension,
            script,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_zero_dimension(style: TargetStyle) -> ResourceTestCase {
        let script = match style {
            TargetStyle::Milvus => r#"import requests, sys, uuid
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":0}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] 0-dim collection created'); sys.exit(1)
else: print(f'properly rejected 0-dim: {r.json()}'); sys.exit(0)"#.to_string(),
            TargetStyle::Qdrant => r#"import requests, sys, uuid
BASE = '{TESTVDB_DB_URL}'
c = 'oracle_res_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":0,"distance":"Cosine"}})
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] 0-dim collection created'); sys.exit(1)
else: print(f'properly rejected 0-dim: {r.status_code}'); sys.exit(0)"#.to_string(),
            TargetStyle::Weaviate | TargetStyle::PgVector => String::new(),
        };
        ResourceTestCase {
            name: "resource_zero_dimension".to_string(),
            resource_pattern: ResourcePattern::ZeroDimension,
            script,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }

    fn generate_long_collection_name(style: TargetStyle) -> ResourceTestCase {
        let script = match style {
            TargetStyle::Milvus => r#"import requests, sys, uuid
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'a' * 256
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers=HEADERS, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] 256-char collection name accepted'); sys.exit(1)
else: print(f'properly rejected 256-char name: {r.json()}'); sys.exit(0)"#.to_string(),
            TargetStyle::Qdrant => r#"import requests, sys
BASE = '{TESTVDB_DB_URL}'
c = 'a' * 256
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS] 256-char collection name accepted'); sys.exit(1)
else: print(f'properly rejected 256-char name: {r.status_code}'); sys.exit(0)"#.to_string(),
            TargetStyle::Weaviate | TargetStyle::PgVector => String::new(),
        };
        ResourceTestCase {
            name: "resource_long_collection_name".to_string(),
            resource_pattern: ResourcePattern::LongCollectionName,
            script,
            defect_marker: "ILLEGAL_SUCCESS".to_string(),
        }
    }
}

pub struct ComboTestGenerator;

impl ComboTestGenerator {
    pub fn from_store(store: &ContractStore, style: TargetStyle) -> Vec<ComboTestCase> {
        let mut cases = Vec::new();

        let has_create = store.type_constraints.iter().any(|atc| {
            atc.endpoint.contains("collections/create") || atc.endpoint.contains("collections")
        });
        if !has_create {
            return cases;
        }

        let (default_metrics, default_indexes) = match style {
            TargetStyle::Qdrant => (
                vec!["Cosine".to_string(), "Euclid".to_string(), "Dot".to_string()],
                vec!["HNSW".to_string()],
            ),
            TargetStyle::Milvus => (
                vec!["L2".to_string(), "COSINE".to_string(), "IP".to_string()],
                vec!["IVF_FLAT".to_string(), "HNSW".to_string(), "FLAT".to_string(), "AUTOINDEX".to_string()],
            ),
            TargetStyle::Weaviate => (
                vec!["cosine".to_string(), "dot".to_string(), "l2".to_string()],
                vec!["hnsw".to_string()],
            ),
            TargetStyle::PgVector => (
                vec!["vector_l2_ops".to_string(), "vector_cosine_ops".to_string(), "vector_ip_ops".to_string()],
                vec!["hnsw".to_string(), "ivfflat".to_string()],
            ),
        };

        let metrics = store.enum_values.get("metricType").cloned().unwrap_or(default_metrics);
        let index_types = store.enum_values.get("indexType").cloned().unwrap_or(default_indexes);

        let dims = Self::extract_dims(store);

        for dim in &dims {
            for metric in &metrics {
                for idx in &index_types {
                    if let Some(tc) = Self::generate_combo(*dim, metric, idx, style) {
                        cases.push(tc);
                    }
                }
            }
        }

        cases.dedup_by(|a, b| a.name == b.name);
        cases
    }

    fn extract_dims(store: &ContractStore) -> Vec<usize> {
        let dim_range = store.range_constraints.iter().find(|arc| {
            arc.constraint.param_name == "dim"
        });
        match dim_range {
            Some(arc) => {
                let min = arc.constraint.min.unwrap_or(1.0) as usize;
                let max = arc.constraint.max.unwrap_or(32768.0) as usize;
                let mut dims = vec![min.max(4)];
                if max > 4 && max != min.max(4) {
                    dims.push(max.min(128));
                }
                if !dims.contains(&8) && max >= 8 {
                    dims.push(8);
                }
                dims.sort();
                dims.dedup();
                dims
            }
            None => vec![4, 8, 128],
        }
    }

    fn generate_combo(dim: usize, metric: &str, idx: &str, style: TargetStyle) -> Option<ComboTestCase> {
        let name = format!("combo_{}_{}_{}", dim, metric, idx.to_lowercase());
        let script = match style {
            TargetStyle::Milvus => format!(r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
HEADERS = {{'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}}
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
else: print(f'param combo verified: dim={dim} metric={metric} index={idx}'); sys.exit(0)"#),
            TargetStyle::Qdrant => format!(r#"import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'oracle_combo_' + uuid.uuid4().hex[:8]
r = requests.put(f'{{BASE}}/collections/{{c}}', json={{"vectors":{{"size":{dim},"distance":"{metric}"}}}})
if r.status_code != 200: print(f'create failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
vec_data = [0.01] * {dim}
r = requests.put(f'{{BASE}}/collections/{{c}}/points', json={{"points":[{{"id":1,"vector":vec_data,"payload":{{}}}}]}})
if r.status_code != 200: print(f'upsert failed: {{r.text}}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{{BASE}}/collections/{{c}}/points/search', json={{"vector":vec_data,"limit":3}})
if r.status_code != 200: print(f'[DEFECT: SEQUENCE_VIOLATION] search failed for dim={dim} distance={metric}: {{r.text}}'); sys.exit(1)
else: print(f'param combo verified: dim={dim} distance={metric}'); sys.exit(0)"#),
            TargetStyle::Weaviate | TargetStyle::PgVector => String::new(),
        };
        Some(ComboTestCase {
            name,
            combo_pattern: ComboPattern::DimMetricIndex,
            script,
            defect_marker: "SEQUENCE_VIOLATION".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{
        RangeConstraint, TypeConstraint,
    };

    fn make_milvus_store() -> ContractStore {
        let mut store = ContractStore::new("milvus", "v2.4");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "collectionName".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/collections/create".to_string(),
            source: crate::contract::store::ConstraintSource::ExplicitDoc,
            confidence: crate::contract::store::Confidence::High,
        });
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "dim".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/collections/create".to_string(),
            source: crate::contract::store::ConstraintSource::ExplicitDoc,
            confidence: crate::contract::store::Confidence::High,
        });
        store.range_constraints.push(crate::contract::store::AnnotatedRangeConstraint {
            constraint: RangeConstraint {
                param_name: "dim".to_string(),
                description: "dim must be >= 1".to_string(),
                min: Some(1.0),
                max: Some(32768.0),
                violation_examples: vec![],
            },
            endpoint: "/v2/vectordb/collections/create".to_string(),
            source: crate::contract::store::ConstraintSource::ExplicitDoc,
            confidence: crate::contract::store::Confidence::High,
        });
        store.set_enum_values("metricType", vec!["L2".to_string(), "COSINE".to_string(), "IP".to_string()]);
        store.set_enum_values("indexType", vec!["IVF_FLAT".to_string(), "HNSW".to_string(), "FLAT".to_string(), "AUTOINDEX".to_string()]);
        store
    }

    #[test]
    fn test_resource_generator_milvus() {
        let store = make_milvus_store();
        let cases = ResourceTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.len() >= 3, "Should have at least 3 resource tests, got {}", cases.len());

        let names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("large_dimension")));
        assert!(names.iter().any(|n| n.contains("zero_dimension")));
        assert!(names.iter().any(|n| n.contains("long_collection_name")));

        for case in &cases {
            assert!(case.script.contains("[DEFECT:"));
            assert!(case.script.contains("sys.exit"));
        }
    }

    #[test]
    fn test_resource_generator_no_create() {
        let store = ContractStore::new("milvus", "v2.4");
        let cases = ResourceTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.is_empty(), "Should have no resource tests without create endpoint");
    }

    #[test]
    fn test_resource_generator_qdrant() {
        let mut store = ContractStore::new("qdrant", "v1.7");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "collectionName".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: "/collections/create".to_string(),
            source: crate::contract::store::ConstraintSource::ExplicitDoc,
            confidence: crate::contract::store::Confidence::High,
        });
        let cases = ResourceTestGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(cases.len() >= 2, "Should have at least 2 resource tests for Qdrant");
    }

    #[test]
    fn test_combo_generator_milvus() {
        let store = make_milvus_store();
        let cases = ComboTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(!cases.is_empty(), "Should have combo tests");

        for case in &cases {
            assert!(case.script.contains("[DEFECT:"));
            assert!(case.script.contains("sys.exit"));
            assert!(case.name.starts_with("combo_"));
        }
    }

    #[test]
    fn test_combo_generator_no_create() {
        let store = ContractStore::new("milvus", "v2.4");
        let cases = ComboTestGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(cases.is_empty(), "Should have no combo tests without create endpoint");
    }

    #[test]
    fn test_combo_dims_from_range() {
        let store = make_milvus_store();
        let dims = ComboTestGenerator::extract_dims(&store);
        assert!(!dims.is_empty());
        assert!(dims.contains(&4), "Should contain dim=4");
    }

    #[test]
    fn test_combo_generator_qdrant() {
        let mut store = ContractStore::new("qdrant", "v1.7");
        store.type_constraints.push(crate::contract::store::AnnotatedTypeConstraint {
            constraint: TypeConstraint {
                param_name: "name".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            },
            endpoint: "collections".to_string(),
            source: crate::contract::store::ConstraintSource::ExplicitDoc,
            confidence: crate::contract::store::Confidence::High,
        });
        let cases = ComboTestGenerator::from_store(&store, TargetStyle::Qdrant);
        assert!(!cases.is_empty(), "Should have combo tests for Qdrant");
        for case in &cases {
            assert!(case.script.contains("[DEFECT: SEQUENCE_VIOLATION]"));
            assert!(case.script.contains("qdrant") || case.script.contains("requests"));
        }
    }
}
