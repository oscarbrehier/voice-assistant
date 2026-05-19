use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::broadcast;

use crate::{
    audio::tts::TTSService,
    config::Config,
    llm::LLMEngine,
    memory::MemoryManager,
    state::SharedContext,
    worker::{Packet, WorkerContext},
};

pub mod eligibility;
pub mod theme;

pub struct RitualConfig {
    pub day_boundary_hour: u32,
    pub theme_path: PathBuf,
    pub theme_volume: f32,
    pub ducked_volume: f32,
    pub fade_in_secs: f32,
    pub fade_out_secs: f32,
}

pub struct RitualContext {
    pub global: SharedContext,
    pub memory: Arc<Mutex<MemoryManager>>,
    pub llm_engine: LLMEngine,
    pub tts: TTSService,
    pub config: Config,
    pub tx: broadcast::Sender<Packet>,
}

pub async fn maybe_run_startup_ritual(
    config: RitualConfig,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
) -> anyhow::Result<()> {
    let is_first_launch =
        eligibility::is_first_launch_today(&ctx.memory, config.day_boundary_hour)?;
    if !is_first_launch {
        return Ok(());
    }

    let theme_handle =
        theme::start_with_fade_in(&config.theme_path, config.theme_volume, config.fade_in_secs)?;

    tokio::time::sleep(Duration::from_secs(5)).await;

    let greeting = match ctx.llm_engine.generate_greeting(&ctx.config).await? {
        Some(text) => text,
        None => return Ok(()),
    };

    theme_handle.duck(config.ducked_volume);

    ctx.tts
        .speak_async(&greeting, ctx.global_ctx.clone(), &tx.clone(), None, false)
        .await?;

    theme_handle.unduck(config.theme_volume);

    tokio::time::sleep(Duration::from_secs(30)).await;

    theme_handle.fade_out_and_stop(config.fade_out_secs).await;

    if let Err(e) = eligibility::record_greeting_now(&ctx.memory) {
        eprintln!("Failed to record greeting timestamp: {e}");
    }

    Ok(())
}
