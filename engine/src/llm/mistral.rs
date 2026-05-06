use reqwest::Client;
use serde_json::Value;

use crate::llm::{LLMResponse, Message, MistralRequest, MistralResponse, Tool, history::ConversationHistory};

pub async fn call_mistral_with_history(
    system_prompt: String,
    history: &mut ConversationHistory,
) -> anyhow::Result<(LLMResponse, String)> {
    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let mut messages = vec![serde_json::json!({ "role": "system", "content": system_prompt })];

    // messages.extend(history.build_history_string());

    let client = Client::new();
    let response = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&serde_json::json!({
                "model": "ministral-8b-latest",
                "messages": messages,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "jarvis_response",
                        "strict": true,
                        "schema": {
                            "type": "object",
                            "properties": {
                                "action": { "type": ["string", "null"] },
                                "params": { "type": "object" },
                                "message": { "type": "string" },
                                "save_to_memory": {
                                    "type": ["object", "null"],
                                    "properties": {
                                        "key": { "type": "string" },
                                        "value": { "type": "string" },
                                        "type": { "type": "string", "enum": ["Identity", "Situational"] }
                                    },
                                    "required": ["key", "value", "type"]
                                }
                            },
                            "required": ["action", "params", "message", "save_to_memory"]
                        }
                    }
                }
        }))
        .send()
        .await?;

    let result: Value = response.json().await?;
    let content_str = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Mistral returned empty content"))?;

    println!("json response: {}", content_str);

    let parsed_response: LLMResponse = serde_json::from_str(content_str)?;

    Ok((parsed_response, content_str.to_string()))
}

pub async fn call_mistral_with_tools(
    system_prompt: String,
    messages: &[Message],
    tools: Vec<Tool>,
) -> anyhow::Result<MistralResponse> {

    let client = Client::new();

    let mut full_messages = vec![Message::User { content: system_prompt }];
    full_messages.extend_from_slice(messages);

    let request = MistralRequest {
        model: "ministral-8b-latest".to_string(),
        messages: full_messages,
        tools,
        tool_choice: "auto".to_string()
    };

    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let response = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Mistral API error: {}", error_text);
    }

    let raw_body = response.text().await?;
    println!("raw response: {}", raw_body);

    let result: MistralResponse = serde_json::from_str(&raw_body)?;

    Ok(result)
}
