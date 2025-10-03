// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, PhysicalPosition};
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};
use tauri::menu::{Menu, MenuItem};

mod audio_management;
mod hardware_input;
mod simvar_input;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Create tray menu
            let show_i = MenuItem::with_id(app, "show", "Show ClearComms", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "Hide ClearComms", true, None::<&str>)?;
            let pin_i = MenuItem::with_id(app, "pin", "Pin on top", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            
            let menu = Menu::with_items(app, &[&show_i, &hide_i, &pin_i, &quit_i])?;
            
            // Build tray icon
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("ClearComms - Aviation Audio Control")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                position_window_bottom_right(&window);
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "hide" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.hide();
                            }
                        }
                        "pin" => {
                            if let Some(window) = app.get_webview_window("main") {
                                // Show and position window
                                position_window_bottom_right(&window);
                                let _ = window.show();
                                let _ = window.set_focus();
                                
                                // Toggle always_on_top state
                                let current_state = window.is_always_on_top().unwrap_or(false);
                                let _ = window.set_always_on_top(!current_state);
                            }
                        }
                        "quit" => {
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event {
                        let app = tray.app_handle();
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
            
            // Calculate position: bottom-right with 20px padding
            let x = screen_size.width as i32 - window_size.width as i32 - 20;
            let y = screen_size.height as i32 - window_size.height as i32 - 60; // Extra padding for taskbar
            
            let position = PhysicalPosition::new(x, y);
            let _ = window.set_position(position);
        }
    }
}
