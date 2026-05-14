use super::tools::{
    get_clone_repo_tool, get_crawl_url_tool, get_read_file_tool, get_search_code_tool,
    get_submit_contract_tool, crawl_docs
};
use super::llm::{DeepSeekClient, Message};
use crate::contract::schema::StructuredContract;
use crate::sandbox::manager::Sandbox;
use anyhow::Result;
use tracing::{info, warn};

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

    let system_prompt = format!(
        "You are an expert Security Knowledge Agent for the '{}' database.\n\
        Target Repo: {}\n\
        Repo is at /workspace.\n\n\
        TASK: Extract constraints for endpoint '{}' ({}). Docs: {}\n\n\
        Constraints use prefixes:\n\
        - [TYPE] for types: '[TYPE] limit must be integer'\n\
        - [RANGE] for ranges: '[RANGE] limit must be > 0'\n\
        - [STATE] for state: '[STATE:deterministic] collection must exist'\n\
        - [BEHAVIOR:STATE] for state consistency: '[BEHAVIOR:STATE] upsert N points -> count == N'\n\
        - [BEHAVIOR:SEMANTIC] for semantic correctness: '[BEHAVIOR:SEMANTIC] search results sorted by score descending'\n\
        - [BEHAVIOR:INTERFACE] for interface consistency: '[BEHAVIOR:INTERFACE] gRPC and REST return same results'\n\
        - [BEHAVIOR:DIAGNOSTIC] for diagnostic quality: '[BEHAVIOR:DIAGNOSTIC] error message mentions parameter name'\n\n\
        Crawl docs + grep 1-2 times, then call submit_contract. Submit by turn 3.",
        target_name, target_repo_url,
        endpoint_name, endpoint_api_path, endpoint_docs_url
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
        // Turn 3: force LLM summary
        if turn == 3 {
            messages.push(Message::user(format!(
                "[SYSTEM] DO NOT use any tools. Just reply in plain text.\n\
                List ALL constraints you found for endpoint '{}' as assertions with prefixes:\n\
                One per line, format: [TYPE] description or [RANGE] description or [STATE:deterministic] description\n\
                Example:\n\
                [TYPE] limit must be integer\n\
                [RANGE] limit must be > 0\n\
                [STATE:deterministic] collection must exist\n\n\
                Just list them. No explanations.",
                endpoint_name
            )));
        }
        // Turn 4: parse LLM summary and auto-submit, or fallback
        if turn == 4 {
            // Try to parse tagged assertions from the last LLM text response
            let mut parsed: Vec<String> = Vec::new();
            for line in last_assistant_text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("[TYPE]") || trimmed.starts_with("[RANGE]") || trimmed.starts_with("[STATE]") {
                    parsed.push(trimmed.to_string());
                }
            }
            if !parsed.is_empty() {
                info!("B1: Parsed {} tagged assertions from LLM summary for '{}'.", parsed.len(), endpoint_name);
                return Ok(StructuredContract {
                    api_endpoint: endpoint_name.to_string(),
                    doc_url: endpoint_docs_url.to_string(),
                    assertions: parsed,
                    type_constraints: vec![],
                    range_constraints: vec![],
                    state_constraints: vec![],
                    state_invariants: vec![],
                    behavioral_contracts: vec![],
                });
            } else {
                warn!("B1: Could not parse tagged assertions from LLM summary for '{}'. Falling back.", endpoint_name);
                return Ok(StructuredContract {
                    api_endpoint: endpoint_name.to_string(),
                    doc_url: endpoint_docs_url.to_string(),
                    assertions: vec![format!("[{}] parameter validation for {}", endpoint_name, endpoint_api_path)],
                    type_constraints: vec![],
                    range_constraints: vec![],
                    state_constraints: vec![],
                    state_invariants: vec![],
                    behavioral_contracts: vec![],
                });
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

                    // Simple validation
                    if api_endpoint.is_empty() {
                        messages.push(Message::tool_response(&tc.id, "REJECTED: api_endpoint is empty."));
                        continue;
                    }
                    if assertions.is_empty() {
                        messages.push(Message::tool_response(&tc.id, "REJECTED: assertions array is empty. Add at least 1 constraint string."));
                        continue;
                    }

                    info!("Contract for {}: {} assertions", endpoint_name, assertions.len());
                    
                    // Build contract — layered fields will be filled by post-processing in main.rs
                    let contract = StructuredContract {
                        api_endpoint,
                        doc_url,
                        assertions,
                        type_constraints: vec![],
                        range_constraints: vec![],
                        state_constraints: vec![],
                        state_invariants: vec![],
                        behavioral_contracts: vec![],
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
    })
}
