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
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
#[cfg(windows)]
use windows::Win32::System::Threading::{SetEvent, WaitForSingleObject};

pub mod state;
mod connection;

use state::{ConnectionState, SimState, SimStateHandle, SimStatusResponse, SimVersion, WasmState};
use crate::sim_detection::SimDetectionEvent;

// ─────────────────────────────────────────────────────────────────────────────
// LVar Command Channel
// ─────────────────────────────────────────────────────────────────────────────

/// Maximum number of LVars the frontend may subscribe at once. Far beyond the
/// 16 needed for a fully assigned radio panel (8 categories × volume + mute),
/// while keeping client data definition/request ID ranges bounded.
pub const MAX_LVAR_SUBSCRIPTIONS: usize = 64;

/// Commands sent from the frontend to the SimConnect thread for MobiFlight
/// LVar operations. All SimConnect API calls must happen on the connection
/// thread, so Tauri commands enqueue work here and signal a Win32 event to
/// wake the dispatch loop.
#[derive(Debug)]
pub enum LvarCommand {
    /// Replace the entire LVar subscription set. An empty list unsubscribes all.
    Subscribe(Vec<String>),
    /// Write a value to an LVar via `MF.SimVars.Set`.
    Set { name: String, value: f64 },
}

/// The sending half of the LVar command channel plus the Win32 event handle
/// (as `isize`, for Send + Sync) used to wake the dispatch loop after a send.
pub struct LvarCommandChannel {
    pub sender: std::sync::mpsc::Sender<LvarCommand>,
    pub wake_event: isize,
}

/// Thread-safe optional command channel. Populated while a SimConnect
/// connection is active; `None` when the simulator is not connected.
pub type LvarCommandHandle = Mutex<Option<LvarCommandChannel>>;

/// Validate an LVar name before it is embedded into MobiFlight calculator code.
/// LVar names consist solely of ASCII alphanumerics and underscores; rejecting
/// anything else prevents malformed (or injected) RPN from ever reaching the
/// WASM module.
fn validate_lvar_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 128 {
        return Err(format!("Invalid LVar name length: {}", name.len()));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("Invalid characters in LVar name: {}", name));
    }
    Ok(())
}

/// Send a command to the SimConnect thread and wake its dispatch loop.
///
/// The mutex is deliberately held across `SetEvent`. The connection thread
/// closes the wake event handle only while holding this same lock, so keeping it
/// for the duration of the signal makes it impossible to `SetEvent` a handle
/// that has just been closed: and possibly already recycled by Windows for an
/// unrelated kernel object elsewhere in the process. Both operations inside the
/// lock are non-blocking (an unbounded channel send and one fast syscall), so
/// there is no contention risk.
fn send_lvar_command(
    handle: &Arc<LvarCommandHandle>,
    command: LvarCommand,
) -> Result<(), String> {
    let lock = handle
        .lock()
        .map_err(|e| format!("Failed to lock LVar command handle: {}", e))?;

    let Some(channel) = lock.as_ref() else {
        return Err("SimConnect is not connected".to_string());
    };

    channel
        .sender
        .send(command)
        .map_err(|e| format!("Failed to queue LVar command: {}", e))?;

    // Wake the dispatch loop so the command is processed immediately.
    #[cfg(windows)]
    unsafe {
        SetEvent(HANDLE(channel.wake_event as *mut std::ffi::c_void)).ok();
    }

    Ok(())
}

/// Replace the LVar subscription set on the MobiFlight WASM module.
/// The backend subscribes each name as `(L:<name>)` and streams value changes
/// back to the frontend as `lvar-value-changed` events.
#[tauri::command]
pub fn subscribe_lvars(
    lvar_handle: State<Arc<LvarCommandHandle>>,
    names: Vec<String>,
) -> Result<(), String> {
    if names.len() > MAX_LVAR_SUBSCRIPTIONS {
        return Err(format!(
            "Too many LVar subscriptions: {} (max {})",
            names.len(),
            MAX_LVAR_SUBSCRIPTIONS
        ));
    }
    for name in &names {
        validate_lvar_name(name)?;
    }
    send_lvar_command(&lvar_handle, LvarCommand::Subscribe(names))
}

