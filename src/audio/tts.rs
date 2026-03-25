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