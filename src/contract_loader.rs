use std::fs;
use std::path::Path;
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, warn};
use anyhow::Context;

use crate::agent::llm::{DeepSeekClient, Message};
use crate::contract;
use crate::contract::schema::StructuredContract;
use crate::contract::store::{Confidence, ConstraintSource, ContractStore};
use crate::crawler::{ChromiumCrawler, CrawledPage, Crawler, ReqwestCrawler, crawl_docs_site};
use crate::sandbox;

#[derive(serde::Serialize)]
pub(crate) struct Phase3Report {
    target: String,
    openapi_available: bool,
    openapi_params_total: usize,
    llm_params_total: usize,
    missing_params: Vec<String>,
    added_type_constraints: usize,
    added_range_constraints: usize,
    added_assertions: usize,
    before_type_count: usize,
    before_range_count: usize,
    before_assertion_count: usize,
    after_type_count: usize,
    after_range_count: usize,
    after_assertion_count: usize,
}

pub async fn run_phase3_param_gap_detection(
    contract: &mut StructuredContract,
    target: &str,
    openapi_path: &Path,
    llm_client: Option<&DeepSeekClient>,
) -> anyhow::Result<Phase3Report> {
    let before_type_count = contract.type_constraints.len();
    let before_range_count = contract.range_constraints.len();
    let before_assertion_count = contract.assertions.len();

    if openapi_path.exists() {
        let spec_content = fs::read_to_string(openapi_path)
            .context("Failed to read OpenAPI spec for Phase 3")?;
        let parser = contract::openapi::OpenApiParser::from_json(&spec_content)
            .context("Failed to parse OpenAPI spec for Phase 3")?;
        let openapi_store = parser.extract_to_contract_store(target, "auto");

        let openapi_params: HashSet<String> = openapi_store.type_constraints.iter()
            .map(|tc| tc.constraint.param_name.clone())
            .chain(openapi_store.range_constraints.iter().map(|rc| rc.constraint.param_name.clone()))
            .chain(openapi_store.required_params.values().flat_map(|v| v.iter().cloned()))
            .chain(openapi_store.enum_values.keys().cloned())
            .collect();

        let llm_params: HashSet<String> = contract.type_constraints.iter()
            .map(|tc| tc.param_name.clone())
            .chain(contract.range_constraints.iter().map(|rc| rc.param_name.clone()))
            .collect();

        let mut missing_params: Vec<String> = Vec::new();
        let mut added_types = 0usize;
        let mut added_ranges = 0usize;
        let mut added_assertions = 0usize;

        for oa_param in &openapi_params {
            let covered = llm_params.iter().any(|llm| {
                llm == oa_param
                    || llm.ends_with(&format!(".{}", oa_param))
            });
            if !covered {
                missing_params.push(oa_param.clone());
            }
        }

        info!(
            "Phase 3: OpenAPI has {} params, LLM has {} params, {} missing",
            openapi_params.len(),
            llm_params.len(),
            missing_params.len(),
        );

        for oa_param in &missing_params {
            for atc in &openapi_store.type_constraints {
                if atc.constraint.param_name == *oa_param {
                    let exists = contract.type_constraints.iter()
                        .any(|tc| tc.param_name == atc.constraint.param_name);
                    if !exists {
                        contract.type_constraints.push(atc.constraint.clone());
                        added_types += 1;
                    }
                }
            }

            for arc in &openapi_store.range_constraints {
                if arc.constraint.param_name == *oa_param {
                    let exists = contract.range_constraints.iter()
                        .any(|rc| rc.param_name == arc.constraint.param_name);
                    if !exists {
                        contract.range_constraints.push(arc.constraint.clone());
                        added_ranges += 1;
                    }
                }
            }

            if let Some(values) = openapi_store.enum_values.get(oa_param) {
                let tag = format!("[IMPLICIT:ENUM] {} must be one of {:?}", oa_param, values);
                if !contract.assertions.iter().any(|a| a.contains(oa_param) && a.starts_with("[IMPLICIT:ENUM]")) {
                    contract.assertions.push(tag);
                    added_assertions += 1;
                }
            }
        }

        for (_endpoint, params) in &openapi_store.required_params {
            for param in params {
                let covered = llm_params.iter().any(|llm| {
                    llm == param || llm.ends_with(&format!(".{}", param))
                });
                if !covered {
                    let tag = format!("[IMPLICIT:REQUIRED] {} is required", param);
                    if !contract.assertions.iter().any(|a| a.contains(param) && a.starts_with("[IMPLICIT:REQUIRED]")) {
                        contract.assertions.push(tag);
                        added_assertions += 1;
                    }
                }
            }
        }

        info!(
            "Phase 3 gap fill: +{} types, +{} ranges, +{} assertions",
            added_types, added_ranges, added_assertions,
        );

        Ok(Phase3Report {
            target: target.to_string(),
            openapi_available: true,
            openapi_params_total: openapi_params.len(),
            llm_params_total: llm_params.len(),
            missing_params,
            added_type_constraints: added_types,
            added_range_constraints: added_ranges,
            added_assertions,
            before_type_count,
            before_range_count,
            before_assertion_count,
            after_type_count: contract.type_constraints.len(),
            after_range_count: contract.range_constraints.len(),
            after_assertion_count: contract.assertions.len(),
        })
    } else {
        info!("Phase 3: OpenAPI spec not available, using fallback assertion validation");

        let (added_types, added_ranges, added_assertions) = run_phase3_fallback(contract, llm_client).await?;

        let report = Phase3Report {
            target: target.to_string(),
            openapi_available: false,
            openapi_params_total: 0,
            llm_params_total: before_type_count + before_range_count,
            missing_params: Vec::new(),
            added_type_constraints: added_types,
            added_range_constraints: added_ranges,
            added_assertions,
            before_type_count,
            before_range_count,
            before_assertion_count,
            after_type_count: contract.type_constraints.len(),
            after_range_count: contract.range_constraints.len(),
            after_assertion_count: contract.assertions.len(),
        };

        if contract.assertions.len() < 20 {
            warn!(
                "Phase 3 fallback: only {} assertions (< 20 target)",
                contract.assertions.len()
            );
        }

        Ok(report)
    }
}

