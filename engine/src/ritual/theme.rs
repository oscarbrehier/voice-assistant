use std::{path::Path, time::Duration};

use rodio::{Decoder, MixerDeviceSink, Player, Source};

pub struct ThemeHandle {
    player: Player,
    _device: MixerDeviceSink,
}

pub fn start_with_fade_in(
    path: &Path,
    target_volume: f32,
    fade_in_secs: f32,
) -> anyhow::Result<ThemeHandle> {
    let stream_handle = rodio::DeviceSinkBuilder::open_default_sink()
        .map_err(|e| anyhow::anyhow!("Failed to open audio device: {}", e))?;

    let player = rodio::Player::connect_new(&stream_handle.mixer());

    let file = std::fs::File::open(path)?;
    let source = Decoder::try_from(file)?
        .amplify(target_volume)
        .fade_in(Duration::from_secs_f32(fade_in_secs));

    player.append(source);

    Ok(ThemeHandle {
        player,
        _device: stream_handle,
    })
}

impl ThemeHandle {
    pub fn duck(&self, duck_volume: f32) {
        self.player.set_volume(duck_volume);
    }

    pub fn unduck(&self, full_volume: f32) {
        self.player.set_volume(full_volume);
    }

    pub async fn fade_out_and_stop(self, fade_secs: f32) {
        let start = self.player.volume();
        let step_dur = Duration::from_millis(30);
        let steps = (fade_secs * 1000.0 / 30.0) as usize;

        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            self.player.set_volume(start + (0.0 - start) * t);
            tokio::time::sleep(step_dur).await;
        }

        self.player.stop();
    }
}
