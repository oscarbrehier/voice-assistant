use std::time::Duration;

use strsim::normalized_levenshtein;
use tokio::sync::broadcast;

use crate::{
    State,
    audio::{tts::TTSService, utils::resample_to_16khz},
    state::SharedContext,
    worker::{Packet, WorkerContext},
};

pub async fn handle_enrollment(
    transcription: String,
    data: Vec<f32>,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
) {
    let enrollment_scripts = [
        "Hey assistant, what's the time right now?",
        "Lock the system, I'm stepping out for a bit.",
        "Set a timer for fifteen minutes and remind me to check the oven.",
        "My voice is my password, and it unlocks this system whenever I need it.",
        "Could you search my notes for anything about the audio project?",
        "The quick brown fox jumps over the lazy dog, while the rainy weather in Paris slows everyone down this morning.",
        "I'm integrating several different modules into this Rust project, trying to make the whole thing run smoothly and efficiently.",
        "Listening carefully to different perspectives helps me understand a situation and make a better, more informed decision overall.",
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
                    State::broadcast(State::Calibrating, &ctx.global_ctx.engine_state, tx);

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

pub async fn run_self_calibration(
    ctx: SharedContext,
    tts: &TTSService,
    tx: &broadcast::Sender<Packet>,
) -> anyhow::Result<()> {
    State::broadcast(State::Calibrating, &ctx.engine_state, tx);

    let calibration_scripts = [
        "I am calibrating my voice recognition parameters.",
        "Testing the acoustic environment for echo cancellation.",
        "Generating synthetic voice patterns to improve authorization accuracy.",
        "Finalizing the negative embedding database for authorized access.",
        "The system is now learning to ignore its own output.",
    ];

    for script in calibration_scripts {
        tts.speak_async(script, ctx.clone(), tx, Some(State::Calibrating), true)
            .await?;
        tokio::time::sleep(Duration::from_millis(800)).await;
    }

    State::broadcast(State::Idle, &ctx.engine_state, tx);

    Ok(())
}
