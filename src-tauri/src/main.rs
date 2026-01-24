//! ClearComms
//!
//! A lightweight desktop application that provides synchronised intercom volume
//! control by linking cockpit audio controls, hardware, and external applications
//! into one seamless system.
//!
//! ## Architecture
//!
//! - **Frontend**: SvelteKit with TypeScript for the UI
//! - **Backend**: Rust with Tauri 2.x for native functionality
//! - **Audio**: Windows Core Audio API for application volume control
//! - **Input**: Windows Joystick API + HID for hardware device input
//!
//! ## Modules
//!
//! - [`audio_management`] - Windows Core Audio API integration
//! - [`hardware_input`] - RawInput/HID device polling
//! - [`simvar_input`] - SimConnect integration (planned)
//! - [`native_menu`] - Windows system tray context menu
//! - [`window_utils`] - Window positioning utilities

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Manager;
use tauri::tray::{TrayIconBuilder, MouseButton, MouseButtonState};

mod audio_management;
mod hardware_input;
mod simvar_input;
mod native_menu;
mod window_utils;

use window_utils::position_window_bottom_right;

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Base window width in pixels (for single channel)
const BASE_WINDOW_WIDTH: u32 = 400;

/// Additional width per audio channel in pixels
const CHANNEL_WIDTH: u32 = 109;

/// Fixed window height in pixels
const WINDOW_HEIGHT: u32 = 1000;

/// Duration of window resize animation in milliseconds
const RESIZE_ANIMATION_DURATION_MS: u64 = 200;

/// Frame interval for resize animation (~60fps)
const RESIZE_ANIMATION_FRAME_MS: u64 = 16;


// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands - Window Management
// ─────────────────────────────────────────────────────────────────────────────

/// Resize the main window to accommodate the number of audio channels.
///
/// Calculates the appropriate width based on the number of bound audio sessions
/// and repositions the window to the bottom-right corner.
///
/// # Arguments
/// * `app` - Tauri application handle
/// * `session_count` - Number of audio sessions to display
///
/// # Returns
/// Success message with new dimensions or error if window not found
#[tauri::command]
fn resize_window_to_content(app: tauri::AppHandle, session_count: usize) -> Result<String, String> {
    let target_width = if session_count <= 1 {
        BASE_WINDOW_WIDTH
    } else {
        BASE_WINDOW_WIDTH + (CHANNEL_WIDTH * (session_count - 1) as u32)
    };
    
    if let Some(window) = app.get_webview_window("main") {
        // Get current window size
        let current_size = window.outer_size().map_err(|e| e.to_string())?;
        let current_width = current_size.width;
        
        // Skip animation if already at target size
        if current_width == target_width {
            return Ok(format!("Already at {}x{}", target_width, WINDOW_HEIGHT));
        }
        
        // Spawn animation thread to avoid blocking
        let window_clone = window.clone();
        std::thread::spawn(move || {
            animate_window_resize(window_clone, current_width, target_width);
        });
        
        return Ok(format!("Animating to {}x{} for {} session(s)", target_width, WINDOW_HEIGHT, session_count));
    }
    
    Err("Main window not found".to_string())
}

/// Animate window width change with easing
fn animate_window_resize(window: tauri::WebviewWindow, start_width: u32, target_width: u32) {
    let start_time = Instant::now();
    let duration = Duration::from_millis(RESIZE_ANIMATION_DURATION_MS);
    let frame_duration = Duration::from_millis(RESIZE_ANIMATION_FRAME_MS);
    
    loop {
        let elapsed = start_time.elapsed();
        
        // Calculate progress (0.0 to 1.0)
        let progress = if elapsed >= duration {
            1.0
        } else {
            elapsed.as_secs_f64() / duration.as_secs_f64()
        };
        
        // Apply ease-out cubic easing: 1 - (1 - t)^3
        let eased_progress = 1.0 - (1.0 - progress).powi(3);
        
        // Interpolate width
        let current_width = if start_width < target_width {
            start_width + ((target_width - start_width) as f64 * eased_progress) as u32
        } else {
            start_width - ((start_width - target_width) as f64 * eased_progress) as u32
        };
        
        // Set window size
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: current_width,
            height: WINDOW_HEIGHT,
        }));
        
        // Reposition window to stay anchored to bottom-right
        position_window_bottom_right(&window);
        
        // Check if animation is complete
        if progress >= 1.0 {
            break;
        }
        
        // Wait for next frame
        std::thread::sleep(frame_duration);
    }
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