async fn run_phase3_fallback(
    contract: &mut StructuredContract,
    llm_client: Option<&DeepSeekClient>,
) -> anyhow::Result<(usize, usize, usize)> {
    let mut added_types = 0usize;
    let mut added_ranges = 0usize;
    let mut added_assertions = 0usize;

    let (parsed_ranges, parsed_types) = contract::parse_constraints_from_assertions(&contract.assertions);

    let mut known_type_names: HashSet<String> = contract.type_constraints.iter()
        .map(|tc| tc.param_name.clone())
        .collect();
    for pt in &parsed_types {
        if known_type_names.insert(pt.param_name.clone()) {
            contract.type_constraints.push(pt.clone());
            added_types += 1;
        }
    }

    let mut known_range_names: HashSet<String> = contract.range_constraints.iter()
        .map(|rc| rc.param_name.clone())
        .collect();
    for pr in &parsed_ranges {
        if known_range_names.insert(pr.param_name.clone()) {
            contract.range_constraints.push(pr.clone());
            added_ranges += 1;
        }
    }

    if let Some(client) = llm_client {
        let broken_indices: Vec<usize> = contract.assertions.iter()
            .enumerate()
            .filter(|(_, a)| {
                let a_lower = a.to_lowercase();
                !a_lower.contains("must be")
                    && !a_lower.contains("one of")
                    && !a_lower.contains("is required")
                    && !a_lower.starts_with("[implicit:")
            })
            .map(|(i, _)| i)
            .collect();

        if !broken_indices.is_empty() {
            for i in &broken_indices {
                let assertion = contract.assertions[*i].clone();
                let fix_prompt = format!(
                    "Reformat this assertion into a clear HARD constraint. Output only the reformatted assertion, no Markdown.\n\nOriginal: {}",
                    assertion
                );
                let messages = vec![
                    Message::system("You are a constraint formatter. Reformat assertions into: 'param must be ...' or 'param must be one of: ...' or 'param must be > N' format. Output exactly one line.".to_string()),
                    Message::user(fix_prompt),
                ];
                match client.send_chat_json_mode(messages).await {
                    Ok(reformatted) => {
                        let cleaned = reformatted.trim().trim_matches('"').to_string();
                        if !cleaned.is_empty() && cleaned != assertion {
                            contract.assertions[*i] = cleaned;
                            added_assertions += 1;
                        }
                    }
                    Err(e) => warn!("Phase 3 fallback LLM reformat failed: {}", e),
                }
            }
        }

        if contract.assertions.len() < 20 {
            let fill_prompt = format!(
                "You are a documentation parser. Extract API constraints from these assertions. \
                 Add missing constraints for parameters that lack explicit validation rules. \
                 Output ONLY new assertions as a JSON array of strings.\n\n\
                 Current assertions ({}):\n{}\n\n\
                 Current type_constraints ({}):\n{:?}\n\n\
                 Current range_constraints ({}):\n{:?}\n\n\
                 Add at least {} more assertions to reach 20 total.",
                contract.assertions.len(),
                contract.assertions.join("\n"),
                contract.type_constraints.len(),
                contract.type_constraints.iter().map(|tc| &tc.param_name).collect::<Vec<_>>(),
                contract.range_constraints.len(),
                contract.range_constraints.iter().map(|rc| &rc.param_name).collect::<Vec<_>>(),
                20usize.saturating_sub(contract.assertions.len()),
            );
            let messages = vec![
                Message::system("Output a JSON array of assertion strings. No markdown.".to_string()),
                Message::user(fill_prompt),
            ];
            match client.send_chat_json_mode(messages).await {
                Ok(new_assertions_json) => {
                    if let Ok(new_assertions) = serde_json::from_str::<Vec<String>>(&new_assertions_json) {
                        for na in new_assertions {
                            if !contract.assertions.contains(&na) {
                                contract.assertions.push(na);
                                added_assertions += 1;
                            }
                        }
                    }
                }
                Err(e) => warn!("Phase 3 fallback LLM fill failed: {}", e),
            }
        }
    }

    info!(
        "Phase 3 fallback: +{} types, +{} ranges, +{} assertions reformatted/filled",
        added_types, added_ranges, added_assertions,
    );

    Ok((added_types, added_ranges, added_assertions))
}

