use reqwest::Client;

use crate::llm::{Message, MistralRequest, MistralResponse, Tool};

pub async fn call_mistral_with_tools(
    system_prompt: String,
    messages: &[Message],
    tools: Vec<Tool>,
) -> anyhow::Result<MistralResponse> {
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

    let result: MistralResponse = response.json().await?;

    Ok(result)
}