use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::{
    io::{BufReader}
};

pub fn play_mp3_audio(path: &str) -> anyhow::Result<()> {
    let sink_handle = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e))?;

    let player = Player::connect_new(sink_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Failed to decode mp3: {e}"))?;

    player.append(source);
    player.sleep_until_end();

    drop(sink_handle);

    Ok(())
}
