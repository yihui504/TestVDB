use super::tools::{
    get_clone_repo_tool, get_crawl_url_tool, get_read_file_tool, get_search_code_tool,
    get_submit_contract_tool, crawl_docs
};
use super::llm::{DeepSeekClient, Message};
use crate::contract::schema::{
    Determinism, RangeConstraint, StateConstraint,
    StructuredContract, TypeConstraint,
};
use crate::contract::store::{Confidence, ConstraintSource, ContractStore};
use crate::sandbox::manager::Sandbox;
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{info, warn};

/// Build the system prompt for the Knowledge Agent that extracts constraints from docs + source.
fn knowledge_agent_system_prompt(
    target_name: &str,
    target_repo_url: &str,
    endpoint_name: &str,
    endpoint_api_path: &str,
    endpoint_docs_url: &str,
) -> String {
    format!(
        "You are an expert Security Knowledge Agent for the '{}' database.\n\
        Target Repo: {}\n\
        Repo is at /workspace.\n\n\
        TASK: Extract constraints for endpoint '{}' ({}). Docs: {}\n\n\
        ## Constraint Categories\n\
        Use these prefixes:\n\
        - [TYPE] for types: '[TYPE] limit must be integer'\n\
        - [RANGE] for ranges: '[RANGE] limit must be > 0'\n\
        - [STATE] for state: '[STATE:deterministic] collection must exist'\n\
        - [BEHAVIOR:STATE] for state consistency: '[BEHAVIOR:STATE] upsert N points -> count == N'\n\
        - [BEHAVIOR:SEMANTIC] for semantic correctness: '[BEHAVIOR:SEMANTIC] search results sorted by score descending'\n\
        - [BEHAVIOR:INTERFACE] for interface consistency: '[BEHAVIOR:INTERFACE] gRPC and REST return same results'\n\
        - [BEHAVIOR:DIAGNOSTIC] for diagnostic quality: '[BEHAVIOR:DIAGNOSTIC] error message mentions parameter name'\n\n\
        ## Implicit Constraints (CRITICAL)\n\
        Also extract IMPLICIT constraints that are NOT explicitly stated but can be inferred:\n\
        - [IMPLICIT:REQUIRED] doc says optional but API behavior suggests required\n\
        - [IMPLICIT:OPTIONAL] doc says required but API accepts request without it\n\
        - [IMPLICIT:ACCEPTED] doc does not mention parameter but API accepts it\n\
        - [IMPLICIT:REJECTED] doc does not mention constraint but API enforces it\n\
        - [IMPLICIT:DEFAULT] parameter has undocumented default value\n\
        - [IMPLICIT:COERCION] API silently converts type (e.g., string '123' -> int 123)\n\n\
        ## Required/Enum Extraction\n\
        For each endpoint, identify:\n\
        - Which parameters are REQUIRED (must be present, not null)\n\
        - Which parameters have ENUM values (fixed set of allowed values)\n\
        - Which parameters have RANGE constraints (min/max values)\n\n\
        Crawl docs + grep 1-2 times, then call submit_contract. Submit by turn 3.",
        target_name, target_repo_url,
        endpoint_name, endpoint_api_path, endpoint_docs_url
    )
}

