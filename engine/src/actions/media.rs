use enigo::{Enigo, Key, Keyboard, Settings};

pub fn play_pause() -> anyhow::Result<()> {

	let mut enigo = Enigo::new(&Settings::default()).unwrap();

	enigo.key(Key::MediaPlayPause, enigo::Direction::Press)?;

	Ok(())

}