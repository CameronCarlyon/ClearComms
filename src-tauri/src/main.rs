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
//! - [`lvar_input`] - Flight Simulator LVar integration
//! - [`native_menu`] - Windows system tray context menu
//! - [`window_utils`] - Window positioning utilities

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::image::Image;
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, TrayIconId, MouseButton, MouseButtonState};

mod audio_management;
mod hardware_input;
mod lvar_input;
mod native_menu;
mod window_utils;

use window_utils::position_window_bottom_right;

// ─────────────────────────────────────────────────────────────────────────────
// Layout Measurement System
// ─────────────────────────────────────────────────────────────────────────────

/// Stores measured layout dimensions from the frontend
/// This allows window sizing to adapt to any DPI scaling or CSS changes
#[derive(Debug, Clone)]
struct LayoutMeasurements {
    /// Actual rendered width of one ApplicationChannel component (logical pixels)
    channel_width: u32,
    /// Actual rendered gap between channels (logical pixels)
    channel_gap: u32,
    /// Base window width for single channel (logical pixels)
    base_width: u32,
}

impl Default for LayoutMeasurements {
    fn default() -> Self {
        LayoutMeasurements {
            channel_width: 48,   // CSS: max-width: 3rem = 48px at 100% scale
            channel_gap: 48,     // CSS: gap: 3rem = 48px at 100% scale
            base_width: 250,     // Standard base width for single channel
        }
    }
}

// Global layout measurements, protected by mutex
lazy_static::lazy_static! {
    static ref LAYOUT_MEASUREMENTS: Arc<Mutex<LayoutMeasurements>> = 
        Arc::new(Mutex::new(LayoutMeasurements::default()));
}

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Fixed window height in pixels (logical pixels)
/// This doesn't need to scale dynamically as content doesn't wrap vertically
const WINDOW_HEIGHT: u32 = 700;

/// Duration of window resize animation in milliseconds
const RESIZE_ANIMATION_DURATION_MS: u64 = 500;

/// Frame interval for resize animation in milliseconds
/// Set to 8ms (~125fps) to provide smooth animations on high refresh rate monitors.
/// The actual display refresh rate is handled by the OS, so this oversamples safely
/// and provides butter-smooth animations on 60Hz, 120Hz, 144Hz, and 240Hz+ displays.
const RESIZE_ANIMATION_FRAME_MS: u64 = 8;

/// Tray icon identifier
const TRAY_ICON_ID: &str = "clearcomms-tray";

// ─────────────────────────────────────────────────────────────────────────────
// Theme Detection (Windows)
// ─────────────────────────────────────────────────────────────────────────────

/// Checks if Windows is using light mode for applications.
/// Returns `true` for light mode (use black icon), `false` for dark mode (use white icon).
#[cfg(target_os = "windows")]
fn is_windows_light_mode() -> bool {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };
    use windows::core::w;
    
    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        
        // Open the registry key
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        
        let value_name = w!("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut data_size = std::mem::size_of::<u32>() as u32;
        let mut data_type = REG_DWORD;
        
        let result = RegQueryValueExW(
            hkey,
            value_name,
            None,
            Some(&mut data_type),
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut data_size),
        );
        
        let _ = RegCloseKey(hkey);
        
        if result.is_ok() {
            // 1 = light mode, 0 = dark mode
            data == 1
        } else {
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn is_windows_light_mode() -> bool {
    false
}

/// Loads the appropriate tray icon based on the current Windows theme.
/// Returns the white icon for dark mode, black icon for light mode.
fn load_theme_appropriate_icon() -> Image<'static> {
    let is_light = is_windows_light_mode();
    let icon_bytes: &[u8] = if is_light {
        // Light mode: use black icon for contrast
        include_bytes!("../icons/black/32x32.png")
    } else {
        // Dark mode: use white icon for contrast
        include_bytes!("../icons/white/32x32.png")
    };
    
    // Decode PNG to RGBA
    let img = image::load_from_memory(icon_bytes).expect("Failed to decode tray icon PNG");
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    
    Image::new_owned(rgba.into_raw(), width, height)
}


// ─────────────────────────────────────────────────────────────────────────────
// Window Width Calculation
// ─────────────────────────────────────────────────────────────────────────────

