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
//! - [`simconnect`] - Flight Simulator SimConnect integration
//! - [`native_menu`] - Windows system tray context menu
//! - [`window_utils`] - Window positioning utilities

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Arc, Mutex, LazyLock};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use std::fs;

use tauri::image::Image;
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, TrayIconId, MouseButton, MouseButtonState};

mod audio_management;
mod hardware_input;
mod native_menu;
mod window_utils;
mod notification;
mod theme;
mod simconnect;
mod sim_detection;

use window_utils::{position_window_bottom_right, get_display_info_for_window, set_window_pos_and_size};

// ─────────────────────────────────────────────────────────────────────────────
// Pin State
// ─────────────────────────────────────────────────────────────────────────────

/// Logical pin-on-top state, tracked independently of the Win32 window style
/// which can be cleared by Tauri's internal `SetWindowPos` calls during
/// resize/redraw. This is the single source of truth for pin state, read by
/// the native context menu, the frontend `is_window_pinned` command, and the
/// focus-change handler.
static PIN_STATE: AtomicBool = AtomicBool::new(false);

/// Shutdown signal for the theme monitor thread.
/// Set to `true` during app shutdown so the thread exits cleanly.
static THEME_MONITOR_SHUTDOWN: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────────────────────
// Sim Detection Shutdown Handles
// ─────────────────────────────────────────────────────────────────────────────

/// Shutdown event handle for the sim detection thread (stored as isize).
static SIM_DETECTION_SHUTDOWN_EVENT: AtomicIsize = AtomicIsize::new(0);

/// JoinHandle for the sim detection thread. Stored so we can join it during shutdown.
static SIM_DETECTION_THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
/// Update the tracked pin state. Call this whenever the pin state changes.
pub fn set_pin_state(pinned: bool) {
    PIN_STATE.store(pinned, Ordering::Relaxed);
}

/// Read the tracked pin state.
pub fn get_pin_state() -> bool {
    PIN_STATE.load(Ordering::Relaxed)
}

// ─────────────────────────────────────────────────────────────────────────────
// Layout Measurement System
// ─────────────────────────────────────────────────────────────────────────────

/// Stores measured layout dimensions from the frontend.
/// This allows window sizing to adapt to any DPI scaling or CSS changes.
#[derive(Debug, Clone)]
struct LayoutMeasurements {
    /// Actual rendered width of one ApplicationChannel component (logical pixels)
    channel_width: u32,
    /// Actual rendered gap between channels (logical pixels)
    channel_gap: u32,
    /// Horizontal padding between the window edge and the mixer content (one side, logical pixels)
    padding: u32,
}

impl Default for LayoutMeasurements {
    fn default() -> Self {
        LayoutMeasurements {
            channel_width: 50,   // CSS: max-width on .application-channel
            channel_gap: 50,     // CSS: gap on .mixer-container
            padding: 50,         // CSS: padding on main container (one side) = 100px total
        }
    }
}

// Global layout measurements, protected by mutex
static LAYOUT_MEASUREMENTS: LazyLock<Arc<Mutex<LayoutMeasurements>>> =
    LazyLock::new(|| Arc::new(Mutex::new(LayoutMeasurements::default())));

/// Serialises all reads and writes to the UI config file to prevent
/// concurrent save_config_value calls from losing each other's updates.
static UI_CONFIG_LOCK: Mutex<()> = Mutex::new(());

// ─────────────────────────────────────────────────────────────────────────────
// Singleton Animation Thread (Fix 4)
// ─────────────────────────────────────────────────────────────────────────────

/// Message sent to the animation thread with the target resize parameters.
struct AnimationTarget {
    window: tauri::WebviewWindow,
    start_width: u32,
    target_width: u32,
    target_height: u32,
}

/// Global sender for the singleton animation thread.
/// Lazily initialised on first use by `ensure_animation_thread()`.
static ANIMATION_SENDER: Mutex<Option<std::sync::mpsc::Sender<AnimationTarget>>> = Mutex::new(None);

