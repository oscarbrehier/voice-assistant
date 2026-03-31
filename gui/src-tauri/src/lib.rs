use engine::EnginePaths;
use engine::{start_engine, AudioMessage};
use tauri::Emitter;
use tauri::Manager;

struct AudioStream(cpal::Stream);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let config_path = app
                .path()
                .resource_dir()
                .expect("failed to get resource dir")
                .join("config");

            let scripts_path = app
                .path()
                .resource_dir()
                .expect("failed to get resource dir")
                .join("python");

            tauri::async_runtime::spawn(async move {
                let paths = EnginePaths {
                    config_dir: config_path,
                    script_dir: scripts_path,
                };

                match start_engine(paths, Some(22)).await {
                    Ok((tx, stream)) => {
                        handle.manage(AudioStream(stream));
                        let mut ui_rx = tx.subscribe();

                        while let Ok(msg) = ui_rx.recv().await {
                            if let AudioMessage::Pulse(samples) = msg {
                                let rms = (samples.iter().map(|x| x * x).sum::<f32>()
                                    / samples.len() as f32)
                                    .sqrt();
                                let _ = handle.emit("audio", rms);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to start engine: {:?}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
