use crate::{actions::{datetime::get_time, media::play_pause, system::spawn_app}, audio::tts::speak};

pub mod datetime;
pub mod media;
pub mod system;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    PlayMusic,
    OpenApp(String),
    GetTime,
    Unknown,
}

#[derive(Debug)]
pub enum ActionResult {
	Success,
	Message(String)
}

pub fn execute_action(action: Action) -> anyhow::Result<ActionResult> {
	
    match action {
        Action::PlayMusic => {
			play_pause()?;
			Ok(ActionResult::Success)
		},
        Action::OpenApp(app) => {
			spawn_app(app)?;
			Ok(ActionResult::Success)
		},
        Action::GetTime => {
			let time = get_time()?;
			Ok(ActionResult::Message(time))
		},
        Action::Unknown => Ok(ActionResult::Success),
    }

}

pub fn handle_action(action: Action) -> anyhow::Result<()> {

	match execute_action(action)? {
		ActionResult::Success => Ok(()),
		ActionResult::Message(msg) => {
			speak(&msg)?;
			Ok(())
		}
	}

}