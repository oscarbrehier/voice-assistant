use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    }, time::Duration
};

use clap::Parser;
use tokio::runtime::Runtime;

use crate::{
    audio::{
        capture::{init_audio_capture, run_vad_loop},
        setup_audio_device, stt::stt_service::STTService
    },
    commands::CommandMatcher,
    llm::LLMEngine,
};

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
enum State {
    Silence,
    Recording,
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

    let (device, config) = setup_audio_device(opt.device)?;
    let sample_rate = config.sample_rate() as usize;
    let channels = config.channels() as usize;
    
    let stt = STTService::new()?;
    let rt = Runtime::new()?;
    let command_matcher = CommandMatcher::from_file("config/commands.json")?;
    let llm_engine = LLMEngine::new(&command_matcher.config);

    let (stream, audio_buffer) =
        init_audio_capture(&device, config).expect("failed to init audio capture");
    let (tx, rx) = mpsc::channel::<Vec<f32>>();

    let assistant_active_worker = assistant_active.clone();

    let worker_handle =
        audio::stt::spawn_transcription_worker(rx, stt, command_matcher, llm_engine, rt, sample_rate, assistant_active_worker);

    run_vad_loop(running, audio_buffer, tx, sample_rate, channels, assistant_active);

    drop(stream);

    if let Err(e) = worker_handle.join() {
        eprintln!("Transcription thread panicked: {:?}", e);
    }

    Ok(())
}
