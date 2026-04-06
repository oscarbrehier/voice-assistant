pub mod stt_service;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    ActiveGuard, State,
    actions::{Action, handle_action},
    audio::{Packet, stt::stt_service::STTService, tts::TTSService, utils::resample_to_16khz},
    commands::CommandMatcher,
    config::Config,
    llm::LLMEngine,
};

pub struct WorkerContext {
    pub stt: STTService,
    pub tts: TTSService,
    pub command_matcher: CommandMatcher,
    pub llm_engine: LLMEngine,
    pub config: Config,
    pub sample_rate: usize,
}

pub fn spawn_transcription_worker(
    tx: broadcast::Sender<Packet>,
    mut rx: broadcast::Receiver<Packet>,
    mut ctx: WorkerContext,
    assistant_active: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let message = match rx.recv().await {
                Ok(data) => data,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    eprintln!("Worked lagged by {n} chunks.");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            let chunk = match message {
                Packet::Speech(data) => data,
                _ => continue,
            };

            let resampled = resample_to_16khz(&chunk, ctx.sample_rate);

            let transcription_result = ctx.stt.transcribe(&resampled).await;

            match transcription_result {
                Ok(transcription) => {
                    let trimmed = transcription.trim().to_lowercase().to_string();
                    if trimmed.is_empty() {
                        continue;
                    }

                    println!("TRANSCRIPTION: {}", trimmed);

                    let _ = tx.send(Packet::Transcription(trimmed.clone()));

                    let has_wake_word = trimmed.contains(&ctx.config.name);

                    if has_wake_word || state.load(Ordering::SeqCst) == State::Active as u8 {
                        if has_wake_word {
                            State::broadcast(State::Active, &state, &tx);
                        }

                        let action = ctx.command_matcher.match_command(&trimmed);

                        println!("action: {:?}", action);

                        State::broadcast(State::Processing, &state, &tx);

                        if action != Action::Unknown {
                            let _guard = ActiveGuard::new(
                                assistant_active.clone(),
                                Arc::clone(&state),
                                tx.clone(),
                            );
                            let _ = handle_action(action, &ctx.tts, Arc::clone(&state), &tx);
                        } else {
                            let _guard = ActiveGuard::new(
                                assistant_active.clone(),
                                Arc::clone(&state),
                                tx.clone(),
                            );

                            match ctx.llm_engine.generate(&trimmed).await {
                                Ok(response) => {
                                    let action: Action =
                                        serde_json::from_value(serde_json::json!({
                                            "action": response.action,
                                            "params": response.params
                                        }))
                                        .unwrap_or(Action::Unknown);

                                    if action != Action::Unknown {
                                        let _ = handle_action(
                                            action,
                                            &ctx.tts,
                                            Arc::clone(&state),
                                            &tx,
                                        );
                                    }

                                    if !response.message.is_empty() {
                                        match ctx.tts.speak(
                                            &response.message,
                                            Arc::clone(&state),
                                            &tx,
                                        ) {
                                            Err(e) => {
                                                eprintln!("failed to generate speech: {e}")
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                Err(e) => eprintln!("Failed to generate: {e}"),
                            }
                        }
                    }
                }
                Err(e) => eprintln!("transcription error: {}", e),
            }
        }
    })
}
