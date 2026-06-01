use engine::{EnginePaths, llm::mistral::call_mistral_with_vision, start_engine};
use mel_spec::fbank::FbankConfig;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{fs, signal};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = EnginePaths {
        config_dir: PathBuf::from("config"),
        script_dir: PathBuf::from("engine/python"),
    };

    match start_engine(paths, Some(3), Some(0)).await {
        Ok((tx, _stream, _loop_back_stream)) => {
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
