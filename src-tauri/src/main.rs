// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};

mod audio_management;
mod hardware_input;
mod simvar_input;
mod native_menu;
mod window_utils;

use window_utils::position_window_bottom_right;

/// Resize window to fit content width and height
#[tauri::command]
fn resize_window_to_content(app: tauri::AppHandle, session_count: usize) -> Result<String, String> {
    let base_width = 400;
    let channel_width = 109;
    
    let new_width = if session_count <= 1 {
        base_width
    } else {
        base_width + (channel_width * (session_count - 1) as u32)
    };
    
    let new_height = 1000;
    
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: new_width,
            height: new_height,
        }));
        
        position_window_bottom_right(&window);
        
        return Ok(format!("Resized to {}x{} for {} session(s)", new_width, new_height, session_count));
    }
    
    Err("Main window not found".to_string())
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

/// Check if window is pinned on top
#[tauri::command]
fn is_window_pinned(app: tauri::AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        Ok(window.is_always_on_top().unwrap_or(false))
    } else {
        Err("Main window not found".to_string())
    }
}

/// Quit the application
#[tauri::command]
fn quit_application() {
    std::process::exit(0);
}

/// Open a URL in the default browser
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

fn main() {
    let ignore_focus_loss = Arc::new(Mutex::new(false));
    let ignore_focus_loss_for_setup = ignore_focus_loss.clone();
    let ignore_focus_loss_for_events = ignore_focus_loss.clone();

    tauri::Builder::default()
        .setup(move |app| {
            // Get main window and position it
            if let Some(window) = app.get_webview_window("main") {
                // Position window in bottom-right corner
                position_window_bottom_right(&window);
                
                // Don't show window on startup (starts in tray)
                let _ = window.hide();
            }
            
            // Build tray icon without menu (we'll use custom window)
            let ignore_focus_loss_tray = ignore_focus_loss_for_setup.clone();
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClearComms - Aviation Audio Control")
                .on_tray_icon_event(move |tray, event| {
                    let app = tray.app_handle();
                    
                    match event {
                        tauri::tray::TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            // Left click: Toggle main window
                            if let Some(window) = app.get_webview_window("main") {
                                match window.is_visible() {
                                    Ok(true) => {
                                        // Window is visible - hide it and unpin
                                        println!("[Tray] Hiding visible window");
                                        let _ = window.set_always_on_top(false);
                                        let _ = window.hide();
                                        if let Ok(mut flag) = ignore_focus_loss_tray.lock() {
                                            *flag = false;
                                        }
                                    }
                                    Ok(false) | Err(_) => {
                                        // Window is hidden or error - show it
                                        println!("[Tray] Showing hidden window");
                                        position_window_bottom_right(&window);
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                        if let Ok(mut flag) = ignore_focus_loss_tray.lock() {
                                            *flag = true;
                                        }
                                    }
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
        .on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Prevent window from closing, hide it instead
                    let _ = window.hide();
                    api.prevent_close();
                }
                tauri::WindowEvent::Focused(false) => {
                    // Window lost focus - hide it if not pinned, unless we just toggled it from the tray
                    let mut ignore = ignore_focus_loss_for_events.lock().unwrap_or_else(|e| e.into_inner());
                    if *ignore {
                        *ignore = false;
                        let _ = window.set_focus();
                    } else if !window.is_always_on_top().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
                _ => {}
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
            audio_management::check_default_device_changed,
            resize_window_to_content,
            show_main_window,
            hide_main_window,
            toggle_pin_window,
            is_window_pinned,
            quit_application,
            open_url,
        ])
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    clearcomms_lib::run()
}
