use std::{path::PathBuf, process::Command};

use anyhow::Ok;

use crate::audio::output::play_mp3_audio;

pub struct TTSService {
    script_dir: PathBuf,
}

impl TTSService {
    pub fn new(script_dir: PathBuf) -> Self {
        Self { script_dir }
    }

    pub fn speak(&self, text: &str) -> anyhow::Result<()> {

		let script_path = self.script_dir.join("tts_service.py");

        let status = Command::new("python")
            .arg(script_path)
			.arg(text)
            .status()?;

        if !status.success() {
            anyhow::bail!("TTS generation failed");
        }

        let temp_path = "output.mp3";
        play_mp3_audio(temp_path)?;

        Ok(())
    }
}