pub fn augment_contract(contract: &mut StructuredContract, target: &str) {
    let behavioral_path = format!("contracts/{}_behavioral_templates.json", target);
    let behavioral_templates_path = Path::new(&behavioral_path);
    if behavioral_templates_path.exists() {
        match contract::load_behavioral_templates(behavioral_templates_path) {
            Ok(templates) => {
                info!("Loaded {} behavioral contract templates for {}", templates.len(), target);
                contract.behavioral_contracts.extend(templates);
            }
            Err(e) => warn!("Failed to load behavioral templates for {}: {}", target, e),
        }
    }

    let mut merged_store = ContractStore::from_structured_contracts(
        target,
        "auto",
        std::slice::from_ref(&*contract),
        ConstraintSource::ExplicitDoc,
        Confidence::Medium,
    );

    let openapi_str = format!("contracts/{}_openapi.json", target);
    let openapi_path = Path::new(&openapi_str);
    if openapi_path.exists() {
        if let Ok(spec_content) = std::fs::read_to_string(openapi_path) {
            if let Ok(parser) = contract::openapi::OpenApiParser::from_json(&spec_content) {
                let openapi_store = parser.extract_to_contract_store(target, "auto");
                info!(
                    "OpenAPI store: {} type, {} range, {} required, {} enum",
                    openapi_store.type_constraints.len(),
                    openapi_store.range_constraints.len(),
                    openapi_store.required_params.len(),
                    openapi_store.enum_values.len(),
                );
                merged_store.merge(openapi_store);
            }
        }
    }

    // ── Extract type/range constraints from assertions (best-effort fill) ──
    let (parsed_ranges, parsed_types) = contract::parse_constraints_from_assertions(&contract.assertions);
    if !parsed_ranges.is_empty() || !parsed_types.is_empty() {
        info!(
            "Parsed from assertions: {} range constraints, {} type constraints",
            parsed_ranges.len(),
            parsed_types.len()
        );
        if contract.range_constraints.is_empty() {
            contract.range_constraints = parsed_ranges.clone();
        }
        if contract.type_constraints.is_empty() {
            contract.type_constraints = parsed_types.clone();
        }
    }

    // ── Always merge OpenAPI-derived constraints into the contract ──
    // OpenAPI provides the most authoritative type/range data; assertion-based
    // parsing is only a fallback.  We merge unconditionally because the
    // assertion parser may have already populated some fields with less-
    // structured data, and we want OpenAPI precision wherever available.
    if !merged_store.type_constraints.is_empty() {
        let existing: std::collections::HashSet<String> = contract
            .type_constraints
            .iter()
            .map(|tc| format!("{}.{}", tc.expected_type, tc.param_name))
            .collect();
        for atc in &merged_store.type_constraints {
            let key = format!("{}.{}", atc.constraint.expected_type, atc.constraint.param_name);
            if !existing.contains(&key) {
                contract.type_constraints.push(atc.constraint.clone());
            }
        }
    }
    if !merged_store.range_constraints.is_empty() {
        let existing: std::collections::HashSet<String> = contract
            .range_constraints
            .iter()
            .map(|rc| format!("{}.{}.{:?}.{:?}", rc.param_name, rc.description, rc.min, rc.max))
            .collect();
        for arc in &merged_store.range_constraints {
            let key = format!("{}.{}.{:?}.{:?}", arc.constraint.param_name, arc.constraint.description, arc.constraint.min, arc.constraint.max);
            if !existing.contains(&key) {
                contract.range_constraints.push(arc.constraint.clone());
            }
        }
    }

    for (endpoint, params) in &merged_store.required_params {
        for param in params {
            let tag = format!("[IMPLICIT:REQUIRED] {} is required", param);
            if !contract.assertions.iter().any(|a| a.contains(param) && a.starts_with("[IMPLICIT:REQUIRED]")) {
                contract.assertions.push(tag);
            }
        }
        let _ = endpoint;
    }
    for (param_name, values) in &merged_store.enum_values {
        let tag = format!("[IMPLICIT:ENUM] {} must be one of {:?}", param_name, values);
        if !contract.assertions.iter().any(|a| a.contains(param_name) && a.starts_with("[IMPLICIT:ENUM]")) {
            contract.assertions.push(tag);
        }
    }

    info!(
        "Augmented contract: {} assertions, {} type_constraints, {} range_constraints",
        contract.assertions.len(),
        contract.type_constraints.len(),
        contract.range_constraints.len(),
    );
}

// ── Contract cross-validation ─────────────────────────────────────

#[derive(serde::Serialize)]
pub struct ConflictEntry {
    pub param_name: String,
    pub llm_value: String,
    pub openapi_value: String,
    pub conflict_type: String,
}

#[derive(serde::Serialize)]
pub struct ValidationReport {
    pub target: String,
    pub type_conflicts: Vec<ConflictEntry>,
    pub range_conflicts: Vec<ConflictEntry>,
    pub enum_conflicts: Vec<ConflictEntry>,
    pub missing_range_for_type: Vec<String>,
    pub missing_type_for_range: Vec<String>,
    pub redundant_type_constraints: Vec<String>,
}

fn validate_contract(
    contract: &StructuredContract,
    target: &str,
    openapi_store: Option<&ContractStore>,
) -> ValidationReport {
    let mut report = ValidationReport {
        target: target.to_string(),
        type_conflicts: Vec::new(),
        range_conflicts: Vec::new(),
        enum_conflicts: Vec::new(),
        missing_range_for_type: Vec::new(),
        missing_type_for_range: Vec::new(),
        redundant_type_constraints: Vec::new(),
    };

    let type_params: HashSet<String> = contract.type_constraints.iter()
        .map(|tc| tc.param_name.clone())
        .collect();

    let range_params: HashSet<String> = contract.range_constraints.iter()
        .map(|rc| rc.param_name.clone())
        .collect();

    for rc in &contract.range_constraints {
        if !type_params.contains(&rc.param_name) {
            report.missing_type_for_range.push(format!(
                "range_constraint '{}' (min={:?}, max={:?}) has no matching type_constraint",
                rc.param_name, rc.min, rc.max
            ));
        }
    }

    for tc in &contract.type_constraints {
        if !range_params.contains(&tc.param_name)
            && !tc.expected_type.contains("enum")
            && !tc.expected_type.contains("string")
        {
            report.missing_range_for_type.push(format!(
                "type_constraint '{}' (expected_type='{}') has no matching range_constraint",
                tc.param_name, tc.expected_type
            ));
        }
    }

    let mut seen: HashMap<&str, &str> = HashMap::new();
    for tc in &contract.type_constraints {
        if let Some(prev) = seen.get(tc.param_name.as_str()) {
            if *prev != tc.expected_type {
                report.redundant_type_constraints.push(format!(
                    "param '{}' has conflicting types: '{}' vs '{}'",
                    tc.param_name, prev, tc.expected_type
                ));
            }
        } else {
            seen.insert(&tc.param_name, &tc.expected_type);
        }
    }

    if let Some(openapi) = openapi_store {
        let openapi_type_map: HashMap<&str, &str> = openapi.type_constraints.iter()
            .map(|tc| (tc.constraint.param_name.as_str(), tc.constraint.expected_type.as_str()))
            .collect();

        let openapi_range_map: HashMap<&str, (&Option<f64>, &Option<f64>)> = openapi.range_constraints.iter()
            .map(|rc| (rc.constraint.param_name.as_str(), (&rc.constraint.min, &rc.constraint.max)))
            .collect();

        for tc in &contract.type_constraints {
            if let Some(&openapi_type) = openapi_type_map.get(tc.param_name.as_str()) {
                if !types_compatible(&tc.expected_type, openapi_type) {
                    report.type_conflicts.push(ConflictEntry {
                        param_name: tc.param_name.clone(),
                        llm_value: tc.expected_type.clone(),
                        openapi_value: openapi_type.to_string(),
                        conflict_type: "type_mismatch".to_string(),
                    });
                }
            }
        }

        for rc in &contract.range_constraints {
            if let Some(&(openapi_min, openapi_max)) = openapi_range_map.get(rc.param_name.as_str()) {
                if ranges_conflict(rc.min, rc.max, *openapi_min, *openapi_max) {
                    report.range_conflicts.push(ConflictEntry {
                        param_name: rc.param_name.clone(),
                        llm_value: format!("min={:?},max={:?}", rc.min, rc.max),
                        openapi_value: format!("min={:?},max={:?}", openapi_min, openapi_max),
                        conflict_type: "range_mismatch".to_string(),
                    });
                }
            }
        }

        for assertion in &contract.assertions {
            if let Some((param_name, llm_values)) = parse_enum_assertion(assertion) {
                if let Some(openapi_values) = openapi.enum_values.get(&param_name) {
                    let llm_set: HashSet<&str> = llm_values.iter().map(|s| s.as_str()).collect();
                    let openapi_set: HashSet<&str> = openapi_values.iter().map(|s| s.as_str()).collect();
                    if !llm_set.is_subset(&openapi_set) && !openapi_set.is_subset(&llm_set) {
                        report.enum_conflicts.push(ConflictEntry {
                            param_name: param_name.clone(),
                            llm_value: format!("{:?}", llm_values),
                            openapi_value: format!("{:?}", openapi_values),
                            conflict_type: "enum_mismatch".to_string(),
                        });
                    }
                }
            }
        }
    }

    report
}

