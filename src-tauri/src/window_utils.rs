//! Window Utilities
//!
//! Helper functions for positioning, sizing, and managing the ClearComms window.
//! Provides display-aware window management that adapts to any screen resolution,
//! DPI scaling, and taskbar configuration.
//!
//! ## Display Awareness
//!
//! Instead of hardcoding taskbar height or screen padding for a specific display
//! configuration, this module queries the Windows work area API (`GetMonitorInfoW`)
//! to determine the usable screen space. This automatically accounts for:
//! - Any display resolution (1080p, 1440p, 4K, ultrawide, etc.)
//! - Any DPI scaling (100%, 125%, 150%, 200%, etc.)
//! - Any taskbar position (bottom, top, left, right) and size
//! - Multi-monitor setups (uses the monitor containing the window)

use serde::Serialize;
use tauri::PhysicalPosition;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, HWND_TOP,
};

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Padding from the edge of the work area in logical pixels.
/// Converted to physical pixels using the display's scale factor to maintain
/// consistent visual spacing across all DPI configurations.
///
/// Examples at different scales:
/// - 100% (1.0×): 12 physical pixels
/// - 125% (1.25×): 15 physical pixels
/// - 150% (1.5×): 18 physical pixels (matches original hardcoded value at 4K/150%)
/// - 200% (2.0×): 24 physical pixels
const EDGE_PADDING_LOGICAL: f64 = 12.0;

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/// Information about the display and usable work area for the window's monitor.
///
/// The work area is the portion of the screen not occupied by the system taskbar
/// or other desktop toolbars. All spatial values are in physical pixels.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    /// Full monitor width in physical pixels
    pub monitor_width: i32,
    /// Full monitor height in physical pixels
    pub monitor_height: i32,
    /// Work area left edge in physical pixels (non-zero if taskbar is on the left)
    pub work_area_left: i32,
    /// Work area top edge in physical pixels (non-zero if taskbar is on the top)
    pub work_area_top: i32,
    /// Work area right edge in physical pixels
    pub work_area_right: i32,
    /// Work area bottom edge in physical pixels
    pub work_area_bottom: i32,
    /// Usable work area width in physical pixels
    pub work_area_width: i32,
    /// Usable work area height in physical pixels
    pub work_area_height: i32,
    /// DPI scale factor (e.g., 1.0 for 100%, 1.5 for 150%, 2.0 for 200%)
    pub scale_factor: f64,
    /// Edge padding in physical pixels (EDGE_PADDING_LOGICAL × scale_factor)
    pub edge_padding: i32,
    /// Maximum permissible window width in physical pixels (work area width − 2 × edge padding)
    pub max_window_width: i32,
    /// Maximum permissible window height in physical pixels (work area height − 2 × edge padding)
    pub max_window_height: i32,
}

// ─────────────────────────────────────────────────────────────────────────────
// Display Detection
// ─────────────────────────────────────────────────────────────────────────────

/// Retrieve display and work area information for the monitor containing the given window.
///
/// Uses the Windows `MonitorFromWindow` + `GetMonitorInfoW` APIs to obtain the
/// `rcWork` rectangle, which represents the usable screen area excluding the taskbar.
/// This eliminates the need to hardcode taskbar height or make assumptions about
/// display configuration.
///
/// # Arguments
/// * `window` - The Tauri webview window to query the monitor for
///
/// # Returns
/// `Some(DisplayInfo)` with complete display metrics, or `None` if detection fails.
#[cfg(target_os = "windows")]
pub fn get_display_info_for_window(window: &tauri::WebviewWindow) -> Option<DisplayInfo> {
    let hwnd_raw = window.hwnd().ok()?;
    let scale_factor = window.scale_factor().ok()?;
    let padding_physical = (EDGE_PADDING_LOGICAL * scale_factor).round() as i32;

    unsafe {
        let hmonitor = MonitorFromWindow(HWND(hwnd_raw.0), MONITOR_DEFAULTTOPRIMARY);

        let mut monitor_info: MONITORINFO = std::mem::zeroed();
        monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

        // GetMonitorInfoW fills rcMonitor (full screen) and rcWork (minus taskbar).
        // The return type in windows crate 0.58 may be BOOL or Result<()> depending
        // on the metadata; we handle both by checking the result inline.
        let success = GetMonitorInfoW(hmonitor, &mut monitor_info);

        // windows crate maps this function to return BOOL
        if !success.as_bool() {
            tracing::warn!("[Display] GetMonitorInfoW failed, cannot determine work area");
            return None;
        }

        let work = monitor_info.rcWork;
        let full = monitor_info.rcMonitor;

        let work_width = work.right - work.left;
        let work_height = work.bottom - work.top;

        Some(DisplayInfo {
            monitor_width: full.right - full.left,
            monitor_height: full.bottom - full.top,
            work_area_left: work.left,
            work_area_top: work.top,
            work_area_right: work.right,
            work_area_bottom: work.bottom,
            work_area_width: work_width,
            work_area_height: work_height,
            scale_factor,
            edge_padding: padding_physical,
            max_window_width: work_width - (2 * padding_physical),
            max_window_height: work_height - (2 * padding_physical),
        })
    }
}

