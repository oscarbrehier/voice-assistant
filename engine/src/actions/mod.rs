use std::sync::{Arc, atomic::AtomicU8};

use anyhow::Ok;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{
    Packet,
    actions::{datetime::get_time, media::{next_track, play_pause, previous_track}, system::spawn_app},
    audio::tts::TTSService, state::SharedContext,
};

pub mod datetime;
pub mod media;
pub mod system;
pub mod obsidian;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "action", content = "params", rename_all = "PascalCase")]
pub enum Action {
    PlayMusic,
    #[serde(rename_all = "lowercase")]
    OpenApp {
        app: String,
    },
    GetTime {},
    NextTrack {},
    PreviousTrack {},
    Unknown,
}

impl Action {
    pub fn execute(&self, template: Option<String>) -> anyhow::Result<ActionResult> {
        match self {
            Action::PlayMusic => {
                play_pause()?;
                Ok(ActionResult::Success)
            }
            Action::OpenApp { app } => {
                spawn_app(app.clone())?;
                Ok(ActionResult::Success)
            }
            Action::GetTime {} => {
                let time = get_time(template)?;
                Ok(ActionResult::Message(time))
            }
            Action::NextTrack {} => {
                next_track()?;
                Ok(ActionResult::Success)
            }
            Action::PreviousTrack {} => {
                previous_track()?;
                Ok(ActionResult::Success)
            }
            Action::Unknown => Ok(ActionResult::Success),
        }
    }
}

#[derive(Debug)]
pub enum ActionResult {
    Success,
    Message(String),
}

pub fn handle_action(
    action: Action,
    tts: &TTSService,
    ctx: SharedContext,
    sender: &broadcast::Sender<Packet>,
    template: Option<String>,
) -> anyhow::Result<()> {
    match action.execute(template)? {
        ActionResult::Success => Ok(()),
        ActionResult::Message(msg) => {
            tts.speak(&msg, ctx, sender)?;
            Ok(())
        }
    }
}