fn types_compatible(llm_type: &str, openapi_type: &str) -> bool {
    let llm = llm_type.to_lowercase();
    let openapi = openapi_type.to_lowercase();
    if llm == openapi { return true; }
    let integer_types = ["integer", "int", "int32", "int64"];
    let is_llm_int = integer_types.iter().any(|t| llm.contains(t));
    let is_openapi_int = integer_types.iter().any(|t| openapi.contains(t));
    if is_llm_int && is_openapi_int { return true; }
    let string_types = ["string", "str"];
    let is_llm_str = string_types.iter().any(|t| llm.contains(t));
    let is_openapi_str = string_types.iter().any(|t| openapi.contains(t));
    if is_llm_str && is_openapi_str { return true; }
    let numeric_types = ["float", "double", "number"];
    let is_llm_numeric = numeric_types.iter().any(|t| llm.contains(t));
    let is_openapi_numeric = numeric_types.iter().any(|t| openapi.contains(t));
    if is_llm_numeric && is_openapi_numeric { return true; }
    if is_llm_int && is_openapi_str { return false; }
    if is_llm_str && is_openapi_int { return false; }
    if is_llm_numeric && is_openapi_str { return false; }
    if is_llm_str && is_openapi_numeric { return false; }
    if is_llm_int && is_openapi_numeric { return true; }
    if is_llm_numeric && is_openapi_int { return true; }
    true
}

fn ranges_conflict(
    llm_min: Option<f64>, llm_max: Option<f64>,
    openapi_min: Option<f64>, openapi_max: Option<f64>,
) -> bool {
    if let (Some(l_min), Some(o_max)) = (llm_min, openapi_max) {
        if l_min > o_max { return true; }
    }
    if let (Some(l_max), Some(o_min)) = (llm_max, openapi_min) {
        if l_max < o_min { return true; }
    }
    false
}

fn parse_enum_assertion(assertion: &str) -> Option<(String, Vec<String>)> {
    if !assertion.starts_with("[IMPLICIT:ENUM]") { return None; }
    let rest = assertion.strip_prefix("[IMPLICIT:ENUM]")?.trim();
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() < 2 { return None; }
    let param_name = parts[0].to_string();
    let values_str = parts[1].trim();
    let values_str = values_str.strip_prefix("must be one of ").unwrap_or(values_str);
    let values_str = values_str.trim().trim_start_matches('[').trim_end_matches(']');
    let values: Vec<String> = values_str
        .split(',')
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
        .collect();
    if values.is_empty() { return None; }
    Some((param_name, values))
}

pub fn cross_validate_with_openapi(
    contract: &mut StructuredContract,
    target: &str,
    openapi_path: &Path,
) -> ValidationReport {
    let openapi_store = if openapi_path.exists() {
        match fs::read_to_string(openapi_path) {
            Ok(spec) => match contract::openapi::OpenApiParser::from_json(&spec) {
                Ok(parser) => Some(parser.extract_to_contract_store(target, "auto")),
                Err(e) => {
                    warn!("Failed to parse OpenAPI spec: {}", e);
                    None
                }
            },
            Err(e) => {
                warn!("Failed to read OpenAPI spec: {}", e);
                None
            }
        }
    } else {
        None
    };

    let report = validate_contract(contract, target, openapi_store.as_ref());

    let conflict_param_names: HashSet<&str> = report.type_conflicts.iter()
        .chain(report.range_conflicts.iter())
        .chain(report.enum_conflicts.iter())
        .map(|c| c.param_name.as_str())
        .collect();

    if !conflict_param_names.is_empty() {
        for tc in &mut contract.type_constraints {
            if conflict_param_names.contains(tc.param_name.as_str()) {
                let tag = format!("[LOW_CONFIDENCE:type_conflict] {}", tc.param_name);
                if !contract.assertions.iter().any(|a| a.contains(&tc.param_name) && a.starts_with("[LOW_CONFIDENCE:type_conflict]")) {
                    contract.assertions.push(tag);
                }
            }
        }
        for rc in &mut contract.range_constraints {
            if conflict_param_names.contains(rc.param_name.as_str()) {
                let tag = format!("[LOW_CONFIDENCE:range_conflict] {}", rc.param_name);
                if !contract.assertions.iter().any(|a| a.contains(&rc.param_name) && a.starts_with("[LOW_CONFIDENCE:range_conflict]")) {
                    contract.assertions.push(tag);
                }
            }
        }
    }

    report
}

