use anyhow::Result;
use serde_json::json;
use tracing::info;
use crate::crawler::engine::{Crawler, ChromiumCrawler, ReqwestCrawler};
use crate::crawler::parser::clean_content;
use crate::agent::llm::{Tool, Function};
use crate::sandbox::manager::Sandbox;

pub enum ToolResult {
    Success(String),
    Error(String),
}

pub async fn execute_test_script(
    code: &str,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
) -> Result<(String, Sandbox, String)> {
    info!("Agent invoked execute_test_script. Creating fresh sandbox...");
    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port).await?;
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
    
    Ok((result_str, sandbox, db_url))
}

pub async fn execute_test_in_sandbox(
    code: &str,
    sandbox: &Sandbox,
    db_port: u16,
) -> Result<(String, String)> {
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
    
    Ok((result_str, db_url))
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

pub fn get_fuzz_boundary_values_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "fuzz_boundary_values".to_string(),
            description: Some("Generates boundary value test scripts from contract constraints. Systematically tests zero values, out-of-range values, and type violations for API parameters. Returns a list of test scripts to execute.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "focus_params": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional: specific parameter names to focus on. If empty, tests all constrained parameters."
                    }
                },
                "required": []
            }),
        },
    }
}

pub fn get_fuzz_api_sequence_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "fuzz_api_sequence".to_string(),
            description: Some("Generates multi-step API sequence test scripts. Tests state dependencies between API calls: missing steps, redundant operations, wrong order, and state transitions. Returns test scripts for each sequence type.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "sequence_type": {
                        "type": "string",
                        "enum": ["missing_step", "redundant_op", "wrong_order", "state_transition", "all"],
                        "description": "Type of sequence to generate. Use 'all' for all types."
                    }
                },
                "required": []
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
                        "description": "The classification: IllegalSuccess, TYPE_VIOLATION, RANGE_VIOLATION, STATE_VIOLATION, ServerCrash, Timeout."
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
