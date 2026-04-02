use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    },
    time::Duration,
};

use clap::Parser;
use cpal::Stream;
use tokio::{runtime::Runtime, sync::broadcast};

use crate::{
    audio::{
        capture::{init_audio_capture, run_vad_loop},
        setup_audio_device,
        stt::{WorkerContext, stt_service::STTService}, tts::TTSService,
    },
    commands::CommandMatcher,
    config::Config,
    llm::LLMEngine,
};

mod actions;
mod audio;
mod commands;
mod config;
mod llm;

pub use audio::AudioMessage;

#[derive(Parser, Debug)]
#[command(version, about = "")]
struct Opt {
    #[arg(short, long)]
    device: Option<usize>,
}

type AudioQueue = Arc<Mutex<VecDeque<f32>>>;

#[derive(PartialEq)]
#[repr(u8)]
enum State {
    Idle = 0,
    Recording = 1,
    Active = 2,
}

struct ActiveGuard {
    assistant: Arc<AtomicBool>,
}

impl ActiveGuard {
    fn new(assistant: Arc<AtomicBool>) -> Self {
        assistant.store(true, Ordering::SeqCst);
        Self { assistant }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        std::thread::sleep(Duration::from_millis(500));
        self.assistant.store(false, Ordering::SeqCst);
    }
}

pub struct EnginePaths {
    pub config_dir: PathBuf,
    pub script_dir: PathBuf
}

pub async fn start_engine(
    paths: EnginePaths,
    device: Option<usize>,
) -> anyhow::Result<(broadcast::Sender<AudioMessage>, Stream)> {
    tracing_subscriber::fmt::init();

    let env_file = paths.config_dir.join(".env");
    let config_file = paths.config_dir.join("config.json");
    let commands_file = paths.config_dir.join("commands.json");
    let prompt_path = paths.config_dir.join("system_prompt.txt");

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

    let stt = STTService::new(paths.script_dir.clone())?;
    let tts = TTSService::new(paths.script_dir);
    let rt = Runtime::new()?;

    let command_matcher = CommandMatcher::from_file(commands_file)?;

    let llm_engine = LLMEngine::new(prompt_path, &config, &command_matcher.config);

    let state = Arc::new(AtomicU8::new(State::Idle as u8));

    let (stream, audio_buffer) =
        init_audio_capture(&device, stream_config).expect("failed to init audio capture");

    let (tx, rx) = broadcast::channel::<AudioMessage>(16);

    let engine_tx = tx.clone();

    let assistant_active_worker = assistant_active.clone();

    let stt_state = state.clone();

    let worker_context = WorkerContext {
        stt,
        tts,
        command_matcher,
        llm_engine,
        sample_rate,
        config,
    };

    audio::stt::spawn_transcription_worker(
        rx,
        worker_context,
        rt,
        assistant_active_worker,
        stt_state,
    );

    let vad_state = state.clone();

    tokio::task::spawn_blocking(move || {
        run_vad_loop(
            running,
            audio_buffer,
            engine_tx,
            sample_rate,
            channels,
            assistant_active,
            vad_state,
        );
    });

    Ok((tx, stream))
}
