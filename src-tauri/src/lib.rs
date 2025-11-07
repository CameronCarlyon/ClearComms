//! ClearComms library entry point.
//!
//! This module exists to satisfy Tauri's build requirements but the main
//! application logic is in main.rs. For desktop-only applications like
//! ClearComms, we use main.rs directly.

/// Empty run function to satisfy Tauri's library requirements.
/// The actual application is initialised in main.rs.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Desktop application uses main.rs directly.
    // This function exists only for Tauri's build system compatibility.
}
