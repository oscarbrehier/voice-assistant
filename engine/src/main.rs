use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc,
    }, time::Duration
};

use clap::Parser;
use tokio::runtime::Runtime;

use crate::{
    audio::{
        capture::{init_audio_capture, run_vad_loop},
        setup_audio_device, stt::stt_service::STTService
    }, commands::CommandMatcher, config::Config, llm::LLMEngine
};

mod config;
mod actions;
mod audio;
mod commands;
mod llm;

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
    assistant: Arc<AtomicBool>
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

fn main() -> Result<(), anyhow::Error> {
    dotenv::dotenv().ok();

    tracing_subscriber::fmt::init();

    let running = Arc::new(AtomicBool::new(true));
    let running_clone = running.clone();
    ctrlc::set_handler(move || {
        running_clone.store(false, Ordering::SeqCst);
    })
    .expect("failed to set ctrlc handler");

    let assistant_active = Arc::new(AtomicBool::new(false));

    let opt = Opt::parse();
    let config = Config::load("config/config.json")?;

    let (device, stream_config) = setup_audio_device(opt.device)?;
    let sample_rate = stream_config.sample_rate() as usize;
    let channels = stream_config.channels() as usize;
    
    let stt = STTService::new()?;
    let rt = Runtime::new()?;
    let command_matcher = CommandMatcher::from_file("config/commands.json")?;
    let llm_engine = LLMEngine::new(&config, &command_matcher.config);

    let state = Arc::new(AtomicU8::new(State::Idle as u8));

    let (stream, audio_buffer) =
        init_audio_capture(&device, stream_config).expect("failed to init audio capture");
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    let assistant_active_worker = assistant_active.clone();

    let stt_state = state.clone();
    let worker_handle =
        audio::stt::spawn_transcription_worker(rx, stt, command_matcher, llm_engine, rt, sample_rate, assistant_active_worker, config, stt_state);

    let vad_state = state.clone();
    run_vad_loop(running, audio_buffer, tx, sample_rate, channels, assistant_active, vad_state);

    drop(stream);

    if let Err(e) = worker_handle.join() {
        eprintln!("Transcription thread panicked: {:?}", e);
    }

    Ok(())
}