/// Ensure the singleton animation thread is running and return a clone of the sender.
fn ensure_animation_thread() -> std::result::Result<std::sync::mpsc::Sender<AnimationTarget>, String> {
    let mut lock = ANIMATION_SENDER.lock().map_err(|e| format!("Animation lock poisoned: {}", e))?;
    
    if let Some(ref sender) = *lock {
        // Thread already running: return a clone of the sender
        return Ok(sender.clone());
    }
    
    // First call: spawn the animation thread
    let (tx, rx) = std::sync::mpsc::channel::<AnimationTarget>();
    
    std::thread::Builder::new()
        .name("window-anim".to_string())
        .spawn(move || {
            tracing::debug!("[Anim] Singleton animation thread started");
            
            while let Ok(target) = rx.recv() {
                // Drain any queued messages and use the latest target
                // (cancels in-progress animations in favour of the newest request)
                let mut latest = target;
                while let Ok(newer) = rx.try_recv() {
                    latest = newer;
                }
                
                animate_window_resize(
                    latest.window,
                    latest.start_width,
                    latest.target_width,
                    latest.target_height,
                    &rx,
                );
            }
            
            tracing::debug!("[Anim] Singleton animation thread exited");
        })
        .map_err(|e| format!("Failed to spawn animation thread: {}", e))?;
    
    *lock = Some(tx.clone());
    Ok(tx)
}

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Fixed window height in pixels (logical pixels)
/// This doesn't need to scale dynamically as content doesn't wrap vertically
const WINDOW_HEIGHT: u32 = 700;

/// Duration of window resize animation in milliseconds
const RESIZE_ANIMATION_DURATION_MS: u64 = 500;

/// Frame interval for resize animation in microseconds.
/// Set to 4,167µs (~240fps) for smoother interpolation on high refresh rate monitors.
/// The actual display refresh rate is handled by the OS, so this oversamples safely.
const RESIZE_ANIMATION_FRAME_US: u64 = 4_167;

/// Tray icon identifier
const TRAY_ICON_ID: &str = "clearcomms-tray";

/// Decode a PNG icon once at first use and cache the result.
fn decode_icon(bytes: &[u8]) -> Image<'static> {
    let img = image::load_from_memory(bytes).expect("Failed to decode tray icon PNG");
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Image::new_owned(rgba.into_raw(), width, height)
}

static ICON_LIGHT: LazyLock<Image<'static>> = LazyLock::new(|| {
    decode_icon(include_bytes!("../icons/white/32x32.png"))
});
static ICON_DARK: LazyLock<Image<'static>> = LazyLock::new(|| {
    decode_icon(include_bytes!("../icons/black/32x32.png"))
});