// ── Phase 1: Parameter extraction prompt ──
const PARAM_EXTRACTION_PROMPT: &str = r#"
You are a documentation parser. Extract ALL API parameters from the documentation.

For each parameter, output a JSON object with these fields:
- "param_name": qualified name (e.g., "search.limit", "create.dim", "hnsw.m")
- "endpoint": the API operation (e.g., "search", "create_collection", "index_create")
- "appears_in": where the parameter appears ("request body", "query", "header", "path", "config")
- "doc_description": what the documentation says about this parameter (1 sentence)

Output a JSON array. Do NOT wrap in Markdown code blocks.
Example: [{"param_name":"search.limit","endpoint":"search","appears_in":"request body","doc_description":"Maximum number of results to return."}]
"#;

// ── Phase 2: Constraint extraction prompt with few-shot examples ──
const CONTRACT_SCHEMA_PROMPT: &str = r#"
You are a highly capable database testing agent.
Your task is to read the provided Markdown documentation and parameter list, then extract API constraints into a STRICT JSON object.

## Critical Distinction
- HARD CONSTRAINT: Explicitly stated as required, forbidden, or mandatory. Example: "limit must be greater than 0" or "offset must be a non-negative integer".
- SOFT RECOMMENDATION: Phrased as a suggestion using words like "recommended", "should", "we suggest", "typically". Example: "it is recommended to use values between 10 and 100".
- ONLY extract HARD constraints. Do NOT extract recommendations or usage suggestions.
- Do NOT infer constraints from code examples alone — only extract constraints that the documentation explicitly states in prose.

## Few-Shot Examples

### CORRECT (extract these):
- "[CREATE] dim must be > 0 and <= 32768"
- "[SEARCH] limit must be > 0"
- "[CREATE] metricType must be one of: L2, IP, COSINE, HAMMING, JACCARD"
- "[SEARCH] offset must be >= 0"
- "[INSERT] vector dimension must match collection dimension"
- "[SEARCH] nprobe rejection_policy=ignore (server silently ignores invalid nprobe)"
- "[CREATE] dimension rejection_policy=reject (server rejects invalid dimension)"
- "[SEARCH] searchParams nested: [nprobe, ef]"

### INCORRECT (do NOT extract):
- "it is recommended to use nlist=128" → SOFT RECOMMENDATION (skip)
- "you can set limit to 10" → CODE EXAMPLE inference (skip)
- "typically offset defaults to 0" → default behavior description (skip)
- "nprobe can be set to any value" → no constraint, just a description (skip)

## Schema
Do NOT wrap the JSON in Markdown code blocks.
The JSON must exactly match the following schema:
{
    "api_endpoint": "string (the name of the API, e.g., create_collection)",
    "doc_url": "string (the URL provided to you)",
    "parameters": {
        "param_name": "type (e.g., string, int)"
    },
    "data_types": {
        "field_name": "allowed_values (e.g., positive integer, non-empty string)"
    },
    "assertions": [
        "string (HARD constraints only, e.g., 'limit must be > 0')"
    ],
    "rejection_policies": {
        "param_name": "reject or ignore - whether the server rejects invalid values (reject) or silently ignores them (ignore). Default is reject. Use ignore for search/query params that the server silently ignores when invalid."
    },
    "nested_params": {
        "parent_param": ["child_param1", "child_param2"] - nested JSON structure, e.g., searchParams contains nprobe, ef"
    }
}

## Confidence
For each assertion, ask yourself: "Is this constraint explicitly stated in the prose, or am I inferring it from a code example?" If you are inferring, do NOT include it.
"#;

