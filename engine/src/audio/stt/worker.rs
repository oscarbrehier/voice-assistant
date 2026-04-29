use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    ActiveGuard, State,
    actions::{Action, ActionResult, handle_action},
    audio::{Packet, stt::stt_service::STTService, tts::TTSService, utils::resample_to_16khz},
    commands::CommandMatcher,
    config::Config,
    llm::LLMEngine,
    memory::{MemoryManager, MemoryType},
};

pub struct WorkerContext {
    pub stt: STTService,
    pub tts: TTSService,
    pub command_matcher: CommandMatcher,
    pub llm_engine: LLMEngine,
    pub config: Config,
    pub sample_rate: usize,
    pub memory: Arc<tokio::sync::Mutex<MemoryManager>>,
}

async fn get_transcription(ctx: &mut WorkerContext, data: &[f32]) -> Option<String> {
    let resampled = resample_to_16khz(&data, ctx.sample_rate);
    match ctx.stt.transcribe(&resampled).await {
        Ok(data) => {
            let trimmed = data.trim().to_lowercase();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        Err(_) => None,
    }
}

async fn process_speech_logic(
    trimmed: String,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
    state: &Arc<AtomicU8>,
    assistant_active: &Arc<AtomicBool>,
) {
    println!("TRANSCRIPTION: {}", trimmed);

    let _ = tx.send(Packet::Transcription(trimmed.clone()));

    if state.load(Ordering::SeqCst) == State::Active as u8 {
        State::broadcast(State::Processing, &state, &tx);

        let action = ctx.command_matcher.match_command(&trimmed);

        println!("action: {:?}", action);

        if action != Action::Unknown {
            let _guard = ActiveGuard::new(assistant_active.clone(), Arc::clone(&state), tx.clone());
            let _ = handle_action(action, &ctx.tts, Arc::clone(&state), &tx, None);
        } else {
            let _guard = ActiveGuard::new(assistant_active.clone(), Arc::clone(&state), tx.clone());

            let (core_identity, relevant_memories) = {

                let memory_guard = ctx.memory.lock().await;

                let core = if ctx.llm_engine.needs_identity_refresh {
                    memory_guard.get_core_identity().unwrap_or_default()
                } else {
                    vec![]
                };

                let relevant = memory_guard.get_relevant_memories(&trimmed).unwrap_or_default();

                (core, relevant)

            };

            match ctx.llm_engine.generate(&trimmed, core_identity, relevant_memories).await {
                Ok(response) => {
                    let action: Action = serde_json::from_value(serde_json::json!({
                        "action": response.action,
                        "params": response.params
                    }))
                    .unwrap_or(Action::Unknown);

                    println!("action: {:?}", action);

                    let mut final_message = response.message.clone();

                    if action != Action::Unknown {
                        let template = Some(response.message.clone());
                        if let Ok(ActionResult::Message(msg)) = action.execute(template) {
                            final_message = msg;
                        }
                    }

                    if !final_message.is_empty() {
                        if let Err(e) = ctx.tts.speak(&final_message, Arc::clone(&state), &tx) {
                            eprintln!("failed to generate speech: {e}");
                        }
                    }

                    if let Some(new_memory) = response.save_to_memory {
                        let lock = ctx.memory.lock().await;

                        let memory_type = new_memory.memory_type;

                        if let MemoryType::Identity = memory_type {
                            ctx.llm_engine.mark_identity_dirty();
                        }

                        if let Err(e) = lock.save(&new_memory.key, &new_memory.value, memory_type) {
                            eprintln!("Save error: {e}");
                        }
                    }
                }
                Err(e) => eprintln!("Failed to generate: {e}"),
            }
        }
    }
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

            match message {
                Packet::Speech(data) => {
                    if let Some(transcription) = get_transcription(&mut ctx, &data).await {
                        if state.load(Ordering::SeqCst) == State::Recording as u8 {
                            let has_wake_word = transcription.contains(&ctx.config.name);
                            if !has_wake_word {
                                State::broadcast(State::Idle, &state, &tx);
                                continue;
                            }
                            State::broadcast(State::Active, &state, &tx);
                        }
                        process_speech_logic(
                            transcription,
                            &mut ctx,
                            &tx,
                            &state,
                            &assistant_active,
                        )
                        .await;
                    }
                }
                Packet::WakeWordCheck(data) => {
                    if let Some(transcription) = get_transcription(&mut ctx, &data).await {
                        println!("WAKE WORD CHECK: {transcription}");

                        let has_wake_word = transcription.contains(&ctx.config.name);

                        if has_wake_word {
                            State::broadcast(State::Active, &state, &tx);
                        }
                    }
                }
                _ => continue,
            };
        }
    })
}
