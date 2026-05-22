use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

const DEEPSEEK_API_URL: &str = "https://api.deepseek.com/chat/completions";

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
}

impl DeepSeekClient {
    pub fn new() -> Result<Self> {
        let api_key = env::var("DEEPSEEK_API_KEY")
            .context("DEEPSEEK_API_KEY environment variable not set")?;
        
        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    /// Sends a chat completion request to DeepSeek API, enforcing JSON mode.
    pub async fn send_chat_json_mode(&self, messages: Vec<Message>) -> Result<String> {
        let req_body = ChatRequest {
            model: "deepseek-chat".to_string(),
            messages,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
            tools: None,
            tool_choice: None,
            temperature: 0.1,
        };

        let mut last_err = None;
        for attempt in 0..3 {
            let response = self
                .client
                .post(DEEPSEEK_API_URL)
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
                    return Ok(choice.message.content.clone().unwrap_or_default());
                } else {
                    bail!("No choices returned from DeepSeek API")
                }
            }

            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            last_err = Some(format!("DeepSeek API error ({}): {}", status, err_text));

            if status.as_u16() == 503 && attempt < 2 {
                let delay = std::time::Duration::from_secs(10 * (attempt as u64 + 1));
                tracing::warn!("DeepSeek 503, retrying in {:?} (attempt {}/3)...", delay, attempt + 1);
                tokio::time::sleep(delay).await;
                continue;
            }
            bail!("{}", last_err.unwrap());
        }
        bail!("{}", last_err.unwrap())
    }

    /// Sends a chat completion request to DeepSeek API with tools.
    pub async fn send_chat_with_tools(&self, messages: Vec<Message>, tools: Vec<Tool>) -> Result<Message> {
        let req_body = ChatRequest {
            model: "deepseek-chat".to_string(),
            messages,
            response_format: None,
            tools: Some(tools),
            tool_choice: Some(ToolChoice::String("auto".to_string())),
            temperature: 0.7,
        };

        let mut last_err = None;
        for attempt in 0..3 {
            let response = self
                .client
                .post(DEEPSEEK_API_URL)
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
            last_err = Some(format!("DeepSeek API error ({}): {}", status, err_text));

            if status.as_u16() == 503 && attempt < 2 {
                let delay = std::time::Duration::from_secs(10 * (attempt as u64 + 1));
                tracing::warn!("DeepSeek 503, retrying in {:?} (attempt {}/3)...", delay, attempt + 1);
                tokio::time::sleep(delay).await;
                continue;
            }
            bail!("{}", last_err.unwrap());
        }
        bail!("{}", last_err.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires DEEPSEEK_API_KEY"]
    async fn test_deepseek_json_mode() {
        let client = DeepSeekClient::new().unwrap();
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