pub async fn run_extract(target: &str, docs_url: &str, out_dir: &str, llm_client: &DeepSeekClient) -> anyhow::Result<()> {
    info!("Starting contract extraction for target: {}", target);
    fs::create_dir_all(out_dir)?;

    // Crawler selection: try Chromium, fall back to Reqwest
    let crawler: Box<dyn crate::crawler::Crawler> = match ChromiumCrawler::new().fetch_page(docs_url).await {
        Ok(_) => {
            info!("Using ChromiumCrawler (headless browser)");
            Box::new(ChromiumCrawler)
        }
        Err(e) => {
            warn!("ChromiumCrawler unavailable: {}. Falling back to ReqwestCrawler.", e);
            Box::new(ReqwestCrawler::new())
        }
    };

    // BFS crawl: full documentation tree
    let pages = crawl_docs_site(crawler.as_ref(), docs_url, 50, 3).await?;
    info!("Crawled {} pages", pages.len());

    // Incremental: merge with previously crawled pages
    let crawled_path = Path::new(out_dir).join(format!("{}_crawled_pages.json", target));
    let all_pages = {
        let mut merged: Vec<CrawledPage> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if crawled_path.exists() {
            if let Ok(json) = fs::read_to_string(&crawled_path) {
                if let Ok(existing) = serde_json::from_str::<Vec<CrawledPage>>(&json) {
                    for p in existing {
                        seen.insert(p.url.clone());
                        merged.push(p);
                    }
                    info!("Loaded {} previously crawled pages (incremental mode)", merged.len());
                }
            }
        }
        for p in pages {
            if seen.insert(p.url.clone()) {
                merged.push(p);
            }
        }
        merged
    };

    let crawled_json = serde_json::to_string_pretty(&all_pages)?;
    fs::write(&crawled_path, &crawled_json)?;
    info!("Saved {} crawled pages to {:?}", all_pages.len(), crawled_path);

    // ── Auto-discover OpenAPI spec ──
    let openapi_path = Path::new(out_dir).join(format!("{}_openapi.json", target));
    if !openapi_path.exists() {
        let (root_origin, proto) = {
            let parts: Vec<&str> = docs_url.splitn(4, '/').collect();
            let proto = parts[0].to_string(); // "https:" or "http:"
            let origin = if parts.len() >= 3 {
                format!("{}//{}", proto, parts[2])
            } else {
                docs_url.trim_end_matches('/').to_string()
            };
            (origin, proto)
        };
        let host = root_origin
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let probe_origins = vec![
            root_origin.clone(),
            format!("{}//api.{}", proto, host),
            format!("{}//api.qdrant.tech", proto),
            format!("{}//qdrant.github.io", proto),
        ];
        let probe_paths = ["/openapi.json", "/swagger.json", "/api-docs", "/api/openapi.json", "/redoc/openapi.json", "/api/v1/openapi.json"];
        let spec_crawler = ReqwestCrawler::new();
        'probe: for origin in &probe_origins {
            for probe in &probe_paths {
                let spec_url = format!("{}{}", origin, probe);
                match spec_crawler.fetch_page(&spec_url).await {
                    Ok(body) => {
                        if body.trim().starts_with('{')
                            && (body.contains("\"openapi\"") || body.contains("\"swagger\""))
                        {
                            fs::write(&openapi_path, &body)?;
                            info!("OpenAPI spec auto-discovered at {} → saved to {:?}", spec_url, openapi_path);
                            break 'probe;
                        }
                    }
                    Err(_) => continue,
                }
            }
        }
        if !openapi_path.exists() {
            info!("No OpenAPI spec found at standard paths (probed {} origins × {} paths)", probe_origins.len(), probe_paths.len());
        }
    } else {
        info!("OpenAPI spec already exists at {:?}, skipping auto-discovery", openapi_path);
    }

    // Merge all page markdowns for LLM extraction
    let merged_md = all_pages.iter()
        .map(|p| format!("## Source: {}\n\n{}", p.url, p.markdown))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    // llm_client is now provided by the caller via parameter

    // ── Phase 1: Extract parameter list ──
    info!("Phase 1: Extracting parameter list from {} pages...", all_pages.len());
    let phase1_messages = vec![
        Message::system(PARAM_EXTRACTION_PROMPT.to_string()),
        Message::user(merged_md.clone()),
    ];
    let param_json = llm_client.send_chat_json_mode(phase1_messages).await?;
    let param_count = param_json.matches("\"param_name\"").count();
    info!("Phase 1: extracted {} parameters", param_count);

    // ── Phase 2: Extract constraints with parameter list + few-shot ──
    info!("Phase 2: Extracting constraints from parameter list...");
    let phase2_input = format!(
        "Doc URL: {}\n\nParameter List ({} params):\n{}\n\nMarkdown Content ({} pages):\n{}",
        docs_url,
        param_count,
        param_json,
        all_pages.len(),
        merged_md,
    );
    let phase2_messages = vec![
        Message::system(CONTRACT_SCHEMA_PROMPT.to_string()),
        Message::user(phase2_input),
    ];
    let json_response = llm_client.send_chat_json_mode(phase2_messages).await?;

    match serde_json::from_str::<StructuredContract>(&json_response) {
        Ok(mut contract) => {
            let openapi_file = Path::new(out_dir).join(format!("{}_openapi.json", target));

            let phase3_report = run_phase3_param_gap_detection(
                &mut contract, target, &openapi_file, Some(&llm_client),
            ).await?;
            info!(
                "Phase 3 report: {} params found, {} missing, +{} types +{} ranges +{} assertions ({} -> {} assertions, {} -> {} types, {} -> {} ranges)",
                phase3_report.openapi_params_total,
                phase3_report.missing_params.len(),
                phase3_report.added_type_constraints,
                phase3_report.added_range_constraints,
                phase3_report.added_assertions,
                phase3_report.before_assertion_count,
                phase3_report.after_assertion_count,
                phase3_report.before_type_count,
                phase3_report.after_type_count,
                phase3_report.before_range_count,
                phase3_report.after_range_count,
            );
            let report_path = Path::new(out_dir).join(format!("{}_phase3_report.json", target));
            fs::write(&report_path, serde_json::to_string_pretty(&phase3_report)?)?;
            info!("Phase 3 report saved to {:?}", report_path);

            augment_contract(&mut contract, target);

            let cv_report = cross_validate_with_openapi(&mut contract, target, &openapi_file);
            info!(
                "Cross-validation: {} type_conflicts, {} range_conflicts, {} enum_conflicts, {} missing_range_for_type, {} missing_type_for_range, {} redundant",
                cv_report.type_conflicts.len(),
                cv_report.range_conflicts.len(),
                cv_report.enum_conflicts.len(),
                cv_report.missing_range_for_type.len(),
                cv_report.missing_type_for_range.len(),
                cv_report.redundant_type_constraints.len(),
            );

            let file_path = Path::new(out_dir).join(format!("{}_contract.json", target));
            contract::save_contract_json(&contract, &file_path)?;
            info!("Successfully extracted and saved contract to {:?}", file_path);

            let total_issues = cv_report.type_conflicts.len()
                + cv_report.range_conflicts.len()
                + cv_report.enum_conflicts.len()
                + cv_report.missing_range_for_type.len()
                + cv_report.missing_type_for_range.len()
                + cv_report.redundant_type_constraints.len();
            if total_issues > 0 {
                let report_path = Path::new(out_dir).join(format!("{}_validation_report.json", target));
                let report_json = serde_json::to_string_pretty(&cv_report)?;
                fs::write(&report_path, &report_json)?;
                info!("Validation report saved to {:?} ({} issues)", report_path, total_issues);
            }
        }
        Err(e) => {
            error!("Raw LLM Output:\n{}", json_response);
            anyhow::bail!("Failed to parse LLM JSON output into StructuredContract: {}", e);
        }
    }
    Ok(())
}

