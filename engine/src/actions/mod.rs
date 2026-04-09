use std::sync::{Arc, atomic::AtomicU8};

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::{
    Packet, actions::{datetime::get_time, media::play_pause, system::spawn_app}, audio::tts::TTSService
};

pub mod datetime;
pub mod media;
pub mod system;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "action", content = "params", rename_all = "PascalCase")]
pub enum Action {
    PlayMusic,
    #[serde(rename_all = "lowercase")]
    OpenApp { app: String },
    GetTime {},
    Unknown,
}

impl Action {
    pub fn execute(&self, template: Option<String>) -> anyhow::Result<ActionResult> {
        match self {
            Action::PlayMusic => {
                play_pause()?;
                Ok(ActionResult::Success)
            }
            Action::OpenApp { app} => {
                spawn_app(app.clone())?;
                Ok(ActionResult::Success)
            }
            Action::GetTime {} => {
                let time = get_time(template)?;
                Ok(ActionResult::Message(time))
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

pub fn handle_action(action: Action, tts: &TTSService, state: Arc<AtomicU8>, sender: &broadcast::Sender<Packet>, template: Option<String>) -> anyhow::Result<()> {
    match action.execute(template)? {
        ActionResult::Success => Ok(()),
        ActionResult::Message(msg) => {
            tts.speak(&msg, state, sender)?;
            Ok(())
        }
    }
}
