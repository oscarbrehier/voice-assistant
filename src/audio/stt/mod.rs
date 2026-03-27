pub mod stt_service;

use std::{
    fs::OpenOptions,
    io::Write,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

use tokio::runtime::Runtime;
use tracing::{Level, span};

use crate::{
    ActiveGuard,
    actions::{Action, handle_action},
    audio::{stt::stt_service::STTService, tts::speak, utils::resample_to_16khz},
    commands::CommandMatcher,
    llm::LLMEngine,
};

pub fn spawn_transcription_worker(
    rx: mpsc::Receiver<Vec<f32>>,
    mut stt: STTService,
    command_matcher: CommandMatcher,
    mut llm_engine: LLMEngine,
    rt: Runtime,
    sample_rate: usize,
    assistant_active: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let last_transcription = Arc::new(Mutex::new(String::new()));

        while let Ok(chunk) = rx.recv() {
            let pipeline_span = span!(Level::INFO, "speech_processing_pipeline").entered();

            let resampled = resample_to_16khz(&chunk, sample_rate);

            let stt_span = span!(Level::INFO, "stt_transcription").entered();
            let transcription_result = stt.transcribe(&resampled);
            drop(stt_span);

            match transcription_result {
                Ok(transcription) => {
                    
                    let mut trimmed = transcription.trim().to_lowercase().to_string();
                    
                    if !trimmed.is_empty() {

                        let wake_word = "hey jarvis";

                        if trimmed.contains(&wake_word) {
                            println!("wake word detected");
                        }
                        
                        let mut last = last_transcription.lock().unwrap();

                        println!("{}", trimmed);

                        let match_span = span!(Level::DEBUG, "command_matching").entered();
                        let action = command_matcher.match_command(&trimmed);
                        drop(match_span);

                        println!("action: {:?}", action);

                        if action != Action::Unknown {
                            trimmed.push_str(&format!("command: {:?}", action));
                            let _guard = ActiveGuard::new(assistant_active.clone());
                            let _ = handle_action(action);
                        } else {
                            let llm_span = span!(Level::INFO, "llm_fallback_generation").entered();
                            let _guard = ActiveGuard::new(assistant_active.clone());

                            rt.block_on(async {
                                match llm_engine.generate(&trimmed).await {
                                    Ok(response) => {
                                        let action: Action =
                                            serde_json::from_value(serde_json::json!({
                                                "action": response.action,
                                                "params": response.params
                                            }))
                                            .unwrap_or(Action::Unknown);

                                        if action != Action::Unknown {
                                            let _ = handle_action(action);
                                        }

                                        if !response.message.is_empty() {
                                            let tts_span =
                                                span!(Level::INFO, "tts_speech").entered();

                                            match speak(&response.message) {
                                                Err(e) => {
                                                    eprintln!("failed to generate speech: {e}")
                                                }
                                                _ => {}
                                            }

                                            drop(tts_span);
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to generate: {e}"),
                                }
                            });
                            drop(llm_span);
                        }

                        if trimmed != *last {
                            if let Ok(mut file) = OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("transcription.txt")
                            {
                                writeln!(file, "{}", trimmed).ok();
                            }

                            *last = trimmed.to_string();
                        }
                    }
                }
                Err(e) => eprintln!("transcription error: {}", e),
            }
        }
    })
}
