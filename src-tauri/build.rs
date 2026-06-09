use std::path::PathBuf;

fn main() {
    tauri_build::build();
    link_simconnect();
}

/// Locate the MSFS SDK and emit cargo link directives for static SimConnect linking.
fn link_simconnect() {
    let sdk_path = resolve_sdk_path();

    let simconnect_lib_dir = sdk_path
        .join("SimConnect SDK")
        .join("lib")
        .join("static");

    if !simconnect_lib_dir.exists() {
        panic!(
            "SimConnect static library directory not found. \
             Please install the MSFS 2024 SDK or MSFS 2020 SDK. \
             Expected at: {}",
            simconnect_lib_dir.display()
        );
    }

    let lib_file = simconnect_lib_dir.join("SimConnect.lib");
    if !lib_file.exists() {
        panic!(
            "SimConnect.lib not found. \
             Please install the MSFS 2024 SDK or MSFS 2020 SDK. \
             Expected at: {}",
            lib_file.display()
        );
    }

    println!(
        "cargo:rustc-link-search=native={}",
        simconnect_lib_dir.display()
    );
    println!("cargo:rustc-link-lib=static=SimConnect");

    // Required Windows system libraries for SimConnect
    println!("cargo:rustc-link-lib=shlwapi");
    println!("cargo:rustc-link-lib=user32");
    println!("cargo:rustc-link-lib=Ws2_32");

    // Re-run if SDK env vars change
    println!("cargo:rerun-if-env-changed=MSFS2024_SDK");
    println!("cargo:rerun-if-env-changed=MSFS_SDK");
}

/// Resolve the MSFS SDK installation path.
///
/// Checks environment variables first (set by SDK installers), then falls back
/// to default installation directories.
fn resolve_sdk_path() -> PathBuf {
    if let Ok(path) = std::env::var("MSFS2024_SDK") {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    if let Ok(path) = std::env::var("MSFS_SDK") {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }

    let fallback_2024 = PathBuf::from("C:\\MSFS 2024 SDK");
    if fallback_2024.exists() {
        return fallback_2024;
    }

    let fallback_2020 = PathBuf::from("C:\\MSFS SDK");
    if fallback_2020.exists() {
        return fallback_2020;
    }

    panic!(
        "MSFS SDK not found. Please install the MSFS 2024 SDK or MSFS 2020 SDK, \
         or set the MSFS2024_SDK or MSFS_SDK environment variable."
    );
}
