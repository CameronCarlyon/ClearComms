// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, PhysicalPosition};
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};

mod audio_management;
mod hardware_input;
mod simvar_input;
mod native_menu;

/// Resize window to fit content width and height
#[tauri::command]
fn resize_window_to_content(app: tauri::AppHandle, session_count: usize) -> Result<String, String> {
    // Calculate width based on number of sessions
    // Each channel strip is ~90px + 12px gap, plus 24px padding on sides
    let base_width = 48; // Left + right padding (24px each)
    let channel_width = 90; // Width per channel
    let gap_width = 12; // Gap between channels
    
    let content_width = if session_count > 0 {
        base_width + (channel_width * session_count as u32) + (gap_width * (session_count.saturating_sub(1)) as u32)
    } else {
        400 // Default width if no sessions
    };
    
    // Clamp width to reasonable bounds (min 400px, max 1400px to handle many apps)
    let new_width = content_width.clamp(400, 1400);
    
    // Calculate height based on content
    // Header: ~70px
    // Channel strip: ~380px (fader 180px + buttons + spacing)
    // Footer: ~50px
    // Total with padding: ~800px
    let new_height = 1000;
    
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: new_width,
            height: new_height,
        }));
        
        // Re-position window after resize to keep it bottom-right with proper padding
        position_window_bottom_right(&window);
        
        return Ok(format!("Window resized to {}x{} for {} session(s)", new_width, new_height, session_count));
    }
    
    Ok("Window size unchanged".to_string())
}

/// Show the main application window
#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        position_window_bottom_right(&window);
        let _ = window.show();
        let _ = window.set_focus();
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Hide the main application window
#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Toggle pin on top for main window
#[tauri::command]
fn toggle_pin_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        position_window_bottom_right(&window);
        let _ = window.show();
        let _ = window.set_focus();
        
        let current_state = window.is_always_on_top().unwrap_or(false);
        let _ = window.set_always_on_top(!current_state);
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Quit the application
#[tauri::command]
fn quit_application() {
    std::process::exit(0);
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Get main window and position it
            if let Some(window) = app.get_webview_window("main") {
                // Position window in bottom-right corner
                position_window_bottom_right(&window);
                
                // Don't show window on startup (starts in tray)
                let _ = window.hide();
            }
            
            // Build tray icon without menu (we'll use custom window)
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClearComms - Aviation Audio Control")
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    
                    match event {
                        tauri::tray::TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            // Left click: Toggle main window
                            if let Some(window) = app.get_webview_window("main") {
                                if window.is_visible().unwrap_or(false) {
                                    let _ = window.hide();
                                } else {
                                    position_window_bottom_right(&window);
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                        }
                        tauri::tray::TrayIconEvent::Click {
                            button: MouseButton::Right,
                            button_state: MouseButtonState::Up,
                            position,
                            ..
                        } => {
                            // Show native Windows context menu
                            let app_clone = app.clone();
                            let x = position.x as i32;
                            let y = position.y as i32;
                            
                            if let Err(e) = native_menu::show_native_context_menu(&app_clone, x, y) {
                                eprintln!("[Tray] Error showing native menu: {}", e);
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            // Get main window and position it
            if let Some(window) = app.get_webview_window("main") {
                // Position window in bottom-right corner
                position_window_bottom_right(&window);
                
                // Don't show window on startup (starts in tray)
                let _ = window.hide();
            }
            
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent window from closing, hide it instead
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            hardware_input::init_direct_input,
            hardware_input::get_direct_input_status,
            hardware_input::enumerate_input_devices,
            hardware_input::get_all_axis_values,
            hardware_input::update_test_axis_value,
            audio_management::init_audio_manager,
            audio_management::get_audio_sessions,
            audio_management::set_session_volume,
            audio_management::set_session_mute,
            resize_window_to_content,
            show_main_window,
            hide_main_window,
            toggle_pin_window,
            quit_application,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    clearcomms_lib::run()
}

/// Position window in the bottom-right corner of the primary monitor
fn position_window_bottom_right(window: &tauri::WebviewWindow) {
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
            
            println!("Screen: {}x{}, Window: {}x{}, Position: ({}, {}), Taskbar: {}", 
                     screen_width, screen_height, window_width, window_height, x, y, taskbar_height);
            
            let position = PhysicalPosition::new(x, y);
            let _ = window.set_position(position);
        }
    }
}
