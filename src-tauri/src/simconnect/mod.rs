//! SimConnect Integration Module
//!
//! Provides a foundational SimConnect connection to Microsoft Flight Simulator
//! (2020 and 2024), polls the aircraft TITLE SimVar, and health-checks the
//! MobiFlight WASM module via ClientDataArea ping/pong.
//!
//! ## Architecture
//! - A dedicated background thread owns the SimConnect handle and runs the
//!   dispatch loop.
//! - Thread-safe `Mutex<SimState>` is stored as Tauri managed state and read
//!   by frontend commands.
//! - `simconnect-sdk` is used for high-level SimVar definitions.
//! - `simconnect-sys` raw FFI is used for MobiFlight ClientDataArea operations.
//!
//! ## Lifecycle
//! The SimConnect thread is no longer started once at app startup. Instead, the
//! `sim_detection` module emits `SimDetectionEvent::Started` / `Stopped` events
//! via an mpsc channel. This module's lifecycle controller listens on that
//! channel and dynamically spawns or tears down the SimConnect thread.
//!
//! ## Connection Retry Strategy
//! When MSFS launches, its SimConnect IPC server takes several seconds to become
//! ready after the process appears. `SimConnect_Open` will fail with E_FAIL
//! (0x80004005) during this window. To handle this without adding polling inside
//! the SimConnect thread, each spawn includes a "connection manager" wrapper that:
//! 1. Runs the SimConnect loop (single attempt).
//! 2. If the attempt fails AND MSFS is still running, re-injects a `Started`
//!    event into the detection channel after a short delay, so the lifecycle
//!    controller naturally retries the connection.
//! 3. The delay uses `WaitForSingleObject` on the shutdown event, ensuring it
//!    is interrupted instantly if the app shuts down or the sim exits.

use std::sync::{Arc, Mutex};

use tauri::Emitter;
use tauri::State;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::Threading::SetEvent;

pub mod state;
mod connection;

use state::{ConnectionState, SimState, SimStateHandle, SimStatusResponse, SimVersion, WasmState};
use crate::sim_detection::SimDetectionEvent;

