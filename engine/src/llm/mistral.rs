use reqwest::Client;
use serde_json::Value;

use crate::llm::{LLMResponse, history::ConversationHistory};

pub async fn call_mistral_with_history(
    system_prompt: String,
    history: &mut ConversationHistory,
    relevant_memories: Vec<String>,
) -> anyhow::Result<(LLMResponse, String)> {
    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let memory_block = if relevant_memories.is_empty() {
        "No specific memories recalled for this request.".to_string()
    } else {
        relevant_memories.join("\n")
    };

    let mut messages = vec![serde_json::json!({ "role": "system", "content": system_prompt })];

    messages.extend(history.build_history_string());

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
