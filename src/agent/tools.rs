use anyhow::Result;
use serde_json::json;
use tracing::info;
use crate::crawler::engine::{Crawler, ChromiumCrawler, ReqwestCrawler};
use crate::crawler::parser::clean_content;
use crate::agent::llm::{Tool, Function};
use crate::sandbox::manager::{Sandbox, SidecarSpec};

pub enum ToolResult {
    Success(String),
    Error(String),
}

pub async fn execute_test_script(
    code: &str,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
    sidecars: &[SidecarSpec],
    db_env: &[(String, String)],
    db_command: &[String],
    auth_header: &str,
) -> Result<(String, Sandbox, String, bool)> {
    info!("Creating fresh sandbox for script execution...");
    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port, sidecars, db_env, db_command).await?;
    let db_url = crate::infra::build_db_url(sandbox.db_host.as_ref().ok_or_else(|| anyhow::anyhow!("sandbox db_host missing"))?, db_port);
    let script_code = code
        .replace("{{TESTVDB_DB_URL}}", &db_url)
        .replace("{{TESTVDB_AUTH_HEADER}}", auth_header);
    
    info!("Executing script in sandbox runner...");
    let output = sandbox.exec_script(&script_code, &[("TESTVDB_DB_URL", &db_url)]).await?;
    let normalized_stdout = crate::agent::classifier::normalize_observed_output(&output.stdout);
    let normalized_stderr = crate::agent::classifier::normalize_observed_output(&output.stderr);
    
    let mut result_str = String::new();
    result_str.push_str("STDOUT:\n");
    result_str.push_str(&normalized_stdout);
    result_str.push_str("\nSTDERR:\n");
    result_str.push_str(&normalized_stderr);
    
    Ok((result_str, sandbox, db_url, output.success))
}

pub async fn execute_test_in_sandbox(
    code: &str,
    sandbox: &Sandbox,
    db_port: u16,
    auth_header: &str,
) -> Result<(String, String, bool)> {
    info!("Reusing existing sandbox for script execution...");
    let db_url = crate::infra::build_db_url(sandbox.db_host.as_ref().unwrap_or(&"localhost".to_string()), db_port);
    let script_code = code
        .replace("{{TESTVDB_DB_URL}}", &db_url)
        .replace("{{TESTVDB_AUTH_HEADER}}", auth_header);
    
    let output = sandbox.exec_script(&script_code, &[("TESTVDB_DB_URL", &db_url)]).await?;
    let normalized_stdout = crate::agent::classifier::normalize_observed_output(&output.stdout);
    let normalized_stderr = crate::agent::classifier::normalize_observed_output(&output.stderr);
    
    let mut result_str = String::new();
    result_str.push_str("STDOUT:\n");
    result_str.push_str(&normalized_stdout);
    result_str.push_str("\nSTDERR:\n");
    result_str.push_str(&normalized_stderr);
    
    Ok((result_str, db_url, output.success))
}

pub fn get_execute_test_script_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_test_script".to_string(),
            description: Some("Executes a Python test script. By default REUSES the existing sandbox (preserving database state from previous calls). Set fresh_sandbox=true to start a clean database. Use {{TESTVDB_DB_URL}} as the database URL placeholder. For behavioral tests, keep fresh_sandbox=false so you can build on previous state (e.g., insert data in one call, then verify count in the next).".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The complete Python script to execute."
                    },
                    "fresh_sandbox": {
                        "type": "boolean",
                        "description": "If true, creates a fresh sandbox with a clean database. If false (default), reuses the existing sandbox preserving all data. Set to true only for the first call or when you need a clean state."
                    }
                },
                "required": ["code"]
            }),
        },
    }
}