pub async fn run_knowledge_agent(
    llm_client: &DeepSeekClient,
    target: &str,
    version: &str,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
) -> anyhow::Result<String> {
    use contract::schema::{EndpointRegistry, EndpointEntry};

    let repo = repo_url.as_ref().context("repo_url is required when contracts directory is not provided")?;
    let docs = docs_url.as_ref().context("docs_url is required when contracts directory is not provided")?;

    info!("Starting Knowledge Agent Phase... Target Repo: {}", repo);

    let registry_file = format!(".trae/endpoints/{}.toml", target);
    let registry_path = Path::new(&registry_file);
    let registry = if registry_path.exists() {
        contract::load_endpoint_registry(registry_path)?
    } else {
        EndpointRegistry {
            target: target.to_string(),
            version: version.to_string(),
            endpoints: vec![EndpointEntry {
                name: target.to_string(),
                api_path: "search".to_string(),
                docs_url: docs.clone(),
                category: "points".to_string(),
            }],
        }
    };

    let kw_sandbox = sandbox::manager::Sandbox::create_knowledge_worker("ubuntu:latest", &["git", "curl", "grep", "ca-certificates"]).await?;
    let clone_result = kw_sandbox.exec_command(&["git", "clone", repo, "/workspace"]).await?;
    if !clone_result.success {
        warn!("Git clone may have failed: {}", clone_result.stderr);
    }

    let mut all_contracts: Vec<StructuredContract> = Vec::new();
    for entry in &registry.endpoints {
        info!("KA: extracting contract for '{}' ({})", entry.name, entry.api_path);
        match crate::agent::engine::knowledge_exploration_loop(llm_client, &kw_sandbox, target, repo, &entry.name, &entry.api_path, &entry.docs_url, 8).await {
            Ok(contract) => {
                info!("Contract for '{}': {} assertions", entry.name, contract.assertions.len());
                all_contracts.push(contract);
            }
            Err(e) => warn!("Failed to extract contract for '{}': {}", entry.name, e),
        }
    }

    kw_sandbox.cleanup().await?;
    if all_contracts.is_empty() {
        anyhow::bail!("Knowledge Agent failed to extract any contracts from {} endpoints.", registry.endpoints.len());
    }

    let generated_contract = contract::merge_contracts_from_ka(&all_contracts, docs);
    fs::create_dir_all(".trae/auto_contracts").unwrap_or_default();
    let contract_path = format!(".trae/auto_contracts/{}_contract.json", target);
    let json_str = serde_json::to_string_pretty(&generated_contract)?;
    fs::write(&contract_path, &json_str)?;
    info!("Saved auto-generated contract to {}", contract_path);

    Ok(json_str)
}

pub async fn load_contract_content(
    contracts_dir: &Option<String>,
    target: &str,
    version: &str,
    repo_url: &Option<String>,
    docs_url: &Option<String>,
    llm_client: &DeepSeekClient,
) -> anyhow::Result<String> {
    use crate::target::TargetRegistry;

    if let Some(dir) = contracts_dir {
        info!("Loading contracts from: {}", dir);
        let contract_path = Path::new(dir).join(format!("{}_contract.json", target));
        if !contract_path.exists() {
            anyhow::bail!("Contract file not found: {:?}", contract_path);
        }
        return Ok(fs::read_to_string(&contract_path)?);
    }

    // ── Auto-trigger: check local contracts/ directory first ──
    let local_contract = Path::new("contracts").join(format!("{}_contract.json", target));
    if local_contract.exists() {
        if let Ok(content) = fs::read_to_string(&local_contract) {
            if let Ok(contract) = serde_json::from_str::<StructuredContract>(&content) {
                if contract.assertions.len() >= 20 {
                    info!("Using local contract with {} assertions (skip Knowledge Agent)", contract.assertions.len());
                    return Ok(content);
                }
                info!("Local contract has only {} assertions (< 20), triggering Knowledge Agent", contract.assertions.len());
            }
        }
    } else {
        info!("No local contract found at {:?}, triggering Knowledge Agent", local_contract);
    }

    // ── Resolve repo_url and docs_url with plugin defaults ──
    let registry = TargetRegistry::new_with_all();
    let plugin_defaults = registry.get(target).map(|p| (p.default_repo_url(), p.default_docs_url()));

    let repo = repo_url.clone().or_else(|| {
        plugin_defaults.and_then(|(r, _)| r.map(String::from))
    });
    let docs = docs_url.clone().or_else(|| {
        plugin_defaults.and_then(|(_, d)| d.map(String::from))
    });

    match (repo.as_ref(), docs.as_ref()) {
        (Some(r), Some(d)) => {
            info!("Knowledge Agent auto-triggered: repo={}, docs={}", r, d);
            run_knowledge_agent(llm_client, target, version, &repo, &docs).await
        }
        _ => anyhow::bail!(
            "No contracts provided and Knowledge Agent requires --repo-url and --docs-url. Tip: set defaults in TargetPlugin for '{}'.",
            target
        ),
    }
}

