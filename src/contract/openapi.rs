use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::schema::{EndpointEntry, RangeConstraint, TypeConstraint};

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
struct OpenApiOperation {
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    parameters: Vec<OpenApiParameter>,
    #[serde(default)]
    request_body: Option<OpenApiRequestBody>,
}

#[derive(Debug, Deserialize)]
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
                    let name = op.operation_id.clone().unwrap_or_else(|| {
                        format!("{}_{}", method.to_lowercase(), path.replace('/', "_").trim_matches('_'))
                    });
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

    pub fn extract_type_constraints(&self, endpoint_name: &str) -> Vec<TypeConstraint> {
        let mut constraints = Vec::new();
        if let Some((_path, _item, op)) = self.find_operation(endpoint_name) {
            if let Some(body) = &op.request_body {
                for (_content_type, media) in &body.content {
                    if let Some(schema) = &media.schema {
                        self.extract_type_constraints_from_schema(schema, "", &mut constraints);
                    }
                }
            }
        }
        constraints
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

    pub fn extract_range_constraints(&self, endpoint_name: &str) -> Vec<RangeConstraint> {
        let mut constraints = Vec::new();
        if let Some((_path, _item, op)) = self.find_operation(endpoint_name) {
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

    fn find_operation(&self, endpoint_name: &str) -> Option<(&String, &OpenApiPathItem, &OpenApiOperation)> {
        for (path, item) in &self.spec.paths {
            for op in [&item.post, &item.put, &item.delete, &item.get, &item.patch].iter() {
                if let Some(op) = op {
                    if op.operation_id.as_deref() == Some(endpoint_name) {
                        return Some((path, item, op));
                    }
                }
            }
        }
        None
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
}