/// Write a value to an LVar in the simulator.
#[tauri::command]
pub fn set_sim_lvar(
    lvar_handle: State<Arc<LvarCommandHandle>>,
    name: String,
    value: f64,
) -> Result<(), String> {
    validate_lvar_name(&name)?;
    if !value.is_finite() {
        return Err("LVar value must be finite".to_string());
    }
    send_lvar_command(&lvar_handle, LvarCommand::Set { name, value })
}

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
/// `State<Arc<SimStateHandle>>` rather than `State<SimStateHandle>`, so that
/// it matches the TypeId of what was actually registered. Rust's auto-deref chain means
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
///
/// `lvar_handle` is shared with the Tauri commands above: it is populated with
/// a fresh command channel each time a SimConnect thread spawns and cleared
/// when that thread exits, so commands always target the live connection.
pub fn start_lifecycle_controller(
    app: tauri::AppHandle,
    state: Arc<SimStateHandle>,
    lvar_handle: Arc<LvarCommandHandle>,
    receiver: std::sync::mpsc::Receiver<SimDetectionEvent>,
    retry_sender: std::sync::mpsc::Sender<SimDetectionEvent>,
) -> Arc<SimConnectSessionHandle> {
    let session: Arc<SimConnectSessionHandle> = Arc::new(Mutex::new(None));
    let session_for_thread = session.clone();
    let state_for_thread = state.clone();

    std::thread::Builder::new()
        .name("simconnect-ctrl".to_string())
        // Blocks on an mpsc receive and spawns; the default reserve is far more
        // address space than it can use. Reserve only, so this trims virtual
        // size rather than working set.
        .stack_size(256 * 1024)
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
                            &lvar_handle,
                            version,
                            retry_sender.clone(),
                        );
                    }
                    SimDetectionEvent::Stopped => {
                        tracing::info!("[SimConnect] Lifecycle: simulator stopped");
                        stop_simconnect_thread(&app, &session_for_thread, &state_for_thread, &lvar_handle);
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
        let mut lock = match session.lock() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("[SimConnect] Session mutex poisoned during shutdown: {}", e);
                return;
            }
        };
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

/// Ceiling for the backoff below.
const RETRY_DELAY_MAX_MS: u32 = 60_000;

/// Consecutive reconnect attempts that never reached a live connection.
///
/// Every reconnect re-opens SimConnect, re-registers the MobiFlight client and
/// rebuilds the SimVar subscription, all of which runs inside the simulator
/// process. A condition that ends the dispatch loop immediately and repeatably
/// (an invalid wait handle, say) would otherwise drive that churn every five
/// seconds indefinitely, so the delay doubles until something connects.
static CONSECUTIVE_FAILED_ATTEMPTS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

