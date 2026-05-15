use tokio::sync::broadcast;

use crate::{
    State,
    proactive::TriggerKind,
    worker::{Packet, Urgency, WorkerContext},
};

pub async fn process_proactive_trigger(
    kind: TriggerKind,
    context: String,
    urgency: Urgency,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
) {
    match ctx.llm_engine.generate_proactive(&context, &urgency).await {
        Ok(Some(message)) => {
            let _ = ctx
                .tts
                .speak(&message, ctx.global_ctx.clone(), tx, None, false);
        }
        Ok(None) => {}
        Err(e) => {
            println!("Proactive generation failed: {e}");
            State::broadcast(State::Idle, &ctx.global_ctx.engine_state, tx);
        }
    }
}
