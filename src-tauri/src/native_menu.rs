// Native Windows context menu implementation
#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        CreatePopupMenu, AppendMenuW, TrackPopupMenu, DestroyMenu, SetForegroundWindow,
        TPM_LEFTALIGN, TPM_TOPALIGN, TPM_RETURNCMD, MF_STRING, MF_SEPARATOR, PostMessageW, WM_NULL,
    },
};

#[cfg(windows)]
use tauri::Manager;

#[cfg(windows)]
const MENU_SHOW: usize = 1001;
#[cfg(windows)]
const MENU_HIDE: usize = 1002;
#[cfg(windows)]
const MENU_PIN: usize = 1003;
#[cfg(windows)]
const MENU_QUIT: usize = 1004;

#[cfg(windows)]
pub fn show_native_context_menu(app: &tauri::AppHandle, x: i32, y: i32) -> Result<(), String> {
    use windows::core::PCWSTR;
    
    unsafe {
        // Create the popup menu
        let hmenu = CreatePopupMenu().map_err(|e| format!("Failed to create menu: {}", e))?;
        
        // Add menu items
        let show_text: Vec<u16> = "Show ClearComms\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_SHOW, PCWSTR(show_text.as_ptr()))
            .map_err(|e| format!("Failed to add Show item: {}", e))?;
        
        let hide_text: Vec<u16> = "Hide ClearComms\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_HIDE, PCWSTR(hide_text.as_ptr()))
            .map_err(|e| format!("Failed to add Hide item: {}", e))?;
        
        // Separator
        AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null())
            .map_err(|e| format!("Failed to add separator: {}", e))?;
        
        let pin_text: Vec<u16> = "Pin on top\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_PIN, PCWSTR(pin_text.as_ptr()))
            .map_err(|e| format!("Failed to add Pin item: {}", e))?;
        
        // Separator
        AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null())
            .map_err(|e| format!("Failed to add separator: {}", e))?;
        
        let quit_text: Vec<u16> = "Quit\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_QUIT, PCWSTR(quit_text.as_ptr()))
            .map_err(|e| format!("Failed to add Quit item: {}", e))?;
        
        // Get a window handle - use the main window
        let hwnd = if let Some(window) = app.get_webview_window("main") {
            let raw_handle = window.hwnd().map_err(|e| format!("Failed to get HWND: {}", e))?.0;
            HWND(raw_handle as *mut _)
        } else {
            HWND(std::ptr::null_mut())
        };
        
        // SetForegroundWindow is required for TrackPopupMenu to work properly
        if !hwnd.is_invalid() {
            let _ = SetForegroundWindow(hwnd);
        }
        
        // Show the menu and get the selected item
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD,
            x,
            y,
            0,
            hwnd,
            None,
        );
        
        // Post a null message to ensure the menu is properly dismissed
        if !hwnd.is_invalid() {
            let _ = PostMessageW(hwnd, WM_NULL, None, None);
        }
        
        // Clean up
        let _ = DestroyMenu(hmenu);
        
        // Handle the selected menu item (cmd is the menu item ID)
        let app_clone = app.clone();
        let selected = cmd.0 as usize;
        
        if selected == MENU_SHOW {
            if let Some(window) = app_clone.get_webview_window("main") {
                position_window_bottom_right(&window);
                let _ = window.show();
                let _ = window.set_focus();
            }
        } else if selected == MENU_HIDE {
            if let Some(window) = app_clone.get_webview_window("main") {
                let _ = window.hide();
            }
        } else if selected == MENU_PIN {
            if let Some(window) = app_clone.get_webview_window("main") {
                position_window_bottom_right(&window);
                let _ = window.show();
                let _ = window.set_focus();
                
                // Toggle always_on_top state
                let current_state = window.is_always_on_top().unwrap_or(false);
                let _ = window.set_always_on_top(!current_state);
            }
        } else if selected == MENU_QUIT {
            std::process::exit(0);
        }
        
        Ok(())
    }
}

#[cfg(windows)]
fn position_window_bottom_right(window: &tauri::WebviewWindow) {
    use tauri::PhysicalPosition;
    
    if let Ok(Some(monitor)) = window.primary_monitor() {
        if let Ok(window_size) = window.outer_size() {
            let screen_size = monitor.size();
            
            // Work with physical pixels
            let screen_width = screen_size.width as i32;
            let screen_height = screen_size.height as i32;
            let window_width = window_size.width as i32;
            let window_height = window_size.height as i32;
            
            // Padding from edges (in physical pixels)
            let padding = 18;
            
            // Windows taskbar height (typically 48-72px in physical pixels depending on scaling)
            // For 150% scaling on 4K: taskbar is ~72px in physical pixels
            let taskbar_height = 72;
            
            let x = screen_width - window_width - padding;
            let y = screen_height - window_height - taskbar_height - padding;
            
            let position = PhysicalPosition::new(x, y);
            let _ = window.set_position(position);
        }
    }
}

#[cfg(not(windows))]
pub fn show_native_context_menu(_app: &tauri::AppHandle, _x: i32, _y: i32) -> Result<(), String> {
    Err("Native context menu is only available on Windows".to_string())
}
