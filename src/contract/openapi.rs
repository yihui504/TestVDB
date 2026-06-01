use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

use super::schema::{EndpointEntry, RangeConstraint, RejectionPolicy, TypeConstraint};
use super::store::{
    AnnotatedRangeConstraint, AnnotatedTypeConstraint, Confidence, ConstraintSource, ContractStore,
};

#[derive(Debug, Deserialize)]
struct OpenApiSpec {
    paths: HashMap<String, OpenApiPathItem>,
    components: Option<OpenApiComponents>,
}

#[derive(Debug, Deserialize)]
struct OpenApiPathItem {
    #[serde(default)]
    post: Option<OpenApiOperation>,
    #[serde(default)]
    put: Option<OpenApiOperation>,
    #[serde(default)]
    delete: Option<OpenApiOperation>,
    #[serde(default)]
    get: Option<OpenApiOperation>,
    #[serde(default)]
    patch: Option<OpenApiOperation>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenApiOperation {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "operationId")]
    operation_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    parameters: Vec<OpenApiParameter>,
    #[serde(default, rename = "requestBody")]
    request_body: Option<OpenApiRequestBody>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OpenApiParameter {
    name: String,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    schema: Option<OpenApiSchema>,
}

#[derive(Debug, Deserialize)]
struct OpenApiRequestBody {
    #[serde(default)]
    content: HashMap<String, OpenApiMediaType>,
}

#[derive(Debug, Deserialize)]
struct OpenApiMediaType {
    #[serde(default)]
    schema: Option<OpenApiSchema>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct OpenApiSchema {
    #[serde(rename = "type", default)]
    schema_type: Option<String>,
    #[serde(default)]
    properties: HashMap<String, OpenApiSchema>,
    #[serde(default)]
    minimum: Option<f64>,
    #[serde(default)]
    maximum: Option<f64>,
    #[serde(default)]
    min_length: Option<u64>,
    #[serde(default)]
    max_length: Option<u64>,
    #[serde(rename = "enum", default)]
    enum_values: Vec<serde_json::Value>,
    #[serde(default)]
    items: Option<Box<OpenApiSchema>>,
    #[serde(rename = "$ref", default)]
    ref_path: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: Vec<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    default: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenApiComponents {
    #[serde(default)]
    schemas: HashMap<String, OpenApiSchema>,
}

pub struct OpenApiParser {
    spec: OpenApiSpec,
}

impl OpenApiParser {
    pub fn from_json(json_str: &str) -> Result<Self> {
        let spec: OpenApiSpec = serde_json::from_str(json_str)
            .context("Failed to parse OpenAPI JSON spec")?;
        Ok(Self { spec })
    }

    pub fn extract_endpoints(&self) -> Vec<EndpointEntry> {
        let mut endpoints = Vec::new();
        for (path, item) in &self.spec.paths {
            for (method, op) in [
                ("POST", &item.post),
                ("PUT", &item.put),
                ("DELETE", &item.delete),
                ("GET", &item.get),
                ("PATCH", &item.patch),
            ] {
                if let Some(op) = op {
                    let name = op.operation_id.clone().unwrap_or_else(|| path.clone());
                    let category = op.tags.first().cloned().unwrap_or_else(|| "general".to_string());
                    endpoints.push(EndpointEntry {
                        name,
                        api_path: format!("{} {}", method, path),
                        docs_url: String::new(),
                        category,
                    });
                }
            }
        }
        endpoints
    }

    pub fn extract_all_type_constraints(&self) -> Vec<TypeConstraint> {
        let mut constraints = Vec::new();
        for (_path, item) in &self.spec.paths {
            for op in [&item.post, &item.put, &item.delete, &item.get, &item.patch].iter() {
                if let Some(op) = op {
                    if let Some(body) = &op.request_body {
                        for (_content_type, media) in &body.content {
                            if let Some(schema) = &media.schema {
                                self.extract_type_constraints_from_schema(schema, "", &mut constraints);
                            }
                        }
                    }
                }
            }
        }
        constraints
    }

    pub fn extract_all_range_constraints(&self) -> Vec<RangeConstraint> {
        let mut constraints = Vec::new();
        for (_path, item) in &self.spec.paths {
            for op in [&item.post, &item.put, &item.delete, &item.get, &item.patch].iter() {
                if let Some(op) = op {
                    if let Some(body) = &op.request_body {
                        for (_content_type, media) in &body.content {
                            if let Some(schema) = &media.schema {
                                self.extract_range_constraints_from_schema(schema, "", &mut constraints);
                            }
                        }
                    }
                    for param in &op.parameters {
                        if let Some(schema) = &param.schema {
                            self.extract_range_constraints_from_schema(schema, &param.name, &mut constraints);
                        }
                    }
                }
            }
        }
        constraints
    }

    fn extract_type_constraints_from_schema(
        &self,
        schema: &OpenApiSchema,
        prefix: &str,
        constraints: &mut Vec<TypeConstraint>,
    ) {
        let resolved = self.resolve_ref(schema);
        for (name, prop) in &resolved.properties {
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", prefix, name)
            };
            let prop_resolved = self.resolve_ref(prop);
            if let Some(t) = &prop_resolved.schema_type {
                constraints.push(TypeConstraint {
                    param_name: full_name.clone(),
                    expected_type: t.clone(),
                    violation_examples: vec![],
                });
            }
            if !prop_resolved.properties.is_empty() {
                self.extract_type_constraints_from_schema(prop, &full_name, constraints);
            }
        }
    }

    fn extract_range_constraints_from_schema(
        &self,
        schema: &OpenApiSchema,
        param_name: &str,
        constraints: &mut Vec<RangeConstraint>,
    ) {
        let resolved = self.resolve_ref(schema);

        if resolved.minimum.is_some() || resolved.maximum.is_some() {
            constraints.push(RangeConstraint {
                param_name: param_name.to_string(),
                description: resolved.description.clone().unwrap_or_default(),
                min: resolved.minimum,
                max: resolved.maximum,
                violation_examples: vec![],
            });
        }

        for (name, prop) in &resolved.properties {
            let full_name = if param_name.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", param_name, name)
            };
            let prop_resolved = self.resolve_ref(prop);
            self.extract_range_constraints_from_schema(prop_resolved, &full_name, constraints);
        }
    }