pub async fn knowledge_exploration_loop(
    llm_client: &DeepSeekClient,
    sandbox: &Sandbox,
    target_name: &str,
    target_repo_url: &str,
    endpoint_name: &str,
    endpoint_api_path: &str,
    endpoint_docs_url: &str,
    max_turns: usize,
) -> Result<StructuredContract> {
    let tools = vec![
        get_clone_repo_tool(),
        get_read_file_tool(),
        get_search_code_tool(),
        get_crawl_url_tool(),
        get_submit_contract_tool(),
    ];

    let system_prompt = knowledge_agent_system_prompt(
        target_name, target_repo_url,
        endpoint_name, endpoint_api_path, endpoint_docs_url,
    );

    let mut messages = vec![
        Message::system(system_prompt),
        Message::user(format!("Extract constraints for endpoint '{}'. Start by crawling {}", endpoint_name, endpoint_docs_url)),
    ];

    let mut consecutive_same_errors = 0;
    let mut last_read_path = String::new();
    let mut same_file_reads = 0u32;
    let mut last_assistant_text = String::new();

    for turn in 0..max_turns {
        info!("Knowledge Exploration Turn {}/{}", turn + 1, max_turns);
        
        // === B1: Protocol-level forced submission ===
        // Turn 2: gentle reminder
        if turn == 2 {
            messages.push(Message::user(
                "[REMINDER] Next turn you MUST summarize and submit. Gather your findings now."
            ));
        }
        // Turn 3: force LLM to output structured JSON
        if turn == 3 {
            messages.push(Message::user(format!(
                "[SYSTEM] DO NOT use any tools. Output a JSON object with this EXACT schema:\n\
                {{\n\
                  \"api_endpoint\": \"{}\",\n\
                  \"doc_url\": \"\",\n\
                  \"assertions\": [\"[TYPE] ...\", \"[RANGE] ...\", \"[STATE:deterministic] ...\", \"[IMPLICIT:REQUIRED] ...\"],\n\
                  \"type_constraints\": [\n\
                    {{\"param_name\": \"limit\", \"expected_type\": \"integer\"}}\n\
                  ],\n\
                  \"range_constraints\": [\n\
                    {{\"param_name\": \"limit\", \"description\": \"limit range\", \"min\": 1, \"max\": 16384}}\n\
                  ],\n\
                  \"required_params\": [\"collectionName\", \"data\"],\n\
                  \"enum_values\": {{\"metricType\": [\"COSINE\", \"L2\", \"IP\"]}},\n\
                  \"state_constraints\": [],\n\
                  \"state_invariants\": [],\n\
                  \"behavioral_contracts\": []\n\
                }}\n\n\
                Rules:\n\
                - assertions: ALL constraints with their prefix tags (including [IMPLICIT:*])\n\
                - type_constraints: one per parameter with its expected JSON type\n\
                - range_constraints: only for parameters with min/max bounds\n\
                - required_params: parameters that MUST be present\n\
                - enum_values: parameters with fixed allowed values\n\
                - min/max in range_constraints must be numbers (not strings)\n\
                - Output ONLY the JSON object, no markdown, no explanation.",
                endpoint_name
            )));
        }
        // Turn 4: parse LLM structured JSON and auto-submit, or fallback
        if turn == 4 {
            let contract = parse_structured_contract_from_text(
                &last_assistant_text,
                endpoint_name,
                endpoint_docs_url,
            );

            match contract {
                Ok(c) => {
                    info!("B1: Parsed structured contract from LLM JSON for '{}': {} assertions, {} type_constraints, {} range_constraints, {} required, {} enum",
                        endpoint_name, c.assertions.len(), c.type_constraints.len(),
                        c.range_constraints.len(),
                        c.assertions.iter().filter(|a| a.starts_with("[IMPLICIT:REQUIRED]")).count(),
                        c.assertions.iter().filter(|a| a.starts_with("[IMPLICIT:ENUM]")).count(),
                    );
                    return Ok(c);
                }
                Err(e) => {
                    warn!("B1: Failed to parse structured JSON for '{}': {}. Falling back to assertion-only.", endpoint_name, e);
                    let mut parsed: Vec<String> = Vec::new();
                    for line in last_assistant_text.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("[TYPE]") || trimmed.starts_with("[RANGE]") || trimmed.starts_with("[STATE]") || trimmed.starts_with("[IMPLICIT:") {
                            parsed.push(trimmed.to_string());
                        }
                    }
                    if !parsed.is_empty() {
                        return Ok(StructuredContract {
                            api_endpoint: endpoint_name.to_string(),
                            doc_url: endpoint_docs_url.to_string(),
                            assertions: parsed,
                            type_constraints: vec![],
                            range_constraints: vec![],
                            state_constraints: vec![],
                            state_invariants: vec![],
                            behavioral_contracts: vec![],
                            rejection_policies: HashMap::new(),
                            nested_params: HashMap::new(),
                        });
                    } else {
                        return Ok(StructuredContract {
                            api_endpoint: endpoint_name.to_string(),
                            doc_url: endpoint_docs_url.to_string(),
                            assertions: vec![format!("[{}] parameter validation for {}", endpoint_name, endpoint_api_path)],
                            type_constraints: vec![],
                            range_constraints: vec![],
                            state_constraints: vec![],
                            state_invariants: vec![],
                            behavioral_contracts: vec![],
                            rejection_policies: HashMap::new(),
                            nested_params: HashMap::new(),
                        });
                    }
                }
            }
        }
        // Force at turn 4 is removed — B1 handles it
        // Dead stop at turn 6 is removed — B1 handles it
        
        let response_msg = llm_client.send_chat_with_tools(messages.clone(), tools.clone()).await?;
        messages.push(response_msg.clone());

        // Capture last text response (for B1 parsing)
        if let Some(ref content) = response_msg.content {
            if !content.trim().is_empty() {
                last_assistant_text = content.clone();
            }
        }

        if let Some(tool_calls) = response_msg.tool_calls {
            if tool_calls.is_empty() {
                continue;
            }
            if tool_calls.len() > 1 {
                warn!("Agent attempted parallel tool calls. Rejecting.");
                
                // Important: OpenAI format requires that we append the tool calls message FIRST,
                // then respond to each tool_call_id.
                // The assistant message with the tool calls is already appended above via:
                // `messages.push(response_msg.clone());`
                
                for tc in tool_calls {
                    messages.push(Message::tool_response(
                        &tc.id,
                        "Error: Parallel tool calls are not supported. Please call one tool at a time."
                    ));
                }
                continue;
            }

            let tc = &tool_calls[0];
            let args: serde_json::Value = serde_json::from_str(&tc.function.arguments).unwrap_or_default();

            match tc.function.name.as_str() {
                "clone_repo" => {
                    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    // Check if already cloned
                    let check = sandbox.exec_command(&["sh", "-c", "test -d /workspace/.git && echo 'exists' || echo 'missing'"]).await?;
                    if check.stdout.trim() == "exists" {
                        info!("Repository already exists at /workspace.");
                        messages.push(Message::tool_response(&tc.id, "Repository already exists at /workspace. Skip clone and use read_file/search_code directly."));
                    } else {
                        info!("Cloning repo: {}", url);
                        let output = sandbox.exec_command(&["git", "clone", url, "/workspace"]).await?;
                        if output.success {
                            messages.push(Message::tool_response(&tc.id, "Repository cloned successfully into /workspace."));
                        } else {
                            messages.push(Message::tool_response(&tc.id, format!("Clone failed: {}", output.stderr)));
                        }
                    }
                }
                "read_file" => {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
                    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(200).min(200);
                    
                    // === LARGE FILE GUARD #1: Check file size ===
                    let size_script = format!("wc -c < /workspace/{} 2>/dev/null || echo 0", path);
                    let size_out = sandbox.exec_command(&["sh", "-c", &size_script]).await?;
                    let file_size: u64 = size_out.stdout.trim().parse().unwrap_or(0);
                    if file_size > 200_000 {
                        messages.push(Message::tool_response(
                            &tc.id,
                            format!("REJECTED: File '{}' is {} bytes (>200KB). Too large to read. Use `search_code` with a keyword to find relevant lines instead.", path, file_size)
                        ));
                        continue;
                    }
                    
                    // === LARGE FILE GUARD #2: Track repeated reads of same file ===
                    if path == last_read_path {
                        same_file_reads += 1;
                    } else {
                        same_file_reads = 1;
                        last_read_path = path.to_string();
                    }
                    if same_file_reads >= 3 {
                        warn!("Agent read same file '{}' {} times. Injecting SYSTEM INTERVENTION.", path, same_file_reads);
                        messages.push(Message::tool_response(
                            &tc.id,
                            format!("Warning: you've read this file multiple times. Consider using search_code on specific keywords instead.")
                        ));
                        messages.push(Message::user(format!(
                            "[SYSTEM INTERVENTION] You've read '{}' {} times without progressing. \
                            This file is likely too large or not the right source. Try `search_code` on specific keywords like 'validation', 'check', 'constraint', or 'error' instead.",
                            path, same_file_reads
                        )));
                        same_file_reads = 0;
                        continue;
                    }
                    
                    info!("Reading file: {}", path);
                    let start_line = offset + 1;
                    let read_limit = limit.min(500);
                    let script = format!("tail -n +{} /workspace/{} | head -n {}", start_line, path, read_limit);
                    let output = sandbox.exec_command(&["sh", "-c", &script]).await?;
                    if output.success {
                        let stdout = output.stdout;
                        let line_count = stdout.lines().count();
                        if line_count > 500 {
                            let truncated: String = stdout.lines().take(500).collect::<Vec<_>>().join("\n");
                            messages.push(Message::tool_response(
                                &tc.id,
                                format!("{}\n... (truncated to 500 lines out of {} lines. Use search_code to find specific patterns.)", truncated, line_count)
                            ));
                        } else {
                            messages.push(Message::tool_response(&tc.id, stdout));
                        }
                    } else {
                        messages.push(Message::tool_response(&tc.id, format!("Read failed: {}", output.stderr)));
                    }
                }
                "search_code" => {
                    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    
                    info!("Searching code for: {}", pattern);
                    let escaped_pattern = pattern.replace('\'', "'\\''");
                    let escaped_path = path.replace('\'', "'\\''");
                    let script = format!("cd /workspace && grep -rn '{}' '{}'", escaped_pattern, escaped_path);
                    let output = sandbox.exec_command(&["sh", "-c", &script]).await?;
                    
                    let mut stdout = output.stdout;
                    if stdout.lines().count() > 100 {
                        stdout = stdout.lines().take(100).collect::<Vec<_>>().join("\n");
                        stdout.push_str("\n... (truncated to 100 lines)");
                    }
                    
                    if output.success || (!stdout.is_empty()) {
                        messages.push(Message::tool_response(&tc.id, stdout));
                    } else {
                        messages.push(Message::tool_response(&tc.id, format!("Search failed or no results: {}", output.stderr)));
                    }
                }
                "crawl_url" => {
                    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
                    match crawl_docs(url).await {
                        Ok(crate::agent::tools::ToolResult::Success(markdown)) => {
                            let mut md = markdown;
                            if md.len() > 15000 {
                                md.truncate(15000);
                                md.push_str("\n... (truncated to 15000 chars)");
                            }
                            messages.push(Message::tool_response(&tc.id, md));
                        }
                        Ok(crate::agent::tools::ToolResult::Error(e)) => {
                            messages.push(Message::tool_response(&tc.id, format!("Crawl failed: {}", e)));
                        }
                        Err(e) => {
                            messages.push(Message::tool_response(&tc.id, format!("Crawl failed: {}", e)));
                        }
                    }
                }
                "submit_contract" => {
                    info!("Agent submitted contract for {}.", endpoint_name);
                    let api_endpoint = args.get("api_endpoint").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let doc_url = args.get("doc_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let assertions_val = args.get("assertions").and_then(|v| v.as_array());
                    
                    let mut assertions: Vec<String> = Vec::new();
                    if let Some(arr) = assertions_val {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                assertions.push(s.trim().to_string());
                            }
                        }
                    }

                    let type_constraints = parse_type_constraints_from_json(&args);
                    let range_constraints = parse_range_constraints_from_json(&args);
                    let required_params = parse_string_array_from_json(&args, "required_params");
                    let enum_values = parse_enum_values_from_json(&args);

                    if !api_endpoint.is_empty() && !assertions.is_empty() {
                        info!("Contract for {}: {} assertions, {} type_constraints, {} range_constraints, {} required, {} enum",
                            endpoint_name, assertions.len(), type_constraints.len(),
                            range_constraints.len(), required_params.len(), enum_values.len());
                    }

                    if api_endpoint.is_empty() {
                        messages.push(Message::tool_response(&tc.id, "REJECTED: api_endpoint is empty."));
                        continue;
                    }
                    if assertions.is_empty() && type_constraints.is_empty() && range_constraints.is_empty() {
                        messages.push(Message::tool_response(&tc.id, "REJECTED: no constraints provided. Add at least 1 assertion, type_constraint, or range_constraint."));
                        continue;
                    }

                    info!("Contract for {}: {} assertions", endpoint_name, assertions.len());

                    for param in &required_params {
                        if !assertions.iter().any(|a| a.contains(param)) {
                            assertions.push(format!("[IMPLICIT:REQUIRED] {} is required", param));
                        }
                    }
                    for (param, values) in &enum_values {
                        if !assertions.iter().any(|a| a.contains(param)) {
                            assertions.push(format!("[IMPLICIT:ENUM] {} must be one of {:?}", param, values));
                        }
                    }
                    
                    let contract = StructuredContract {
                        api_endpoint,
                        doc_url,
                        assertions,
                        type_constraints,
                        range_constraints,
                        state_constraints: vec![],
                        state_invariants: vec![],
                        behavioral_contracts: vec![],
                        rejection_policies: HashMap::new(),
                        nested_params: HashMap::new(),
                    };
                    
                    return Ok(contract);
                }
                _ => {
                    messages.push(Message::tool_response(&tc.id, "Unknown tool."));
                    consecutive_same_errors += 1;
                }
            }

            if consecutive_same_errors >= 3 {
                warn!("Agent hit the same error 3 times. Injecting SYSTEM INTERVENTION.");
                messages.push(Message::user("[SYSTEM INTERVENTION] You are repeating the same failed actions. Please review your strategy and try a different tool or path."));
                consecutive_same_errors = 0;
            }
        } else {
            messages.push(Message::user("Please use the tools to explore knowledge or submit the contract."));
        }
    }

    // Fallback: Agent never submitted a contract. Construct minimal one.
    warn!("Knowledge Agent for '{}' exceeded max turns without submitting. Creating fallback contract.", endpoint_name);
    Ok(StructuredContract {
        api_endpoint: endpoint_name.to_string(),
        doc_url: endpoint_docs_url.to_string(),
        assertions: vec![format!("[{}] parameter validation for {}", endpoint_name, endpoint_api_path)],
        type_constraints: vec![],
        range_constraints: vec![],
        state_constraints: vec![],
        state_invariants: vec![],
        behavioral_contracts: vec![],
        rejection_policies: HashMap::new(),
        nested_params: HashMap::new(),
    })
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KaContractJson {
    #[serde(default)]
    api_endpoint: Option<String>,
    #[serde(default)]
    doc_url: Option<String>,
    #[serde(default)]
    assertions: Vec<String>,
    #[serde(default)]
    type_constraints: Vec<KaTypeConstraint>,
    #[serde(default)]
    range_constraints: Vec<KaRangeConstraint>,
    #[serde(default)]
    required_params: Vec<String>,
    #[serde(default)]
    enum_values: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    state_constraints: Vec<KaStateConstraint>,
    #[serde(default)]
    state_invariants: Vec<serde_json::Value>,
    #[serde(default)]
    behavioral_contracts: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct KaTypeConstraint {
    param_name: String,
    expected_type: String,
}

#[derive(Debug, Deserialize)]
struct KaRangeConstraint {
    param_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct KaStateConstraint {
    #[serde(default)]
    description: String,
    #[serde(default)]
    determinism: Option<String>,
}

fn parse_structured_contract_from_text(
    text: &str,
    endpoint_name: &str,
    endpoint_docs_url: &str,
) -> Result<StructuredContract> {
    let json_str = extract_json_from_text(text);
    let parsed: KaContractJson = serde_json::from_str(&json_str)
        .map_err(|e| anyhow::anyhow!("JSON parse error: {}", e))?;

    let type_constraints: Vec<TypeConstraint> = parsed.type_constraints.iter().map(|tc| {
        TypeConstraint {
            param_name: tc.param_name.clone(),
            expected_type: tc.expected_type.clone(),
            violation_examples: vec![],
        }
    }).collect();

    let range_constraints: Vec<RangeConstraint> = parsed.range_constraints.iter().map(|rc| {
        RangeConstraint {
            param_name: rc.param_name.clone(),
            description: rc.description.clone(),
            min: rc.min,
            max: rc.max,
            violation_examples: vec![],
        }
    }).collect();

    let state_constraints: Vec<StateConstraint> = parsed.state_constraints.iter().map(|sc| {
        let det = match sc.determinism.as_deref() {
            Some("deterministic") => Determinism::Deterministic,
            _ => Determinism::NonDeterministic,
        };
        StateConstraint {
            description: sc.description.clone(),
            determinism: det,
            setup_script_template: None,
        }
    }).collect();

    let mut assertions = parsed.assertions;
    for param in &parsed.required_params {
        if !assertions.iter().any(|a| a.contains(param)) {
            assertions.push(format!("[IMPLICIT:REQUIRED] {} is required", param));
        }
    }
    for (param, values) in &parsed.enum_values {
        if !assertions.iter().any(|a| a.contains(param)) {
            assertions.push(format!("[IMPLICIT:ENUM] {} must be one of {:?}", param, values));
        }
    }

    Ok(StructuredContract {
        api_endpoint: parsed.api_endpoint.unwrap_or_else(|| endpoint_name.to_string()),
        doc_url: parsed.doc_url.unwrap_or_else(|| endpoint_docs_url.to_string()),
        assertions,
        type_constraints,
        range_constraints,
        state_constraints,
        state_invariants: vec![],
        behavioral_contracts: vec![],
        rejection_policies: HashMap::new(),
        nested_params: HashMap::new(),
    })
}

fn extract_json_from_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        return trimmed.to_string();
    }
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    if let Some(start) = trimmed.find('{') {
        let mut depth = 0i32;
        for (i, c) in trimmed[start..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return trimmed[start..start + i + 1].to_string();
                    }
                }
                _ => {}
            }
        }
    }
    trimmed.to_string()
}

