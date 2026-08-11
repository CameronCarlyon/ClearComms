// Native Windows context menu implementation
#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        CreatePopupMenu, AppendMenuW, TrackPopupMenu, DestroyMenu, SetForegroundWindow,
        CheckMenuItem, TPM_LEFTALIGN, TPM_TOPALIGN, TPM_RETURNCMD, MF_STRING, MF_SEPARATOR,
        MF_CHECKED, MF_UNCHECKED, MF_BYCOMMAND,
        PostMessageW, WM_NULL,
    },
};

#[cfg(windows)]
use tauri::{Manager, Emitter};

#[cfg(windows)]
const MENU_SHOW: usize = 1001;
#[cfg(windows)]
const MENU_PIN: usize = 1003;
#[cfg(windows)]
const MENU_RESTART: usize = 1005;
#[cfg(windows)]
const MENU_QUIT: usize = 1004;

/// Run `f` on the main thread *after* the current callback has unwound.
///
/// `AppHandle::run_on_main_thread` cannot be used directly here: when it is
/// already on the main thread, tauri-runtime-wry's `send_user_message`
/// short-circuits and invokes the closure inline, which is precisely what must
/// be avoided. Bouncing the request through a short-lived thread forces it onto
/// the event loop proxy instead, so the closure is queued and only runs once the
/// event loop reaches its next clean iteration — after the tray callback and the
/// `TrackPopupMenu` modal loop have both returned.
#[cfg(windows)]
fn defer_to_event_loop<F>(app: &tauri::AppHandle, what: &'static str, f: F)
where
    F: FnOnce(tauri::AppHandle) + Send + 'static,
{
    let app = app.clone();
    let spawn_result = std::thread::Builder::new()
        .name("menu-defer".to_string())
        .spawn(move || {
            let app_for_task = app.clone();
            if let Err(e) = app.run_on_main_thread(move || f(app_for_task)) {
                tracing::error!("[Menu] Failed to queue {} on main thread: {}", what, e);
            }
        });

    if let Err(e) = spawn_result {
        tracing::error!("[Menu] Failed to spawn deferral thread for {}: {}", what, e);
    }
}

#[cfg(windows)]
pub fn show_native_context_menu(app: &tauri::AppHandle, x: i32, y: i32) -> Result<(), String> {
    use windows::core::PCWSTR;
    
    unsafe {
        // Create the popup menu
        let hmenu = CreatePopupMenu().map_err(|e| format!("Failed to create menu: {}", e))?;
        
        // Add menu items
        let show_text: Vec<u16> = "Open\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_SHOW, PCWSTR(show_text.as_ptr()))
            .map_err(|e| format!("Failed to add Open item: {}", e))?;
        
        // Use tracked pin state with the native Windows menu checkmark.
        let is_pinned = crate::get_pin_state();
        let pin_label = "Pin on Top\0";
        let pin_text: Vec<u16> = pin_label.encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_PIN, PCWSTR(pin_text.as_ptr()))
            .map_err(|e| format!("Failed to add Pin item: {}", e))?;
        let pin_check_state = if is_pinned { MF_CHECKED } else { MF_UNCHECKED };
        let pin_flags = (MF_BYCOMMAND | pin_check_state).0;
        let _ = CheckMenuItem(hmenu, MENU_PIN as u32, pin_flags);
        
        // Separator
        AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null())
            .map_err(|e| format!("Failed to add separator: {}", e))?;
        
        let restart_text: Vec<u16> = "Restart\0".encode_utf16().collect();
        AppendMenuW(hmenu, MF_STRING, MENU_RESTART, PCWSTR(restart_text.as_ptr()))
            .map_err(|e| format!("Failed to add Restart item: {}", e))?;
        
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
        match cmd.0 as usize {
            MENU_SHOW => {
                if let Err(e) = crate::show_main_window_internal(app) {
                    tracing::error!("[Menu] Failed to show main window: {}", e);
                }
            }
            MENU_PIN => {
                if let Some(window) = app.get_webview_window("main") {
                    let is_visible = window.is_visible().unwrap_or(false);

                    match crate::perform_pin_toggle(&window) {
                        Ok(new_pin_state) => {
                            if let Err(e) = app.emit("window-pin-changed", new_pin_state) {
                                tracing::error!("[Menu] Failed to emit pin state event: {}", e);
                            }

                            if !is_visible {
                                tracing::info!("[Menu] Window shown and pinned on top");
                            } else if new_pin_state {
                                tracing::info!("[Menu] Pin on top toggled: false -> true");
                            } else {
                                tracing::info!("[Menu] Pin on top toggled: true -> false (hidden)");
                            }
                        }
                        Err(e) => {
                            tracing::error!("[Menu] Failed to toggle pin: {}", e);
                        }
                    }
                }
            }
            // Restart and Quit tear down windows, the WebView2 host and every
            // background thread. Doing that inline would run the teardown from
            // inside the tray-icon callback, which is itself nested inside the
            // message loop that TrackPopupMenu just re-entered — destroying
            // objects the event loop is still walking. Both are therefore posted
            // back to the event loop and run once this callback has unwound.
            MENU_RESTART => {
                defer_to_event_loop(app, "restart", |app| {
                    if let Err(e) = crate::restart_application_internal(&app) {
                        tracing::error!("[Menu] Failed to restart application: {}", e);
                    }
                });
            }
            MENU_QUIT => {
                // Route through the shared graceful-quit helper so all background
                // threads receive a clean shutdown signal before the process exits.
                defer_to_event_loop(app, "quit", |app| {
                    crate::perform_graceful_quit(&app);
                });
            }
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn show_native_context_menu(_app: &tauri::AppHandle, _x: i32, _y: i32) -> Result<(), String> {
    Err("Native context menu is only available on Windows".to_string())
}
