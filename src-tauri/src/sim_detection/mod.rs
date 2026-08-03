//! Sim Detection Module
//!
//! Detects Microsoft Flight Simulator process lifecycle using periodic
//! Toolhelp32 process snapshots. This is a lightweight, stable approach
//! that requires no COM, no WMI, and no administrator privileges.
//!
//! ## Architecture
//! - Periodic Toolhelp32 snapshot every 2 seconds to detect process transitions.
//! - One-shot process snapshot at startup for already-running detection.
//! - Events emitted via [`mpsc::Sender<SimDetectionEvent>`].
//! - Thread sleeps via [`WaitForSingleObject`] between polls — zero CPU while idle.
//! - No COM, no WMI, no unsafe FFI beyond standard Windows process enumeration.

use std::sync::mpsc::Sender;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{SetEvent, WaitForSingleObject};

use crate::simconnect::state::SimVersion;

// Re-export the event enum for use by the lifecycle controller
pub enum SimDetectionEvent {
    Started(SimVersion),
    Stopped,
}

/// Internal state tracking which simulator (if any) is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MsfsState {
    NotRunning,
    Running2020,
    Running2024,
}

// Raw FFI for CreateEventW
#[cfg(windows)]
extern "system" {
    fn CreateEventW(
        lpEventAttributes: *mut std::ffi::c_void,
        bManualReset: i32,
        bInitialState: i32,
        lpName: *const u16,
    ) -> *mut std::ffi::c_void;
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Spawn the sim detection background thread.
///
/// Returns a tuple of `(shutdown_event, thread_handle)` where:
/// - `shutdown_event` is the Win32 event handle that should be signalled
///   to request clean shutdown.
/// - `thread_handle` is the [`JoinHandle`] that must be joined after
///   signalling shutdown to wait for clean thread exit.
///
/// # Shutdown Sequence
/// 1. Signal `shutdown_event` via [`signal_shutdown`].
/// 2. Join `thread_handle` to wait for thread exit cleanly.
/// 3. The thread closes the event handle itself on exit.
pub fn spawn_sim_detection_thread(
    sender: Sender<SimDetectionEvent>,
) -> (isize, std::thread::JoinHandle<()>) {
    let shutdown_event = unsafe {
        CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null())
    };
    if shutdown_event.is_null() {
        tracing::error!("[SimDetection] Failed to create shutdown event");
        let dummy = std::thread::Builder::new()
            .name("sim-detection".to_string())
            .spawn(move || {})
            .expect("Failed to spawn sim detection thread");
        return (0, dummy);
    }

    let shutdown_event_int = shutdown_event as isize;
    let handle = std::thread::Builder::new()
        .name("sim-detection".to_string())
        .spawn(move || {
            // RAII guard ensures the event handle is closed even if the thread panics.
            struct EventGuard(isize);
            impl Drop for EventGuard {
                fn drop(&mut self) {
                    unsafe {
                        CloseHandle(HANDLE(self.0 as *mut std::ffi::c_void)).ok();
                    }
                }
            }
            let _guard = EventGuard(shutdown_event_int);

            run_detection(sender, shutdown_event_int);
        })
        .expect("Failed to spawn sim detection thread");

    (shutdown_event_int, handle)
}

/// Signal the shutdown event and join the sim detection thread.
///
/// This is the correct shutdown sequence:
/// 1. Signal the shutdown event to wake the thread from its wait.
/// 2. Join the thread to wait for it to exit cleanly.
/// 3. The thread closes the event handle itself on exit.
pub fn shutdown_sim_detection_thread(
    shutdown_event: isize,
    handle: Option<std::thread::JoinHandle<()>>,
) {
    let handle = match handle {
        Some(h) => h,
        None => return,
    };

    unsafe {
        SetEvent(HANDLE(shutdown_event as *mut std::ffi::c_void)).ok();
    }
    tracing::info!("[SimDetection] Shutdown signal sent");

    if let Err(e) = handle.join() {
        tracing::error!("[SimDetection] Failed to join thread: {:?}", e);
    }
    tracing::info!("[SimDetection] Thread exited cleanly");
}

