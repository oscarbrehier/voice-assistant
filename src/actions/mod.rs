use crate::actions::{media::play_pause, system::spawn_app};

pub mod media;
pub mod system;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
	PlayMusic,
	OpenApp(String),
	Unknown
}

pub fn execute_action(action: Action) -> anyhow::Result<()> {

	match action {
		Action::PlayMusic => play_pause(),
		Action::OpenApp(app) => spawn_app(app),
		Action::Unknown => Ok(())
	}

}