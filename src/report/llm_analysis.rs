use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::agent::llm::{DeepSeekClient, Message};

#[derive(Debug, Serialize, Deserialize)]
pub struct RootCauseAnalysis {
    pub is_real_defect: bool,
    pub defect_type: Option<String>,
    pub root_cause: String,
    pub confidence: f32,
    pub fixed_script: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VariantVerificationResult {
    pub variant_script: String,
    pub variant_description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OptimizedReport {
    pub title: String,
    pub root_cause_analysis: String,
    pub improvement_suggestions: String,
    pub github_issue_body: String,
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        match s.char_indices().nth(max_len) {
            Some((i, _)) => &s[..i],
            None => s,
        }
    }
}

fn llm_fallback_analysis() -> RootCauseAnalysis {
    RootCauseAnalysis {
        is_real_defect: false,
        defect_type: None,
        root_cause: "LLM analysis unavailable".to_string(),
        confidence: 0.0,
        fixed_script: None,
    }
}

pub async fn analyze_defect_with_llm(
    stdout: &str,
    stderr: &str,
    original_defect_type: &str,
    mre_code: &str,
) -> RootCauseAnalysis {
    let client = match DeepSeekClient::new() {
        Ok(c) => c,
        Err(e) => {
            warn!("LLM analysis: DeepSeekClient::new() failed: {}", e);
            return llm_fallback_analysis();
        }
    };

    let system_prompt = r#"You are a defect analysis expert. Analyze the following script execution output to determine if a real defect was found.

Rules:
1. If stdout contains [DEFECT: ...] markers, the defect is REAL regardless of any Traceback that follows
2. If the script crashed (Traceback) BEFORE finding any defect marker, the defect is NOT real - it's a script error
3. If the script found a defect marker AND then crashed, the defect IS real - the crash is in the verification code, not the defect detection
4. If is_real_defect is true AND the crash happened after the defect marker, provide a fixed_script that removes the crashing code after the defect detection

Respond in JSON format:
{
  "is_real_defect": boolean,
  "defect_type": "the defect type from the marker, or null",
  "root_cause": "one-line explanation of what happened",
  "confidence": 0.0-1.0,
  "fixed_script": "the fixed MRE script if applicable, or null"
}"#;

    let user_prompt = format!(
        "## Script stdout (truncated):\n{}\n\n## Script stderr (truncated):\n{}\n\n## Original defect type: {}\n\n## MRE script:\n{}",
        truncate(stdout, 2000),
        truncate(stderr, 1000),
        original_defect_type,
        mre_code,
    );

    let messages = vec![
        Message::system(system_prompt),
        Message::user(user_prompt),
    ];

    match client.send_chat_json_mode(messages).await {
        Ok(text) => {
            info!("LLM analysis raw response (first 500 chars): {}", &text[..text.len().min(500)]);
            serde_json::from_str::<RootCauseAnalysis>(&text).unwrap_or_else(|e| {
                warn!("LLM analysis JSON parse failed: {}. Raw: {}", e, &text[..text.len().min(200)]);
                llm_fallback_analysis()
            })
        }
        Err(e) => {
            warn!("LLM analysis API call failed: {}", e);
            llm_fallback_analysis()
        }
    }
}

pub async fn generate_verification_variant(
    mre_code: &str,
    defect_type: &str,
    defect_evidence: &str,
) -> Result<VariantVerificationResult> {
    let client = DeepSeekClient::new()?;

    let system_prompt = r#"You are a test variant generator. Given an MRE (Minimal Reproducible Example) script that found a defect, generate a VERIFICATION VARIANT script.

The variant must:
1. Test the SAME defect hypothesis but with DIFFERENT parameter values
2. Use a different collection name (uuid-based)
3. Use {{TESTVDB_DB_URL}} as the database URL placeholder
4. Be self-contained (import all needed modules)
5. Print [DEFECT: TYPE] marker when defect is confirmed
6. sys.exit(1) on defect found, sys.exit(0) on pass
7. Include try/except around all API calls to prevent crashes after defect detection
8. Add time.sleep(0.5) after create operations, 0.3 after insert/upsert

Respond in JSON:
{
  "variant_script": "the complete Python script",
  "variant_description": "one-line description of what differs from the original"
}"#;

    let user_prompt = format!(
        "Defect type: {}\n\nDefect evidence:\n{}\n\nOriginal MRE script:\n{}",
        defect_type, defect_evidence, mre_code
    );

    let messages = vec![
        Message::system(system_prompt),
        Message::user(user_prompt),
    ];

    match client.send_chat_json_mode(messages).await {
        Ok(raw) => {
            let result: VariantVerificationResult = serde_json::from_str(&raw)?;
            Ok(result)
        }
        Err(_) => Ok(VariantVerificationResult {
            variant_script: mre_code.to_string(),
            variant_description: "LLM variant generation failed; using original MRE as fallback".to_string(),
        }),
    }
}

pub async fn optimize_defect_report(
    defect_type: &str,
    surviving_assertions: &[String],
    mre_code: &str,
    initial_evidence: &str,
) -> Result<OptimizedReport> {
    let client = DeepSeekClient::new()?;

    let system_prompt = r#"You are a defect report optimization expert. Given defect information, produce a polished, submission-ready report.

Generate:
1. title: concise issue title in format "[REST API] <summary of the bug>"
2. root_cause_analysis: 1-2 paragraphs of technical root cause analysis
3. improvement_suggestions: specific, actionable fix directions
4. github_issue_body: full GitHub Issue body with sections: Steps to Reproduce, Expected Behavior, Actual Behavior

Respond in JSON:
{
  "title": "[REST API] ...",
  "root_cause_analysis": "...",
  "improvement_suggestions": "...",
  "github_issue_body": "..."
}"#;

    let user_prompt = format!(
        "Defect type: {}\n\nSurviving assertions:\n{}\n\nMRE code:\n{}\n\nInitial evidence:\n{}",
        defect_type,
        surviving_assertions.join("\n"),
        mre_code,
        initial_evidence
    );

    let messages = vec![
        Message::system(system_prompt),
        Message::user(user_prompt),
    ];

    match client.send_chat_json_mode(messages).await {
        Ok(raw) => {
            let result: OptimizedReport = serde_json::from_str(&raw)?;
            Ok(result)
        }
        Err(_) => Ok(OptimizedReport {
            title: format!("[REST API] {} defect detected", defect_type),
            root_cause_analysis: format!("Defect of type {} was detected but LLM report optimization failed.", defect_type),
            improvement_suggestions: "Review the MRE and surviving assertions manually for fix directions.".to_string(),
            github_issue_body: format!("## Defect: {}\n\n### MRE\n```\n{}\n```\n\n### Evidence\n{}", defect_type, mre_code, initial_evidence),
        }),
    }
}
