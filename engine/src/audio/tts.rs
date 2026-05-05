use std::{
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    time::Duration,
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
        next_state: Option<State>,
        bypass_state_change: bool,
    ) -> anyhow::Result<()> {
        self.perform_speech(
            text.to_string(),
            shared_context,
            sender.clone(),
            next_state,
            bypass_state_change,
        )
        .await
    }

    pub fn speak(
        &self,
        text: &str,
        shared_context: SharedContext,
        sender: &broadcast::Sender<Packet>,
        next_state: Option<State>,
        bypass_state_change: bool,
    ) -> anyhow::Result<()> {
        let self_clone = self.clone();
        let ctx_clone = shared_context.clone();
        let tx_clone = sender.clone();
        let text_string = text.to_string();

        tokio::spawn(async move {
            if let Err(e) = self_clone
                .perform_speech(
                    text_string,
                    ctx_clone,
                    tx_clone,
                    next_state,
                    bypass_state_change,
                )
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
        next_state: Option<State>,
        bypass_state_change: bool,
    ) -> anyhow::Result<()> {
        if !bypass_state_change {
            State::broadcast(State::Speaking, &shared_context.engine_state, &sender);
        }

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

        if !bypass_state_change {
            let target_state = next_state.unwrap_or(State::Active);
            State::broadcast(target_state, &shared_context.engine_state, &sender);
        }

        Ok(())
    }
}

pub async fn run_self_calibration(
    ctx: SharedContext,
    tts: &TTSService,
    tx: &broadcast::Sender<Packet>,
) -> anyhow::Result<()> {
    let calibration_scripts = [
        "I am calibrating my voice recognition parameters.",
        "Testing the acoustic environment for echo cancellation.",
        "Generating synthetic voice patterns to improve authorization accuracy.",
        "Finalizing the negative embedding database for authorized access.",
        "The system is now learning to ignore its own output.",
    ];

    State::broadcast(State::Calibrating, &ctx.engine_state, tx);

    for script in calibration_scripts {
        tts.speak_async(script, ctx.clone(), tx, Some(State::Calibrating), true)
            .await?;
        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    State::broadcast(State::Idle, &ctx.engine_state, tx);

    Ok(())
}
