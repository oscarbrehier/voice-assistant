use anyhow::Ok;
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use reqwest::Client;
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        handshake::client::{Response, generate_key},
        protocol::Message,
    },
};
use tracing_subscriber::fmt::format;

use crate::{
    audio::utils::f32_to_i16_pcm,
    llm::{LLMResponse, history::ConversationHistory},
};

pub async fn call_mistral_with_history(
    history: &mut ConversationHistory,
) -> anyhow::Result<(LLMResponse, String)> {
    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

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

pub type MistralSink = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;
pub type MistralStream = SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>;

pub async fn start_mistral_session() -> anyhow::Result<(MistralSink, MistralStream)> {
    let mistral_api_key: String =
        std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY key not found");

    let url = "wss://api.mistral.ai/v1/audio/transcriptions";

    let request = http::Request::builder()
        .uri(url)
        .header("Host", "api.mistral.ai")
        .header("Origin", "https://api.mistral.ai")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", generate_key())
        .header("Authorization", format!("Bearer {}", mistral_api_key))
        .body(())?;

    let (ws_stream, _) = connect_async(request).await?;
    let (mut sink, stream) = ws_stream.split();

    let config = serde_json::json!({
        "type": "session.update",
        "session": {
            "model": "voxtral-mini-transcribe-realtime-2602",
            "audio_format": "pcm16",
            "sample_rate": 16000,
            "hotwords": ["Jarvis"]
        }
    });

    sink.send(Message::Text(config.to_string().into())).await?;

    Ok((sink, stream))
}

pub async fn mistral_send_audio(sink: &mut MistralSink, data: &[f32]) -> anyhow::Result<()> {
    let pcm_bytes = f32_to_i16_pcm(data);
    sink.send(Message::Binary(pcm_bytes.into())).await?;

    Ok(())
}