/// Loads the appropriate tray icon based on the current resolved theme.
/// Returns the white icon for dark mode, black icon for light mode.
fn load_theme_appropriate_icon() -> Image<'static> {
    match theme::get_icon_set() {
        "black" => ICON_DARK.clone(),
        _ => ICON_LIGHT.clone(),
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// Window Width Calculation
// ─────────────────────────────────────────────────────────────────────────────

/// Calculate the required window width for a given number of audio channels.
///
/// Uses dynamically measured layout dimensions from the frontend to adapt to any DPI scaling
/// or CSS changes. Falls back to sensible defaults if measurements haven't been set.
///
/// Formula: (n × channel_width) + ((n − 1) × channel_gap) + (2 × padding)
/// where n is clamped to a minimum of 2 to maintain a suitable base window width.
///
/// Example for 2 channels in edit mode (displayCount=4) with 50px channels, 50px gaps, 50px padding:
///   (4 × 50) + (3 × 50) + (2 × 50) = 200 + 150 + 100 = 450px
///
/// Returns logical pixel width. This value is converted to physical pixels in
/// `resize_window_to_content()` using the display's DPI scale factor.
///
/// # Arguments
/// * `session_count` - Number of audio sessions to display
///
/// # Returns
/// Window width in logical pixels (before DPI scaling)
fn calculate_window_width(session_count: usize) -> u32 {
    let measurements = LAYOUT_MEASUREMENTS.lock().unwrap();
    let n = session_count.max(2) as u32; // Clamp to minimum of 2 channels for base width

    let channels_width = n * measurements.channel_width;
    let gaps_width = n.saturating_sub(1) * measurements.channel_gap;
    let total_padding = measurements.padding * 2;

    channels_width + gaps_width + total_padding
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
/// * `padding` - Horizontal padding between the window edge and mixer content, one side (logical pixels)
///
/// # Returns
/// Confirmation message with the stored measurements
#[tauri::command]
fn update_layout_measurements(
    channel_width: u32,
    channel_gap: u32,
    padding: u32,
) -> Result<String, String> {
    let mut measurements = LAYOUT_MEASUREMENTS.lock().map_err(|e| format!("Failed to lock measurements: {}", e))?;
    measurements.channel_width = channel_width;
    measurements.channel_gap = channel_gap;
    measurements.padding = padding;
    
    Ok(format!("Layout measurements updated: channel={}px, gap={}px, padding={}px", 
               channel_width, channel_gap, padding))
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
        let mut physical_target_width = (logical_target_width as f64 * scale_factor) as u32;
        let mut physical_window_height = (WINDOW_HEIGHT as f64 * scale_factor) as u32;
        
        // Cap dimensions to stay within the usable work area (screen minus taskbar)
        // This prevents the window from going off-screen on any display configuration
        if let Some(display) = get_display_info_for_window(&window) {
            physical_target_width = physical_target_width.min(display.max_window_width as u32);
            physical_window_height = physical_window_height.min(display.max_window_height as u32);
        }
        
        // Get current window size (already in physical pixels)
        let current_size = window.outer_size().map_err(|e| e.to_string())?;
        let current_width = current_size.width;
        
        // Skip animation if already at target size (within 1px tolerance for rounding)
        if (current_width as i32 - physical_target_width as i32).abs() <= 1 {
            return Ok(format!("Already at {:?}x{:?} (scale: {})", physical_target_width, physical_window_height, scale_factor));
        }
        
        // Send to the singleton animation thread (cancels any in-progress animation)
        let sender = ensure_animation_thread()?;
        sender.send(AnimationTarget {
            window: window.clone(),
            start_width: current_width,
            target_width: physical_target_width,
            target_height: physical_window_height,
        }).map_err(|_| "Animation thread has exited".to_string())?;
        
        return Ok(format!("Animating to {:?}x{:?} for {} session(s) (scale: {})", physical_target_width, physical_window_height, session_count, scale_factor));
    }
    
    Err("Main window not found".to_string())
}

/// Animate window width change with easing, anchored to the bottom-right corner.
///
/// Pre-computes the bottom-right anchor point from the display work area, then
/// derives the window position directly from the interpolated width each frame.
/// This eliminates the visual stutter caused by separate size/position updates:
/// the right edge and bottom edge of the window remain perfectly fixed throughout
/// the entire animation.
///
/// The `target_height` is pre-computed in physical pixels by the caller,
/// already capped to fit within the display's work area.
///
/// Accepts a reference to the channel receiver so it can check for a newer
/// animation target mid-animation (cancellation signal).
fn animate_window_resize(
    window: tauri::WebviewWindow,
    start_width: u32,
    target_width: u32,
    target_height: u32,
    rx: &std::sync::mpsc::Receiver<AnimationTarget>,
) {
    let start_time = Instant::now();
    let duration = Duration::from_millis(RESIZE_ANIMATION_DURATION_MS);
    let frame_duration = Duration::from_micros(RESIZE_ANIMATION_FRAME_US);

    let physical_window_height = target_height;

    // Pre-compute the fixed anchor point (bottom-right corner including padding).
    // The right edge and bottom edge of the window stay pinned here for the whole
    // animation, so only the left edge moves as the width changes.
    let display = get_display_info_for_window(&window);
    let (anchor_right, anchor_bottom, clamp_left, clamp_top) = match &display {
        Some(d) => (
            d.work_area_right - d.edge_padding,   // right anchor
            d.work_area_bottom - d.edge_padding,   // bottom anchor
            d.work_area_left + d.edge_padding,     // left clamp
            d.work_area_top + d.edge_padding,      // top clamp
        ),
        None => {
            // Fallback: use current position + size as anchor
            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.outer_size()) {
                (
                    pos.x + size.width as i32,
                    pos.y + size.height as i32,
                    0,
                    0,
                )
            } else {
                (0, 0, 0, 0)
            }
        }
    };

    loop {
        let elapsed = start_time.elapsed();

        let progress = if elapsed >= duration {
            1.0
        } else {
            elapsed.as_secs_f64() / duration.as_secs_f64()
        };

        // Ease-out cubic: 1 - (1 - t)^3
        let eased_progress = 1.0 - (1.0 - progress).powi(3);

        // Interpolate width
        let current_width = if start_width < target_width {
            start_width + ((target_width - start_width) as f64 * eased_progress) as u32
        } else {
            start_width - ((start_width - target_width) as f64 * eased_progress) as u32
        };

        // Derive position from the fixed anchor so the right/bottom edges never move
        let x = (anchor_right - current_width as i32).max(clamp_left);
        let y = (anchor_bottom - physical_window_height as i32).max(clamp_top);

        // Atomic move + resize in a single Win32 call: no in-between frame flicker
        set_window_pos_and_size(&window, x, y, current_width, physical_window_height);

        if progress >= 1.0 {
            break;
        }

        // Check if a newer animation target has arrived: if so, abort this animation
        // so the caller loop can pick up the new target
        if rx.try_recv().is_ok() {
            tracing::debug!("[Anim] Cancelled in-progress animation for newer target");
            break;
        }

        std::thread::sleep(frame_duration);
    }
}