    fn resolve_ref<'a>(&'a self, schema: &'a OpenApiSchema) -> &'a OpenApiSchema {
        if let Some(ref_path) = &schema.ref_path {
            let ref_name = ref_path.split('/').last().unwrap_or("");
            if let Some(components) = &self.spec.components {
                if let Some(resolved) = components.schemas.get(ref_name) {
                    return resolved;
                }
            }
        }
        schema
    }

    pub fn extract_to_contract_store(&self, target: &str, version: &str) -> ContractStore {
        let mut store = ContractStore::new(target, version);

        for (path, item) in &self.spec.paths {
            for (method, op) in [
                ("POST", &item.post),
                ("PUT", &item.put),
                ("DELETE", &item.delete),
                ("GET", &item.get),
                ("PATCH", &item.patch),
            ] {
                if let Some(op) = op {
                    let endpoint_name = op.operation_id.clone().unwrap_or_else(|| path.clone());

                    if let Some(body) = &op.request_body {
                        for (_content_type, media) in &body.content {
                            if let Some(schema) = &media.schema {
                                self.extract_annotated_type_constraints(
                                    schema, &endpoint_name, "", &mut store.type_constraints,
                                );
                                self.extract_annotated_range_constraints(
                                    schema, &endpoint_name, "", &mut store.range_constraints,
                                );
                                self.extract_required_from_schema(
                                    schema, &endpoint_name, &mut store.required_params,
                                );
                                self.extract_enum_from_schema(
                                    schema, &endpoint_name, "", &mut store.enum_values,
                                );
                            }
                        }
                    }

                    for param in &op.parameters {
                        if let Some(schema) = &param.schema {
                            let resolved = self.resolve_ref(schema);
                            if resolved.minimum.is_some() || resolved.maximum.is_some() {
                                store.range_constraints.push(AnnotatedRangeConstraint {
                                    constraint: RangeConstraint {
                                        param_name: param.name.clone(),
                                        description: resolved.description.clone().unwrap_or_default(),
                                        min: resolved.minimum,
                                        max: resolved.maximum,
                                        violation_examples: vec![],
                                    },
                                    endpoint: Some(endpoint_name.clone()),
                                    source: ConstraintSource::OpenapiDerived,
                                    confidence: Confidence::High,
                                    rejection_policy: Some(RejectionPolicy::Reject),
                                });
                            }
                            if let Some(t) = &resolved.schema_type {
                                store.type_constraints.push(AnnotatedTypeConstraint {
                                    constraint: TypeConstraint {
                                        param_name: param.name.clone(),
                                        expected_type: t.clone(),
                                        violation_examples: vec![],
                                    },
                                    endpoint: Some(endpoint_name.clone()),
                                    source: ConstraintSource::OpenapiDerived,
                                    confidence: Confidence::High,
                                    rejection_policy: Some(RejectionPolicy::Reject),
                                });
                            }
                            if let Some(values) = &param.required {
                                if *values {
                                    store.required_params
                                        .entry(endpoint_name.clone())
                                        .or_default()
                                        .push(param.name.clone());
                                }
                            }
                        }
                    }

                    store.endpoints.push(EndpointEntry {
                        name: endpoint_name,
                        api_path: format!("{} {}", method, path),
                        docs_url: String::new(),
                        category: op.tags.first().cloned().unwrap_or_else(|| "general".to_string()),
                    });
                }
            }
        }

        store
    }

