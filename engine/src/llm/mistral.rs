use reqwest::Client;
use serde_json::Value;

use crate::llm::{Message, MistralRequest, MistralToolResponse, Tool};

pub async fn call_mistral_proactive(system_prompt: String) -> anyhow::Result<String> {
    let client = Client::new();

    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let response: Value = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&serde_json::json!({
            "model": "ministral-8b-latest".to_string(),
            "messages": [
                {"role": "system", "content": system_prompt },
                {"role": "user", "content": "proceed".to_string() }
            ]
        }))
        .send()
        .await?
        .json()
        .await?;

    let content = response["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to extract content from Mistral response"))?;

    Ok(content.trim().to_string())
}

pub async fn call_mistral_with_tools(
    system_prompt: String,
    messages: &[Message],
    tools: Vec<Tool>,
) -> anyhow::Result<MistralToolResponse> {
    let client = Client::new();

    let mut full_messages = vec![Message::User {
        content: system_prompt,
    }];
    full_messages.extend_from_slice(messages);

    let request = MistralRequest {
        model: "ministral-8b-latest".to_string(),
        messages: full_messages,
        tools,
        tool_choice: "auto".to_string(),
        web_search: None,
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

    let result: MistralToolResponse = response.json().await?;

    Ok(result)
}