fn parse_type_constraints_from_json(args: &serde_json::Value) -> Vec<TypeConstraint> {
    let mut result = Vec::new();
    if let Some(arr) = args.get("type_constraints").and_then(|v| v.as_array()) {
        for item in arr {
            let param_name = item.get("param_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let expected_type = item.get("expected_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if !param_name.is_empty() && !expected_type.is_empty() {
                result.push(TypeConstraint {
                    param_name,
                    expected_type,
                    violation_examples: vec![],
                });
            }
        }
    }
    result
}

fn parse_range_constraints_from_json(args: &serde_json::Value) -> Vec<RangeConstraint> {
    let mut result = Vec::new();
    if let Some(arr) = args.get("range_constraints").and_then(|v| v.as_array()) {
        for item in arr {
            let param_name = item.get("param_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let min = item.get("min").and_then(|v| v.as_f64());
            let max = item.get("max").and_then(|v| v.as_f64());
            if !param_name.is_empty() {
                result.push(RangeConstraint {
                    param_name,
                    description,
                    min,
                    max,
                    violation_examples: vec![],
                });
            }
        }
    }
    result
}

fn parse_string_array_from_json(args: &serde_json::Value, key: &str) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(arr) = args.get(key).and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                result.push(s.to_string());
            }
        }
    }
    result
}