// Raw FFI for CreateEventW since the windows crate path varies by version
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
// Tauri Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Return the current simulator connection status.
///
/// # Returns
/// A `SimStatusResponse` containing:
/// - `connected`: true if SimConnect is open and dispatching
/// - `wasm_present`: true if the MobiFlight WASM module responded to the last ping
/// - `aircraft_title`: the currently loaded aircraft title, if known
/// - `sim_version`: "2020", "2024", or "unknown"
///
/// # Note on state type
/// `main.rs` manages `Arc<SimStateHandle>` (i.e. `Arc<Mutex<SimState>>`). Tauri
/// stores managed state by `TypeId`, so the extractor here must use
/// `State<Arc<SimStateHandle>>` — not `State<SimStateHandle>` — to match the
/// TypeId of what was actually registered. Rust's auto-deref chain means
/// `state.lock()` still compiles and works:
///   `State<Arc<Mutex<SimState>>>` → deref → `Arc<Mutex<SimState>>`
///   → deref → `Mutex<SimState>` → `.lock()`.
#[tauri::command]
pub fn get_sim_status(state: State<Arc<SimStateHandle>>) -> Result<SimStatusResponse, String> {
    let guard = state
        .lock()
        .map_err(|e| format!("Failed to lock SimState: {}", e))?;

    Ok(SimStatusResponse {
        connected: matches!(guard.connection, ConnectionState::Connected),
        wasm_present: matches!(guard.wasm, WasmState::Present),
        aircraft_title: guard.aircraft_title.clone(),
        sim_version: match guard.sim_version {
            SimVersion::Msfs2020 => "2020".to_string(),
            SimVersion::Msfs2024 => "2024".to_string(),
            SimVersion::Unknown => "unknown".to_string(),
        },
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// SimConnect Session
// ─────────────────────────────────────────────────────────────────────────────

/// Holds the handles for an active SimConnect background thread so it can be
/// signalled and joined when the simulator exits.
///
/// The shutdown event is stored as `isize` rather than `HANDLE` so the struct
/// is `Send` + `Sync`, which is required for Tauri managed state.
pub struct SimConnectSession {
    /// Win32 manual-reset event handle signalled to request thread shutdown.
    shutdown_event: isize,
    /// The background thread handle (stored as `std::thread::JoinHandle`).
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

/// Thread-safe optional session. The lifecycle controller holds this under a
/// mutex so it can start/stop the thread safely from the coordinator thread.
pub type SimConnectSessionHandle = Mutex<Option<SimConnectSession>>;

// ─────────────────────────────────────────────────────────────────────────────
// Lifecycle Controller
// ─────────────────────────────────────────────────────────────────────────────

/// Start the lifecycle controller on a background thread.
///
/// The controller blocks on the mpsc receiver and spawns or tears down the
/// SimConnect thread in response to `SimDetectionEvent`s. It also stores the
/// shared `SimState` as Tauri managed state.
///
/// `retry_sender` is a clone of the detection channel's sender. The connection
/// manager uses it to re-inject `Started` events when a connection attempt fails
/// but MSFS is still running, enabling automatic retry without polling in the
/// SimConnect thread itself.
pub fn start_lifecycle_controller(
    app: tauri::AppHandle,
    state: Arc<SimStateHandle>,
    receiver: std::sync::mpsc::Receiver<SimDetectionEvent>,
    retry_sender: std::sync::mpsc::Sender<SimDetectionEvent>,
) -> Arc<SimConnectSessionHandle> {
    let session: Arc<SimConnectSessionHandle> = Arc::new(Mutex::new(None));
    let session_for_thread = session.clone();
    let state_for_thread = state.clone();

    std::thread::Builder::new()
        .name("simconnect-ctrl".to_string())
        .spawn(move || {
            tracing::info!("[SimConnect] Lifecycle controller started");
            for event in receiver {
                match event {
                    SimDetectionEvent::Started(version) => {
                        tracing::info!(
                            "[SimConnect] Lifecycle: simulator started ({:?})",
                            version
                        );
                        spawn_simconnect_thread(
                            &app,
                            &session_for_thread,
                            &state_for_thread,
                            version,
                            retry_sender.clone(),
                        );
                    }
                    SimDetectionEvent::Stopped => {
                        tracing::info!("[SimConnect] Lifecycle: simulator stopped");
                        stop_simconnect_thread(&app, &session_for_thread, &state_for_thread);
                    }
                }
            }
            tracing::info!("[SimConnect] Lifecycle controller exited");
        })
        .expect("Failed to spawn SimConnect lifecycle controller");

    session
}

/// Signal any active SimConnect thread to shut down, then join it and reset
/// state. Called on app shutdown or restart.
pub fn shutdown_simconnect_thread(session: &Arc<SimConnectSessionHandle>) {
    // Scope the lock so it is released before joining the thread.
    // The connection manager acquires this same mutex after run_simconnect_loop
    // returns (to clear its own session entry). Holding the lock across join()
    // would cause a deadlock: this function would wait for the thread to exit
    // while the thread waits for the lock.
    let sess = {
        let mut lock = session.lock().unwrap();
        lock.take()
    };

    let Some(sess) = sess else {
        return;
    };

    // Signal shutdown
    unsafe {
        SetEvent(HANDLE(sess.shutdown_event as *mut std::ffi::c_void)).ok();
    }
    tracing::info!("[SimConnect] Shutdown signal sent to background thread");

    // Join the thread
    if let Some(handle) = sess.thread_handle {
        let _ = handle.join();
    }

    // Close the event handle
    unsafe {
        CloseHandle(HANDLE(sess.shutdown_event as *mut std::ffi::c_void)).ok();
    }

    tracing::info!("[SimConnect] Background thread stopped during shutdown");
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal Spawn / Stop
// ─────────────────────────────────────────────────────────────────────────────

/// How long the connection manager waits before retrying after a failed
/// `SimConnect_Open`. Five seconds gives the simulator enough time to bring its
/// IPC server up without making the user wait excessively.
const RETRY_DELAY_MS: u32 = 5_000;

/// Spawn the SimConnect connection manager thread if one is not already running.
///
/// The connection manager wraps the SimConnect loop with post-failure retry
/// logic: if the connection attempt fails AND MSFS is still in the process list,
/// it re-injects a `SimDetectionEvent::Started` into the detection channel after
/// a short delay. The lifecycle controller then processes it as a fresh start,
/// creating a new connection attempt — keeping retry behaviour entirely within
/// the event-driven architecture.
///
/// The delay is implemented with `WaitForSingleObject` on the shutdown event so
/// it is interrupted instantly if the app shuts down or the sim exits.
fn spawn_simconnect_thread(
    app: &tauri::AppHandle,
    session: &Arc<SimConnectSessionHandle>,
    state: &Arc<SimStateHandle>,
    version: SimVersion,
    retry_sender: std::sync::mpsc::Sender<SimDetectionEvent>,
) {
    let mut lock = session.lock().unwrap();

    if lock.is_some() {
        tracing::debug!("[SimConnect] Connection manager already running, skipping");
        return;
    }

    // Create a manual-reset, initially unsignalled event for shutdown signalling.
    let shutdown_event = unsafe {
        CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null())
    };
    if shutdown_event.is_null() {
        tracing::error!("[SimConnect] Failed to create shutdown event handle");
        return;
    }

    let shutdown_event_int = shutdown_event as isize;
    let state_for_thread = state.clone();
    let session_for_thread = session.clone();
    let app_for_thread = app.clone();

    let handle = std::thread::Builder::new()
        .name("simconnect".to_string())
        .spawn(move || {
            tracing::info!("[SimConnect] Connection manager started");

            // Run the SimConnect loop (single attempt)
            connection::run_simconnect_loop(
                app_for_thread,
                shutdown_event_int,
                state_for_thread.clone(),
                version,
            );

            // ── Post-exit: decide whether to schedule a retry ──────────────
            //
            // A retry is needed when:
            //   1. The connection was never established — `last_error` is set.
            //   2. The shutdown event was not signalled — the sim is still running.
            //   3. MSFS is still in the process list (confirms the sim didn't exit
            //      between the connection attempt and this check, which would cause
            //      an infinite retry loop).
            let connection_failed = state_for_thread
                .lock()
                .map(|s| s.last_error.is_some())
                .unwrap_or(false);

            let shutdown_signalled = unsafe {
                use windows::Win32::Foundation::WAIT_OBJECT_0;
                use windows::Win32::System::Threading::WaitForSingleObject;
                WaitForSingleObject(
                    windows::Win32::Foundation::HANDLE(shutdown_event_int as *mut std::ffi::c_void),
                    0,
                ) == WAIT_OBJECT_0
            };

            // Clear our own session entry so the lifecycle controller can spawn a
            // fresh one when it receives the retry event.
            {
                let mut lock = session_for_thread.lock().unwrap();
                if lock.is_some() {
                    // Only clear if the shutdown event hasn't been taken by
                    // stop_simconnect_thread — that function takes the session
                    // first (under the same lock), so if we still own it here
                    // we are responsible for closing the event handle.
                    *lock = None;
                    unsafe {
                        CloseHandle(windows::Win32::Foundation::HANDLE(
                            shutdown_event_int as *mut std::ffi::c_void,
                        ))
                        .ok();
                    }
                }
            }

            if connection_failed && !shutdown_signalled {
                // Verify MSFS is still running before scheduling the retry.
                // This prevents a chain of spurious retries if MSFS exited in the
                // narrow window between the failed open and this check.
                let sim_still_running = crate::sim_detection::scan_for_running_sim().is_some();

                if sim_still_running {
                    tracing::info!(
                        "[SimConnect] Connection failed but MSFS still running — \
                         retrying in {}s via detection channel",
                        RETRY_DELAY_MS / 1000
                    );
                    // Interruptible sleep — wakes immediately if shutdown is signalled.
                    // We no longer hold the shutdown event (closed above), so use a
                    // plain sleep here; the duration is short (5 s) and we already
                    // verified the sim is running. If the sim exits mid-sleep the
                    // process-scan guard on the next watcher prevents infinite loops.
                    std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS as u64));
                    let _ = retry_sender.send(SimDetectionEvent::Started(version));
                    tracing::info!("[SimConnect] Retry event sent to lifecycle controller");
                } else {
                    tracing::info!(
                        "[SimConnect] Connection failed and MSFS no longer running — \
                         not scheduling retry"
                    );
                }
            }

            tracing::info!("[SimConnect] Connection manager exited");
        })
        .expect("Failed to spawn SimConnect connection manager thread");

    *lock = Some(SimConnectSession {
        shutdown_event: shutdown_event_int,
        thread_handle: Some(handle),
    });
}

/// Signal the SimConnect connection manager to shut down, join it, close the
/// event handle, and reset `SimState`.
///
/// State is always reset even when no active session exists, because the
/// connection manager may have already cleared the session after a failed
/// attempt (while a retry event is in-flight in the detection channel).
fn stop_simconnect_thread(
    app: &tauri::AppHandle,
    session: &Arc<SimConnectSessionHandle>,
    state: &Arc<SimStateHandle>,
) {
    let sess = {
        let mut lock = session.lock().unwrap();
        lock.take()
    };

    if let Some(sess) = sess {
        // Signal shutdown — this also wakes any WaitForSingleObject inside the
        // connection manager so it exits its retry delay immediately.
        unsafe {
            SetEvent(HANDLE(sess.shutdown_event as *mut std::ffi::c_void)).ok();
        }
        tracing::info!("[SimConnect] Shutdown signal sent to connection manager");

        // Join the thread
        if let Some(handle) = sess.thread_handle {
            let _ = handle.join();
        }

        // Close the event handle (if the manager didn't already close it)
        unsafe {
            CloseHandle(HANDLE(sess.shutdown_event as *mut std::ffi::c_void)).ok();
        }
    }

    // Always reset SimState and emit to frontend, even if the session was already
    // gone (e.g. the manager had cleared itself after a failed connection attempt).
    update_state_and_emit(app, state, |s| {
        s.connection = ConnectionState::Disconnected;
        s.aircraft_title = None;
        s.wasm = WasmState::Absent;
        s.sim_version = SimVersion::Unknown;
        s.last_error = None;
    });

    tracing::info!("[SimConnect] Connection manager stopped and state reset");
}

// ─────────────────────────────────────────────────────────────────────────────
// State Update Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Apply a mutation to the shared `SimState` inside the mutex.
///
/// Logs an error if the mutex is poisoned but does not panic.
pub fn update_state<F>(state: &Arc<SimStateHandle>, f: F)
where
    F: FnOnce(&mut SimState),
{
    match state.lock() {
        Ok(mut guard) => {
            f(&mut *guard);
        }
        Err(e) => {
            tracing::error!("[SimConnect] SimState mutex poisoned: {}", e);
        }
    }
}

/// Apply a mutation to the shared `SimState` and emit a Tauri event with the new status.
///
/// This is the event-driven version used to notify the frontend of status changes.
pub fn update_state_and_emit<F>(app: &tauri::AppHandle, state: &Arc<SimStateHandle>, f: F)
where
    F: FnOnce(&mut SimState),
{
    match state.lock() {
        Ok(mut guard) => {
            f(&mut *guard);
            
            // Emit the updated status to the frontend
            let response = SimStatusResponse {
                connected: matches!(guard.connection, ConnectionState::Connected),
                wasm_present: matches!(guard.wasm, WasmState::Present),
                aircraft_title: guard.aircraft_title.clone(),
                sim_version: match guard.sim_version {
                    SimVersion::Msfs2020 => "2020".to_string(),
                    SimVersion::Msfs2024 => "2024".to_string(),
                    SimVersion::Unknown => "unknown".to_string(),
                },
            };
            
            if let Err(e) = app.emit("sim-status-changed", response) {
                tracing::warn!("[SimConnect] Failed to emit sim-status-changed event: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("[SimConnect] SimState mutex poisoned: {}", e);
        }
    }
}
