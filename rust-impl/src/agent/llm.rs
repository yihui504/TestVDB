use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

const LLM_MAX_RETRIES: u32 = 3;
const LLM_JSON_TEMPERATURE: f64 = 0.1;
const LLM_TOOL_TEMPERATURE: f64 = 0.7;
const LLM_RETRY_BASE_DELAY_SECS: u64 = 1;

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    pub temperature: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tool {
    pub r#type: String, // "function"
    pub function: Function,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Function {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum ToolChoice {
    String(String), // "auto" or "none"
    Object(ToolChoiceObject),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolChoiceObject {
    pub r#type: String, // "function"
    pub function: ToolChoiceFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolChoiceFunction {
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String, // "function"
    pub function: ToolCallFunction,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String, // JSON string
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    pub fn tool_response(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    pub fn append_content(&mut self, additional: &str) {
        self.content = Some(format!("{}{}", self.content.take().unwrap_or_default(), additional));
    }
}

#[derive(Debug, Serialize)]
pub struct ResponseFormat {
    pub r#type: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Message,
}

pub struct DeepSeekClient {
    client: Client,
    api_key: String,
    api_url: String,
    model: String,
    temperature_override: Option<f64>,
}

impl DeepSeekClient {
    pub fn new(api_url: String, model: String, temperature_override: Option<f64>) -> Result<Self> {
        let api_key = env::var("DEEPSEEK_API_KEY")
            .context("DEEPSEEK_API_KEY environment variable not set")?;
        
        Ok(Self {
            client: Client::new(),
            api_key,
            api_url,
            model,
            temperature_override,
        })
    }

    /// Sends a chat completion request to DeepSeek API, enforcing JSON mode.
    pub async fn send_chat_json_mode(&self, messages: Vec<Message>) -> Result<String> {
        let temperature = self.temperature_override.unwrap_or(LLM_JSON_TEMPERATURE) as f32;
        let req_body = ChatRequest {
            model: self.model.clone(),
            messages,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
            tools: None,
            tool_choice: None,
            temperature,
        };

        let mut last_err = None;
        for attempt in 0..LLM_MAX_RETRIES {
            match self.try_chat_request(&req_body).await {
                Ok(msg) => return Ok(msg),
                Err(e) => {
                    let err_str = format!("{:#}", e);
                    let is_retryable = Self::is_retryable_error(&err_str);
                    last_err = Some(err_str);
                    if is_retryable && attempt < LLM_MAX_RETRIES - 1 {
                        let delay = std::time::Duration::from_secs(LLM_RETRY_BASE_DELAY_SECS * (1u64 << attempt as u64));
                        tracing::warn!(
                            "DeepSeek transient error, retrying in {:?} (attempt {}/3): {}",
                            delay,
                            attempt + 1,
                            last_err.as_ref().expect("last_err set in Err branch above")
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    bail!("DeepSeek API error after {} attempts: {}", attempt + 1, last_err.expect("last_err set in Err branch"));
                }
            }
        }
        bail!("{}", last_err.expect("at least one error occurred in retry loop"))
    }

    /// Sends a chat completion request to DeepSeek API with tools.
    pub async fn send_chat_with_tools(&self, messages: Vec<Message>, tools: Vec<Tool>) -> Result<Message> {
        let temperature = self.temperature_override.unwrap_or(LLM_TOOL_TEMPERATURE) as f32;
        let req_body = ChatRequest {
            model: self.model.clone(),
            messages,
            response_format: None,
            tools: Some(tools),
            tool_choice: Some(ToolChoice::String("auto".to_string())),
            temperature,
        };

        let mut last_err = None;
        for attempt in 0..LLM_MAX_RETRIES {
            match self.try_chat_request_raw(&req_body).await {
                Ok(msg) => return Ok(msg),
                Err(e) => {
                    let err_str = format!("{:#}", e);
                    let is_retryable = Self::is_retryable_error(&err_str);
                    last_err = Some(err_str);
                    if is_retryable && attempt < LLM_MAX_RETRIES - 1 {
                        let delay = std::time::Duration::from_secs(LLM_RETRY_BASE_DELAY_SECS * (1u64 << attempt as u64));
                        tracing::warn!(
                            "DeepSeek transient error, retrying in {:?} (attempt {}/3): {}",
                            delay,
                            attempt + 1,
                            last_err.as_ref().expect("last_err set in Err branch above")
                        );
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    bail!("DeepSeek API error after {} attempts: {}", attempt + 1, last_err.expect("last_err set in Err branch"));
                }
            }
        }
        bail!("{}", last_err.expect("at least one error occurred in retry loop"))
    }

    fn is_retryable_error(err_str: &str) -> bool {
        err_str.contains("close_notify")
            || err_str.contains("connection")
            || err_str.contains("timed out")
            || err_str.contains("reset")
            || err_str.contains("503")
    }

    async fn try_chat_request_raw(&self, req_body: &ChatRequest) -> Result<Message> {
        let response = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&req_body)
            .send()
            .await
            .context("Failed to send request to DeepSeek API")?;

        if response.status().is_success() {
            let chat_resp: ChatResponse = response
                .json()
                .await
                .context("Failed to deserialize DeepSeek response")?;

            if let Some(choice) = chat_resp.choices.first() {
                return Ok(choice.message.clone());
            } else {
                bail!("No choices returned from DeepSeek API")
            }
        }

        let status = response.status();
        let err_text = response.text().await.unwrap_or_default();
        bail!("DeepSeek API HTTP error ({}): {}", status, err_text)
    }

    async fn try_chat_request(&self, req_body: &ChatRequest) -> Result<String> {
        let msg = self.try_chat_request_raw(req_body).await?;
        Ok(msg.content.clone().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires DEEPSEEK_API_KEY"]
    async fn test_deepseek_json_mode() {
        let client = DeepSeekClient::new(
            "https://api.deepseek.com/chat/completions".to_string(),
            "deepseek-chat".to_string(),
            None,
        ).unwrap();
        let messages = vec![
            Message::system("You are a helpful assistant. Please output a valid JSON object with keys 'hello' and 'world'."),
            Message::user("Generate the JSON."),
        ];

        let result = client.send_chat_json_mode(messages).await.unwrap();
        // Since it's JSON mode, we expect it to be parseable
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("hello").is_some());
    }
}
