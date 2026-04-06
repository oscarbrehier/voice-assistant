use tauri::{Manager, Runtime, WebviewWindow};
use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

#[tauri::command]
pub async fn set_window_size(window: WebviewWindow, width: u32, height: u32) {
    let monitor = window.current_monitor().unwrap().unwrap();
    let scale_factor = monitor.scale_factor();
    let screen_size = monitor.size();
    let screen_pos = monitor.position();

    let target_width_phys = (width as f64 * scale_factor) as i32;
    let target_height_phys = (height as f64 * scale_factor) as i32;
    let margin_phys = (24.0 * scale_factor) as i32;

    let new_x = screen_pos.x + (screen_size.width as i32 - target_width_phys) - margin_phys;
    let new_y = screen_pos.y + margin_phys;

    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;

        let hwnd = window.hwnd().unwrap().0 as *mut c_void;

        unsafe {
            SetWindowPos(
                hwnd as _,
                std::ptr::null_mut(),
                new_x,
                new_y,
                target_width_phys,
                target_height_phys,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }

        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
            target_width_phys as u32,
            target_height_phys as u32,
        )));
    }

    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_size(tauri::Size::Logical(tauri::LogicalSize::new(
                width as f64,
                height as f64,
            )))
            .unwrap();
        window
            .set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                new_x, new_y,
            )))
            .unwrap();
    }
}
