use std::{path::PathBuf, process::Command, sync::{Arc, atomic::{AtomicU8, Ordering}}};

use anyhow::Ok;
use tokio::sync::broadcast;

use crate::{Packet, State, audio::output::play_mp3_audio, state::SharedContext};

pub struct TTSService {
    script_dir: PathBuf,
}

impl TTSService {
    pub fn new(script_dir: PathBuf) -> Self {
        Self { script_dir }
    }

    pub fn speak(&self, text: &str, shared_context: SharedContext, sender: &broadcast::Sender<Packet>) -> anyhow::Result<()> {

        State::broadcast(State::Speaking, &shared_context.engine_state, &sender);
        
		let script_path = self.script_dir.join("tts_service.py");
        
        let status = Command::new("python")
            .arg(script_path)
			.arg(text)
            .status()?;
        
        if !status.success() {
            anyhow::bail!("TTS generation failed");
        }
        
        let temp_path = "output.mp3";
        play_mp3_audio(temp_path, shared_context.clone())?;

       State::broadcast(State::Active, &shared_context.engine_state, &sender);

        Ok(())
    }
}
