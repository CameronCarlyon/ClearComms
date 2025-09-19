// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_management;
mod hardware_input;
mod simvar_input;
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            hardware_input::init_direct_input,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    clearcomms_lib::run()
}
