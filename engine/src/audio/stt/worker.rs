use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use num_traits::FromPrimitive;
use strsim::normalized_levenshtein;
use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    ActiveGuard, State,
    actions::{Action, ActionResult, handle_action},
    audio::{
        Packet, stt::stt_service::STTService, tts::TTSService, utils::resample_to_16khz,
        voice::SpeakerID,
    },
    commands::CommandMatcher,
    config::Config,
    llm::LLMEngine,
    memory::{MemoryManager, MemoryType},
    state::SharedContext,
};

pub struct WorkerContext {
    pub stt: STTService,
    pub tts: TTSService,
    pub command_matcher: CommandMatcher,
    pub llm_engine: LLMEngine,
    pub config: Config,
    pub sample_rate: usize,
    pub memory: Arc<tokio::sync::Mutex<MemoryManager>>,
    pub global_ctx: SharedContext,
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
    assistant_active: &Arc<AtomicBool>,
) {
    println!("TRANSCRIPTION: {}", trimmed);

    let _ = tx.send(Packet::Transcription(trimmed.clone()));

    let current_state_u8 = ctx.global_ctx.engine_state.load(Ordering::SeqCst);
    let current_state = State::from_u8(current_state_u8).unwrap_or(State::Idle);

    if current_state == State::Active {
        State::broadcast(State::Processing, &ctx.global_ctx.engine_state, &tx);

        let action = ctx.command_matcher.match_command(&trimmed);

        println!("action: {:?}", action);

        if action != Action::Unknown {
            let _guard = ActiveGuard::new(
                assistant_active.clone(),
                Arc::clone(&ctx.global_ctx.engine_state),
                tx.clone(),
            );
            let _ = handle_action(action, &ctx.tts, ctx.global_ctx.clone(), &tx, None);
        } else {
            let _guard = ActiveGuard::new(
                assistant_active.clone(),
                Arc::clone(&ctx.global_ctx.engine_state),
                tx.clone(),
            );

            let (core_identity, relevant_memories) = {
                let memory_guard = ctx.memory.lock().await;

                let core = if ctx.llm_engine.needs_identity_refresh {
                    memory_guard.get_core_identity().unwrap_or_default()
                } else {
                    vec![]
                };

                let relevant = memory_guard
                    .get_relevant_memories(&trimmed)
                    .unwrap_or_default();

                (core, relevant)
            };

            match ctx
                .llm_engine
                .generate(&trimmed, &ctx.global_ctx, core_identity, relevant_memories)
                .await
            {
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
                        if let Err(e) = ctx.tts.speak(&final_message, ctx.global_ctx.clone(), &tx, None) {
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

pub async fn handle_enrollment(
    transcription: String,
    data: Vec<f32>,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
    assistant_active: &Arc<AtomicBool>,
) {
    let enrollment_scripts = [
        "The quick brown fox jumps over the lazy dog, but the rainy weather in Paris might slow him down today.",
        "My voice is my unique password, and it grants me secure access to this system.",
        "Hey Joe, set a timer for fifteen minutes and remind me to check the oven.",
        "I'm currently integrating several different modules into this Rust project to make everything run as efficiently as possible.",
        "Confirm authorization. Everything looks good, let's get started.",
    ];

    let step = ctx
        .global_ctx
        .speaker
        .read()
        .enrolment_state
        .as_ref()
        .map(|s| s.current_step)
        .unwrap_or(0);

    if step >= enrollment_scripts.len() {
        return;
    }

    let target_script = enrollment_scripts[step];

    let clean_transcript = transcription
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && !c.is_whitespace(), "");
    let clean_script = target_script
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && !c.is_whitespace(), "");

    let similarity = normalized_levenshtein(&clean_transcript, &clean_script);

    let similarity_threshold = 0.80;

    println!("enrolment similarity score: {:.2}%", similarity * 100.0);

    let resampled_data = resample_to_16khz(&data, ctx.sample_rate);

    if similarity > similarity_threshold {
        let mut speaker = ctx.global_ctx.speaker.write();
        
        match speaker.add_enrollment_sample(&resampled_data) {
            Ok(is_complete) => {
                if is_complete {
                    let _ = ctx.tts.speak(
                        "Voice profile saved successfully.",
                        ctx.global_ctx.clone(),
                        tx,
                        None
                    );
                    State::broadcast(State::Idle, &ctx.global_ctx.engine_state, tx);
                } else {
                    let next_step = step + 1;
                    if let Some(next_script) = enrollment_scripts.get(next_step) {
                        let next_msg = format!("Got it! Next, please say");
                        println!("{}", next_script);
                        let _ = ctx.tts.speak(&next_msg, ctx.global_ctx.clone(), tx, Some(State::Enrolling));
                        State::broadcast(State::Enrolling, &ctx.global_ctx.engine_state, tx);
                    }
                }
            }
            Err(e) => {
                let _ = ctx.tts.speak(
                    "Audio quality was too low. Please try again.",
                    ctx.global_ctx.clone(),
                    tx,
                    Some(State::Enrolling)
                );
                State::broadcast(State::Enrolling, &ctx.global_ctx.engine_state, tx);
            }
        }
    } else {
        if transcription.len() > 3 {
            let retry_msg = format!(
                "I didn't catch that quite right. Please repeat: {}",
                target_script
            );
            let _ = ctx.tts.speak(&retry_msg, ctx.global_ctx.clone(), tx, Some(State::Enrolling));
            State::broadcast(State::Enrolling, &ctx.global_ctx.engine_state, tx);
        }
    }
}

pub fn spawn_transcription_worker(
    tx: broadcast::Sender<Packet>,
    mut rx: broadcast::Receiver<Packet>,
    mut ctx: WorkerContext,
    assistant_active: Arc<AtomicBool>,
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
                        println!("transcription: {}", transcription);

                        let current_state = ctx.global_ctx.engine_state.load(Ordering::SeqCst);

                        println!("current state: {}", current_state);
                        
                        if current_state == State::Enrolling as u8 {
                            handle_enrollment(
                                transcription,
                                data,
                                &mut ctx,
                                &tx,
                                &assistant_active,
                            )
                            .await;
                            continue;
                        }

                        if !ctx.global_ctx.speaker.read().is_enrolled() {
                            continue;
                        }

                        if current_state == State::Recording as u8 {
                            let has_wake_word = transcription.contains(&ctx.config.name);
                            if !has_wake_word {
                                State::broadcast(State::Idle, &ctx.global_ctx.engine_state, &tx);
                                continue;
                            }

                            let is_verified = {
                                let mut handle = ctx.global_ctx.speaker.write();
                                handle.verify(&data, ctx.sample_rate).unwrap_or(false)
                            };

                            if !is_verified {
                                println!("Unauthorized speaker detected");
                                State::broadcast(State::Idle, &ctx.global_ctx.engine_state, &tx);
                                continue;
                            }

                            println!("Authorized speaker");

                            State::broadcast(State::Active, &ctx.global_ctx.engine_state, &tx);
                        }

                        if current_state == State::Active as u8 {
                            let is_verified = {
                                let mut handle = ctx.global_ctx.speaker.write();
                                handle.verify(&data, ctx.sample_rate).unwrap_or(false)
                            };

                            if !is_verified {
                                println!("Unauthorized speaker detected");
                                State::broadcast(State::Idle, &ctx.global_ctx.engine_state, &tx);
                                continue;
                            }
                        }

                        process_speech_logic(transcription, &mut ctx, &tx, &assistant_active).await;
                    }
                }
                Packet::WakeWordCheck(data) => {
                    if let Some(transcription) = get_transcription(&mut ctx, &data).await {
                        println!("WAKE WORD CHECK: {transcription}");

                        let has_wake_word = transcription.contains(&ctx.config.name);

                        if has_wake_word {
                            let is_verified = {
                                let mut handle = ctx.global_ctx.speaker.write();
                                handle.verify(&data, ctx.sample_rate).unwrap_or(false)
                            };

                            if is_verified {
                                State::broadcast(State::Active, &ctx.global_ctx.engine_state, &tx);
                            }
                        }
                    }
                }
                _ => continue,
            };
        }
    })
}
