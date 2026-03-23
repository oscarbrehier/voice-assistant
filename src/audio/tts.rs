use std::process::Command;

use anyhow::Ok;

use crate::audio::output::{play_mp3_audio};

pub fn speak(text: &str) -> anyhow::Result<()> {

	let status = Command::new("python")
		.args(["python/tts_service.py", text])
		.status()?;

	if !status.success() {
		anyhow::bail!("TTS generation failed");
	}

	let temp_path = "output.mp3";
	play_mp3_audio(temp_path)?;

	Ok(())

}

// async fn tts_request(text: &str) -> anyhow::Result<Vec<u8>> {
	
// 	let api_key = dotenv::var("ELEVENLABS_API_KEY").expect("ELEVENLABS_API_KEY key not found");
// 	let voice_id = "o9yXv9EFSasRrRM3x6xK";
// 	let output_format = "mp3_44100_128";

// 	let client = reqwest::Client::new();
// 	let res = client.post(format!("https://api.elevenlabs.io/v1/text-to-speech/{voice_id}?output_format={output_format}"))
// 		.header("Content-Type", "application/json")
// 		.header("xi-api-key", api_key)
// 		.json(&json!({ 
// 			"text": text, 
// 			"model_id": "eleven_multilingual_v2",
// 		}))
// 		.send()
// 		.await
// 		.expect("Failed to generate speech");

// 	if !res.status().is_success() {
// 		let error = res.text().await?;
// 		return Err(anyhow::anyhow!("TTS error: {}", error));
// 	};

// 	let bytes = res.bytes().await?;

// 	Ok(bytes.to_vec())

// }