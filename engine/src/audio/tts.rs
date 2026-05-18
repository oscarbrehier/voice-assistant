use std::{
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{Mutex, broadcast},
};

use crate::{Packet, State, audio::output::play_mp3_audio, state::SharedContext};

#[derive(Clone)]
pub struct TTSService {
    inner: Arc<Mutex<TTSInner>>,
}

struct TTSInner {
    process: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl TTSService {
    pub async fn new(script_dir: PathBuf) -> anyhow::Result<Self> {
        let script_path = script_dir.join("tts_service.py");

        let mut process = Command::new("python")
            .arg(script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("failed to start python tts service")?;

        let stdout = process.stdout.take().context("Failed to get stdout")?;
        let stdin = process.stdin.take().context("Failed to get stdin")?;

        let mut stdout = BufReader::new(stdout);

        let mut ready = String::new();
        stdout.read_line(&mut ready).await?;

        if !ready.contains("READY") {
            anyhow::bail!("TTS service failed to start: {}", ready);
        }

        let inner = Arc::new(Mutex::new(TTSInner {
            process,
            stdin,
            stdout,
        }));

        Ok(Self { inner })
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
        if text.is_empty() {
            return Ok(());
        }

        if !bypass_state_change {
            State::broadcast(State::Speaking, &shared_context.engine_state, &sender);
        }

        let output_path = {
            let mut wrapper = self.inner.lock().await;

            match wrapper.process.try_wait() {
                Ok(Some(status)) => anyhow::bail!("TTS process exited with status: {}", status),
                Ok(None) => {}
                Err(e) => anyhow::bail!("Failed to check TTS process status: {}", e),
            }

            wrapper
                .stdin
                .write_all(format!("TEXT: {}\n", text).as_bytes())
                .await
                .context("Failed to send speech text bytes")?;
            wrapper.stdin.flush().await?;

            let mut response = String::new();
            tokio::time::timeout(
                Duration::from_secs(15),
                wrapper.stdout.read_line(&mut response),
            )
            .await
            .context("TTS service timeout")?
            .context("Failed to read TTS response")?;

            let response = response.trim();

            if let Some(path) = response.strip_prefix("DONE ") {
                path.to_string()
            } else if let Some(err) = response.strip_prefix("ERROR ") {
                anyhow::bail!("TTS error: {}", err)
            } else {
                anyhow::bail!("Unexpected TTS response: {}", response)
            }
        };

        let ctx_clone = shared_context.clone();
        tokio::task::spawn_blocking(move || play_mp3_audio(&output_path, ctx_clone)).await??;

        if !bypass_state_change {
            let target_state = next_state.unwrap_or(State::Active);
            State::broadcast(target_state, &shared_context.engine_state, &sender);
        }

        Ok(())
    }
}