pub fn get_execute_stateful_test_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_stateful_test".to_string(),
            description: Some("STATEFUL MODEL TESTING. Tests multi-step API sequences with automatic state verification. This tool verifies that the actual database state matches the expected model state after EACH operation. Use this to find STATE_LOGIC_VIOLATION bugs that deterministic generators cannot detect.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_name": {
                        "type": "string",
                        "description": "A descriptive name for this stateful test."
                    },
                    "pattern_category": {
                        "type": "string",
                        "enum": ["count_consistency", "data_visibility", "state_residual", "idempotency", "search_correctness", "partition_isolation", "alias_state", "index_state"],
                        "description": "The state interaction pattern being tested."
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "description": "The API endpoint path (e.g., '/v2/vectordb/collections/create')."
                                },
                                "params": {
                                    "type": "object",
                                    "description": "Request parameters for this step."
                                },
                                "expect_success": {
                                    "type": "boolean",
                                    "description": "Whether this step should succeed."
                                },
                                "state_check": {
                                    "type": "object",
                                    "properties": {
                                        "method": {
                                            "type": "string",
                                            "enum": ["describe_collection", "query_entities", "search_results", "list_collections", "get_index"],
                                            "description": "The method to verify state after this step."
                                        },
                                        "expected": {
                                            "type": "object",
                                            "description": "Expected state values (e.g., {\"rowCount\": 100}, {\"exists\": false}, {\"resultCount\": 5}, {\"distancesAscending\": true})."
                                        }
                                    },
                                    "required": ["method", "expected"],
                                    "description": "REQUIRED. State verification to perform after this step. This is what makes stateful testing unique — without it, the tool degrades to a simple sequence test."
                                }
                            },
                            "required": ["action", "params", "expect_success", "state_check"]
                        },
                        "description": "Ordered list of steps to execute."
                    },
                    "invariant": {
                        "type": "string",
                        "description": "The final invariant to verify after all steps."
                    }
                },
                "required": ["test_name", "pattern_category", "steps"]
            }),
        },
    }
}



pub fn get_execute_differential_test_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_differential_test".to_string(),
            description: Some("DIFFERENTIAL TESTING. Compare results of two API calls on the same data to find SEARCH_CORRECTNESS or CROSS_ENDPOINT_INCONSISTENCY defects. For SEARCH_CORRECTNESS: search with different params (e.g. searchParams ef=1 vs ef=100) — results SHOULD DIFFER, identical = defect. For CROSS_ENDPOINT_INCONSISTENCY: same data via different endpoints (search vs query) — counts SHOULD MATCH, different = defect. The tool automatically compares and flags mismatches with the correct [DEFECT:...] marker.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_type": {
                        "type": "string",
                        "enum": ["search_correctness", "cross_endpoint_consistency"],
                        "description": "search_correctness: same search with different params should return different results (identical = defect). cross_endpoint_consistency: same data via different endpoints should return consistent counts (mismatch = defect)."
                    },
                    "setup_code": {
                        "type": "string",
                        "description": "Python code to set up preconditions: create collection, insert 10+ entities, create HNSW index, load. Must print 'SETUP_OK' on success. Use BASE and HEADERS variables already defined."
                    },
                    "call_a_label": {
                        "type": "string",
                        "description": "Label for call A (e.g. 'search ef=1' or 'search count')"
                    },
                    "call_a_code": {
                        "type": "string",
                        "description": "Python code for call A. Must set variable 'result_a' to the value to compare (e.g. top-1 distance float, or result count int)."
                    },
                    "call_b_label": {
                        "type": "string",
                        "description": "Label for call B (e.g. 'search ef=100' or 'query count')"
                    },
                    "call_b_code": {
                        "type": "string",
                        "description": "Python code for call B. Must set variable 'result_b' to the value to compare (e.g. top-1 distance float, or result count int)."
                    },
                    "comparison": {
                        "type": "string",
                        "enum": ["should_differ", "should_match"],
                        "description": "should_differ: result_a != result_b expected (e.g. different ef → different distances). should_match: result_a == result_b expected (e.g. search count should equal query count)."
                    }
                },
                "required": ["test_type", "setup_code", "call_a_label", "call_a_code", "call_b_label", "call_b_code", "comparison"]
            }),
        },
    }
}

pub fn get_coverage_report_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "get_coverage_report".to_string(),
            description: Some("Returns the current API coverage report showing which endpoint/parameter combinations have been tested and which remain untested. Use this to guide your exploration direction.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
    }
}

pub fn get_submit_mre_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "submit_mre".to_string(),
            description: Some("Submits a Minimum Reproducible Example (MRE) script when a defect is successfully triggered. Do not call this unless you have verified the bug via execute_test_script.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The final Python script that reproduces the bug."
                    },
                    "defect_type": {
                        "type": "string",
                        "description": "The classification: IllegalSuccess, ParamIgnored, TYPE_VIOLATION, RANGE_VIOLATION, STATE_VIOLATION, ServerCrash, Timeout."
                    },
                    "surviving_assertions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "A list of assertion statements that failed to properly reject the bad input."
                    }
                },
                "required": ["code", "defect_type", "surviving_assertions"]
            }),
        },
    }
}

