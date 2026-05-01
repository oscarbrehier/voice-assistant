use rodio::{Decoder, DeviceSinkBuilder, Player};
use std::{io::BufReader, time::Duration};

use crate::state::SharedContext;

pub fn play_mp3_audio(path: &str, context: SharedContext) -> anyhow::Result<()> {
    let sink_handle = DeviceSinkBuilder::open_default_sink()
        .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e))?;

    let player = Player::connect_new(sink_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::new(BufReader::new(file))
        .map_err(|e| anyhow::anyhow!("Failed to decode mp3: {e}"))?;
    
    player.append(source);
    
    {
        let mut lock = context.audio_player.write().unwrap();
        *lock = Some(player);
    }

    loop {
        let (still_active, is_finished) = {
            let lock = context.audio_player.read().unwrap();
            match &*lock {
                Some(p) => {      
                    (true, p.len() == 0)
                }
                None => (false, true)
            }
        };

        if !still_active || is_finished {
            break ;
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    let mut lock = context.audio_player.write().unwrap();
    *lock = None;

    Ok(())
}