fn parse_enum_values_from_json(args: &serde_json::Value) -> std::collections::HashMap<String, Vec<String>> {
    let mut result = std::collections::HashMap::new();
    if let Some(obj) = args.get("enum_values").and_then(|v| v.as_object()) {
        for (key, val) in obj {
            if let Some(arr) = val.as_array() {
                let values: Vec<String> = arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if !values.is_empty() {
                    result.insert(key.clone(), values);
                }
            }
        }
    }
    result
}

pub fn knowledge_contracts_to_store(
    contracts: &[StructuredContract],
    target: &str,
    version: &str,
) -> ContractStore {
    let mut store = ContractStore::from_structured_contracts(
        target,
        version,
        contracts,
        ConstraintSource::ExplicitDoc,
        Confidence::Medium,
    );

    for contract in contracts {
        let endpoint = &contract.api_endpoint;
        let mut required: Vec<String> = Vec::new();
        let mut enum_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        for assertion in &contract.assertions {
            if let Some(rest) = assertion.strip_prefix("[IMPLICIT:REQUIRED]") {
                let param = rest.trim().split_whitespace().next().unwrap_or("").trim_end_matches(':');
                if !param.is_empty() && !required.contains(&param.to_string()) {
                    required.push(param.to_string());
                }
            }
            if let Some(rest) = assertion.strip_prefix("[IMPLICIT:ENUM]") {
                if let Some(bracket_start) = rest.find('[') {
                    let before_bracket = rest[..bracket_start].trim();
                    let param = before_bracket.split_whitespace().next().unwrap_or("").trim_end_matches(':');
                    if !param.is_empty() {
                        let after_bracket = &rest[bracket_start + 1..];
                        let bracket_end = after_bracket.find(']').unwrap_or(after_bracket.len());
                        let bracket_content = &after_bracket[..bracket_end];
                        let values: Vec<String> = bracket_content
                            .split(',')
                            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
                            .filter(|v| !v.is_empty())
                            .collect();
                        if !values.is_empty() {
                            enum_map.insert(param.to_string(), values);
                        }
                    }
                }
            }
        }

        if !required.is_empty() {
            store.set_required_params(endpoint, required);
        }
        for (param, values) in enum_map {
            store.set_enum_values(&param, values);
        }
    }

    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_structured_contract_from_text() {
        let json = r#"{
            "api_endpoint": "search",
            "doc_url": "https://milvus.io/docs",
            "assertions": ["[TYPE] limit must be integer", "[IMPLICIT:REQUIRED] collectionName is required"],
            "type_constraints": [{"param_name": "limit", "expected_type": "integer"}],
            "range_constraints": [{"param_name": "limit", "description": "limit range", "min": 1, "max": 16384}],
            "required_params": ["collectionName", "data"],
            "enum_values": {"metricType": ["COSINE", "L2", "IP"]},
            "state_constraints": [],
            "state_invariants": [],
            "behavioral_contracts": []
        }"#;

        let contract = parse_structured_contract_from_text(json, "search", "https://milvus.io/docs").unwrap();

        assert_eq!(contract.api_endpoint, "search");
        assert_eq!(contract.type_constraints.len(), 1);
        assert_eq!(contract.type_constraints[0].param_name, "limit");
        assert_eq!(contract.range_constraints.len(), 1);
        assert_eq!(contract.range_constraints[0].min, Some(1.0));
        assert!(contract.assertions.iter().any(|a| a.starts_with("[IMPLICIT:REQUIRED]")));
        assert!(contract.assertions.iter().any(|a| a.starts_with("[IMPLICIT:ENUM]")));
    }

    #[test]
    fn test_parse_structured_contract_with_markdown_wrapper() {
        let text = "```json\n{\"api_endpoint\": \"create\", \"assertions\": [\"[TYPE] dim must be integer\"], \"type_constraints\": [{\"param_name\": \"dim\", \"expected_type\": \"integer\"}], \"range_constraints\": [], \"required_params\": [], \"enum_values\": {}, \"state_constraints\": [], \"state_invariants\": [], \"behavioral_contracts\": []}\n```";
        let contract = parse_structured_contract_from_text(text, "create", "").unwrap();
        assert_eq!(contract.api_endpoint, "create");
        assert_eq!(contract.type_constraints.len(), 1);
    }

    #[test]
    fn test_extract_json_from_text_pure_json() {
        let json = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_text(json), json);
    }

    #[test]
    fn test_extract_json_from_text_markdown() {
        let text = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_text(text), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_knowledge_contracts_to_store() {
        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "https://milvus.io".to_string(),
            assertions: vec![
                "[TYPE] limit must be integer".to_string(),
                "[RANGE] limit must be > 0".to_string(),
                "[IMPLICIT:REQUIRED] collectionName is required".to_string(),
                "[IMPLICIT:ENUM] metricType must be one of [COSINE, L2, IP]".to_string(),
            ],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![RangeConstraint {
                param_name: "limit".to_string(),
                description: "limit range".to_string(),
                min: Some(1.0),
                max: Some(16384.0),
                violation_examples: vec![],
            }],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: HashMap::new(),
            nested_params: HashMap::new(),
        };

        let store = knowledge_contracts_to_store(&[contract], "milvus", "2.4");

        assert_eq!(store.target, "milvus");
        assert_eq!(store.type_constraints.len(), 1);
        assert_eq!(store.range_constraints.len(), 1);
        assert!(store.required_params.contains_key("search"));
        assert!(store.required_params["search"].contains(&"collectionName".to_string()));
        assert!(store.enum_values.contains_key("metricType"));
        assert!(store.enum_values["metricType"].contains(&"COSINE".to_string()));
    }

    #[test]
    fn test_knowledge_contracts_to_store_source_and_confidence() {
        let contract = StructuredContract {
            api_endpoint: "search".to_string(),
            doc_url: "".to_string(),
            assertions: vec![],
            type_constraints: vec![TypeConstraint {
                param_name: "limit".to_string(),
                expected_type: "integer".to_string(),
                violation_examples: vec![],
            }],
            range_constraints: vec![],
            state_constraints: vec![],
            state_invariants: vec![],
            behavioral_contracts: vec![],
            rejection_policies: HashMap::new(),
            nested_params: HashMap::new(),
        };

        let store = knowledge_contracts_to_store(&[contract], "milvus", "2.4");

        assert_eq!(store.type_constraints[0].source, ConstraintSource::ExplicitDoc);
        assert_eq!(store.type_constraints[0].confidence, Confidence::Medium);
    }

    #[test]
    fn test_parse_structured_contract_invalid_json() {
        let result = parse_structured_contract_from_text("not json at all", "search", "https://docs.io");
        assert!(result.is_err(), "Invalid JSON should return error");
    }

    #[test]
    fn test_parse_structured_contract_missing_fields() {
        // Minimal valid JSON with missing optional fields defaulting
        let json = r#"{"api_endpoint": "create"}"#;
        let contract = parse_structured_contract_from_text(json, "create", "https://docs.io").unwrap();
        assert_eq!(contract.api_endpoint, "create");
        assert!(contract.assertions.is_empty());
        assert!(contract.type_constraints.is_empty());
        assert!(contract.range_constraints.is_empty());
        assert!(contract.behavioral_contracts.is_empty());
    }

    #[test]
    fn test_parse_type_constraints_from_json() {
        let args: serde_json::Value = serde_json::json!({
            "type_constraints": [
                {"param_name": "limit", "expected_type": "integer"},
                {"param_name": "collectionName", "expected_type": "string"},
                {"param_name": "", "expected_type": "integer"},  // empty param_name: skipped
                {"param_name": "dim", "expected_type": ""},      // empty expected_type: skipped
            ]
        });
        let result = parse_type_constraints_from_json(&args);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].param_name, "limit");
        assert_eq!(result[0].expected_type, "integer");
        assert_eq!(result[1].param_name, "collectionName");
        assert_eq!(result[1].expected_type, "string");
    }

    #[test]
    fn test_parse_range_constraints_from_json() {
        let args: serde_json::Value = serde_json::json!({
            "range_constraints": [
                {"param_name": "limit", "description": "limit range", "min": 1, "max": 16384},
                {"param_name": "offset", "description": "offset range", "min": 0},
                {"param_name": "", "description": "empty name"},  // empty param_name: skipped
            ]
        });
        let result = parse_range_constraints_from_json(&args);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].param_name, "limit");
        assert_eq!(result[0].min, Some(1.0));
        assert_eq!(result[0].max, Some(16384.0));
        assert_eq!(result[1].param_name, "offset");
        assert_eq!(result[1].min, Some(0.0));
        assert!(result[1].max.is_none());
    }

    #[test]
    fn test_parse_enum_values_from_json() {
        let args: serde_json::Value = serde_json::json!({
            "enum_values": {
                "metricType": ["COSINE", "L2", "IP"],
                "indexType": ["IVF_FLAT", "HNSW"]
            }
        });
        let result = parse_enum_values_from_json(&args);
        assert_eq!(result.len(), 2);
        assert!(result.contains_key("metricType"));
        assert_eq!(result["metricType"].len(), 3);
        assert!(result.contains_key("indexType"));
        assert_eq!(result["indexType"].len(), 2);
    }

    #[test]
    fn test_parse_enum_values_from_json_empty() {
        let args: serde_json::Value = serde_json::json!({"enum_values": {}});
        let result = parse_enum_values_from_json(&args);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_json_from_text_code_block() {
        // Code block without json tag
        let text = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_text(text), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_from_text_embedded_json() {
        // JSON embedded in prose text
        let text = "Here is the result:\n{\"api_endpoint\": \"search\", \"assertions\": []}\nDone.";
        let result = extract_json_from_text(text);
        assert!(result.contains("\"api_endpoint\""));
        assert!(result.contains("\"search\""));
    }

    #[test]
    fn test_parse_string_array_from_json() {
        let args: serde_json::Value = serde_json::json!({
            "required_params": ["collectionName", "data", "vector"],
            "other_field": ["not_this"]
        });
        let result = parse_string_array_from_json(&args, "required_params");
        assert_eq!(result, vec!["collectionName", "data", "vector"]);
    }

    #[test]
    fn test_parse_string_array_from_json_missing_key() {
        let args: serde_json::Value = serde_json::json!({"other_field": ["a"]});
        let result = parse_string_array_from_json(&args, "required_params");
        assert!(result.is_empty());
    }
}
