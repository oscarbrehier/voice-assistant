use reqwest::Client;
use serde_json::Value;

use crate::llm::history::{ConversationHistory, Message};

pub async fn call_mistral_with_history(history: &ConversationHistory) -> anyhow::Result<Value> {
    let mistral_api_key = dotenv::var("MISTRA_API_KEY").expect("MISTRA_API_KEY key not found");

    let messages = history.build_history_string();

    let client = Client::new();
    let response = client
        .post("https://api.mistral.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&serde_json::json!({
            "model": "ministral-8b-latest",
			"messages": messages,
			"response_format": { "type": "json_object" },
        }))
        .send()
        .await?;

	let result: Value = response.json().await?;
	Ok(result["choices"][0]["message"]["content"].clone())
}
