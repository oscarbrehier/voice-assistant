use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

use futures_util::StreamExt;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    ActiveGuard, State,
    actions::{Action, ActionResult, handle_action},
    audio::{Packet, stt::stt_service::STTService, tts::TTSService, utils::resample_to_16khz},
    commands::CommandMatcher,
    config::Config,
    llm::{
        LLMEngine,
        mistral::{self, MistralSink, MistralStream, mistral_send_audio, start_mistral_session},
    },
};

pub struct WorkerContext {
    pub stt: STTService,
    pub tts: TTSService,
    pub command_matcher: CommandMatcher,
    pub llm_engine: LLMEngine,
    pub config: Config,
    pub sample_rate: usize,
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

            match ctx.llm_engine.generate(&trimmed).await {
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
        let (mut mistral_sink, mut mistral_stream) = match start_mistral_session().await {
            Ok(session) => session,
            Err(e) => {
                eprintln!("Failed to start Mistral session {e}");
                return;
            }
        };

        let (trans_tx, mut trans_rx) = mpsc::channel::<String>(100);

        tokio::spawn(async move {
            while let Some(result) = mistral_stream.next().await {
                match result {
                    Ok(msg) => {
                        if let Message::Text(ref t) = msg {
                            println!("Mistral response: {}", t);
                        }

                        if let Message::Text(msg) = msg {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg) {
                                if let Some(transcript) = json["text"].as_str() {
                                    if !transcript.trim().is_empty() {
                                        let _ = trans_tx.send(transcript.to_string()).await;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("WebSocket read error: {}", e),
                }
            }
        });

        loop {
            tokio::select! {
                Ok(message) = rx.recv() => {
                    match message {
                        Packet::Speech(data) | Packet::WakeWordCheck(data) => {
                            let resampled = resample_to_16khz(&data, ctx.sample_rate);
                            let _ = mistral_send_audio(&mut mistral_sink, &resampled).await;
                        }
                        _ => {}
                    }
                }
                Some(transcription) = trans_rx.recv() => {
                    let trimmed = transcription.trim().to_lowercase();

                    let current_state = state.load(Ordering::SeqCst);

                    if current_state == State::Recording as u8 || current_state == State::Idle as u8 {
                        if trimmed.contains(&ctx.config.name) {
                            State::broadcast(State::Active, &state, &tx);
                        }
                    }

                    if current_state == State::Active as u8 {
                        process_speech_logic(trimmed, &mut ctx, &tx, &state, &assistant_active).await;
                    }
                }
            }
        }

        // loop {
        //     let message = match rx.recv().await {
        //         Ok(data) => data,
        //         Err(broadcast::error::RecvError::Lagged(n)) => {
        //             eprintln!("Worked lagged by {n} chunks.");
        //             continue;
        //         }
        //         Err(broadcast::error::RecvError::Closed) => break,
        //     };

        //     match message {
        //         Packet::Speech(data) => {
        //             if let Some(transcription) = get_transcription(&mut ctx, &data, mistral_sink, mistral_stream).await {
        //                 if state.load(Ordering::SeqCst) == State::Recording as u8 {
        //                     let has_wake_word = transcription.contains(&ctx.config.name);
        //                     if !has_wake_word {
        //                         State::broadcast(State::Idle, &state, &tx);
        //                         return;
        //                     }
        //                     State::broadcast(State::Active, &state, &tx);
        //                 }
        //                 process_speech_logic(
        //                     transcription,
        //                     &mut ctx,
        //                     &tx,
        //                     &state,
        //                     &assistant_active,
        //                 )
        //                 .await;
        //             }
        //         }
        //         Packet::WakeWordCheck(data) => {
        //             if let Some(transcription) = get_transcription(&mut ctx, &data, mistral_sink, mistral_stream).await {
        //                 println!("WAKE WORD CHECK: {transcription}");

        //                 let has_wake_word = transcription.contains(&ctx.config.name);

        //                 if has_wake_word {
        //                     State::broadcast(State::Active, &state, &tx);
        //                 }
        //             }
        //         }
        //         _ => continue,
        //     };
        // }
    })
}