/// Show the main application window
pub fn show_main_window_internal(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        position_window_bottom_right(&window);
        let _ = window.show();
        let _ = window.set_focus();
        Ok(())
    } else {
        Err("Main window not found".to_string())
    }
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> Result<(), String> {
    show_main_window_internal(&app)
}

fn hide_main_window_internal(
    app: &tauri::AppHandle,
    last_hidden: Option<&Arc<Mutex<Instant>>>,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("Main window not found".to_string());
    };

    if let Some(last_hidden) = last_hidden {
        if let Ok(is_visible) = window.is_visible() {
            if is_visible {
                if let Ok(mut last) = last_hidden.lock() {
                    *last = Instant::now();
                }
            }
        }
    }
    let _ = window.hide();

    Ok(())
}

/// Hide the main application window
#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> Result<(), String> {
    hide_main_window_internal(&app, None)
}

/// Helper function: Perform the actual pin toggle operation
/// Returns the new pin state after toggling
pub fn perform_pin_toggle(window: &tauri::WebviewWindow) -> Result<bool, String> {
    position_window_bottom_right(window);
    let _ = window.show();
    let _ = window.set_focus();
    
    let current_state = get_pin_state();
    let new_state = !current_state;
    let _ = window.set_always_on_top(new_state);
    set_pin_state(new_state);
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
    if let Some(_window) = app.get_webview_window("main") {
        Ok(get_pin_state())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Get display and work area information for the window's monitor.
///
/// Returns complete display metrics including the usable work area (screen minus
/// taskbar), scale factor, edge padding, and maximum window dimensions.
#[tauri::command]
fn get_display_info(app: tauri::AppHandle) -> Result<window_utils::DisplayInfo, String> {
    if let Some(window) = app.get_webview_window("main") {
        get_display_info_for_window(&window)
            .ok_or_else(|| "Failed to retrieve display information".to_string())
    } else {
        Err("Main window not found".to_string())
    }
}

/// Restart the application
#[tauri::command]
async fn restart_application(app: tauri::AppHandle) -> Result<(), String> {
    restart_application_internal(&app)
}

/// Persistent UI config filename stored in the app config directory.
const UI_CONFIG_FILE_NAME: &str = "ui-state.json";

fn get_ui_config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to resolve app config directory: {}", e))?;

    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create app config directory: {}", e))?;

    Ok(config_dir.join(UI_CONFIG_FILE_NAME))
}

fn read_ui_config_map(app: &tauri::AppHandle) -> Result<serde_json::Map<String, serde_json::Value>, String> {
    let path = get_ui_config_path(app)?;

    if !path.exists() {
        return Ok(serde_json::Map::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read UI config file: {}", e))?;

    if content.trim().is_empty() {
        return Ok(serde_json::Map::new());
    }

    let parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse UI config JSON: {}", e))?;

    match parsed {
        serde_json::Value::Object(map) => Ok(map),
        _ => Err("UI config file is not a JSON object".to_string()),
    }
}

fn write_ui_config_map(
    app: &tauri::AppHandle,
    map: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let path = get_ui_config_path(app)?;
    let json = serde_json::to_string_pretty(&serde_json::Value::Object(map.clone()))
        .map_err(|e| format!("Failed to serialise UI config JSON: {}", e))?;

    fs::write(path, json).map_err(|e| format!("Failed to write UI config file: {}", e))
}

/// Save one UI config value by key.
#[tauri::command]
fn save_config_value(
    app: tauri::AppHandle,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let _guard = UI_CONFIG_LOCK.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
    let mut map = read_ui_config_map(&app)?;
    map.insert(key, value);
    write_ui_config_map(&app, &map)
}

/// Load one UI config value by key.
#[tauri::command]
fn load_config_value(
    app: tauri::AppHandle,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    let _guard = UI_CONFIG_LOCK.lock().map_err(|e| format!("Config lock poisoned: {}", e))?;
    let map = read_ui_config_map(&app)?;
    Ok(map.get(&key).cloned())
}

/// Quit the application gracefully.
#[tauri::command]
fn quit_application(app: tauri::AppHandle) {
    perform_graceful_quit(&app);
}

/// Signal all background threads to shut down cleanly, then exit the process.
pub fn perform_graceful_quit(app: &tauri::AppHandle) {
    THEME_MONITOR_SHUTDOWN.store(true, Ordering::Relaxed);
    audio_management::shutdown_audio_thread();
    hardware_input::shutdown_input_thread();
    
    // Signal the SimConnect lifecycle controller to shut down the thread.
    if let Some(session) = app.try_state::<std::sync::Arc<std::sync::Mutex<Option<simconnect::SimConnectSession>>>>() {
        simconnect::shutdown_simconnect_thread(&session);
    }
    
    // Signal the sim detection thread to shut down and join it.
    // This is critical: without joining, the thread may become orphaned
    // when app.exit(0) terminates the process.
    let shutdown_event = SIM_DETECTION_SHUTDOWN_EVENT.load(Ordering::Relaxed);
    if shutdown_event != 0 {
        if let Ok(mut thread_guard) = SIM_DETECTION_THREAD.lock() {
            if let Some(thread) = thread_guard.take() {
                sim_detection::shutdown_sim_detection_thread(shutdown_event, Some(thread));
            }
        }
    }
    
    app.exit(0);
}

fn show_launch_notification(app: &tauri::AppHandle) {
    notification::show(app, "ClearComms", "Running in your system tray.");
}

pub fn restart_application_internal(app: &tauri::AppHandle) -> Result<(), String> {
    if tauri::is_dev() {
        audio_management::shutdown_audio_thread();
        hardware_input::shutdown_input_thread();
        // Signal the SimConnect lifecycle controller to shut down the thread.
        if let Some(session) = app.try_state::<std::sync::Arc<std::sync::Mutex<Option<simconnect::SimConnectSession>>>>() {
            simconnect::shutdown_simconnect_thread(&session);
        }

        // Signal the sim detection thread to shut down (dev mode: no join, as restart continues)
        let shutdown_event = SIM_DETECTION_SHUTDOWN_EVENT.load(Ordering::Relaxed);
        if shutdown_event != 0 {
            if let Ok(mut thread_guard) = SIM_DETECTION_THREAD.lock() {
                if let Some(_thread) = thread_guard.take() {
                    sim_detection::signal_shutdown(shutdown_event);
                }
            }
        }

        let Some(window) = app.get_webview_window("main") else {
            return Err("Main window not found".to_string());
        };

        let _ = window.set_always_on_top(false);
        set_pin_state(false);
        let _ = window.hide();
        window
            .reload()
            .map_err(|e| format!("Failed to reload main window: {}", e))?;

        show_launch_notification(app);
        return Ok(());
    }

    THEME_MONITOR_SHUTDOWN.store(true, Ordering::Relaxed);
    audio_management::shutdown_audio_thread();
    hardware_input::shutdown_input_thread();
    // Signal the SimConnect lifecycle controller to shut down the thread.
    if let Some(session) = app.try_state::<std::sync::Arc<std::sync::Mutex<Option<simconnect::SimConnectSession>>>>() {
        simconnect::shutdown_simconnect_thread(&session);
    }

    // Signal the sim detection thread to shut down and join it (release mode: clean restart)
    let shutdown_event = SIM_DETECTION_SHUTDOWN_EVENT.load(Ordering::Relaxed);
    if shutdown_event != 0 {
        if let Ok(mut thread_guard) = SIM_DETECTION_THREAD.lock() {
            if let Some(thread) = thread_guard.take() {
                sim_detection::shutdown_sim_detection_thread(shutdown_event, Some(thread));
            }
        }
    }

    app.request_restart();
    Ok(())
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
    // Debug builds log to stdout through a non-blocking writer: the previous
    // blocking writer serialised every thread behind one console write, and a
    // chatty background thread could stall the UI thread for as long as the
    // terminal took to render. The guard must outlive the app: dropping it
    // flushes the buffer and stops the writer thread.
    //
    // Verbosity comes from RUST_LOG when set, e.g.
    //   RUST_LOG=ClearComms=debug,ClearComms::simconnect=info
    #[cfg(debug_assertions)]
    let _log_guard = {
        let (writer, guard) = tracing_appender::non_blocking(std::io::stdout());
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    // Global WARN keeps dependency diagnostics that matter:
                    // tao's event-loop starvation warnings in particular:
                    // while unmatched targets stay quiet.
                    tracing_subscriber::EnvFilter::new("warn,ClearComms=debug")
                }),
            )
            .with_writer(writer)
            .init();
        guard
    };

    // Track when window was last hidden - used to detect if tray click caused focus loss
    let last_hidden: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(10)));
    let last_hidden_for_setup = last_hidden.clone();
    let last_hidden_for_events = last_hidden.clone();

    let sim_state = Arc::new(Mutex::new(simconnect::state::SimState::default()));
    let sim_state_for_setup = sim_state.clone();

    // Shared LVar command channel: populated by the SimConnect lifecycle
    // controller while a simulator connection is active.
    let lvar_command_handle = Arc::new(simconnect::LvarCommandHandle::default());
    let lvar_command_handle_for_setup = lvar_command_handle.clone();

    // Create the mpsc channel that bridges sim_detection events to the
    // SimConnect lifecycle controller.
    let (detection_sender, detection_receiver) = std::sync::mpsc::channel::<sim_detection::SimDetectionEvent>();

    // Clone the sender before handing ownership to the detection thread so the
    // SimConnect connection manager can re-inject `Started` events when a
    // connection attempt fails but MSFS is still running.
    let retry_sender = detection_sender.clone();

    // Wrap the detection_receiver in an Arc<Mutex> so it can be moved into the setup closure
    let detection_receiver_for_setup = Arc::new(Mutex::new(Some(detection_receiver)));
    let retry_sender_for_setup = Arc::new(Mutex::new(Some(retry_sender)));

    // Spawn the sim detection thread (Toolhelp32 process snapshot polling).
    let (detection_shutdown_event, detection_thread) = sim_detection::spawn_sim_detection_thread(detection_sender);
    SIM_DETECTION_SHUTDOWN_EVENT.store(detection_shutdown_event, Ordering::Relaxed);
    if let Ok(mut thread_guard) = SIM_DETECTION_THREAD.lock() {
        *thread_guard = Some(detection_thread);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // The second launch hands off to this instance and exits, so
            // without this the user gets no feedback at all.
            notification::show(app, "ClearComms", "Already running in your system tray.");
        }))
        .manage(sim_state)
        .manage(lvar_command_handle)
        .setup(move |app| {
            // Start the lifecycle controller now that we have the app handle
            if let (Some(receiver), Some(retry_tx)) = (
                detection_receiver_for_setup.lock().unwrap().take(),
                retry_sender_for_setup.lock().unwrap().take(),
            ) {
                let sim_session = simconnect::start_lifecycle_controller(
                    app.handle().clone(),
                    sim_state_for_setup.clone(),
                    lvar_command_handle_for_setup.clone(),
                    receiver,
                    retry_tx,
                );
                app.manage(sim_session);
            }
            
            // Get main window and position it
            if let Some(window) = app.get_webview_window("main") {
                // Apply Windows Acrylic effect and rounded corners
                #[cfg(target_os = "windows")]
                {
                    crate::window_utils::apply_standard_window_visuals(&window, "main");
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
                                    set_pin_state(false);
                                    let _ = window.hide();
                                } else if just_hidden {
                                    // Window was just hidden by this click's focus loss - do nothing
                                    tracing::debug!("[Tray] Ignoring (just hidden by focus loss)");
                                } else {
                                    // Window is hidden and wasn't just hidden - show it
                                    tracing::debug!("[Tray] Showing window");
                                    let _ = show_main_window_internal(&app);
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
           
           theme::update_resolved_theme();

           // The window starts hidden, so without this the app appears not to
           // have launched at all.
           show_launch_notification(app.handle());
            
            // Spawn a background thread to monitor Windows theme changes
            // and update the tray icon accordingly
            let app_handle = app.handle().clone();
            std::thread::Builder::new()
                .name("theme-monitor".to_string())
                .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_secs(2));

                    // Check shutdown flag each iteration so the thread exits
                    // cleanly when the app is closed
                    if THEME_MONITOR_SHUTDOWN.load(Ordering::Relaxed) {
                        tracing::info!("[Theme] Shutdown signal received, exiting theme monitor");
                        break;
                    }
                    
                    // Only update if in Automatic mode and system theme changed
                    if theme::get_theme_mode() == theme::ThemeMode::Automatic {
                        if theme::update_resolved_theme() {
                            // Theme changed - update tray icon on the main thread
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
                }
            }).expect("Failed to spawn theme monitor thread");
            
            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" {
                return;
            }

            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Prevent window from closing, hide it instead
                    let _ = window.hide();
                    api.prevent_close();
                }
                tauri::WindowEvent::Focused(focused) => {
                    let is_pinned = get_pin_state();
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
                        let _ = hide_main_window_internal(&window.app_handle(), Some(&last_hidden_for_events));
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            hardware_input::init_input,
            hardware_input::get_input_status,
            hardware_input::cleanup_input_manager,
            audio_management::init_audio_manager,
            audio_management::get_audio_sessions,
            audio_management::set_session_volume,
            audio_management::set_session_mute,
            audio_management::cleanup_audio_manager,
            audio_management::get_system_volume,
            audio_management::get_system_mute,
            audio_management::set_system_volume,
            audio_management::set_system_mute,
            simconnect::get_sim_status,
            simconnect::subscribe_lvars,
            simconnect::set_sim_lvar,
            update_layout_measurements,
            resize_window_to_content,
            show_main_window,
            hide_main_window,
            toggle_pin_window,
            is_window_pinned,
            get_display_info,
            restart_application,
            save_config_value,
            load_config_value,
            quit_application,
            open_url,
            theme::get_theme_mode_command,
            theme::set_theme_mode_command,
            theme::get_resolved_theme_name_command,
            theme::get_theme_state_command,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}