/// Calculate the required window width for a given number of audio channels.
///
/// Uses dynamically measured layout dimensions from the frontend to adapt to any DPI scaling
/// or CSS changes. Falls back to sensible defaults if measurements haven't been set.
///
/// Formula: base_width + ((channel_width + channel_gap) × (session_count - 1))
///
/// Returns logical pixel width. This value is converted to physical pixels in resize_window_to_content()
/// using the display's DPI scale factor (e.g., 1.5 for 150% scaling on 4K displays).
///
/// # Arguments
/// * `session_count` - Number of audio sessions to display
///
/// # Returns
/// Window width in logical pixels (before DPI scaling)
fn calculate_window_width(session_count: usize) -> u32 {
    if session_count == 0 {
        let measurements = LAYOUT_MEASUREMENTS.lock().unwrap();
        return measurements.base_width;
    }
    
    let measurements = LAYOUT_MEASUREMENTS.lock().unwrap();
    let increment = measurements.channel_width + measurements.channel_gap;
    
    if session_count == 1 {
        measurements.base_width
    } else {
        measurements.base_width + (increment * (session_count - 1) as u32)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands - Window Management
// ─────────────────────────────────────────────────────────────────────────────

/// Update the layout measurements from the frontend.
///
/// The frontend measures the actual rendered width of UI components and sends these
/// measurements to ensure accurate window sizing across all DPI scales and resolutions.
///
/// # Arguments
/// * `channel_width` - Measured width of one ApplicationChannel component (logical pixels)
/// * `channel_gap` - Measured gap between channels (logical pixels)
/// * `base_width` - Measured base width for single channel (logical pixels)
///
/// # Returns
/// Confirmation message with the stored measurements
#[tauri::command]
fn update_layout_measurements(
    channel_width: u32,
    channel_gap: u32,
    base_width: u32,
) -> Result<String, String> {
    let mut measurements = LAYOUT_MEASUREMENTS.lock().map_err(|e| format!("Failed to lock measurements: {}", e))?;
    measurements.channel_width = channel_width;
    measurements.channel_gap = channel_gap;
    measurements.base_width = base_width;
    
    tracing::debug!("[Layout] Updated measurements: channel={}px, gap={}px, base={}px",
             channel_width, channel_gap, base_width);
    
    Ok(format!("Layout measurements updated: channel={}px, gap={}px, base={}px", 
               channel_width, channel_gap, base_width))
}

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
    if let Some(window) = app.get_webview_window("main") {
        // Calculate logical pixel width
        let logical_target_width = calculate_window_width(session_count);
        
        // Get the DPI scale factor (1.0 for 100%, 1.5 for 150%, etc.)
        let scale_factor = window.scale_factor().map_err(|e| e.to_string())?;
        
        // Convert logical pixels to physical pixels
        let physical_target_width = (logical_target_width as f64 * scale_factor) as u32;
        let physical_window_height = (WINDOW_HEIGHT as f64 * scale_factor) as u32;
        
        // Get current window size (already in physical pixels)
        let current_size = window.outer_size().map_err(|e| e.to_string())?;
        let current_width = current_size.width;
        
        // Skip animation if already at target size (within 1px tolerance for rounding)
        if (current_width as i32 - physical_target_width as i32).abs() <= 1 {
            return Ok(format!("Already at {:?}x{:?} (scale: {})", physical_target_width, physical_window_height, scale_factor));
        }
        
        // Spawn animation thread to avoid blocking
        let window_clone = window.clone();
        std::thread::spawn(move || {
            animate_window_resize(window_clone, current_width, physical_target_width, scale_factor);
        });
        
        return Ok(format!("Animating to {:?}x{:?} for {} session(s) (scale: {})", physical_target_width, physical_window_height, session_count, scale_factor));
    }
    
    Err("Main window not found".to_string())
}

/// Animate window width change with easing
fn animate_window_resize(window: tauri::WebviewWindow, start_width: u32, target_width: u32, scale_factor: f64) {
    let start_time = Instant::now();
    let duration = Duration::from_millis(RESIZE_ANIMATION_DURATION_MS);
    let frame_duration = Duration::from_millis(RESIZE_ANIMATION_FRAME_MS);
    
    // Calculate physical height from logical height
    let physical_window_height = (WINDOW_HEIGHT as f64 * scale_factor) as u32;
    
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
        
        // Set window size using physical pixels
        let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: current_width,
            height: physical_window_height,
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

/// Helper function: Perform the actual pin toggle operation
/// Returns the new pin state after toggling
pub fn perform_pin_toggle(window: &tauri::WebviewWindow) -> Result<bool, String> {
    position_window_bottom_right(window);
    let _ = window.show();
    let _ = window.set_focus();
    
    let current_state = window.is_always_on_top().unwrap_or(false);
    let new_state = !current_state;
    let _ = window.set_always_on_top(new_state);
    Ok(new_state)
}

/// Toggle pin on top for main window
/// Returns the new pin state
#[tauri::command]
fn toggle_pin_window(app: tauri::AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("main") {
        perform_pin_toggle(&window)
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

/// Restart the application
#[tauri::command]
async fn restart_application(app: tauri::AppHandle) -> Result<(), String> {
    // Close the current app gracefully
    app.exit(0);
    
    // Relaunch the application
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::env;
        
        let current_exe = env::current_exe()
            .map_err(|e| format!("Failed to get current executable: {}", e))?;
        
        Command::new(current_exe)
            .spawn()
            .map_err(|e| format!("Failed to restart application: {}", e))?;
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        // Placeholder for non-Windows platforms
        return Err("Restart not implemented for this platform".to_string());
    }
    
    Ok(())
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
    #[cfg(debug_assertions)]
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    // Track when window was last hidden - used to detect if tray click caused focus loss
    let last_hidden: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(10)));
    let last_hidden_for_setup = last_hidden.clone();
    let last_hidden_for_events = last_hidden.clone();

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
                    let hwnd = HWND(window.hwnd().unwrap().0);
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
            
            // Build tray icon with theme-appropriate icon
            let last_hidden_tray = last_hidden_for_setup.clone();
            let tray_id = TrayIconId::new(TRAY_ICON_ID);
            let _tray = TrayIconBuilder::with_id(tray_id)
                .icon(load_theme_appropriate_icon())
                .tooltip("ClearComms")
                .on_tray_icon_event(move |tray, event| {
                    let app = tray.app_handle();
                    
                    match event {
                        tauri::tray::TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            if let Some(window) = app.get_webview_window("main") {
                                // Check if window was hidden very recently (within 200ms)
                                // If so, this tray click caused that hide via focus loss - don't reopen
                                let just_hidden = last_hidden_tray.lock()
                                    .map(|t| t.elapsed() < Duration::from_millis(200))
                                    .unwrap_or(false);
                                
                                let is_visible = window.is_visible().unwrap_or(false);
                                
                                tracing::debug!("[Tray] Click - visible: {}, just_hidden: {}", is_visible, just_hidden);
                                
                                if is_visible {
                                    // Window is visible - hide it
                                    tracing::debug!("[Tray] Hiding window");
                                    let _ = window.set_always_on_top(false);
                                    let _ = window.hide();
                                } else if just_hidden {
                                    // Window was just hidden by this click's focus loss - do nothing
                                    tracing::debug!("[Tray] Ignoring (just hidden by focus loss)");
                                } else {
                                    // Window is hidden and wasn't just hidden - show it
                                    tracing::debug!("[Tray] Showing window");
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
                                tracing::error!("[Tray] Error showing native menu: {}", e);
                            }
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            
            // Spawn a background thread to monitor Windows theme changes
            // and update the tray icon accordingly
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last_light_mode = is_windows_light_mode();
                
                loop {
                    std::thread::sleep(Duration::from_secs(2));
                    
                    let current_light_mode = is_windows_light_mode();
                    if current_light_mode != last_light_mode {
                        last_light_mode = current_light_mode;
                        
                        // Update tray icon on the main thread
                        let app_for_tray = app_handle.clone();
                        let _ = app_handle.run_on_main_thread(move || {
                            match app_for_tray.tray_by_id(TRAY_ICON_ID) {
                                Some(tray) => {
                                    let new_icon = load_theme_appropriate_icon();
                                    if let Err(e) = tray.set_icon(Some(new_icon)) {
                                        tracing::error!("[Theme] Failed to update tray icon: {}", e);
                                    }
                                }
                                None => {
                                    tracing::warn!("[Theme] Could not find tray icon with id '{}'", TRAY_ICON_ID);
                                }
                            }
                        });
                    }
                }
            });
            
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
                    tracing::debug!("[Window] Focused: {}, Pinned: {}", focused, is_pinned);
                    
                    // Force redraw on any focus change when pinned to clear title bar artifacts
                    if is_pinned {
                        tracing::debug!("[Window] Pinned, forcing redraw");
                        if let Ok(size) = window.outer_size() {
                            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
                                width: size.width,
                                height: size.height + 1,
                            }));
                            let _ = window.set_size(tauri::Size::Physical(size));
                        }
                    } else if !focused {
                        // Window not pinned and lost focus - hide it and record timestamp
                        tracing::debug!("[Window] Lost focus, hiding");
                        // Only update last_hidden if the window was actually visible
                        if let Ok(is_visible) = window.is_visible() {
                            if is_visible {
                                if let Ok(mut last) = last_hidden_for_events.lock() {
                                    *last = Instant::now();
                                }
                            }
                        }
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
            hardware_input::cleanup_input_manager,
            audio_management::init_audio_manager,
            audio_management::get_audio_sessions,
            audio_management::set_session_volume,
            audio_management::set_session_mute,
            audio_management::check_default_device_changed,
            audio_management::cleanup_audio_manager,
            audio_management::get_system_volume,
            audio_management::get_system_mute,
            audio_management::set_system_volume,
            audio_management::set_system_mute,
            update_layout_measurements,
            resize_window_to_content,
            show_main_window,
            hide_main_window,
            toggle_pin_window,
            is_window_pinned,
            restart_application,
            quit_application,
            open_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}