pub fn build_contract_store(
    contract: &StructuredContract,
    target: &str,
    version: &str,
) -> contract::store::ContractStore {
    let mut store = contract::store::ContractStore::from_structured_contracts(
        target,
        version,
        std::slice::from_ref(contract),
        contract::store::ConstraintSource::ExplicitDoc,
        contract::store::Confidence::Medium,
    );

    let openapi_str = format!("contracts/{}_openapi.json", target);
    let openapi_path = Path::new(&openapi_str);
    if openapi_path.exists() {
        if let Ok(spec_content) = std::fs::read_to_string(openapi_path) {
            if let Ok(parser) = contract::openapi::OpenApiParser::from_json(&spec_content) {
                let openapi_store = parser.extract_to_contract_store(target, version);
                info!(
                    "OpenAPI augmentation: +{} type, +{} range, +{} required, +{} enum",
                    openapi_store.type_constraints.len(),
                    openapi_store.range_constraints.len(),
                    openapi_store.required_params.len(),
                    openapi_store.enum_values.len(),
                );
                store.merge(openapi_store);
            }
        }
    }

    let mut ignore_count = 0u32;
    let mut total_checked = 0u32;
    for atc in store.type_constraints.iter_mut() {
        if atc.rejection_policy == Some(contract::schema::RejectionPolicy::Reject) {
            total_checked += 1;
            if let Some(ep) = &atc.endpoint {
                let ep_short = ep.rsplit('/').next().unwrap_or(ep);
                let param_short = atc.constraint.param_name.rsplit('.').next().unwrap_or(&atc.constraint.param_name);
                let qualified = format!("{}.{}", ep_short, param_short);
                if let Some(policy) = contract.rejection_policies.get(&qualified)
                    .or_else(|| contract.rejection_policies.get(&atc.constraint.param_name))
                    .or_else(|| contract.rejection_policies.get(param_short))
                {
                    if *policy == contract::schema::RejectionPolicy::Ignore {
                        ignore_count += 1;
                    }
                    atc.rejection_policy = Some(policy.clone());
                }
            }
        }
    }

    for arc in store.range_constraints.iter_mut() {
        if arc.rejection_policy == Some(contract::schema::RejectionPolicy::Reject) {
            total_checked += 1;
            if let Some(ep) = &arc.endpoint {
                let ep_short = ep.rsplit('/').next().unwrap_or(ep);
                let param_short = arc.constraint.param_name.rsplit('.').next().unwrap_or(&arc.constraint.param_name);
                let qualified = format!("{}.{}", ep_short, param_short);
                if let Some(policy) = contract.rejection_policies.get(&qualified)
                    .or_else(|| contract.rejection_policies.get(&arc.constraint.param_name))
                    .or_else(|| contract.rejection_policies.get(param_short))
                {
                    if *policy == contract::schema::RejectionPolicy::Ignore {
                        ignore_count += 1;
                    }
                    arc.rejection_policy = Some(policy.clone());
                }
            }
        }
    }
    debug!("Rejection policy propagation: {}/{} constraints updated to Ignore after OpenAPI merge", ignore_count, total_checked);

    let mut filtered_required = 0u32;
    let filtered: std::collections::HashMap<String, Vec<String>> = store.required_params.iter()
        .map(|(endpoint, params)| {
            let kept: Vec<String> = params.iter()
                .filter(|param| store.get_rejection_policy(param, endpoint) != contract::schema::RejectionPolicy::Ignore)
                .cloned()
                .collect();
            filtered_required += (params.len() - kept.len()) as u32;
            (endpoint.clone(), kept)
        })
        .filter(|(_, params)| !params.is_empty())
        .collect();
    store.required_params = filtered;
    if filtered_required > 0 {
        debug!("Filtered {} required_params with rejection_policy=Ignore", filtered_required);
    }

    store
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::schema::{TypeConstraint, RangeConstraint};
    use std::io::Write;

    fn write_openapi_json(dir: &std::path::Path, json: &str) -> std::path::PathBuf {
        let path = dir.join("openapi.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        path
    }

    #[test]
    fn test_type_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        let openapi_json = r#"{
            "paths": {
                "/collections/test": {
                    "post": {
                        "operationId": "test_op",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "limit": {
                                                "type": "integer"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }"#;
        let openapi_path = write_openapi_json(dir.path(), openapi_json);

        let mut contract = StructuredContract {
            api_endpoint: "test".to_string(),
            doc_url: String::new(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "string".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };

        let report = cross_validate_with_openapi(&mut contract, "test", &openapi_path);

        assert!(!report.type_conflicts.is_empty());
        assert!(report.type_conflicts.iter().any(|c| c.param_name == "limit"));
        assert!(contract.assertions.iter().any(|a| a.contains("[LOW_CONFIDENCE:type_conflict]") && a.contains("limit")));
    }

    #[test]
    fn test_range_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        let openapi_json = r#"{
            "paths": {
                "/collections/test": {
                    "post": {
                        "operationId": "test_op",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "limit": {
                                                "type": "integer",
                                                "minimum": 1,
                                                "maximum": 16384
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }"#;
        let openapi_path = write_openapi_json(dir.path(), openapi_json);

        let mut contract = StructuredContract {
            api_endpoint: "test".to_string(),
            doc_url: String::new(),
            assertions: vec![],
            type_constraints: vec![],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: String::new(),
                min: Some(20000.0),
                max: Some(50000.0),
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };

        let report = cross_validate_with_openapi(&mut contract, "test", &openapi_path);

        assert!(!report.range_conflicts.is_empty());
        assert!(report.range_conflicts.iter().any(|c| c.param_name == "limit"));
    }

    #[test]
    fn test_no_conflict_when_openapi_missing_param() {
        let dir = tempfile::tempdir().unwrap();
        let openapi_json = r#"{
            "paths": {
                "/collections/test": {
                    "post": {
                        "operationId": "test_op",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "limit": {
                                                "type": "integer"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }"#;
        let openapi_path = write_openapi_json(dir.path(), openapi_json);

        let mut contract = StructuredContract {
            api_endpoint: "test".to_string(),
            doc_url: String::new(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "custom_param".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };

        let report = cross_validate_with_openapi(&mut contract, "test", &openapi_path);

        assert!(report.type_conflicts.is_empty());
    }

    #[test]
    fn test_enum_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        let openapi_json = r#"{
            "paths": {
                "/collections/test": {
                    "post": {
                        "operationId": "test_op",
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "metricType": {
                                                "type": "string",
                                                "enum": ["COSINE", "L2", "IP"]
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }"#;
        let openapi_path = write_openapi_json(dir.path(), openapi_json);

        let mut contract = StructuredContract {
            api_endpoint: "test".to_string(),
            doc_url: String::new(),
            assertions: vec!["[IMPLICIT:ENUM] metricType must be one of [\"L2\", \"IP\"]".to_string()],
            type_constraints: vec![],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: std::collections::HashMap::new(),
            nested_params: std::collections::HashMap::new(),
        };

        let report = cross_validate_with_openapi(&mut contract, "test", &openapi_path);

        assert!(report.enum_conflicts.is_empty());
    }
}