/// Open a URL in the default browser and bring it to the foreground
#[tauri::command]
async fn open_url(url: String) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsStr;
    
    // Use ShellExecuteW with SW_SHOWNORMAL to ensure the browser window is shown and focused
    let url_wide: Vec<u16> = OsStr::new(&url)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let operation: Vec<u16> = OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    
    let result = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            windows::Win32::Foundation::HWND::default(),
            windows::core::PCWSTR(operation.as_ptr()),
            windows::core::PCWSTR(url_wide.as_ptr()),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };
    
    // ShellExecuteW returns a value > 32 on success
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!("Failed to open URL: error code {}", result.0 as usize))
    }
}

fn main() {
    // Timestamp of when the window was last shown - used to ignore focus loss immediately after showing
    let window_show_time: Arc<Mutex<std::time::Instant>> = Arc::new(Mutex::new(std::time::Instant::now()));
    let window_show_time_for_setup = window_show_time.clone();
    let window_show_time_for_events = window_show_time.clone();

    tauri::Builder::default()
        .setup(move |app| {
            // Get main window and position it
            if let Some(window) = app.get_webview_window("main") {
                // Apply Windows Acrylic effect and rounded corners
                #[cfg(target_os = "windows")]
                {
                    use window_vibrancy::apply_acrylic;
                    use windows::Win32::Graphics::Dwm::*;
                    use windows::Win32::Foundation::HWND;

                    // Apply acrylic with automatic color matching to Windows theme
                    let _ = apply_acrylic(&window, None);

                    // Apply rounded corners
                    let hwnd = HWND(window.hwnd().unwrap().0 as *mut std::ffi::c_void);
                    let corner_preference: i32 = DWMWCP_ROUND.0;
                    unsafe {
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            DWMWA_WINDOW_CORNER_PREFERENCE,
                            &corner_preference as *const _ as *const _,
                            std::mem::size_of::<i32>() as u32,
                        );
                    }

                    // Disable window animations (instant hide/show)
                    let disable_transitions: i32 = 1; // TRUE
                    unsafe {
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            DWMWA_TRANSITIONS_FORCEDISABLED,
                            &disable_transitions as *const _ as *const _,
                            std::mem::size_of::<i32>() as u32,
                        );
                    }
                }
                // Position window in bottom-right corner
                position_window_bottom_right(&window);
                
                // Don't show window on startup (starts in tray)
                let _ = window.hide();
            }
            
            // Build tray icon without menu (we'll use custom window)
            let window_show_time_tray = window_show_time_for_setup.clone();
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClearComms")
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
                                    }
                                    Ok(false) | Err(_) => {
                                        // Window is hidden or error - show it
                                        println!("[Tray] Showing hidden window");
                                        position_window_bottom_right(&window);
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                        // Record when we showed the window to ignore immediate focus loss
                                        if let Ok(mut time) = window_show_time_tray.lock() {
                                            *time = std::time::Instant::now();
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
            
            Ok(())
        })
        .on_window_event(move |window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Prevent window from closing, hide it instead
                    let _ = window.hide();
                    api.prevent_close();
                }
                tauri::WindowEvent::Focused(focused) => {
                    let is_pinned = window.is_always_on_top().unwrap_or(false);
                    println!("[Window] Focused: {}, Pinned: {}", focused, is_pinned);
                    
                    if !focused {
                        // Window lost focus - check if we should ignore it (just showed window from tray)
                        let should_ignore = if let Ok(time) = window_show_time_for_events.lock() {
                            time.elapsed() < std::time::Duration::from_millis(100)
                        } else {
                            false
                        };
                        
                        if should_ignore {
                            // We just opened the window from tray - ignore this focus loss
                            println!("[Window] Ignoring focus loss (window just shown)");
                        } else if is_pinned {
                            // Window is pinned - keep it visible
                            println!("[Window] Pinned, staying visible");
                        } else {
                            // Window is not pinned - hide it immediately
                            println!("[Window] Not pinned, hiding");
                            let _ = window.hide();
                        }
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
            hardware_input::cleanup_input_manager,
            audio_management::init_audio_manager,
            audio_management::get_audio_sessions,
            audio_management::set_session_volume,
            audio_management::set_session_mute,
            audio_management::check_default_device_changed,
            audio_management::cleanup_audio_manager,
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
}