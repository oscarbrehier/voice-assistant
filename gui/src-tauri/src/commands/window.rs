use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

#[tauri::command]
pub fn set_window_size(window: WebviewWindow, width: u16, height: u16) {
    window.set_size(LogicalSize::new(width, height)).unwrap();

    if let Some(monitor) = window.current_monitor().unwrap() {
        let screen_size = monitor.size();
        let screen_pos = monitor.position();

        let window_size = window.outer_size().unwrap();

        let margin = 24;

        let pos_x = screen_pos.x + (screen_size.width as i32 - window_size.width as i32) - margin;
        let pos_y = screen_pos.y + margin;

        let bottom_right = PhysicalPosition { x: pos_x, y: pos_y };

        window.set_position(bottom_right).unwrap();
    }
}
