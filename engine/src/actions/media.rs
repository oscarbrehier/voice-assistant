use enigo::{Enigo, Key, Keyboard, Settings};

pub fn play_pause() -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).unwrap();

    enigo.key(Key::MediaPlayPause, enigo::Direction::Press)?;

    Ok(())
}

pub fn next_track() -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    enigo.key(Key::MediaNextTrack, enigo::Direction::Press)?;
    Ok(())
}

pub fn previous_track() -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).unwrap();
    enigo.key(Key::MediaNextTrack, enigo::Direction::Press)?;
    Ok(())
}

// pub fn adjust_volume(delta: i32) -> anyhow::Result<()> {
//     Ok(())
// }