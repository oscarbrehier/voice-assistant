use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use num_traits::FromPrimitive;
use tokio::sync::broadcast;

use crate::{ActiveGuard, State, worker::{Packet, WorkerContext}};

pub async fn process_speech_logic(
    trimmed: String,
    ctx: &mut WorkerContext,
    tx: &broadcast::Sender<Packet>,
    assistant_active: &Arc<AtomicBool>,
) {
    println!("TRANSCRIPTION: {}", trimmed);

    let _ = tx.send(Packet::Transcription(trimmed.clone()));

    let current_state_u8 = ctx.global_ctx.engine_state.load(Ordering::SeqCst);
    let current_state = State::from_u8(current_state_u8).unwrap_or(State::Idle);

    if current_state == State::Active {
        State::broadcast(State::Processing, &ctx.global_ctx.engine_state, &tx);

        let _guard = ActiveGuard::new(
            assistant_active.clone(),
            Arc::clone(&ctx.global_ctx.engine_state),
            tx.clone(),
        );

        let (core_identity, relevant_memories) = {
            let memory_guard = ctx.memory.lock().expect("Memory mutex poisoned");

            let core = if ctx.llm_engine.needs_identity_refresh {
                memory_guard.get_core_identity().unwrap_or_default()
            } else {
                vec![]
            };

            let relevant = memory_guard
                .get_relevant_memories(&trimmed, None)
                .unwrap_or_default();

            (core, relevant)
        };

        let tools = ctx.command_config.tools.clone();

        match ctx
            .llm_engine
            .generate(
                &trimmed,
                &ctx.global_ctx,
                core_identity,
                relevant_memories
            )
            .await
        {
            Ok(response) => {
                let _ = ctx
                    .tts
                    .speak(&response, ctx.global_ctx.clone(), &tx, None, false);
            }
            Err(e) => eprintln!("Failed to generate: {e}"),
        }
    }
}