/// Reset the reconnect backoff. Called once a connection is actually live.
pub fn note_connection_established() {
    CONSECUTIVE_FAILED_ATTEMPTS.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// The delay before the next reconnect, doubling per consecutive failure.
fn next_retry_delay_ms() -> u32 {
    let attempts = CONSECUTIVE_FAILED_ATTEMPTS
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .min(8);

    RETRY_DELAY_MS
        .saturating_mul(1u32 << attempts)
        .min(RETRY_DELAY_MAX_MS)
}

/// Spawn the SimConnect connection manager thread if one is not already running.
///
/// The connection manager wraps the SimConnect loop with post-failure retry
/// logic: if the connection attempt fails AND MSFS is still in the process list,
/// it re-injects a `SimDetectionEvent::Started` into the detection channel after
/// a short delay. The lifecycle controller then processes it as a fresh start,
/// creating a new connection attempt: keeping retry behaviour entirely within
/// the event-driven architecture.
///
/// The delay is implemented with `WaitForSingleObject` on the shutdown event so
/// it is interrupted instantly if the app shuts down or the sim exits.
fn spawn_simconnect_thread(
    app: &tauri::AppHandle,
    session: &Arc<SimConnectSessionHandle>,
    state: &Arc<SimStateHandle>,
    lvar_handle: &Arc<LvarCommandHandle>,
    version: SimVersion,
    retry_sender: std::sync::mpsc::Sender<SimDetectionEvent>,
) {
    let mut lock = match session.lock() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(
                "[SimConnect] Session mutex poisoned: cannot spawn thread: {}",
                e
            );
            return;
        }
    };

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

    // Create the LVar command channel and an auto-reset wake event. The sender
    // is shared with the Tauri commands; the receiver moves to the connection
    // thread, which drains it whenever the wake event is signalled.
    let (lvar_tx, lvar_rx) = std::sync::mpsc::channel::<LvarCommand>();
    let lvar_wake_event = unsafe {
        CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null())
    };
    if lvar_wake_event.is_null() {
        tracing::error!("[SimConnect] Failed to create LVar wake event handle");
        unsafe { CloseHandle(HANDLE(shutdown_event)).ok(); }
        return;
    }

    let lvar_wake_event_int = lvar_wake_event as isize;
    match lvar_handle.lock() {
        Ok(mut lvar_lock) => {
            *lvar_lock = Some(LvarCommandChannel {
                sender: lvar_tx,
                wake_event: lvar_wake_event_int,
            });
        }
        Err(e) => {
            tracing::error!("[SimConnect] LVar command handle poisoned: cannot spawn: {}", e);
            unsafe {
                CloseHandle(HANDLE(shutdown_event)).ok();
                CloseHandle(HANDLE(lvar_wake_event)).ok();
            }
            return;
        }
    }

    let shutdown_event_int = shutdown_event as isize;
    let state_for_thread = state.clone();
    let session_for_thread = session.clone();
    let app_for_thread = app.clone();
    let retry_sender_for_thread = retry_sender.clone();
    let lvar_handle_for_thread = lvar_handle.clone();

    let handle = std::thread::Builder::new()
        .name("simconnect".to_string())
        // Generous next to what the dispatch loop and SimConnect FFI need,
        // while still well under the default reserve.
        .stack_size(512 * 1024)
        .spawn(move || {
            tracing::info!("[SimConnect] Connection manager started");

            // Run the SimConnect loop (single attempt)
            connection::run_simconnect_loop(
                app_for_thread,
                shutdown_event_int,
                state_for_thread.clone(),
                version,
                lvar_rx,
                lvar_wake_event_int,
            );

            // Detach the LVar command channel if it is still ours, so Tauri
            // commands fail fast with "not connected" rather than queueing
            // into a dead receiver.
            //
            // The handle is closed *inside* the lock: `send_lvar_command` holds
            // this same mutex across its `SetEvent`, so closing here can never
            // race with an in-flight signal on the handle we are about to
            // invalidate.
            match lvar_handle_for_thread.lock() {
                Ok(mut lvar_lock) => {
                    if lvar_lock.as_ref().map(|c| c.wake_event) == Some(lvar_wake_event_int) {
                        *lvar_lock = None;
                    }
                    unsafe {
                        CloseHandle(HANDLE(lvar_wake_event_int as *mut std::ffi::c_void)).ok();
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "[SimConnect] LVar command handle poisoned during cleanup: {}",
                        e
                    );
                    unsafe {
                        CloseHandle(HANDLE(lvar_wake_event_int as *mut std::ffi::c_void)).ok();
                    }
                }
            }

            // ── Post-exit: decide whether to schedule a retry ──────────────
            //
            // A retry is needed when:
            //   1. The shutdown event was not signalled: neither the app nor the
            //      detection module asked this thread to stop.
            //   2. MSFS is still in the process list (confirms the sim didn't exit
            //      between the connection attempt and this check, which would cause
            //      an infinite retry loop).
            //
            // Deliberately not conditioned on the attempt having errored. A clean
            // exit can leave the sim running too: a spurious process scan, or an
            // unexpected wait result: and treating that as final left the app with
            // no SimConnect thread and no way back.

            let shutdown_signalled = unsafe {
                WaitForSingleObject(
                    HANDLE(shutdown_event_int as *mut std::ffi::c_void),
                    0,
                ) == WAIT_OBJECT_0
            };

            // Clear our own session entry so the lifecycle controller can spawn a
            // fresh one when it receives the retry event.
            {
                let mut lock = match session_for_thread.lock() {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(
                            "[SimConnect] Session mutex poisoned during post-exit cleanup: {}",
                            e
                        );
                        // Still proceed to close the event handle below.
                        return;
                    }
                };
                if lock.as_ref().map(|s| s.shutdown_event) == Some(shutdown_event_int) {
                    *lock = None;
                }
            }

            if !shutdown_signalled {
                // Verify MSFS is still running before scheduling the retry.
                // This prevents a chain of spurious retries if MSFS exited in the
                // narrow window between the connection ending and this check.
                let sim_still_running = crate::sim_detection::scan_for_running_sim().is_some();

                if sim_still_running {
                    let delay_ms = next_retry_delay_ms();
                    tracing::info!(
                        "[SimConnect] Connection ended but MSFS still running: \
                         retrying in {}s via detection channel",
                        delay_ms / 1000
                    );
                    // Interruptible sleep: wakes immediately if shutdown is signalled.
                    // We still own the shutdown event here, so WaitForSingleObject works.
                    let wait_result = unsafe {
                        WaitForSingleObject(
                            HANDLE(shutdown_event_int as *mut std::ffi::c_void),
                            delay_ms,
                        )
                    };
                    if wait_result != WAIT_OBJECT_0 {
                        let _ = retry_sender_for_thread.send(SimDetectionEvent::Started(version));
                        tracing::info!("[SimConnect] Retry event sent to lifecycle controller");
                    } else {
                        tracing::info!("[SimConnect] Shutdown signalled during retry delay: aborting retry");
                    }
                } else {
                    tracing::info!(
                        "[SimConnect] Connection ended and MSFS no longer running: \
                         not scheduling retry"
                    );
                }
            }

            // Close the shutdown event handle now that we are done with it.
            unsafe {
                CloseHandle(HANDLE(shutdown_event_int as *mut std::ffi::c_void)).ok();
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
    lvar_handle: &Arc<LvarCommandHandle>,
) {
    // Detach the LVar command channel immediately so frontend commands fail
    // fast while the connection thread winds down. The thread closes the
    // underlying wake event handle itself after its loop exits.
    if let Ok(mut lvar_lock) = lvar_handle.lock() {
        *lvar_lock = None;
    }

    let sess = {
        let mut lock = match session.lock() {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(
                    "[SimConnect] Session mutex poisoned during stop: {}",
                    e
                );
                // Poisoned mutex: we cannot safely access the data, so
                // simply drop the guard and proceed without a session.
                // The event handle may leak, but crashing is worse.
                return;
            }
        };
        lock.take()
    };

    if let Some(sess) = sess {
        // Signal shutdown: this also wakes any WaitForSingleObject inside the
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
/// The lock is released before emitting to avoid re-entrancy deadlocks.
pub fn update_state_and_emit<F>(app: &tauri::AppHandle, state: &Arc<SimStateHandle>, f: F)
where
    F: FnOnce(&mut SimState),
{
    let response = match state.lock() {
        Ok(mut guard) => {
            f(&mut *guard);

            SimStatusResponse {
                connected: matches!(guard.connection, ConnectionState::Connected),
                wasm_present: matches!(guard.wasm, WasmState::Present),
                aircraft_title: guard.aircraft_title.clone(),
                sim_version: match guard.sim_version {
                    SimVersion::Msfs2020 => "2020".to_string(),
                    SimVersion::Msfs2024 => "2024".to_string(),
                    SimVersion::Unknown => "unknown".to_string(),
                },
            }
        }
        Err(e) => {
            tracing::error!("[SimConnect] SimState mutex poisoned: {}", e);
            return;
        }
    };

    if let Err(e) = app.emit("sim-status-changed", response) {
        tracing::warn!("[SimConnect] Failed to emit sim-status-changed event: {}", e);
    }
}
