use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};

use engine::{
    EnginePaths,
    actions::obsidian::{
        VaultConfig, append_to_note, create_note, get_recent_notes, list_vault_index,
        read_note_content, search_notes, smart_append_to_section,
    },
    monitor, start_engine,
    state::{GlobalContext, SharedContext, Vitals},
};
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = EnginePaths {
        config_dir: PathBuf::from("config"),
        script_dir: PathBuf::from("engine/python"),
    };

    match start_engine(paths, Some(17)).await {
        Ok((tx, _stream)) => {
            println!("engile started");

            let mut rx = tx.subscribe();

            loop {
                tokio::select! {
                    event = rx.recv() => {
                        match event {
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    _ = signal::ctrl_c() => {
                        println!("\nshutting down.");
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("failed to start engine: {:?}", e);
        }
    }

    // let shared_state: SharedContext = Arc::new(GlobalContext {
    //     telemetry: Arc::new(RwLock::new(Vitals::default())),
    // });

    // let monitor_state = Arc::clone(&shared_state);
    // tokio::spawn(async move {
    //     monitor::run_monitoring_loop(monitor_state).await;
    // });

    // signal::ctrl_c().await?;
    // println!("\nShutting down...");

    // get_system_stats()?;

    Ok(())
}
