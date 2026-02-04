//! Window Utilities
//!
//! Helper functions for positioning and managing the ClearComms window.

use tauri::PhysicalPosition;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Padding from screen edge in pixels
const WINDOW_PADDING: i32 = 18;

/// Estimated Windows taskbar height in pixels (for 150% scaling on 4K displays)
/// This accounts for the taskbar so the window doesn't overlap it.
const TASKBAR_HEIGHT: i32 = 72;

// ─────────────────────────────────────────────────────────────────────────────
// Window Positioning
// ─────────────────────────────────────────────────────────────────────────────

/// Position the window in the bottom-right corner of the primary monitor.
///
/// This places the window above the Windows taskbar with appropriate padding,
///
/// # Arguments
/// * `window` - The Tauri webview window to position
///
/// # Notes
/// - Uses the primary monitor for positioning
/// - Accounts for taskbar height and screen edge padding
/// - Silently fails if monitor or window size cannot be determined
pub fn position_window_bottom_right(window: &tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = window.primary_monitor() {
        if let Ok(window_size) = window.outer_size() {
            let screen_size = monitor.size();
            
            let screen_width = screen_size.width as i32;
            let screen_height = screen_size.height as i32;
            let window_width = window_size.width as i32;
            let window_height = window_size.height as i32;
            
            let x = screen_width - window_width - WINDOW_PADDING;
            let y = screen_height - window_height - TASKBAR_HEIGHT - WINDOW_PADDING;
            
            let position = PhysicalPosition::new(x, y);
            let _ = window.set_position(position);
        }
    }
}
