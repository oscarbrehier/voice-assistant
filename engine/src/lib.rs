use std::{
    path::PathBuf,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use clap::Parser;
use cpal::Stream;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::{
    audio::{
        capture::{init_audio_capture, run_vad_loop}, onboarding, setup_audio_device, stt::{
            stt_service::STTService,
            worker::{WorkerContext, spawn_transcription_worker},
        }, tts::TTSService, voice::SpeakerID
    },
    commands::CommandMatcher,
    config::Config,
    llm::LLMEngine,
    memory::MemoryManager,
    state::{GlobalContext, SharedContext, Vitals},
};

pub mod actions;
mod audio;
mod commands;
mod config;
mod llm;
mod memory;
pub mod monitor;
pub mod state;

pub use audio::Packet;

#[derive(Parser, Debug)]
#[command(version, about = "")]
struct Opt {
    #[arg(short, long)]
    device: Option<usize>,
}

#[derive(PartialEq, Serialize, Clone, Debug, FromPrimitive)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum State {
    Idle = 0,
    Recording = 1,
    Active = 2,
    Processing = 3,
    Speaking = 4,
    Enrolling = 5,
    Calibrating = 6
}

impl State {
    fn broadcast(new_state: State, atomic_state: &Arc<AtomicU8>, tx: &broadcast::Sender<Packet>) {
        atomic_state.store(new_state.clone() as u8, Ordering::SeqCst);
        let _ = tx.send(Packet::State(new_state));
    }
}

struct ActiveGuard {
    assistant: Arc<AtomicBool>,
    state: Arc<AtomicU8>,
    tx: broadcast::Sender<Packet>,
}

impl ActiveGuard {
    fn new(
        assistant: Arc<AtomicBool>,
        state: Arc<AtomicU8>,
        tx: broadcast::Sender<Packet>,
    ) -> Self {
        assistant.store(true, Ordering::SeqCst);
        Self {
            assistant,
            state,
            tx,
        }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        // std::thread::sleep(Duration::from_millis(500));

        self.assistant.store(false, Ordering::SeqCst);
        State::broadcast(State::Active, &self.state, &self.tx);
    }
}

pub struct EnginePaths {
    pub config_dir: PathBuf,
    pub script_dir: PathBuf,
}

#[derive(Serialize, Clone, Debug)]
pub struct EngineEvent {
    pub state: State,
    pub data: Packet,
}

pub async fn start_engine(
    paths: EnginePaths,
    device: Option<usize>,
) -> anyhow::Result<(broadcast::Sender<EngineEvent>, Stream)> {
    tracing_subscriber::fmt::init();

    let env_file = paths.config_dir.join(".env");
    let config_file = paths.config_dir.join("config.json");
    let commands_file = paths.config_dir.join("commands.json");
    let prompt_path = paths.config_dir.join("system_prompt.md");

    if let Err(e) = dotenvy::from_path(&env_file) {
        eprintln!("Failed to load .env from {:?}: {}", env_file, e)
    }

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("failed to set ctrlc handler");

    let assistant_active = Arc::new(AtomicBool::new(false));

    let config = Config::load(config_file)?;

    let (device, stream_config) = setup_audio_device(device)?;
    let sample_rate = stream_config.sample_rate() as usize;
    let channels = stream_config.channels() as usize;

    let speaker_id = SpeakerID::new("engine/models/voxceleb_ECAPA1024.onnx", Some("voices/voice_1.bin"), 0.35)?;

    let shared_context: SharedContext = Arc::new(GlobalContext {
        telemetry: Arc::new(RwLock::new(Vitals::default())),
        audio_player: Arc::new(RwLock::new(None)),
        engine_state: Arc::new(AtomicU8::new(State::Idle as u8)),
        speaker: Arc::new(parking_lot::RwLock::new(speaker_id))
    });

    let stt = STTService::new(paths.script_dir.clone()).await?;
    let tts = TTSService::new(paths.script_dir);

    let command_matcher = CommandMatcher::from_file(commands_file)?;

    let monitor_state = Arc::clone(&shared_context);
    tokio::spawn(async move {
        monitor::run_monitoring_loop(monitor_state).await;
    });

    let memory = MemoryManager::new(PathBuf::from("memories.db"))?;

    let llm_engine = LLMEngine::new(prompt_path, &config, &command_matcher.config, &memory)?;

    let (stream, audio_buffer) =
        init_audio_capture(&device, stream_config).expect("failed to init audio capture");

    let (tx_internal, rx_internal) = broadcast::channel::<Packet>(1024);
    let (tx_external, _) = broadcast::channel::<EngineEvent>(1024);

    let bridge_state = shared_context.engine_state.clone();
    let bridge_tx_ext = tx_external.clone();
    let mut bridge_rx_int = tx_internal.subscribe();
    let bridge_running = running.clone();

    tokio::spawn(async move {
        loop {
            if !bridge_running.load(Ordering::SeqCst) {
                break;
            }

            match bridge_rx_int.recv().await {
                Ok(content) => {
                    let s_u8 = bridge_state.load(Ordering::SeqCst);

                    let current_state = State::from_u8(s_u8).unwrap_or(State::Idle);

                    let packet = content.process();

                    let event = EngineEvent {
                        state: current_state,
                        data: packet,
                    };

                    if let Err(_) = bridge_tx_ext.send(event) {
                        break;
                    };
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    let assistant_active_worker = assistant_active.clone();

    let worker_memory = Arc::new(tokio::sync::Mutex::new(memory));
    let worker_tts = tts.clone();

    let worker_context = WorkerContext {
        stt,
        tts: worker_tts,
        command_matcher,
        llm_engine,
        sample_rate,
        config,
        memory: worker_memory,
        global_ctx: shared_context.clone(),
    };

    let worker_tx = tx_internal.clone();

    spawn_transcription_worker(
        worker_tx,
        rx_internal,
        worker_context,
        assistant_active_worker,
    );

    let engine_tx = tx_internal.clone();
    let vad_context = shared_context.clone();

    tokio::task::spawn_blocking(move || {
        run_vad_loop(
            running,
            audio_buffer,
            engine_tx,
            sample_rate,
            channels,
            assistant_active,
            vad_context,
        );
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    if let Err(e) = onboarding::run_startup_verifications(shared_context.clone(), &tts, &tx_internal).await {
        eprintln!("Startup verification error: {e}");
    }

    Ok((tx_external, stream))
}
