use tauri::PhysicalPosition;

/// Position window in the bottom-right corner with proper padding
pub fn position_window_bottom_right(window: &tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        if let Ok(window_size) = window.outer_size() {
            let screen_size = monitor.size();
            
            let screen_width = screen_size.width as i32;
            let screen_height = screen_size.height as i32;
            let window_width = window_size.width as i32;
            let window_height = window_size.height as i32;
            
            let padding = 18;
            let taskbar_height = 72; // For 150% scaling on 4K
            
            let x = screen_width - window_width - padding;
            let y = screen_height - window_height - taskbar_height - padding;
            
            let position = PhysicalPosition::new(x, y);
            let _ = window.set_position(position);
        }
    }
}
