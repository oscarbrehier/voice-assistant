use std::time::Instant;

use base64::Engine;
use reqwest::Client;
use serde_json::Value;
use tokio_tungstenite::tungstenite::handshake::client;

use crate::llm::{Message, MistralRequest, MistralToolResponse, Tool};

pub async fn call_mistral_stateless(
    system_prompt: String,
    message: String,
) -> anyhow::Result<String> {
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
                {"role": "user", "content": message }
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

pub async fn call_mistral_with_vision(
    system_prompt: String,
    image_bytes: &[u8],
) -> anyhow::Result<String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
    let data_uri = format!("data:image/png;base64,{}", b64);

    let body = serde_json::json!({
        "model": "mistral-small-latest",
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": system_prompt},
                {
                    "type": "image_url",
                    "image_url": { "url": data_uri },
                }
            ]
        }]
    });

    let client = Client::new();

    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let response: Value = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&body)
        .send()
        .await?
        .error_for_status()?
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

    let network_start = Instant::now();

    let response = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&request)
        .send()
        .await?;

    let network_elapsed = network_start.elapsed();

    if !response.status().is_success() {
        let error_text = response.text().await?;
        anyhow::bail!("Mistral API error: {}", error_text);
    }

    let parse_start = Instant::now();
    let result: MistralToolResponse = response.json().await?;
    let parse_elapsed = parse_start.elapsed();

    println!(
        "mistral call complete, network_ms = {} parse_ms = {} prompt_msgs = {}",
        network_elapsed.as_millis(),
        parse_elapsed.as_millis(),
        request.messages.len()
    );

    Ok(result)
}
