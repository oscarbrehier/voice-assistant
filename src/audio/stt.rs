use std::{
    fs::OpenOptions,
    io::Write,
    sync::{Arc, Mutex, mpsc},
    thread,
};

use tokio::runtime::Runtime;

use crate::{
    actions::{Action, handle_action},
    audio::{tts::speak, utils::resample_to_16khz},
    commands::CommandMatcher,
    llm::LLMEngine,
    stt::stt_service::STTService,
};

pub fn spawn_transcription_worker(
    rx: mpsc::Receiver<Vec<f32>>,
    mut stt: STTService,
    command_matcher: CommandMatcher,
    mut llm_engine: LLMEngine,
    rt: Runtime,
    sample_rate: usize,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let last_transcription = Arc::new(Mutex::new(String::new()));

        while let Ok(chunk) = rx.recv() {
            let resampled = resample_to_16khz(&chunk, sample_rate);

            match stt.transcribe(&resampled) {
                Ok(transcription) => {
                    let mut trimmed = transcription.trim().to_lowercase().to_string();

                    if !trimmed.is_empty() {
                        let mut last = last_transcription.lock().unwrap();

                        println!("{}", trimmed);

                        let action = command_matcher.match_command(&trimmed);

                        println!("action: {:?}", action);

                        if action != Action::Unknown {
                            trimmed.push_str(&format!("command: {:?}", action));
                            let _ = handle_action(action);
                        } else {
                            rt.block_on(async {
                                match llm_engine.generate(&trimmed).await {
                                    Ok(response) => {
                                        if let Some(action_str) = response.action {
                                            let action = command_matcher
                                                .build_action(&action_str, response.params);

                                            if action != Action::Unknown {
                                                let _ = handle_action(action);
                                            }
                                        }

                                        match speak(&response.message) {
                                            Err(e) => eprintln!("failed to generate speech: {e}"),
                                            _ => {}
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to generate: {e}"),
                                }
                            });
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
