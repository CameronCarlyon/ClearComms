//! Toast Notification Manager
//!
//! Hosts toast notifications in a dedicated Tauri window backed by the app shell.

use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const TOAST_WINDOW_LABEL: &str = "toast";
const TOAST_WIDTH: f64 = 360.0;
const TOAST_HEIGHT: f64 = 90.0;

/// Classification of toast notification variants.
#[derive(Debug, Clone)]
pub enum ToastType {
    General,
    FutureFeature,
}

impl ToastType {
    fn from_key(value: &str) -> Self {
        match value {
            "future" => Self::FutureFeature,
            _ => Self::General,
        }
    }
}

/// Payload delivered to the toast frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastPayload {
    pub title: String,
    pub body: String,
    pub toast_type: String,
    pub theme: String,
}

impl ToastPayload {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            toast_type: "general".into(),
            theme: String::new(),
        }
    }
}

/// Show a toast notification in a dedicated window.
pub fn show_toast(app: &tauri::AppHandle, payload: ToastPayload) -> Result<(), String> {
    if main_window_is_visible(app) {
        close_toast_window_internal(app);
        return Ok(());
    }

    let payload = resolve_payload(payload)?;
    let initialisation_script = build_initialisation_script(&payload)?;

    // Only attempt close if a toast window actually exists (avoids unnecessary lookup on first launch)
    if app.get_webview_window(TOAST_WINDOW_LABEL).is_some() {
        close_toast_window_internal(app);
    }

    let window = WebviewWindowBuilder::new(app, TOAST_WINDOW_LABEL, WebviewUrl::App("toast-launch.html".into()))
        .title("ClearComms Toast")
        .inner_size(TOAST_WIDTH, TOAST_HEIGHT)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .initialization_script(&initialisation_script)
        .build()
        .map_err(|e| format!("Failed to create toast window: {}", e))?;

    crate::window_utils::position_window_bottom_right(&window);

    // Apply visual effects before showing (reduces flicker)
    #[cfg(target_os = "windows")]
    {
        crate::window_utils::apply_standard_window_visuals(&window, "toast");
    }

    let _ = window.show();

    Ok(())
}

#[tauri::command]
pub fn trigger_toast(
    app: tauri::AppHandle,
    title: String,
    body: String,
    toast_type: String,
) -> Result<(), String> {
    let t = ToastType::from_key(&toast_type);
    let payload = build_payload(title, body, t)?;
    show_toast(&app, payload)
}

#[tauri::command]
pub fn close_toast_window(app: tauri::AppHandle) -> Result<(), String> {
    close_toast_window_internal(&app);
    Ok(())
}

fn close_toast_window_internal(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(TOAST_WINDOW_LABEL) {
        let _ = window.close();
        // WebView2 cleanup is async; the window object itself is dropped here.
        // The renderer process is reclaimed by the OS within a few seconds.
    }
}

fn main_window_is_visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

fn resolve_payload(payload: ToastPayload) -> Result<ToastPayload, String> {
    if payload.theme.is_empty() {
        return build_payload(
            payload.title,
            payload.body,
            ToastType::from_key(&payload.toast_type),
        );
    }

    Ok(payload)
}

fn build_payload(
    title: String,
    body: String,
    toast_type: ToastType,
) -> Result<ToastPayload, String> {
    let theme_name = crate::theme::get_resolved_theme_name();

    Ok(ToastPayload {
        title,
        body,
        toast_type: match toast_type {
            ToastType::General => "general".to_string(),
            ToastType::FutureFeature => "future".to_string(),
        },
        theme: theme_name.to_string(),
    })
}

fn build_initialisation_script(payload: &ToastPayload) -> Result<String, String> {
    let payload_json = serde_json::to_string(payload)
        .map_err(|e| format!("Failed to serialise toast payload: {}", e))?;

    Ok(format!("window.__CLEARCOMMS_TOAST__ = {};", payload_json))
}