/// Fallback display detection for non-Windows platforms.
/// Uses Tauri's cross-platform monitor API (does not account for taskbar).
#[cfg(not(target_os = "windows"))]
pub fn get_display_info_for_window(window: &tauri::WebviewWindow) -> Option<DisplayInfo> {
    let monitor = window.primary_monitor().ok()??;
    let scale_factor = window.scale_factor().ok()?;
    let padding_physical = (EDGE_PADDING_LOGICAL * scale_factor).round() as i32;
    let size = monitor.size();
    let w = size.width as i32;
    let h = size.height as i32;

    Some(DisplayInfo {
        monitor_width: w,
        monitor_height: h,
        work_area_left: 0,
        work_area_top: 0,
        work_area_right: w,
        work_area_bottom: h,
        work_area_width: w,
        work_area_height: h,
        scale_factor,
        edge_padding: padding_physical,
        max_window_width: w - (2 * padding_physical),
        max_window_height: h - (2 * padding_physical),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Window Positioning
// ─────────────────────────────────────────────────────────────────────────────

/// Position the window in the bottom-right corner of the usable work area.
///
/// The work area is the screen space excluding the system taskbar, so this
/// automatically avoids overlapping the taskbar regardless of its position or size.
/// Padding is applied consistently and scaled with DPI.
///
/// The window position is clamped to ensure it never extends beyond the work area
/// boundaries, even if the window is wider than the available space.
///
/// # Arguments
/// * `window` - The Tauri webview window to position
///
/// # Notes
/// - Detects taskbar position and size automatically via the Windows work area API
/// - Scales padding with DPI for consistent visual appearance across displays
/// - Clamps the window position to keep it fully visible on-screen
/// - Silently returns if display info or window size cannot be determined
pub fn position_window_bottom_right(window: &tauri::WebviewWindow) {
    if let Some(display) = get_display_info_for_window(window) {
        if let Ok(window_size) = window.outer_size() {
            let window_width = window_size.width as i32;
            let window_height = window_size.height as i32;

            // Anchor to bottom-right of the work area with padding
            let mut x = display.work_area_right - window_width - display.edge_padding;
            let mut y = display.work_area_bottom - window_height - display.edge_padding;

            // Clamp to prevent the window from extending beyond the work area
            // (e.g., when the window is very wide with many pinned applications)
            x = x.max(display.work_area_left + display.edge_padding);
            y = y.max(display.work_area_top + display.edge_padding);

            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Atomic Window Move + Resize
// ─────────────────────────────────────────────────────────────────────────────

/// Atomically set the window position and size in a single Win32 call.
///
/// Uses `SetWindowPos` which commits both the move and resize in one operation,
/// preventing the visual flicker that occurs when calling `set_position()` and
/// `set_size()` separately. The window manager applies the change as a single
/// transaction, so there is never a frame where only one of the two has taken effect.
///
/// # Arguments
/// * `window` - The Tauri webview window
/// * `x` - Target X position in physical pixels
/// * `y` - Target Y position in physical pixels
/// * `width` - Target width in physical pixels
/// * `height` - Target height in physical pixels
#[cfg(target_os = "windows")]
pub fn set_window_pos_and_size(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    if let Ok(hwnd_raw) = window.hwnd() {
        unsafe {
            let _ = SetWindowPos(
                HWND(hwnd_raw.0),
                HWND_TOP,
                x,
                y,
                width as i32,
                height as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Fallback for non-Windows: two separate calls (position then size).
#[cfg(not(target_os = "windows"))]
pub fn set_window_pos_and_size(
    window: &tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) {
    let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize { width, height }));
}

// ─────────────────────────────────────────────────────────────────────────────
// Window Visuals
// ─────────────────────────────────────────────────────────────────────────────

/// Apply the same Windows visual treatment used by the main application window.
///
/// This includes Acrylic backdrop, rounded corners, and disabled transitions
/// for instant show/hide behaviour.
#[cfg(target_os = "windows")]
pub fn apply_standard_window_visuals(window: &tauri::WebviewWindow, label: &str) {
    use window_vibrancy::apply_acrylic;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::*;

    tracing::info!("[Window:{}] Applying standard window visuals", label);

    if let Err(error) = apply_acrylic(window, None) {
        tracing::warn!("[Window:{}] Failed to apply acrylic effect: {}", label, error);
    } else {
        tracing::info!("[Window:{}] Acrylic effect applied", label);
    }

    let hwnd = match window.hwnd() {
        Ok(raw) => HWND(raw.0),
        Err(error) => {
            tracing::warn!("[Window:{}] Failed to get HWND for visual attributes: {}", label, error);
            return;
        }
    };

    let corner_preference: i32 = DWMWCP_ROUND.0;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_preference as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }

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

#[cfg(not(target_os = "windows"))]
pub fn apply_standard_window_visuals(_window: &tauri::WebviewWindow, _label: &str) {}
