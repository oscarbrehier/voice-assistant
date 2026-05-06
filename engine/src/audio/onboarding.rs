use tokio::sync::broadcast;

use crate::{Packet, State, audio::tts::{TTSService, run_self_calibration}, state::SharedContext};

pub async fn run_startup_verifications(
    ctx: SharedContext,
    tts: &TTSService,
    tx: &broadcast::Sender<Packet>,
) -> anyhow::Result<()> {
    let (is_enrolled, needs_calibration) = {
        let speaker = ctx.speaker.read();
        (
            speaker.is_enrolled(),
            speaker.negative_embeddings.is_empty(),
        )
    };

    if !is_enrolled {
        State::broadcast(State::Enrolling, &ctx.engine_state, &tx);

        let msg =
            "Welcome. I need to capture your voiceprint. Please repeat the sentences on the screen";
        tts.speak(msg, ctx.clone(), tx, Some(State::Enrolling), false)?;
        println!(
            "The quick brown fox jumps over the lazy dog, but the rainy weather in Paris might slow him down today."
        );
    } else if needs_calibration {
        State::broadcast(State::Calibrating, &ctx.engine_state, &tx);

        let msg = "I have you voiceprint, but I need to calibrate my echo cancellation. Please stay quiet.";
        tts.speak(msg, ctx.clone(), tx, Some(State::Calibrating), true)?;

        let cal_ctx = ctx.clone();
        let cal_tts = tts.clone();
        let cal_tx = tx.clone();

        tokio::spawn(async move {
            if let Err(e) = run_self_calibration(cal_ctx, &cal_tts, &cal_tx).await {
                eprintln!("Startup calibration failed: {e}");
            }
        });
    }

    Ok(())
}
