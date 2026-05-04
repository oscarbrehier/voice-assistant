use std::{
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use anyhow::Ok;
use tokio::sync::broadcast;

use crate::{Packet, State, audio::output::play_mp3_audio, state::SharedContext};

#[derive(Clone)]
pub struct TTSService {
    script_dir: PathBuf,
}

impl TTSService {
    pub fn new(script_dir: PathBuf) -> Self {
        Self { script_dir }
    }

    pub async fn speak_async(
        &self,
        text: &str,
        shared_context: SharedContext,
        sender: &broadcast::Sender<Packet>,
        next_state: Option<State>
    ) -> anyhow::Result<()> {
        self.perform_speech(text.to_string(), shared_context, sender.clone(), next_state)
            .await
    }

    pub fn speak(
        &self,
        text: &str,
        shared_context: SharedContext,
        sender: &broadcast::Sender<Packet>,
        next_state: Option<State>
    ) -> anyhow::Result<()> {
        let self_clone = self.clone();
        let ctx_clone = shared_context.clone();
        let tx_clone = sender.clone();
        let text_string = text.to_string();

        tokio::spawn(async move {
            if let Err(e) = self_clone
                .perform_speech(text_string, ctx_clone, tx_clone, next_state)
                .await
            {
                eprintln!("Background TTS error: {}", e); 
            }
        });

        Ok(())
    }

    async fn perform_speech(
        &self,
        text: String,
        shared_context: SharedContext,
        sender: broadcast::Sender<Packet>,
        next_state: Option<State>
    ) -> anyhow::Result<()> {
        State::broadcast(State::Speaking, &shared_context.engine_state, &sender);

        let script_path = self.script_dir.join("tts_service.py");

        let status = tokio::process::Command::new("python")
            .arg(script_path)
            .arg(text)
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("TTS generation failed");
        }

        let temp_path = "output.mp3";
        let ctx_clone = shared_context.clone();

        tokio::task::spawn_blocking(move || play_mp3_audio(temp_path, ctx_clone)).await??;

        let target_state = next_state.unwrap_or(State::Active);
        State::broadcast(target_state, &shared_context.engine_state, &sender);

        Ok(())
    }
}
