use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use num_traits::FromPrimitive;
use strsim::normalized_levenshtein;
use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    ActiveGuard, State,
    audio::{
        Packet,
        stt::stt_service::STTService,
        tts::{TTSService, run_self_calibration},
        utils::resample_to_16khz,
    },
    commands::CommandConfig,
    config::Config,
    llm::LLMEngine,
    memory::{MemoryManager},
    state::SharedContext,
};

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

        let _guard = ActiveGuard::new(
            assistant_active.clone(),
            Arc::clone(&ctx.global_ctx.engine_state),
            tx.clone(),
        );

        let (core_identity, relevant_memories) = {
            let memory_guard = ctx.memory.lock().expect("Memory mutex poisoned");

            let core = if ctx.llm_engine.needs_identity_refresh {
                memory_guard.get_core_identity().unwrap_or_default()
            } else {
                vec![]
            };

            let relevant = memory_guard
                .get_relevant_memories(&trimmed, None)
                .unwrap_or_default();

            (core, relevant)
        };

        let tools = ctx.command_config.tools.clone();

        match ctx
            .llm_engine
            .generate(
                &trimmed,
                &ctx.global_ctx,
                core_identity,
                relevant_memories,
                tools,
            )
            .await
        {
            Ok(response) => {
                let _ = ctx.tts.speak(&response, ctx.global_ctx.clone(), &tx, None, false);
            }
            Err(e) => eprintln!("Failed to generate: {e}"),
        }
    }
}

pub async fn handle_enrollment(
    transcription: String,
    data: Vec<f32>,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
) {
    let enrollment_scripts = [
        "The quick brown fox jumps over the lazy dog, but the rainy weather in Paris might slow him down today.",
        "My voice is my unique password, and it grants me secure access to this system whenever I need it.",
        "Hey assistant, set a timer for fifteen minutes and remind me to check the oven before it burns.",
        "I'm currently integrating several different modules into this Rust project to make everything run smoothly and efficiently.",
        "Confirm authorization now. Everything looks good on my end, so let's get started with the process.",
        "Beautiful azure skies stretched endlessly above the mountainous terrain, while golden sunlight filtered through scattered clouds.",
        "Technology evolves rapidly, but human creativity and intuition remain irreplaceable in solving complex problems.",
        "Please schedule a meeting for Thursday afternoon and send the presentation files to everyone on the team.",
        "The experimental prototype exceeded expectations during testing, demonstrating both reliability and exceptional performance metrics.",
        "Listening carefully to diverse perspectives helps us understand nuanced situations and make better informed decisions together.",
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

    let resampled_data = resample_to_16khz(&data, ctx.sample_rate);

    if similarity > similarity_threshold {
        let mut speaker = ctx.global_ctx.speaker.write();

        match speaker.add_enrollment_sample(&resampled_data) {
            Ok(is_complete) => {
                if is_complete {
                    let _ = ctx.tts.speak(
                        "Voice profile saved successfully. Now, please stay quiet while I calibrate my own voice.",
                        ctx.global_ctx.clone(),
                        tx,
                        None,
                        false
                    );

                    let cal_ctx = ctx.global_ctx.clone();
                    let cal_tts = ctx.tts.clone();
                    let cal_tx = tx.clone();

                    tokio::spawn(async move {
                        if let Err(e) = run_self_calibration(cal_ctx, &cal_tts, &cal_tx).await {
                            eprintln!("Calibration error {e}");
                        }
                    });
                } else {
                    let next_step = step + 1;
                    if let Some(next_script) = enrollment_scripts.get(next_step) {
                        let next_msg = format!("Got it! Next, please say");
                        println!("{}", next_script);
                        let _ = ctx.tts.speak(
                            &next_msg,
                            ctx.global_ctx.clone(),
                            tx,
                            Some(State::Enrolling),
                            false,
                        );
                        State::broadcast(State::Enrolling, &ctx.global_ctx.engine_state, tx);
                    }
                }
            }
            Err(_) => {
                let _ = ctx.tts.speak(
                    "Audio quality was too low. Please try again.",
                    ctx.global_ctx.clone(),
                    tx,
                    Some(State::Enrolling),
                    false,
                );
                State::broadcast(State::Enrolling, &ctx.global_ctx.engine_state, tx);
            }
        }
    } else {
        if transcription.len() > 3 {
            let retry_msg = format!("I didn't catch that quite right. Please repeat:");
            let _ = ctx.tts.speak(
                &retry_msg,
                ctx.global_ctx.clone(),
                tx,
                Some(State::Enrolling),
                false,
            );

            println!("{}", retry_msg);

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

                        if current_state == State::Calibrating as u8 {
                            let resampled = resample_to_16khz(&data, ctx.sample_rate);

                            let mut speaker = ctx.global_ctx.speaker.write();
                            if let Err(e) = speaker.add_negative_sample(&resampled) {
                                eprintln!("Failed to add negative sample: {e}");
                            }

                            let _ = speaker.save_profile();
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

                        process_speech_logic(transcription, &mut ctx, &tx, &assistant_active).await;
                    }
                }
                Packet::WakeWordCheck(data) => {
                    if let Some(transcription) = get_transcription(&mut ctx, &data).await {
                        println!("WAKE WORD CHECK: {transcription}");

                        let has_wake_word = transcription.contains(&ctx.config.name);

                        println!("has wake word: {}", has_wake_word);

                        if has_wake_word {
                            State::broadcast(State::Active, &ctx.global_ctx.engine_state, &tx);
                        }
                    }
                }
                _ => continue,
            };
        }
    })
}
