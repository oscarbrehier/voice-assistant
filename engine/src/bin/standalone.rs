use std::path::PathBuf;

use engine::{EnginePaths, start_engine};
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = EnginePaths {
        config_dir: PathBuf::from("config"),
        script_dir: PathBuf::from("engine/python"),
    };

    match start_engine(paths, Some(20)).await {
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

    Ok(())
}