    fn extract_annotated_type_constraints(
        &self,
        schema: &OpenApiSchema,
        endpoint: &str,
        prefix: &str,
        constraints: &mut Vec<AnnotatedTypeConstraint>,
    ) {
        let resolved = self.resolve_ref(schema);
        for (name, prop) in &resolved.properties {
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", prefix, name)
            };
            let prop_resolved = self.resolve_ref(prop);
            if let Some(t) = &prop_resolved.schema_type {
                constraints.push(AnnotatedTypeConstraint {
                    constraint: TypeConstraint {
                        param_name: full_name.clone(),
                        expected_type: t.clone(),
                        violation_examples: vec![],
                    },
                    endpoint: Some(endpoint.to_string()),
                    source: ConstraintSource::OpenapiDerived,
                    confidence: Confidence::High,
                    rejection_policy: Some(RejectionPolicy::Reject),
                });
            }
            if !prop_resolved.properties.is_empty() {
                self.extract_annotated_type_constraints(prop, endpoint, &full_name, constraints);
            }
        }
    }

    fn extract_annotated_range_constraints(
        &self,
        schema: &OpenApiSchema,
        endpoint: &str,
        prefix: &str,
        constraints: &mut Vec<AnnotatedRangeConstraint>,
    ) {
        let resolved = self.resolve_ref(schema);

        if resolved.minimum.is_some() || resolved.maximum.is_some() {
            if !prefix.is_empty() {
                constraints.push(AnnotatedRangeConstraint {
                    constraint: RangeConstraint {
                        param_name: prefix.to_string(),
                        description: resolved.description.clone().unwrap_or_default(),
                        min: resolved.minimum,
                        max: resolved.maximum,
                        violation_examples: vec![],
                    },
                    endpoint: Some(endpoint.to_string()),
                    source: ConstraintSource::OpenapiDerived,
                    confidence: Confidence::High,
                    rejection_policy: Some(RejectionPolicy::Reject),
                });
            }
        }

        for (name, prop) in &resolved.properties {
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", prefix, name)
            };
            let prop_resolved = self.resolve_ref(prop);
            self.extract_annotated_range_constraints(prop_resolved, endpoint, &full_name, constraints);
        }
    }

    fn extract_required_from_schema(
        &self,
        schema: &OpenApiSchema,
        endpoint: &str,
        required_params: &mut HashMap<String, Vec<String>>,
    ) {
        let resolved = self.resolve_ref(schema);
        if !resolved.required.is_empty() {
            let entry = required_params.entry(endpoint.to_string()).or_default();
            for name in &resolved.required {
                if !entry.contains(name) {
                    entry.push(name.clone());
                }
            }
        }
        for (_name, prop) in &resolved.properties {
            let prop_resolved = self.resolve_ref(prop);
            if !prop_resolved.required.is_empty() {
                let entry = required_params.entry(endpoint.to_string()).or_default();
                for req in &prop_resolved.required {
                    if !entry.contains(req) {
                        entry.push(req.clone());
                    }
                }
            }
        }
    }

    fn extract_enum_from_schema(
        &self,
        schema: &OpenApiSchema,
        endpoint: &str,
        prefix: &str,
        enum_values: &mut HashMap<String, Vec<String>>,
    ) {
        let resolved = self.resolve_ref(schema);
        for (name, prop) in &resolved.properties {
            let full_name = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}.{}", prefix, name)
            };
            let prop_resolved = self.resolve_ref(prop);
            if !prop_resolved.enum_values.is_empty() {
                let values: Vec<String> = prop_resolved
                    .enum_values
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !values.is_empty() {
                    enum_values.insert(full_name.clone(), values);
                }
            }
            if !prop_resolved.properties.is_empty() {
                self.extract_enum_from_schema(prop, endpoint, &full_name, enum_values);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_openapi_json() -> String {
        r#"{
            "paths": {
                "/v2/vectordb/collections/create": {
                    "post": {
                        "operationId": "create_collection",
                        "tags": ["collections"],
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["collectionName", "schema"],
                                        "properties": {
                                            "collectionName": {
                                                "type": "string",
                                                "description": "Name of the collection"
                                            },
                                            "schema": {
                                                "type": "object",
                                                "required": ["fields"],
                                                "properties": {
                                                    "fields": {
                                                        "type": "array"
                                                    }
                                                }
                                            },
                                            "dim": {
                                                "type": "integer",
                                                "minimum": 1,
                                                "maximum": 32768
                                            },
                                            "metricType": {
                                                "type": "string",
                                                "enum": ["COSINE", "L2", "IP", "HAMMING", "JACCARD"]
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                "/v2/vectordb/entities/search": {
                    "post": {
                        "operationId": "search",
                        "tags": ["entities"],
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["collectionName", "data"],
                                        "properties": {
                                            "collectionName": {
                                                "type": "string"
                                            },
                                            "data": {
                                                "type": "array"
                                            },
                                            "limit": {
                                                "type": "integer",
                                                "minimum": 1,
                                                "maximum": 16384
                                            },
                                            "offset": {
                                                "type": "integer",
                                                "minimum": 0
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "parameters": [
                            {
                                "name": "X-Request-Id",
                                "required": false,
                                "schema": {
                                    "type": "string"
                                }
                            }
                        ]
                    }
                }
            }
        }"#.to_string()
    }

    #[test]
    fn test_extract_to_contract_store() {
        let parser = OpenApiParser::from_json(&sample_openapi_json()).unwrap();
        let store = parser.extract_to_contract_store("milvus", "2.4");

        assert_eq!(store.target, "milvus");
        assert_eq!(store.version, "2.4");
        assert!(store.endpoints.len() >= 2, "endpoints: {:?}", store.endpoints.len());

        assert!(!store.type_constraints.is_empty(),
            "type_constraints empty! endpoints: {:?}, required_params: {:?}, enum_values: {:?}",
            store.endpoints.len(), store.required_params, store.enum_values);

        assert!(store.type_constraints.iter().any(|tc| tc.constraint.param_name == "collectionName"),
            "no collectionName in type_constraints: {:?}", store.type_constraints.iter().map(|tc| &tc.constraint.param_name).collect::<Vec<_>>());
        assert!(store.type_constraints.iter().any(|tc| tc.constraint.param_name == "limit"));
        assert!(store.type_constraints.iter().any(|tc| tc.constraint.param_name == "metricType"));

        assert!(store.range_constraints.iter().any(|rc| rc.constraint.param_name == "dim" && rc.constraint.min == Some(1.0)));
        assert!(store.range_constraints.iter().any(|rc| rc.constraint.param_name == "limit" && rc.constraint.max == Some(16384.0)));
    }

    #[test]
    fn test_extract_required_params() {
        let parser = OpenApiParser::from_json(&sample_openapi_json()).unwrap();
        let store = parser.extract_to_contract_store("milvus", "2.4");

        let create_required = store.required_params.get("create_collection");
        assert!(create_required.is_some());
        let create_required = create_required.unwrap();
        assert!(create_required.contains(&"collectionName".to_string()));
        assert!(create_required.contains(&"schema".to_string()));

        let search_required = store.required_params.get("search");
        assert!(search_required.is_some());
        let search_required = search_required.unwrap();
        assert!(search_required.contains(&"collectionName".to_string()));
        assert!(search_required.contains(&"data".to_string()));
    }

    #[test]
    fn test_extract_enum_values() {
        let parser = OpenApiParser::from_json(&sample_openapi_json()).unwrap();
        let store = parser.extract_to_contract_store("milvus", "2.4");

        let metric_values = store.enum_values.get("metricType");
        assert!(metric_values.is_some());
        let metric_values = metric_values.unwrap();
        assert!(metric_values.contains(&"COSINE".to_string()));
        assert!(metric_values.contains(&"L2".to_string()));
        assert!(metric_values.contains(&"IP".to_string()));
    }

    #[test]
    fn test_annotated_constraints_have_source() {
        let parser = OpenApiParser::from_json(&sample_openapi_json()).unwrap();
        let store = parser.extract_to_contract_store("milvus", "2.4");

        for tc in &store.type_constraints {
            assert_eq!(tc.source, ConstraintSource::OpenapiDerived);
            assert_eq!(tc.confidence, Confidence::High);
            assert!(tc.endpoint.as_ref().map_or(false, |e| !e.is_empty()));
        }
        for rc in &store.range_constraints {
            assert_eq!(rc.source, ConstraintSource::OpenapiDerived);
            assert_eq!(rc.confidence, Confidence::High);
            assert!(rc.endpoint.as_ref().map_or(false, |e| !e.is_empty()));
        }
    }

    #[test]
    fn test_from_store_generates_boundary_tests() {
        let parser = OpenApiParser::from_json(&sample_openapi_json()).unwrap();
        let store = parser.extract_to_contract_store("milvus", "2.4");

        use crate::agent::vdbfuzz::boundary::BoundaryValueGenerator;
        use crate::target::TargetStyle;

        let cases = BoundaryValueGenerator::from_store(&store, TargetStyle::Milvus);
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("limit")));
        assert!(cases.iter().any(|c| c.name.contains("dim")));
        assert!(cases.iter().any(|c| c.name.contains("missing_required")));
        assert!(cases.iter().any(|c| c.name.contains("invalid_enum")));
    }

    #[test]
    fn test_qdrant_openapi_augmentation_diag() {
        let contract_path = std::path::Path::new("contracts/qdrant_contract.json");
        if !contract_path.exists() {
            eprintln!("SKIP: qdrant_contract.json not found");
            return;
        }

        let contract_content =
            std::fs::read_to_string(contract_path).expect("Failed to read");
        let mut contract: crate::contract::schema::StructuredContract =
            serde_json::from_str(&contract_content).expect("Failed to parse");

        println!("=== Before ===");
        println!("type_constraints: {}", contract.type_constraints.len());
        println!("range_constraints: {}", contract.range_constraints.len());

        crate::contract_loader::augment_contract(&mut contract, "qdrant");

        println!("\n=== After ===");
        println!("type_constraints: {}", contract.type_constraints.len());
        println!("range_constraints: {}", contract.range_constraints.len());

        let unique_t: std::collections::HashSet<_> = contract
            .type_constraints
            .iter()
            .map(|tc| tc.param_name.clone())
            .collect();
        let unique_r: std::collections::HashSet<_> = contract
            .range_constraints
            .iter()
            .map(|rc| rc.param_name.clone())
            .collect();
        println!("Unique types: {}", unique_t.len());
        println!("Unique ranges: {}", unique_r.len());

        assert!(contract.type_constraints.len() >= 30);
        assert!(contract.range_constraints.len() >= 10);

        // Persist augmented contract (overwrite qdrant_contract.json)
        let json = serde_json::to_string_pretty(&contract)
            .expect("Failed to serialize augmented contract");
        std::fs::write(contract_path, &json)
            .expect("Failed to write augmented contract");
        println!("Wrote augmented contract to {:?}", contract_path);
    }

    #[test]
    fn test_milvus_openapi_extraction_rate() {
        let openapi_path = std::path::Path::new("contracts/milvus_openapi.json");
        if !openapi_path.exists() {
            eprintln!("SKIP: contracts/milvus_openapi.json not found (run from TestVDB root)");
            return;
        }

        let spec_content = std::fs::read_to_string(openapi_path)
            .expect("Failed to read milvus_openapi.json");
        let parser = OpenApiParser::from_json(&spec_content)
            .expect("Failed to parse Milvus OpenAPI JSON");
        let store = parser.extract_to_contract_store("milvus", "2.4");

        println!("{}", store.constraint_stats());

        let violation_targets = store.query_violations();
        println!("Violation targets: {}", violation_targets.len());

        let critical_params = ["collectionName", "limit", "dim", "metricType", "data", "filter", "offset"];
        let mut found = 0;
        let total = critical_params.len();
        for param in &critical_params {
            let has_type = store.type_constraints.iter().any(|tc| tc.constraint.param_name == *param);
            let has_range = store.range_constraints.iter().any(|rc| rc.constraint.param_name == *param);
            let has_required = store.required_params.values().any(|params| params.iter().any(|p| p == *param));
            let has_enum = store.enum_values.contains_key(*param);
            if has_type || has_range || has_required || has_enum {
                found += 1;
            } else {
                eprintln!("  MISSING: {} not found in any constraint", param);
            }
        }

        let extraction_rate = found as f64 / total as f64;
        println!("Critical param extraction rate: {}/{} = {:.0}%", found, total, extraction_rate * 100.0);

        assert!(store.endpoints.len() >= 10,
            "Expected at least 10 endpoints, got {}", store.endpoints.len());
        assert!(store.type_constraints.len() >= 20,
            "Expected at least 20 type constraints, got {}", store.type_constraints.len());
        assert!(extraction_rate >= 0.85,
            "Critical param extraction rate {:.0}% < 85%", extraction_rate * 100.0);
    }
}
