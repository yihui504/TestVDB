use anyhow::Result;
use serde_json::json;
use tracing::info;
use crate::crawler::engine::{Crawler, ChromiumCrawler, ReqwestCrawler};
use crate::crawler::parser::clean_content;
use crate::agent::llm::{Tool, Function};
use crate::sandbox::manager::Sandbox;

/// Tool Execution Result
pub enum ToolResult {
    Success(String),
    Error(String),
}

/// Execute a test script in an isolated sandbox.
pub async fn execute_test_script(
    code: &str,
    db_image: &str,
    pip_packages: &[String],
    db_port: u16,
) -> Result<(String, Sandbox, String)> {
    info!("Agent invoked execute_test_script. Restarting sandbox container...");
    let pip_refs: Vec<&str> = pip_packages.iter().map(|s| s.as_str()).collect();
    let sandbox = Sandbox::create_network_and_containers(db_image, &pip_refs, db_port).await?;
    let db_url = format!("http://{}:{}", sandbox.db_host.as_ref().unwrap(), db_port);
    let script_code = code.replace("{{TESTVDB_DB_URL}}", &db_url);
    
    info!("Executing script in sandbox runner...");
    let output = sandbox.exec_command_with_env(&["python", "-c", &script_code], &[("TESTVDB_DB_URL", &db_url)]).await?;
    let normalized_stdout = crate::agent::classifier::normalize_observed_output(&output.stdout);
    let normalized_stderr = crate::agent::classifier::normalize_observed_output(&output.stderr);
    
    let mut result_str = String::new();
    result_str.push_str("STDOUT:\n");
    result_str.push_str(&normalized_stdout);
    result_str.push_str("\nSTDERR:\n");
    result_str.push_str(&normalized_stderr);
    
    Ok((result_str, sandbox, db_url))
}

pub fn get_execute_test_script_tool() -> Tool {
    Tool {
        r#type: "function".to_string(),
        function: Function {
            name: "execute_test_script".to_string(),
            description: Some("Executes a Python test script in an isolated sandbox. The sandbox is freshly restarted for every invocation. Use {{TESTVDB_DB_URL}} as the database URL placeholder.".to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "The complete Python script to execute."
                    }
                },
                "required": ["code"]
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

/// Crawls a document URL, extracts and cleans the content into Markdown for the LLM
pub async fn crawl_docs(url: &str) -> Result<ToolResult> {
    info!("Agent requested to crawl docs from URL: {}", url);
    
    // Try fast Reqwest crawler first (works for most documentation pages)
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
    // Truncate to avoid token explosion
    if markdown.len() > 15000 {
        markdown.truncate(15000);
        markdown.push_str("\n... (truncated to 15000 chars)");
    }
    
    Ok(ToolResult::Success(markdown))
}