//! Theme Management
//!
//! Centralized theme system that provides a single source of truth for all
//! theme-dependent code. Supports multiple theme modes including automatic
//! (follows Windows system theme), forced light/dark, and placeholder for
//! future seasonal themes.
//!
//! ## Architecture
//!
//! - **ThemeMode**: User preference (Automatic, Light, Dark, Seasonal)
//! - **ResolvedTheme**: Actual computed theme (Light or Dark)
//! - **ThemeManager**: Thread-safe singleton that manages state
//!
//! ## Usage
//!
//! All theme-dependent code should use `get_resolved_theme()` to determine
//! the current theme. The theme mode is persisted in UI config and can be
//! changed via Tauri commands.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

// ─────────────────────────────────────────────────────────────────────────────
// Theme Types
// ─────────────────────────────────────────────────────────────────────────────

/// User-selected theme mode. This is what the user chooses in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    /// Follows the Windows system theme (default)
    Automatic,
    /// Force light mode regardless of system setting
    Light,
    /// Force dark mode regardless of system setting
    Dark,
    /// Placeholder for future seasonal themes
    Seasonal,
}

impl ThemeMode {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Automatic,
            1 => Self::Light,
            2 => Self::Dark,
            3 => Self::Seasonal,
            _ => Self::Automatic,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Automatic => 0,
            Self::Light => 1,
            Self::Dark => 2,
            Self::Seasonal => 3,
        }
    }
}

/// The actual resolved theme after applying the mode to the system theme.
/// This is what all theme-dependent code should use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTheme {
    Light,
    Dark,
}

impl ResolvedTheme {
    /// Returns the CSS class name for this theme.
    pub fn as_css_class(&self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Returns the icon set name for this theme.
    pub fn as_icon_set(&self) -> &'static str {
        match self {
            Self::Light => "black",
            Self::Dark => "white",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Thread-Safe State
// ─────────────────────────────────────────────────────────────────────────────

/// Current theme mode (user preference). Defaults to Automatic.
static THEME_MODE: AtomicU8 = AtomicU8::new(ThemeMode::Automatic as u8);

/// Current resolved theme (computed from mode + system theme).
static RESOLVED_THEME: AtomicU8 = AtomicU8::new(ResolvedTheme::Dark as u8);

/// Whether the resolved theme has been initialised.
static RESOLVED_INITIALIZED: AtomicBool = AtomicBool::new(false);

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Get the current theme mode (user preference).
pub fn get_theme_mode() -> ThemeMode {
    ThemeMode::from_u8(THEME_MODE.load(Ordering::Relaxed))
}

/// Set the theme mode (user preference).
/// Returns the new resolved theme after the mode change.
pub fn set_theme_mode(mode: ThemeMode) {
    THEME_MODE.store(mode.to_u8(), Ordering::Relaxed);
    update_resolved_theme();
}

/// Get the current resolved theme.
/// This is what all theme-dependent code should use.
pub fn get_resolved_theme() -> ResolvedTheme {
    if !RESOLVED_INITIALIZED.load(Ordering::Relaxed) {
        update_resolved_theme();
    }
    match RESOLVED_THEME.load(Ordering::Relaxed) {
        0 => ResolvedTheme::Light,
        _ => ResolvedTheme::Dark,
    }
}

/// Get the resolved theme as a string ("light" or "dark").
pub fn get_resolved_theme_name() -> &'static str {
    get_resolved_theme().as_css_class()
}

/// Get the icon set name for the current resolved theme.
pub fn get_icon_set() -> &'static str {
    get_resolved_theme().as_icon_set()
}

/// Update the resolved theme from the current mode and system theme.
/// Returns `true` if the resolved theme changed.
pub fn update_resolved_theme() -> bool {
    let mode = get_theme_mode();
    let new_resolved = match mode {
        ThemeMode::Automatic => {
            if is_windows_light_mode_raw() {
                ResolvedTheme::Light
            } else {
                ResolvedTheme::Dark
            }
        }
        ThemeMode::Light => ResolvedTheme::Light,
        ThemeMode::Dark => ResolvedTheme::Dark,
        ThemeMode::Seasonal => ResolvedTheme::Dark, // Placeholder: default to dark
    };

    let old_resolved = RESOLVED_THEME.load(Ordering::Relaxed);
    let new_value = match new_resolved {
        ResolvedTheme::Light => 0,
        ResolvedTheme::Dark => 1,
    };

    RESOLVED_THEME.store(new_value, Ordering::Relaxed);
    RESOLVED_INITIALIZED.store(true, Ordering::Relaxed);

    old_resolved != new_value
}

// ─────────────────────────────────────────────────────────────────────────────
// System Theme Detection (Internal)
// ─────────────────────────────────────────────────────────────────────────────

/// Raw registry read for Windows light mode detection.
/// Returns `true` if Windows is in light mode.
#[cfg(target_os = "windows")]
fn is_windows_light_mode_raw() -> bool {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };
    use windows::core::w;

