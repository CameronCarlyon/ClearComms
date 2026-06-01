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
