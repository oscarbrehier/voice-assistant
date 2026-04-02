use engine::EnginePaths;
use engine::{start_engine, AudioMessage};
use tauri::{Emitter, PhysicalPosition};
use tauri::Manager;
#[cfg(target_os = "windows")]
use window_vibrancy::apply_acrylic;
#[cfg(target_os = "macos")]
use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy};

mod commands;

struct AudioStream(cpal::Stream);

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
                    x: screen_pos.x + (screen_size.width as i32 - window_size.width as i32) - margin,
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

                        while let Ok(msg) = ui_rx.recv().await {
                            if let AudioMessage::Pulse(samples) = msg {
                                let peak = samples.iter().map(|s| s.abs()).fold(0.0, f32::max);

                                let sum_squares: f32 = samples.iter().map(|&s| s * s).sum();
                                let rms = (sum_squares / samples.len() as f32).sqrt();

                                let combined = (rms * 0.7) + (peak * 0.3);
                                let sensitivity = 20.0;
                                let volume = (combined * sensitivity).powf(0.6).clamp(0.0, 1.0);

                                let _ = handle.emit("audio", volume);
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
        .invoke_handler(tauri::generate_handler![
            commands::window::set_window_size
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
