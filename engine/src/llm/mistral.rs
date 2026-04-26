use reqwest::Client;
use serde_json::Value;
use tokio_tungstenite::connect_async;

use crate::llm::{LLMResponse, history::ConversationHistory};

pub async fn call_mistral_with_history(
    history: &mut ConversationHistory,
) -> anyhow::Result<(LLMResponse, String)> {
    let mistral_api_key: String =
        std::env::var("MISTRA_API_KEY").expect("MISTRA_API_KEY key not found");

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
    let content_str = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Mistral returned empty content"))?;

    println!("json response: {}", content_str);

    let parsed_response: LLMResponse = serde_json::from_str(content_str)?;

    Ok((parsed_response, content_str.to_string()))
}

pub async fn start_mistral_session() -> anyhow::Result<()> {
    let mistral_api_key: String =
        std::env::var("MISTRA_API_KEY").expect("MISTRA_API_KEY key not found");

    let url = "wss://api.mistral.ai/v1/audio/transcriptions";

    let request = http::Request::builder()
        .uri(url)
        .header("Authorization", format!("Bearer {}", std::env::var("MISTRAL_API_KEY")?))
        .body(())?;

    let (mut ws_stream, _) = connect_async(request).await?;

    let config = serde_json

    let client = Client::new();
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .json(&serde_json::json!({
                  "type": "session.update",
        "session": {
          "model": "voxtral-mini-transcribe-realtime-2602",
          "audio_format": "pcm16",
          "sample_rate": 16000,
          "transcription_delay_ms": 480,
          "hotwords": ["Jarvis"]
        }
              }))
        .send()
        .await;

    Ok(())
}
