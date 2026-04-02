use engine::{start_engine, Packet};
use engine::{EnginePaths, State};
use serde::Serialize;
use tauri::Manager;
use tauri::{Emitter, PhysicalPosition};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_acrylic;
#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

mod commands;

struct AudioStream(cpal::Stream);

#[derive(Serialize, Clone, Debug)]
pub struct UIEvent {
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("failed to get app window");

            if let Some(monitor) = window.current_monitor().unwrap() {
                let screen_size = monitor.size();
                let screen_pos = monitor.position();

                let window_size = window.outer_size().unwrap();

                let margin = 24;

                let bottom_right = PhysicalPosition {
                    x: screen_pos.x + (screen_size.width as i32 - window_size.width as i32)
                        - margin,
                    y: screen_pos.y + margin,
                };

                window.set_position(bottom_right).unwrap();
            }

            window.set_shadow(false).unwrap();

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

                        while let Ok(event) = ui_rx.recv().await {
                            let _ = handle.emit("engine-update", &event);
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to start engine: {:?}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::window::set_window_size])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
