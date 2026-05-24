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
) -> Result<(String, Sandbox, String, bool)> {
    info!("Creating fresh sandbox for script execution...");
    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port, sidecars, db_env, db_command).await?;
    let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap(), db_port);
    let script_code = code.replace("{{TESTVDB_DB_URL}}", &db_url);
    
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
) -> Result<(String, String, bool)> {
    info!("Reusing existing sandbox for script execution...");
    let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap_or(&"localhost".to_string()), db_port);
    let script_code = code.replace("{{TESTVDB_DB_URL}}", &db_url);
    
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
            description: Some("STATEFUL MODEL TESTING. Tests multi-step API sequences with automatic state verification. Unlike execute_api_sequence (which only checks response codes), this tool verifies that the actual database state matches the expected model state after EACH operation. Use this to find STATE_LOGIC_VIOLATION bugs that deterministic generators cannot detect.".to_string()),
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

pub fn get_execute_concurrent_test_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_concurrent_test".to_string(),
            description: Some("CONCURRENT RACE CONDITION TESTING. Tests multiple threads performing operations simultaneously against the same database. Finds race conditions like: concurrent inserts causing rowCount mismatch, concurrent upserts on same ID creating duplicates, concurrent delete+query returning stale data. Uses Python threading within a single sandbox.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_name": {
                        "type": "string",
                        "description": "Descriptive name (e.g., 'concurrent_insert_rowcount')"
                    },
                    "pattern_category": {
                        "type": "string",
                        "enum": ["concurrent_insert_count", "concurrent_upsert_duplicate", "concurrent_delete_stale", "concurrent_create_conflict", "concurrent_mixed_ops"],
                        "description": "The concurrency pattern being tested"
                    },
                    "setup_steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {"type": "string", "description": "API endpoint"},
                                "params": {"type": "object", "description": "Request parameters"},
                                "expect_success": {"type": "boolean"}
                            },
                            "required": ["action", "params", "expect_success"]
                        },
                        "description": "Setup steps to run BEFORE concurrent operations (e.g., create collection, insert initial data)"
                    },
                    "concurrent_actions": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": {"type": "string", "description": "Label for this thread (e.g., 'inserter_1')"},
                                "action": {"type": "string", "description": "API endpoint"},
                                "params": {"type": "object", "description": "Request parameters"},
                                "repeat": {"type": "integer", "description": "Number of times to repeat this action in the thread (default: 1)"}
                            },
                            "required": ["label", "action", "params"]
                        },
                        "description": "Actions to execute concurrently in separate threads. All threads start simultaneously."
                    },
                    "thread_count": {
                        "type": "integer",
                        "description": "Number of concurrent threads (default: number of concurrent_actions)"
                    },
                    "state_check": {
                        "type": "object",
                        "properties": {
                            "method": {
                                "type": "string",
                                "enum": ["describe_collection", "query_entities", "search_results", "list_collections"],
                                "description": "Method to verify state after all threads complete"
                            },
                            "expected": {
                                "type": "object",
                                "description": "Expected state values after concurrent operations complete"
                            }
                        },
                        "required": ["method", "expected"],
                        "description": "REQUIRED. State verification after all concurrent threads complete."
                    }
                },
                "required": ["test_name", "pattern_category", "setup_steps", "concurrent_actions", "state_check"]
            }),
        },
    }
}

pub fn get_execute_timing_test_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_timing_test".to_string(),
            description: Some("TIMING-SENSITIVE OPERATION TESTING. Tests operations that depend on timing, such as: flush→immediate search (data may not be visible), load→immediate search (may fail), delete→immediate query (may return stale data). Finds bugs where async operations report success but results are not immediately available.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "test_name": {
                        "type": "string",
                        "description": "Descriptive name (e.g., 'flush_then_immediate_search')"
                    },
                    "pattern_category": {
                        "type": "string",
                        "enum": ["flush_visibility", "load_search_failure", "delete_stale_read", "index_immediate_use", "compact_immediate_effect"],
                        "description": "The timing pattern being tested"
                    },
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {"type": "string", "description": "API endpoint"},
                                "params": {"type": "object", "description": "Request parameters"},
                                "expect_success": {"type": "boolean", "description": "Whether this step should succeed"},
                                "immediate": {"type": "boolean", "description": "If true, NO delay after this step (vs 0.5s default). Set on PREPARATORY steps (flush, load, delete) to test if the NEXT verification step works immediately. Default: false."},
                                "state_check": {
                                    "type": "object",
                                    "properties": {
                                        "method": {
                                            "type": "string",
                                            "enum": ["describe_collection", "query_entities", "search_results", "list_collections", "get_index"]
                                        },
                                        "expected": {"type": "object"}
                                    },
                                    "required": ["method", "expected"]
                                }
                            },
                            "required": ["action", "params", "expect_success", "state_check"]
                        },
                        "description": "Ordered list of steps. Steps with immediate=true will have NO sleep before the next step."
                    },
                    "invariant": {
                        "type": "string",
                        "description": "Final invariant to verify after all steps"
                    }
                },
                "required": ["test_name", "pattern_category", "steps"]
            }),
        },
    }
}

pub fn get_compare_endpoints_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "compare_endpoints".to_string(),
            description: Some("RECOMMENDED FOR TURNS 3-4. Compares two semantically equivalent operations to find behavioral inconsistencies. Just describe two operations that SHOULD behave the same, and the tool runs both and compares. Example: operation_a={endpoint:'/v2/vectordb/entities/delete', params:{collectionName:'test',filter:'id in [1]'}}, operation_b={endpoint:'/v2/vectordb/entities/delete', params:{collectionName:'test',expr:'id in [1]'}}".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "comparison_name": {
                        "type": "string",
                        "description": "A descriptive name for this comparison (e.g., 'rest_vs_sdk_create_index')."
                    },
                    "operation_a": {
                        "type": "object",
                        "properties": {
                            "description": {
                                "type": "string",
                                "description": "Human-readable description of operation A."
                            },
                            "endpoint": {
                                "type": "string",
                                "description": "The API endpoint for operation A (e.g., '/v2/vectordb/indexes/create')."
                            },
                            "params": {
                                "type": "object",
                                "description": "Parameters for operation A."
                            }
                        },
                        "required": ["description", "endpoint", "params"]
                    },
                    "operation_b": {
                        "type": "object",
                        "properties": {
                            "description": {
                                "type": "string",
                                "description": "Human-readable description of operation B."
                            },
                            "endpoint": {
                                "type": "string",
                                "description": "The API endpoint for operation B (e.g., '/v2/vectordb/indexes/create')."
                            },
                            "params": {
                                "type": "object",
                                "description": "Parameters for operation B."
                            }
                        },
                        "required": ["description", "endpoint", "params"]
                    },
                    "expected_equivalence": {
                        "type": "string",
                        "description": "Why these two operations should produce the same result (e.g., 'Both create an index with the same parameters, so both should succeed or both should fail')."
                    }
                },
                "required": ["comparison_name", "operation_a", "operation_b", "expected_equivalence"]
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