    unsafe {
        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

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
            data == 1
        } else {
            false
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn is_windows_light_mode_raw() -> bool {
    false
}

// ─────────────────────────────────────────────────────────────────────────────
// System Theme Monitor
// ─────────────────────────────────────────────────────────────────────────────

// Raw FFI for CreateEventW, matching the other subsystems: the windows crate
// path for it varies by version.
#[cfg(target_os = "windows")]
extern "system" {
    fn CreateEventW(
        lpEventAttributes: *mut std::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lpName: *const u16,
    ) -> *mut std::ffi::c_void;
}

/// Watch the Windows theme setting, calling `on_change` when the resolved theme
/// actually changes.
///
/// The thread blocks on a registry change notification, so it costs nothing
/// until the user changes their theme. It previously opened the key, read a
/// value and closed it every two seconds for the life of the process, to detect
/// something that changes perhaps twice a day.
///
/// Returns the shutdown event handle, stored as `isize` so it is `Send`, and
/// the thread handle. Both are owned by the caller until it calls
/// [`shutdown_theme_monitor`].
#[cfg(target_os = "windows")]
pub fn spawn_theme_monitor<F>(on_change: F) -> Option<(isize, std::thread::JoinHandle<()>)>
where
    F: Fn() + Send + 'static,
{
    use windows::core::w;
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
        KEY_READ, REG_NOTIFY_CHANGE_LAST_SET,
    };
    use windows::Win32::System::Threading::WaitForMultipleObjects;

    // Manual reset: shutdown is a latch, not a pulse.
    let shutdown_event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
    if shutdown_event.is_null() {
        tracing::error!("[Theme] Failed to create shutdown event");
        return None;
    }

    // Auto reset: each notification is consumed by the wait that observes it.
    let change_event = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
    if change_event.is_null() {
        tracing::error!("[Theme] Failed to create change event");
        unsafe {
            CloseHandle(HANDLE(shutdown_event)).ok();
        }
        return None;
    }

    let shutdown_int = shutdown_event as isize;
    let change_int = change_event as isize;

    let spawned = std::thread::Builder::new()
        .name("theme-monitor".to_string())
        // A registry wait and a value read; the default reserve is far more
        // address space than this can use.
        .stack_size(128 * 1024)
        .spawn(move || {
            let shutdown = HANDLE(shutdown_int as *mut std::ffi::c_void);
            let change = HANDLE(change_int as *mut std::ffi::c_void);

            let mut key = HKEY::default();
            let opened = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
                    0,
                    KEY_READ | KEY_NOTIFY,
                    &mut key,
                )
            };

            if opened.is_err() {
                tracing::error!("[Theme] Could not open the Personalize key: theme changes will not be followed");
                unsafe {
                    CloseHandle(change).ok();
                    CloseHandle(shutdown).ok();
                }
                return;
            }

            let handles = [change, shutdown];

            loop {
                // Re-armed every pass: an asynchronous notification fires once
                // and must then be requested again.
                let armed = unsafe {
                    RegNotifyChangeKeyValue(key, false, REG_NOTIFY_CHANGE_LAST_SET, change, true)
                };
                if armed.is_err() {
                    tracing::error!("[Theme] Failed to arm registry notification: {:?}", armed);
                    break;
                }

                let result = unsafe { WaitForMultipleObjects(&handles, false, u32::MAX) };

                if result.0 == WAIT_OBJECT_0.0 + 1 {
                    break;
                }
                if result.0 != WAIT_OBJECT_0.0 {
                    tracing::warn!("[Theme] Unexpected wait result: {:#010x}", result.0);
                    break;
                }

                // The value is read after the notification rather than carried
                // with it, so a change that lands while this is running is
                // picked up by this same read.
                if get_theme_mode() == ThemeMode::Automatic && update_resolved_theme() {
                    on_change();
                }
            }

            unsafe {
                let _ = RegCloseKey(key);
                CloseHandle(change).ok();
                CloseHandle(shutdown).ok();
            }
            tracing::info!("[Theme] Theme monitor exited");
        });

    match spawned {
        Ok(handle) => Some((shutdown_int, handle)),
        Err(error) => {
            tracing::error!("[Theme] Failed to spawn theme monitor: {}", error);
            unsafe {
                CloseHandle(HANDLE(change_int as *mut std::ffi::c_void)).ok();
                CloseHandle(HANDLE(shutdown_int as *mut std::ffi::c_void)).ok();
            }
            None
        }
    }
}

/// Signal the theme monitor to exit and wait for it.
///
/// The thread closes both event handles itself, so nothing is closed here.
#[cfg(target_os = "windows")]
pub fn shutdown_theme_monitor(shutdown_event: isize, thread: Option<std::thread::JoinHandle<()>>) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::SetEvent;

    if shutdown_event == 0 {
        return;
    }

    unsafe {
        SetEvent(HANDLE(shutdown_event as *mut std::ffi::c_void)).ok();
    }

    if let Some(thread) = thread {
        let _ = thread.join();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Get the current theme mode.
#[tauri::command]
pub fn get_theme_mode_command() -> Result<String, String> {
    let mode = get_theme_mode();
    serde_json::to_string(&mode).map_err(|e| format!("Failed to serialize theme mode: {}", e))
}

/// Set the theme mode.
#[tauri::command]
pub fn set_theme_mode_command(mode: String) -> Result<String, String> {
    let mode: ThemeMode = serde_json::from_str(&format!("\"{}\"", mode))
        .map_err(|e| format!("Invalid theme mode: {}", e))?;
    set_theme_mode(mode);
    Ok(format!("Theme mode set to {:?}", mode))
}

/// Get the current resolved theme name.
#[tauri::command]
pub fn get_resolved_theme_name_command() -> Result<String, String> {
    Ok(get_resolved_theme_name().to_string())
}

/// Get both theme mode and resolved theme in a single call.
/// Reduces frontend startup IPC round-trips.
#[tauri::command]
pub fn get_theme_state_command() -> Result<(String, String), String> {
    let mode = get_theme_mode();
    let resolved = get_resolved_theme_name().to_string();
    let mode_json = serde_json::to_string(&mode)
        .map_err(|e| format!("Failed to serialise theme mode: {}", e))?;
    Ok((mode_json, resolved))
}