pub fn get_clone_repo_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "clone_repo".to_string(),
            description: Some("Clones a git repository into the knowledge worker sandbox for analysis.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL of the git repository to clone (e.g., https://github.com/qdrant/qdrant.git)."
                    }
                },
                "required": ["url"]
            }),
        },
    }
}

pub fn get_read_file_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "read_file".to_string(),
            description: Some("Reads lines from a file in the cloned repository. Max 200 lines returned per call.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The relative path to the file inside the repository."
                    },
                    "offset": {
                        "type": "integer",
                        "description": "The line number to start reading from (0-indexed)."
                    },
                    "limit": {
                        "type": "integer",
                        "description": "The number of lines to read (max 200)."
                    }
                },
                "required": ["path"]
            }),
        },
    }
}

pub fn get_search_code_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "search_code".to_string(),
            description: Some("Searches for a keyword or regex pattern in the cloned repository using grep.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "The regex pattern to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "The relative directory or file to search within (use '.' for the whole repo)."
                    }
                },
                "required": ["pattern", "path"]
            }),
        },
    }
}

pub fn get_crawl_url_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "crawl_url".to_string(),
            description: Some("Crawls a web URL and returns its content as cleaned Markdown. Useful for reading official documentation.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to crawl."
                    }
                },
                "required": ["url"]
            }),
        },
    }
}

pub fn get_submit_contract_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "submit_contract".to_string(),
            description: Some("Submits extracted validation constraints for one endpoint. assertions are plain strings with [TYPE]/[RANGE]/[STATE] prefixes.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "api_endpoint": {
                        "type": "string",
                        "description": "The endpoint name (e.g., 'search_points', 'create_collection')."
                    },
                    "doc_url": {
                        "type": "string",
                        "description": "The documentation URL used as reference."
                    },
                    "assertions": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Constraint strings with prefixes: [TYPE] for type, [RANGE] for range, [STATE] or [STATE:deterministic]/[STATE:non-deterministic] for state."
                    }
                },
                "required": ["api_endpoint", "doc_url", "assertions"]
            }),
        },
    }
}

pub async fn crawl_docs(url: &str) -> Result<ToolResult> {
    info!("Agent requested to crawl docs from URL: {}", url);
    
    let reqwest_crawler = ReqwestCrawler::new();
    let html = match reqwest_crawler.fetch_page(url).await {
        Ok(content) => content,
        Err(e) => {
            tracing::warn!("Reqwest crawler failed ({}). Falling back to Chromium.", e);
            let chromium_crawler = ChromiumCrawler::new();
            chromium_crawler.fetch_page(url).await?
        }
    };

    let mut markdown = clean_content(&html);
    if markdown.len() > 15000 {
        markdown.truncate(15000);
        markdown.push_str("\n... (truncated to 15000 chars)");
    }
    
    Ok(ToolResult::Success(markdown))
}

