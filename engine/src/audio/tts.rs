use std::{
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
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

fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        current.push(c);

        if matches!(c, '.' | '!' | '?') {
            match chars.peek() {
                Some(next) if next.is_whitespace() => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        sentences.push(trimmed);
                    }
                    current.clear();
                }
                None => {}
                _ => {}
            }
        }
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    sentences
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

    async fn synthesize_one(&self, text: &str) -> anyhow::Result<String> {
        let mut wrapper = self.inner.lock().await;

        match wrapper.process.try_wait() {
            Ok(Some(status)) => anyhow::bail!("TTS process exited with status: {}", status),
            Ok(None) => {}
            Err(e) => anyhow::bail!("Failed to check TTS process status: {}", e),
        }

        let bytes = text.as_bytes();
        wrapper
            .stdin
            .write_all(format!("TEXT {}\n", bytes.len()).as_bytes())
            .await?;
        wrapper.stdin.write_all(bytes).await?;
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
            Ok(path.to_string())
        } else if let Some(err) = response.strip_prefix("ERROR ") {
            anyhow::bail!("TTS error: {}", err)
        } else {
            anyhow::bail!("Unexpected TTS response: {}", response)
        }
    }

    fn play_queue(
        mut rx: tokio::sync::mpsc::Receiver<String>,
        context: SharedContext,
    ) -> anyhow::Result<()> {
        while let Some(path) = rx.blocking_recv() {
            play_mp3_audio(&path, context.clone())?;
        }

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

        let sentences = split_into_sentences(&text);
        if sentences.is_empty() {
            if !bypass_state_change {
                let target_state = next_state.unwrap_or(State::Active);
                State::broadcast(target_state, &shared_context.engine_state, &sender);
            }
            return Ok(());
        }

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(8);

        let ctx_for_consumer = shared_context.clone();
        let consumer = tokio::task::spawn_blocking(move || Self::play_queue(rx, ctx_for_consumer));

        for sentence in sentences {
            match self.synthesize_one(&sentence).await {
                Ok(path) => {
                    if tx.send(path).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("TTS synthesis error for sentence: {}", e);
                }
            }
        }

        drop(tx);

        consumer.await??;

        if !bypass_state_change {
            let target_state = next_state.unwrap_or(State::Active);
            State::broadcast(target_state, &shared_context.engine_state, &sender);
        }

        Ok(())
    }
}
