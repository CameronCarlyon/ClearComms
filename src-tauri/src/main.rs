// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_management;
mod hardware_input;
mod simvar_input;
fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            hardware_input::init_direct_input,
            hardware_input::get_direct_input_status,
            hardware_input::enumerate_input_devices,
            hardware_input::get_all_axis_values,
            hardware_input::update_test_axis_value,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    clearcomms_lib::run()
}