pub fn generate_state_check_code(method: &str, expected: &serde_json::Value, step_params: &serde_json::Value, plugin_style: crate::target::TargetStyle) -> String {
    let expected_str = serde_json::to_string(expected).unwrap_or_default();
    match method {
        "describe_collection" => {
            if matches!(plugin_style, crate::target::TargetStyle::Milvus) {
                let coll_name = step_params.get("collectionName").and_then(|v| v.as_str()).unwrap_or("unknown");
                format!(
                    "desc = api('/v2/vectordb/collections/describe', {{'collectionName': '{}'}})\n\
                     expected = {}\n\
                     if desc.get('code') == 0 and 'rowCount' in expected and desc.get('data', {{}}).get('rowCount', -1) != expected['rowCount']:\n\
                         print(f'[DEFECT: STATE_LOGIC_VIOLATION] rowCount mismatch: expected {{expected[\"rowCount\"]}}, got {{desc[\"data\"][\"rowCount\"]}}')\n\
                         sys.exit(1)\n",
                    coll_name, expected_str
                )
            } else {
                format!("# State check via {} adapted for non-Milvus target\n", method)
            }
        }
        "search_results" => {
            let mut code = String::new();
            if let Some(obj) = expected.as_object() {
                if obj.contains_key("distancesAscending") {
                    code.push_str("# Verify search result distances are monotonically ordered\n");
                    code.push_str("if isinstance(r, dict) and 'data' in r:\n");
                    code.push_str("    results = r['data'] if isinstance(r['data'], list) else r['data'].get('result', r['data'].get('hits', []))\n");
                    code.push_str("    if results and len(results) > 1:\n");
                    code.push_str("        coll_name = None\n");
                    if let Some(params_obj) = step_params.as_object() {
                        if let Some(cn) = params_obj.get("collectionName").and_then(|v| v.as_str()) {
                            code.push_str(&format!("        coll_name = '{}'\n", cn));
                        }
                    }
                    code.push_str("        metric_type = 'L2'\n");
                    code.push_str("        if coll_name:\n");
                    code.push_str("            try:\n");
                    code.push_str("                idx_resp = api('/v2/vectordb/collections/describe_index', {'collectionName': coll_name})\n");
                    code.push_str("                if isinstance(idx_resp, dict) and 'data' in idx_resp:\n");
                    code.push_str("                    idx_data = idx_resp['data']\n");
                    code.push_str("                    if isinstance(idx_data, list) and idx_data:\n");
                    code.push_str("                        metric_type = idx_data[0].get('metricType', 'L2')\n");
                    code.push_str("            except: pass\n");
                    code.push_str("        distances = [d.get('distance', d.get('score', 0)) for d in results if isinstance(d, dict)]\n");
                    code.push_str("        if len(distances) > 1:\n");
                    code.push_str("            if metric_type == 'IP':\n");
                    code.push_str("                is_valid = all(distances[i] >= distances[i+1] for i in range(len(distances)-1))\n");
                    code.push_str("                direction = 'non-increasing (IP)'\n");
                    code.push_str("            else:\n");
                    code.push_str("                is_valid = all(distances[i] <= distances[i+1] for i in range(len(distances)-1))\n");
                    code.push_str("                direction = 'non-decreasing (L2/COSINE)'\n");
                    code.push_str("            if not is_valid:\n");
                    code.push_str("                print(f'[DEFECT: SEARCH_CORRECTNESS] Distances not {}: {{distances}}')\n");
                    code.push_str("                sys.exit(1)\n");
                }
                if obj.contains_key("resultCount") {
                    code.push_str(&format!(
                        "expected_count = {}\n\
                         actual_count = len(r['data']) if isinstance(r, dict) and 'data' in r and isinstance(r['data'], list) else len(r['data'].get('result', r['data'].get('hits', []))) if isinstance(r, dict) and 'data' in r else -1\n\
                         if actual_count != expected_count:\n\
                             print(f'[DEFECT: SEARCH_CORRECTNESS] Result count mismatch: expected {{expected_count}}, got {{actual_count}}')\n\
                             sys.exit(1)\n",
                        expected.get("resultCount").and_then(|v| v.as_u64()).unwrap_or(0)
                    ));
                }
            }
            if code.is_empty() {
                code = format!("# State check via search_results: no recognized expected keys\n");
            }
            code
        }
        _ => {
            format!("# State check via {} ignored (unsupported method)\n", method)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_all_tools() -> Vec<Tool> {
        vec![
            get_execute_test_script_tool(),
            get_execute_stateful_test_tool(),
            get_coverage_report_tool(),
            get_submit_mre_tool(),
            get_clone_repo_tool(),
            get_read_file_tool(),
            get_search_code_tool(),
            get_crawl_url_tool(),
            get_submit_contract_tool(),
        ]
    }

    #[test]
    fn test_get_execute_test_script_tool_schema() {
        let tool = get_execute_test_script_tool();
        assert_eq!(tool.r#type, "function");
        assert_eq!(tool.function.name, "execute_test_script");
        assert!(tool.function.description.is_some());
        let params = &tool.function.parameters;
        assert_eq!(params["type"], "object");
        assert!(params["properties"]["code"].is_object());
        let required = params["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "code"));
    }

    #[test]
    fn test_get_submit_mre_tool_schema() {
        let tool = get_submit_mre_tool();
        assert_eq!(tool.r#type, "function");
        assert_eq!(tool.function.name, "submit_mre");
        assert!(tool.function.description.is_some());
        let params = &tool.function.parameters;
        assert_eq!(params["type"], "object");
        let required = params["required"].as_array().unwrap();
        assert!(required.iter().any(|r| r == "code"));
        assert!(required.iter().any(|r| r == "defect_type"));
        assert!(required.iter().any(|r| r == "surviving_assertions"));
    }

    #[test]
    fn test_get_coverage_report_tool_schema() {
        let tool = get_coverage_report_tool();
        assert_eq!(tool.r#type, "function");
        assert_eq!(tool.function.name, "get_coverage_report");
        assert!(tool.function.description.is_some());
        let params = &tool.function.parameters;
        assert_eq!(params["type"], "object");
        let required = params["required"].as_array().unwrap();
        assert!(required.is_empty());
    }

    #[test]
    fn test_tool_names_unique() {
        let tools = collect_all_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        let mut unique_names: Vec<&str> = names.clone();
        unique_names.sort();
        unique_names.dedup();
        assert_eq!(names.len(), unique_names.len(), "Duplicate tool names found");
    }

    #[test]
    fn test_tool_schemas_valid_json() {
        let tools = collect_all_tools();
        for tool in &tools {
            let params = &tool.function.parameters;
            assert_eq!(
                params["type"], "object",
                "Tool '{}' parameters missing 'type: object'",
                tool.function.name
            );
            assert!(
                params["properties"].is_object(),
                "Tool '{}' missing 'properties'",
                tool.function.name
            );
        }
    }

    #[test]
    fn test_generate_state_check_describe_collection_milvus() {
        let expected = serde_json::json!({"rowCount": 100});
        let params = serde_json::json!({"collectionName": "test_coll"});
        let code = generate_state_check_code("describe_collection", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("desc = api('/v2/vectordb/collections/describe'"));
        assert!(code.contains("test_coll"));
        assert!(code.contains("rowCount"));
        assert!(code.contains("STATE_LOGIC_VIOLATION"));
    }

    #[test]
    fn test_generate_state_check_describe_collection_non_milvus() {
        let expected = serde_json::json!({"rowCount": 100});
        let params = serde_json::json!({"collectionName": "test_coll"});
        let code = generate_state_check_code("describe_collection", &expected, &params, crate::target::TargetStyle::Qdrant);
        assert!(code.contains("adapted for non-Milvus target"));
        assert!(!code.contains("desc = api"));
    }

    #[test]
    fn test_generate_state_check_search_results_distances_ascending() {
        let expected = serde_json::json!({"distancesAscending": true});
        let params = serde_json::json!({"collectionName": "my_coll"});
        let code = generate_state_check_code("search_results", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("SEARCH_CORRECTNESS"));
        assert!(code.contains("metric_type"));
        assert!(code.contains("distances"));
        assert!(code.contains("my_coll"));
        assert!(code.contains("describe_index"));
        assert!(code.contains("non-decreasing (L2/COSINE)"));
        assert!(code.contains("non-increasing (IP)"));
    }

    #[test]
    fn test_generate_state_check_search_results_result_count() {
        let expected = serde_json::json!({"resultCount": 5});
        let params = serde_json::json!({});
        let code = generate_state_check_code("search_results", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("expected_count = 5"));
        assert!(code.contains("SEARCH_CORRECTNESS"));
        assert!(code.contains("Result count mismatch"));
    }

    #[test]
    fn test_generate_state_check_search_results_both_checks() {
        let expected = serde_json::json!({"distancesAscending": true, "resultCount": 10});
        let params = serde_json::json!({"collectionName": "coll_both"});
        let code = generate_state_check_code("search_results", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("distances"));
        assert!(code.contains("expected_count = 10"));
        assert!(code.contains("coll_both"));
    }

    #[test]
    fn test_generate_state_check_search_results_empty_expected() {
        let expected = serde_json::json!({});
        let params = serde_json::json!({});
        let code = generate_state_check_code("search_results", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("no recognized expected keys"));
    }

    #[test]
    fn test_generate_state_check_unsupported_method() {
        let expected = serde_json::json!({});
        let params = serde_json::json!({});
        let code = generate_state_check_code("query_entities", &expected, &params, crate::target::TargetStyle::Milvus);
        assert!(code.contains("ignored (unsupported method)"));
    }
}