/// Signal the shutdown event without joining the thread.
///
/// Use this when you only need to signal shutdown (e.g. dev mode restart)
/// without waiting for the thread to exit.
pub fn signal_shutdown(shutdown_event: isize) {
    unsafe {
        SetEvent(HANDLE(shutdown_event as *mut std::ffi::c_void)).ok();
    }
    tracing::info!("[SimDetection] Shutdown signal sent (no join)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Detection Runner
// ─────────────────────────────────────────────────────────────────────────────

fn run_detection(sender: Sender<SimDetectionEvent>, shutdown_handle: isize) {
    // One-shot snapshot for already-running simulator
    let mut state = match scan_for_running_sim() {
        Some(version) => {
            tracing::info!(
                "[SimDetection] Simulator already running at startup ({:?})",
                version
            );
            let _ = sender.send(SimDetectionEvent::Started(version));
            match version {
                SimVersion::Msfs2020 => MsfsState::Running2020,
                SimVersion::Msfs2024 => MsfsState::Running2024,
                SimVersion::Unknown => MsfsState::NotRunning,
            }
        }
        None => MsfsState::NotRunning,
    };

    // Start Toolhelp32 polling loop
    run_polling_loop(&sender, shutdown_handle, &mut state);
}

// ─────────────────────────────────────────────────────────────────────────────
// Polling Loop
// ─────────────────────────────────────────────────────────────────────────────

/// Polls the process list every 2 seconds to detect MSFS start/stop transitions.
///
/// The thread sleeps via [`WaitForSingleObject`] with a 2-second timeout,
/// consuming zero CPU while idle. On each wake it takes a Toolhelp32 snapshot
/// and compares the result with the previous state to detect transitions.
#[cfg(windows)]
fn run_polling_loop(
    sender: &Sender<SimDetectionEvent>,
    shutdown_handle: isize,
    state: &mut MsfsState,
) {
    const POLL_INTERVAL_MS: u32 = 2000;

    tracing::info!("[SimDetection] Polling loop started (interval: {} ms)", POLL_INTERVAL_MS);

    loop {
        // Sleep efficiently — the OS blocks this thread, consuming zero CPU
        match unsafe { WaitForSingleObject(HANDLE(shutdown_handle as *mut std::ffi::c_void), POLL_INTERVAL_MS) } {
            WAIT_OBJECT_0 => {
                tracing::info!("[SimDetection] Shutdown signal received");
                break;
            }
            WAIT_TIMEOUT => {
                // Normal wake — time to poll
            }
            _ => {
                tracing::warn!("[SimDetection] WaitForSingleObject returned unexpected result");
                break;
            }
        }

        // Scan the process list for MSFS executables
        let current = scan_for_running_sim();

        // Detect transitions by comparing with tracked state
        match (*state, current) {
            // MSFS started (was not running, now detected)
            (MsfsState::NotRunning, Some(version)) => {
                handle_process_start(sender, state, version);
            }
            // MSFS stopped (was running, now gone)
            (MsfsState::Running2020 | MsfsState::Running2024, None) => {
                handle_process_stop(sender, state);
            }
            // Version changed (e.g. 2020 -> 2024, extremely unlikely but handle it)
            (MsfsState::Running2020, Some(SimVersion::Msfs2024)) => {
                tracing::info!(
                    "[SimDetection] Version change detected: Msfs2020 -> Msfs2024"
                );
                let _ = sender.send(SimDetectionEvent::Stopped);
                let _ = sender.send(SimDetectionEvent::Started(SimVersion::Msfs2024));
                *state = MsfsState::Running2024;
            }
            (MsfsState::Running2024, Some(SimVersion::Msfs2020)) => {
                tracing::info!(
                    "[SimDetection] Version change detected: Msfs2024 -> Msfs2020"
                );
                let _ = sender.send(SimDetectionEvent::Stopped);
                let _ = sender.send(SimDetectionEvent::Started(SimVersion::Msfs2020));
                *state = MsfsState::Running2020;
            }
            // No change — nothing to do
            _ => {}
        }
    }

    tracing::info!("[SimDetection] Polling loop exited");
}

#[cfg(not(windows))]
fn run_polling_loop(
    _sender: &Sender<SimDetectionEvent>,
    _shutdown_handle: isize,
    _state: &mut MsfsState,
) {
    // No-op on non-Windows platforms
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Handling
// ─────────────────────────────────────────────────────────────────────────────

/// Handle a process start event, updating state and emitting notifications.
fn handle_process_start(
    sender: &Sender<SimDetectionEvent>,
    state: &mut MsfsState,
    version: SimVersion,
) {
    let new_state = match version {
        SimVersion::Msfs2020 => MsfsState::Running2020,
        SimVersion::Msfs2024 => MsfsState::Running2024,
        SimVersion::Unknown => return,
    };

    if *state == new_state {
        // Already tracking this version; ignore duplicate.
        return;
    }

    tracing::info!(
        "[SimDetection] Process start detected: {:?} (state: {:?} -> {:?})",
        version,
        state,
        new_state
    );

    *state = new_state;
    let _ = sender.send(SimDetectionEvent::Started(version));
}

/// Handle a process stop event, updating state and emitting notifications.
fn handle_process_stop(sender: &Sender<SimDetectionEvent>, state: &mut MsfsState) {
    if *state == MsfsState::NotRunning {
        return;
    }

    let old_state = *state;
    tracing::info!(
        "[SimDetection] Process stop detected (state: {:?} -> NotRunning)",
        old_state
    );

    *state = MsfsState::NotRunning;
    let _ = sender.send(SimDetectionEvent::Stopped);
}

// ─────────────────────────────────────────────────────────────────────────────
// One-shot Process Snapshot
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn scan_for_running_sim() -> Option<SimVersion> {
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("[SimDetection] CreateToolhelp32Snapshot failed: {}", e);
                return None;
            }
        };

        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snapshot, &mut entry).is_err() {
            CloseHandle(snapshot).ok();
            return None;
        }

        let mut result = None;

        loop {
            let exe_len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let exe_name = String::from_utf16_lossy(&entry.szExeFile[..exe_len]);

            if exe_name.eq_ignore_ascii_case("FlightSimulator2024.exe") {
                result = Some(SimVersion::Msfs2024);
                break;
            }
            if exe_name.eq_ignore_ascii_case("FlightSimulator.exe") {
                result = Some(SimVersion::Msfs2020);
                // Do not break — prefer 2024 if both are somehow present.
            }

            if Process32NextW(snapshot, &mut entry).is_err() {
                break;
            }
        }

        CloseHandle(snapshot).ok();
        result
    }
}

#[cfg(not(windows))]
pub fn scan_for_running_sim() -> Option<SimVersion> {
    None
}
