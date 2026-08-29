//! Native Windows toast notifications.
//!
//! Notifications are published under the AppUserModelID that the installer's
//! Start Menu shortcut registers. That registration is machine-wide rather than
//! per-executable, so development builds share the installed application's
//! identity and need no special handling.
//!
//! On a machine where ClearComms has never been installed there is no such
//! registration. Windows then drops the notification without reporting an
//! error, so the failure cannot be detected or logged. Creating the shortcut at
//! startup, as Microsoft's DesktopToasts sample does, would fix it.

#[cfg(windows)]
use tauri_winrt_notification::{Duration, Toast};
#[cfg(windows)]
use windows::core::HSTRING;
#[cfg(windows)]
use windows::UI::Notifications::ToastNotificationManager;

/// Clears by application ID rather than by tag. Tag removal also takes a group,
/// which an unpackaged application has no value for, and it rejects an empty
/// one.
#[cfg(windows)]
fn clear_history(app_id: &str) {
    let result = ToastNotificationManager::History()
        .and_then(|history| history.ClearWithId(&HSTRING::from(app_id)));

    if let Err(error) = result {
        tracing::debug!("[Notify] Could not clear notification history: {}", error);
    }
}

/// Shows a toast and removes it from the Action Centre once its popup ends.
/// Clicking it opens the main window.
///
/// `ExpirationTime` does not do the removal. It controls whether a notification
/// is still worth raising, and leaves an already-delivered one in place.
///
/// The clear before showing covers a previous run that exited before its
/// dismissal arrived. Activation and dismissal are mutually exclusive, so both
/// handlers clear.
#[cfg(windows)]
pub fn show(app: &tauri::AppHandle, title: &str, body: &str) {
    let app_id = app.config().identifier.clone();
    clear_history(&app_id);

    let app_for_click = app.clone();
    let app_id_for_click = app_id.clone();
    let app_id_for_dismissal = app_id.clone();

    let result = Toast::new(&app_id)
        .title(title)
        .text1(body)
        .duration(Duration::Short)
        .on_activated(move |_action| {
            clear_history(&app_id_for_click);

            // Fires on a WinRT thread pool thread, so the window work has to be
            // marshalled onto the main thread.
            let app = app_for_click.clone();
            let app_for_task = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = crate::show_main_window_internal(&app_for_task) {
                    tracing::warn!("[Notify] Could not show window on click: {}", error);
                }
            });
            Ok(())
        })
        .on_dismissed(move |_reason| {
            clear_history(&app_id_for_dismissal);
            Ok(())
        })
        .show();

    if let Err(error) = result {
        tracing::warn!("[Notify] Failed to show notification: {}", error);
    }
}

#[cfg(not(windows))]
pub fn show(_app: &tauri::AppHandle, _title: &str, _body: &str) {}
