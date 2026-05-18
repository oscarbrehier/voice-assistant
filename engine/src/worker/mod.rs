use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::Serialize;
use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    State,
    audio::{
        enrollment::handle_enrollment, stt::stt_service::STTService, tts::TTSService,
        utils::resample_to_16khz,
    },
    commands::CommandConfig,
    config::Config,
    llm::LLMEngine,
    memory::MemoryManager,
    proactive::TriggerKind,
    state::SharedContext,
    worker::{proactive::process_proactive_trigger, speech::process_speech_logic},
};

pub mod proactive;
pub mod speech;

#[derive(Clone, Debug, Serialize)]
pub enum Urgency {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "content")]
pub enum Packet {
    Pulse(Vec<f32>),
    Speech(Vec<f32>),
    // WakeWordCheck(Vec<f32>),
    Volume(f32),
    Transcription(String),
    State(State),
    ProactiveTrigger {
        kind: TriggerKind,
        context: String,
        urgency: Urgency,
    },
}

impl Packet {
    pub fn process(self) -> Packet {
        if let Packet::Pulse(samples) = self {
            let peak = samples.iter().map(|s| s.abs()).fold(0.0, f32::max);

            let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
            let rms = (sum_squares / samples.len() as f32).sqrt();

            let combined = (rms * 0.7) + (peak * 0.3);
            let sensitivity = 20.0;
            let volume = (combined * sensitivity).powf(0.6).clamp(0.0, 1.0);

            return Packet::Volume(volume);
        }
        self
    }
}

pub struct WorkerContext {
    pub stt: STTService,
    pub tts: TTSService,
    pub command_config: CommandConfig,
    pub llm_engine: LLMEngine,
    pub config: Config,
    pub sample_rate: usize,
    pub memory: Arc<std::sync::Mutex<MemoryManager>>,
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

                        if current_state == State::Calibrating as u8 {
                            let resampled = resample_to_16khz(&data, ctx.sample_rate);

                            let mut speaker = ctx.global_ctx.speaker.write();
                            match speaker.add_negative_sample(&resampled) {
                                Ok(_) => println!("Added negative sample"),
                                Err(e) => eprintln!("Failed to add negative sample: {e}"),
                            };

                            if let Err(e) = speaker.save_profile() {
                                eprintln!(
                                    "Failed to save speaker profile with negative embeddings: {e}"
                                );
                            };

                            continue;
                        }

                        if current_state == State::Enrolling as u8 {
                            handle_enrollment(transcription, data, &mut ctx, &tx).await;
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
                                handle
                                    .verify_with_negative_check(&data, ctx.sample_rate)
                                    .unwrap_or(false)
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
                                handle
                                    .verify_with_negative_check(&data, ctx.sample_rate)
                                    .unwrap_or(false)
                            };

                            if !is_verified {
                                println!("Unauthorized speaker detected");
                                State::broadcast(State::Idle, &ctx.global_ctx.engine_state, &tx);
                                continue;
                            }
                        }

                        let wake_word = ctx.config.name.to_lowercase();
                        let lower_t = transcription.to_lowercase();

                        let clean_transcript = if let Some(index) = lower_t.find(&wake_word) {
                            lower_t[index + wake_word.len()..].trim().to_string()
                        } else {
                            lower_t.trim().to_string()
                        };

                        if clean_transcript.is_empty() {
                            continue;
                        }

                        process_speech_logic(clean_transcript, &mut ctx, &tx, &assistant_active)
                            .await;
                    }
                }
                Packet::ProactiveTrigger {
                    kind,
                    context,
                    urgency,
                } => {
                    let current_state = ctx.global_ctx.engine_state.load(Ordering::SeqCst);

                    let can_speak = match urgency {
                        Urgency::Critical => current_state != State::Speaking as u8,
                        _ => current_state == State::Idle as u8,
                    };

                    if !can_speak {
                        continue;
                    }

                    process_proactive_trigger(kind, context, urgency, &mut ctx, &tx).await;
                }
                // Packet::WakeWordCheck(data) => {
                //     if let Some(transcription) = get_transcription(&mut ctx, &data).await {
                //         println!("WAKE WORD CHECK: {transcription}");
                //         let wake_word = ctx.config.name.to_lowercase();

                //         if transcription.contains(&wake_word) {
                //             println!("wake word detected");
                //             State::broadcast(State::Active, &ctx.global_ctx.engine_state, &tx);
                //         }
                //     }
                // }
                _ => continue,
            };
        }
    })
}
