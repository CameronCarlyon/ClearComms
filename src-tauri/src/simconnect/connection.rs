//! SimConnect Connection Thread
//!
//! Runs the SimConnect connection lifecycle on a dedicated thread:
//! 1. Single connection attempt — SimConnect_Open is called once. If it fails,
//!    the thread exits and the lifecycle controller schedules a retry via the
//!    detection event channel.
//! 2. Dispatch loop — processes SimConnect messages (OPEN, SIMOBJECT_DATA,
//!    CLIENT_DATA, EXCEPTION, QUIT) until the simulator exits or shutdown is
//!    signalled.
//! 3. TITLE SimVar polling — requests the aircraft title with
//!    SIMCONNECT_DATA_REQUEST_FLAG_CHANGED so updates only arrive on change.
//! 4. MobiFlight ping/pong — health-checks the WASM module via ClientDataArea.
//!
//! All SimConnect API calls happen exclusively on this thread.
//!
//! ## Threading Model
//! The dispatch loop uses `WaitForMultipleObjects` with two event handles:
//! - SimConnect event handle — signalled when messages arrive
//! - Shutdown event handle — signalled when the app is shutting down
//!
//! This ensures the thread is truly dormant between messages and shuts down
//! instantly when requested, with zero CPU usage while idle.

use std::ffi::CString;
use std::sync::Arc;

use simconnect_sys::*;

#[cfg(windows)]
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
#[cfg(windows)]
use windows::Win32::System::Threading::{WaitForMultipleObjects, WaitForSingleObject, INFINITE};

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

use crate::simconnect::state::{ConnectionState, SimStateHandle, SimVersion, WasmState};
use crate::simconnect::update_state;

// ═════════════════════════════════════════════════════════════════════════════
// MobiFlight WASM Module Ping/Pong
// ═════════════════════════════════════════════════════════════════════════════

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

const MOBIFLIGHT_COMMAND_NAME: &str = "MobiFlight.Command";
const MOBIFLIGHT_RESPONSE_NAME: &str = "MobiFlight.Response";

const CLIENT_DATA_ID_COMMAND: u32 = 1;
const CLIENT_DATA_ID_RESPONSE: u32 = 2;

const DEFINE_ID_PING: u32 = 10;
const DEFINE_ID_PONG: u32 = 11;

const REQUEST_ID_PONG: u32 = 20;
/// Separate request ID for the one-shot ONCE read. Must differ from
/// REQUEST_ID_PONG to avoid corrupting SimConnect's internal request
/// tracking when an ON_SET subscription is already active.
const REQUEST_ID_PONG_ONCE: u32 = 21;

// ─────────────────────────────────────────────────────────────────────────────
// Ping State
// ─────────────────────────────────────────────────────────────────────────────

/// Tracks whether we are awaiting a pong response.
#[derive(Debug)]
pub struct PingState {
    pub awaiting_pong: bool,
}

impl Default for PingState {
    fn default() -> Self {
        Self {
            awaiting_pong: false,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Setup
// ─────────────────────────────────────────────────────────────────────────────

/// Map the MobiFlight ClientDataArea names to numeric IDs and define the data
/// structures for ping and pong messages.
///
/// # Safety
/// Must be called after `SimConnect_Open` and only on the thread that owns
/// the SimConnect handle.
unsafe fn setup_mobiflight_client_data(handle: *mut std::ffi::c_void) -> Result<(), String> {
    // Map command channel name to ID
    let command_name = CString::new(MOBIFLIGHT_COMMAND_NAME)
        .map_err(|e| format!("Failed to create command CString: {}", e))?;
    let map_result = SimConnect_MapClientDataNameToID(
        handle,
        command_name.as_ptr(),
        CLIENT_DATA_ID_COMMAND,
    );
    if map_result != 0 {
        return Err(format!(
            "SimConnect_MapClientDataNameToID (command) failed: {:#010x}",
            map_result
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_MapClientDataNameToID (command) succeeded");

    // Map response channel name to ID
    let response_name = CString::new(MOBIFLIGHT_RESPONSE_NAME)
        .map_err(|e| format!("Failed to create response CString: {}", e))?;
    let map_result2 = SimConnect_MapClientDataNameToID(
        handle,
        response_name.as_ptr(),
        CLIENT_DATA_ID_RESPONSE,
    );
    if map_result2 != 0 {
        return Err(format!(
            "SimConnect_MapClientDataNameToID (response) failed: {:#010x}",
            map_result2
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_MapClientDataNameToID (response) succeeded");

    // Define the ping data structure: a raw 256-byte buffer for the MobiFlight command channel.
    // dwSizeOrType = 256 (raw byte size), dwDatumID = 0 (unused).
    // Do NOT use SIMCONNECT_DATATYPE_STRING256 here — MobiFlight expects a raw byte buffer.
    let add_result1 = SimConnect_AddToClientDataDefinition(
        handle,
        DEFINE_ID_PING,
        0,   // dwOffset
        256, // dwSizeOrType — raw byte size
        0.0, // fEpsilon (unused)
        0,   // dwDatumID (unused)
    );
    if add_result1 != 0 {
        return Err(format!(
            "SimConnect_AddToClientDataDefinition (ping) failed: {:#010x}",
            add_result1
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_AddToClientDataDefinition (ping) succeeded");

    // Define the pong data structure: a raw 256-byte buffer for the MobiFlight response channel.
    let add_result2 = SimConnect_AddToClientDataDefinition(
        handle,
        DEFINE_ID_PONG,
        0,   // dwOffset
        256, // dwSizeOrType — raw byte size
        0.0, // fEpsilon (unused)
        0,   // dwDatumID (unused)
    );
    if add_result2 != 0 {
        return Err(format!(
            "SimConnect_AddToClientDataDefinition (pong) failed: {:#010x}",
            add_result2
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_AddToClientDataDefinition (pong) succeeded");

    // Subscribe to the response channel with ON_SET so the WASM module pushes
    // data only when it writes — no timer-based polling.
    let req_result = SimConnect_RequestClientData(
        handle,
        CLIENT_DATA_ID_RESPONSE,
        REQUEST_ID_PONG,
        DEFINE_ID_PONG,
        SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET as i32,
        SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_DEFAULT as u32,
        0, // origin
        0, // interval
        0, // limit
    );
    if req_result != 0 {
        return Err(format!(
            "SimConnect_RequestClientData (pong ON_SET) failed: {:#010x}",
            req_result
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_RequestClientData (pong ON_SET) succeeded");

    tracing::debug!("[SimConnect] MobiFlight ClientDataAreas mapped and defined");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Send Ping
// ─────────────────────────────────────────────────────────────────────────────

/// Send an `MF.Ping` message to the MobiFlight WASM module.
///
/// # Safety
/// Must be called on the thread that owns the SimConnect handle.
unsafe fn send_ping(
    handle: *mut std::ffi::c_void,
    ping_state: &mut PingState,
) -> Result<(), String> {
    let ping_data = b"MF.Ping\0";
    let mut buffer = [0u8; 256];
    buffer[..ping_data.len()].copy_from_slice(ping_data);

    tracing::debug!("[SimConnect] Sending MobiFlight ping via SimConnect_SetClientData...");
    let result = SimConnect_SetClientData(
        handle,
        CLIENT_DATA_ID_COMMAND,
        DEFINE_ID_PING,
        SIMCONNECT_CLIENT_DATA_SET_FLAG_DEFAULT as u32,
        0,   // reserved
        256, // size
        buffer.as_mut_ptr() as *mut std::ffi::c_void,
    );

    if result != 0 {
        return Err(format!(
            "SimConnect_SetClientData failed with HRESULT: {:#010x}",
            result
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_SetClientData succeeded");

    ping_state.awaiting_pong = true;

    tracing::debug!("[SimConnect] MobiFlight ping sent");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Active Poll (ONCE Read)
// ─────────────────────────────────────────────────────────────────────────────

/// Actively request the current value of the MobiFlight response area as a
/// one-shot read (`SIMCONNECT_CLIENT_DATA_PERIOD_ONCE`).
///
/// This is required when MSFS is already running at ClearComms startup. In
/// that case, the `MobiFlight.Response` area already contains `MF.Pong` from
/// a prior session. When we send a new `MF.Ping`, the WASM module writes the
/// same `MF.Pong` value back — but SimConnect suppresses the `ON_SET`
/// notification because the data did not change. The ONCE read bypasses
/// value-change deduplication and delivers the current buffer contents
/// unconditionally, allowing us to confirm WASM presence.
///
/// The response arrives as a `SIMCONNECT_RECV_ID_CLIENT_DATA` message with
/// `dwRequestID == REQUEST_ID_PONG`, so the existing `handle_pong_response`
/// function handles it without modification.
///
/// # Safety
/// Must be called on the thread that owns the SimConnect handle.
unsafe fn request_response_once(handle: *mut std::ffi::c_void) -> Result<(), String> {
    let result = SimConnect_RequestClientData(
        handle,
        CLIENT_DATA_ID_RESPONSE,
        REQUEST_ID_PONG_ONCE,
        DEFINE_ID_PONG,
        SIMCONNECT_CLIENT_DATA_PERIOD_ONCE as i32,
        SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_DEFAULT as u32,
        0, // origin
        0, // interval
        0, // limit
    );
    if result != 0 {
        return Err(format!(
            "SimConnect_RequestClientData (pong ONCE) failed: {:#010x}",
            result
        ));
    }
    tracing::debug!("[SimConnect] MobiFlight response ONCE read requested");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Handle Pong Response
// ─────────────────────────────────────────────────────────────────────────────

/// Process a `SIMCONNECT_RECV_CLIENT_DATA` event for the pong request.
///
/// # Safety
/// `data_ptr` must point to the start of the variable-length payload within
/// a valid `SIMCONNECT_RECV_CLIENT_DATA` structure.
/// `data_bytes` must be the actual number of payload bytes available (derived
/// from the `cb` field returned by `SimConnect_GetNextDispatch`, minus the
/// fixed header size). This prevents reading beyond the buffer allocated by
/// SimConnect, which would cause heap corruption.
unsafe fn handle_pong_response(
    app: &tauri::AppHandle,
    data_ptr: *const std::ffi::c_void,
    data_bytes: usize,
    ping_state: &mut PingState,
    state: &std::sync::Arc<SimStateHandle>,
) {
    // Guard against empty payloads — SimConnect should never deliver zero bytes
    // for a valid CLIENT_DATA message, but defend against it anyway.
    if data_bytes == 0 {
        tracing::warn!("[SimConnect] MobiFlight response delivered zero bytes");
        return;
    }

    // Clamp to 256 bytes maximum (our definition size) to avoid reading
    // beyond the expected payload even if SimConnect reports more.
    let len = data_bytes.min(256);
    let slice = std::slice::from_raw_parts(data_ptr as *const u8, len);

    // Find the null terminator to get the actual string length
    let str_len = slice.iter().position(|&b| b == 0).unwrap_or(len);
    let response = String::from_utf8_lossy(&slice[..str_len]);

    tracing::debug!("[SimConnect] MobiFlight response received: {}", response);

    if response.starts_with("MF.Pong") {
        tracing::info!("[SimConnect] MobiFlight WASM module responded to ping");
        ping_state.awaiting_pong = false;
        crate::simconnect::update_state_and_emit(app, state, |s| s.wasm = WasmState::Present);
    } else {
        tracing::warn!(
            "[SimConnect] Unexpected MobiFlight response: {}",
            response
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// SimVar Registration & Thread Entry Point
// ═════════════════════════════════════════════════════════════════════════════

// ─────────────────────────────────────────────────────────────────────────────
// Registration Helper
// ─────────────────────────────────────────────────────────────────────────────

/// Register all SimVars and ClientDataAreas after SIMCONNECT_RECV_ID_OPEN is received.
///
/// This must be called only after the OPEN message has been processed, as the
/// sim's IPC layer may not be fully initialised before that point.
unsafe fn register_simvars_and_client_data(
    handle: *mut std::ffi::c_void,
) -> Result<(), String> {
    // Setup MobiFlight ClientDataAreas
    setup_mobiflight_client_data(handle)?;

    // Register TITLE SimVar as a STRING256 data definition.
    // String SimVars have no units — pass null for UnitsName.
    let title_name = CString::new("TITLE").map_err(|e| {
        format!("Failed to create TITLE CString: {}", e)
    })?;
    let add_result = SimConnect_AddToDataDefinition(
        handle,
        DEFINE_ID_TITLE,
        title_name.as_ptr(),
        std::ptr::null(), // UnitsName — null for string SimVars (no units)
        SIMCONNECT_DATATYPE_STRING256 as i32,
        0.0, // epsilon (unused for strings)
        0,   // datum_id
    );
    if add_result != 0 {
        return Err(format!(
            "SimConnect_AddToDataDefinition failed: {:#010x}",
            add_result
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_AddToDataDefinition succeeded");

    // Request TITLE on the user aircraft (object_id = 0) with event-driven updates.
    // SIMCONNECT_DATA_REQUEST_FLAG_CHANGED means the sim only sends data when
    // the title actually changes, reducing unnecessary traffic and log noise.
    let req_result = SimConnect_RequestDataOnSimObject(
        handle,
        REQUEST_ID_TITLE,
        DEFINE_ID_TITLE,
        0, // object_id = 0 (user aircraft)
        SIMCONNECT_PERIOD_SECOND as i32,
        SIMCONNECT_DATA_REQUEST_FLAG_CHANGED as u32,
        0, // origin
        0, // interval
        0, // limit
    );
    if req_result != 0 {
        return Err(format!(
            "SimConnect_RequestDataOnSimObject failed: {:#010x}",
            req_result
        ));
    }
    tracing::debug!("[SimConnect] SimConnect_RequestDataOnSimObject succeeded");

    tracing::info!("[SimConnect] TITLE SimVar registered and requested");
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/// Data definition ID for the TITLE SimVar.
const DEFINE_ID_TITLE: u32 = 30;
/// Request ID for TITLE SimVar polling.
const REQUEST_ID_TITLE: u32 = 40;

// ─────────────────────────────────────────────────────────────────────────────
// Public Entry Point
// ─────────────────────────────────────────────────────────────────────────────

/// Entry point for the SimConnect background thread.
///
/// Performs a single connection attempt. If `SimConnect_Open` succeeds, enters
/// the dispatch loop until the simulator exits or the shutdown event is
/// signalled. If the connection fails, returns the error immediately — the caller
/// is responsible for retry logic via the detection module.
///
/// # Arguments
/// * `app` — Tauri AppHandle for emitting events to the frontend
/// * `shutdown_event` — a Win32 event handle (stored as `isize`) that, when
///   signalled via `SetEvent`, causes the thread to exit immediately.
/// * `state` — shared `SimState` handle for publishing connection status.
/// * `version` — the simulator version detected by the detection module. This is
///   set in `SimState` immediately so the frontend does not wait for OPEN.
pub fn run_simconnect_loop(
    app: tauri::AppHandle,
    shutdown_event: isize,
    state: Arc<SimStateHandle>,
    version: SimVersion,
) {
    // Initialise COM on this dedicated thread — required for SimConnect
    #[cfg(windows)]
    unsafe {
        if let Err(e) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            tracing::error!("[SimConnect] CoInitializeEx failed: {}", e);
            return;
        }
    }

    let shutdown_handle = HANDLE(shutdown_event as *mut std::ffi::c_void);

    // Set the version immediately — the detection module already knows it
    update_state(&state, |s| {
        s.sim_version = version;
    });

    // Check shutdown before attempting connection
    match unsafe { WaitForSingleObject(shutdown_handle, 0) } {
        WAIT_OBJECT_0 => {
            tracing::info!("[SimConnect] Shutdown signal received before connection attempt");
            #[cfg(windows)]
            unsafe { CoUninitialize(); }
            return;
        }
        _ => {}
    }

    update_state(&state, |s| {
        s.connection = ConnectionState::Connecting;
        s.last_error = None;
    });
    tracing::info!("[SimConnect] Attempting connection to simulator...");

    match try_connect_and_run(&app, &shutdown_handle, &state) {
        Ok(()) => {
            tracing::info!("[SimConnect] Connection closed cleanly");
        }
        Err(e) => {
            tracing::warn!("[SimConnect] Connection error: {}", e);
            update_state(&state, |s| {
                s.connection = ConnectionState::Disconnected;
                s.wasm = WasmState::Absent;
                s.aircraft_title = None;
                s.last_error = Some(e);
            });
        }
    }

    #[cfg(windows)]
    unsafe {
        CoUninitialize();
    }

    tracing::info!("[SimConnect] Background thread exited");
}

// ─────────────────────────────────────────────────────────────────────────────
// Connection & Dispatch Loop
// ─────────────────────────────────────────────────────────────────────────────

/// Attempt a single SimConnect connection and run the dispatch loop.
///
/// Returns `Ok(())` when the connection closes cleanly (simulator quit or
/// shutdown signal). Returns `Err` on connection failure or dispatch error.
fn try_connect_and_run(
    app: &tauri::AppHandle,
    shutdown_event: &HANDLE,
    state: &Arc<SimStateHandle>,
) -> Result<(), String> {
    let app_name = CString::new("ClearComms")
        .map_err(|e| format!("Failed to create app name CString: {}", e))?;

    // Create a Win32 event handle for SimConnect to signal when messages arrive.
    // This is required for pull-mode dispatch to work on a background thread
    // that does not have a Win32 message pump.
    let simconnect_event: HANDLE = unsafe {
        HANDLE(CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()))
    };
    if simconnect_event.0.is_null() {
        return Err("Failed to create Win32 event handle".to_string());
    }

    let mut handle: *mut std::ffi::c_void = std::ptr::null_mut();

    // Open SimConnect connection, passing the event handle so SimConnect
    // can signal us when messages are available.
    let result = unsafe {
        SimConnect_Open(
            &mut handle,
            app_name.as_ptr(),
            std::ptr::null_mut(), // hWnd (optional)
            0,                    // user event Win32 ID
            simconnect_event.0 as *mut std::ffi::c_void, // config event handle
            0,                    // config index
        )
    };

    if result != 0 {
        unsafe { CloseHandle(simconnect_event).ok(); }
        return Err(format!(
            "SimConnect_Open failed with HRESULT: {:#010x}",
            result
        ));
    }

    if handle.is_null() {
        unsafe { CloseHandle(simconnect_event).ok(); }
        return Err("SimConnect_Open returned null handle".to_string());
    }

    tracing::info!("[SimConnect] SimConnect_Open succeeded");

    // ─────────────────────────────────────────────────────────────────────────
    // Dispatch Loop
    // ─────────────────────────────────────────────────────────────────────────

    let mut ping_state = PingState::default();
    let mut setup_complete = false;

    // Use WaitForMultipleObjects with INFINITE timeout so the thread never
    // wakes spuriously. It sleeps at the OS level until either SimConnect
    // signals a message or the shutdown event is signalled.
    loop {
        let handles = [simconnect_event, *shutdown_event];

        let wait_result = unsafe {
            WaitForMultipleObjects(&handles, false, INFINITE)
        };

        // WAIT_OBJECT_0 is the base value (0). The windows crate wraps the
        // return value in a WAIT_EVENT newtype. We compare the inner u32.
        let result_code = wait_result.0;

        if result_code == WAIT_OBJECT_0.0 {
            // SimConnect has messages — drain the queue
            loop {
                let mut msg: *mut SIMCONNECT_RECV = std::ptr::null_mut();
                let mut cb: u32 = 0;

                let result = unsafe { SimConnect_GetNextDispatch(handle, &mut msg, &mut cb) };

                if result == 0 && !msg.is_null() {
                    // handle_message returns false when the dispatch loop should
                    // exit (e.g. SIMCONNECT_RECV_ID_QUIT received).
                    let should_continue = unsafe {
                        handle_message(app, msg, cb, state, &mut setup_complete, &mut ping_state, handle)
                    };
                    if !should_continue {
                        break;
                    }
                } else if result == 0 {
                    // No more messages available
                    break;
                } else {
                    // GetNextDispatch returning E_FAIL when the queue is empty is
                    // expected and normal. Treat it as "no message available" and
                    // continue rather than triggering a reconnect.
                    tracing::trace!("[SimConnect] GetNextDispatch returned {:#010x} — no message available", result);
                    break;
                }
            }
        } else if result_code == WAIT_OBJECT_0.0 + 1 {
            // Shutdown signalled — break cleanly
            tracing::info!("[SimConnect] Shutdown signal received in dispatch loop");
            break;
        } else {
            // Unexpected result — log and break
            tracing::warn!("[SimConnect] WaitForMultipleObjects returned unexpected result");
            break;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cleanup
    // ─────────────────────────────────────────────────────────────────────────

    unsafe {
        SimConnect_Close(handle);
        CloseHandle(simconnect_event).ok();
    }
    tracing::info!("[SimConnect] Connection closed");

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Message Handler
// ─────────────────────────────────────────────────────────────────────────────

/// Dispatch a single SimConnect message to the appropriate handler.
///
/// # Safety
/// `msg` must be a valid pointer to a `SIMCONNECT_RECV` structure allocated
/// by the SimConnect library.
/// Returns `true` to continue the dispatch loop, or `false` to exit it
/// (e.g. when a `SIMCONNECT_RECV_ID_QUIT` message is received).
unsafe fn handle_message(
    app: &tauri::AppHandle,
    msg: *mut SIMCONNECT_RECV,
    cb: u32,
    state: &Arc<SimStateHandle>,
    setup_complete: &mut bool,
    ping_state: &mut PingState,
    handle: *mut std::ffi::c_void,
) -> bool {
    let msg_id = (*msg).dwID as i32;

    match msg_id {
        SIMCONNECT_RECV_ID_OPEN => {
            // Only perform setup once per connection
            if !*setup_complete {
                let open_msg = msg as *mut SIMCONNECT_RECV_OPEN;
                let major = (*open_msg).dwApplicationVersionMajor;
                // Sanity check: the version from the OPEN message should match
                // what the detection module told us. Log a warning if not.
                let detected_version = state
                    .lock()
                    .map(|g| g.sim_version)
                    .unwrap_or(SimVersion::Unknown);
                let open_version = match major {
                    11 => SimVersion::Msfs2020,
                    12 => SimVersion::Msfs2024,
                    _ => {
                        tracing::warn!(
                            "[SimConnect] Unknown simulator version major: {}",
                            major
                        );
                        SimVersion::Unknown
                    }
                };
                if detected_version != open_version && open_version != SimVersion::Unknown {
                    tracing::warn!(
                        "[SimConnect] Version mismatch: detection said {:?}, OPEN says {:?}",
                        detected_version,
                        open_version
                    );
                    update_state(state, |s| {
                        s.sim_version = open_version;
                    });
                }
                tracing::info!(
                    "[SimConnect] Connected to simulator (version: {:?})",
                    open_version
                );
                crate::simconnect::update_state_and_emit(app, state, |s| {
                    s.connection = ConnectionState::Connected;
                });

                // Now that OPEN has been received, perform all registrations
                if let Err(e) = register_simvars_and_client_data(handle) {
                    tracing::error!("[SimConnect] Failed to register SimVars: {}", e);
                    return true; // Continue loop — will exit when sim disconnects
                }

                // Give the sim time to process the registration commands before
                // the next GetNextDispatch call. Without this, the IPC layer
                // may still be processing the registrations when GetNextDispatch
                // is called again, causing E_FAIL.
                std::thread::sleep(std::time::Duration::from_millis(100));

                *setup_complete = true;

                // Send a single ping to confirm the WASM module is present.
                // The ON_SET subscription will deliver the pong when the module
                // responds — no repeating timer is needed.
                unsafe {
                    if let Err(e) = send_ping(handle, ping_state) {
                        tracing::warn!("[SimConnect] Failed to send initial MobiFlight ping: {}", e);
                    }
                    // Also request a one-shot read of the current response area.
                    // This handles the case where MSFS was already running at
                    // startup and the response area already contains MF.Pong from
                    // a prior session — the ON_SET subscription won't fire because
                    // the value hasn't changed, so the ONCE read bypasses the
                    // value-change deduplication.
                    if let Err(e) = request_response_once(handle) {
                        tracing::warn!("[SimConnect] Failed to request MobiFlight response ONCE read: {}", e);
                    }
                }
            }
        }

        SIMCONNECT_RECV_ID_SIMOBJECT_DATA => {
            let data_msg = msg as *mut SIMCONNECT_RECV_SIMOBJECT_DATA;
            if (*data_msg).dwRequestID == REQUEST_ID_TITLE {
                // The data payload starts at the dwData field of the struct.
                // dwData is a DWORD placeholder that marks the start of the
                // variable-length payload appended after the fixed struct fields.
                let recv_data = &*data_msg;
                let payload_ptr = std::ptr::addr_of!(recv_data.dwData) as *const u8;

                // Calculate the actual payload size: total message bytes minus
                // the offset from the start of the message to the dwData field.
                // This prevents reading beyond the buffer allocated by SimConnect.
                let header_end = payload_ptr as usize;
                let msg_start = msg as usize;
                let payload_size = (cb as usize).saturating_sub(header_end - msg_start);
                let payload_size = payload_size.min(256); // clamp to definition size

                let slice = std::slice::from_raw_parts(payload_ptr, payload_size);
                let len = slice.iter().position(|&b| b == 0).unwrap_or(payload_size);
                let title = String::from_utf8_lossy(&slice[..len]).to_string();

                // Only log when the title actually changes to avoid log spam.
                // With SIMCONNECT_DATA_REQUEST_FLAG_CHANGED, the sim should only
                // send updates when the title changes, but we check anyway.
                let needs_update = state
                    .lock()
                    .map(|guard| guard.aircraft_title != Some(title.clone()))
                    .unwrap_or(true);

                if needs_update {
                    tracing::info!("[SimConnect] Aircraft title: {}", title);
                    update_state(state, |s| {
                        s.aircraft_title = Some(title);
                    });
                }
            }
        }

        SIMCONNECT_RECV_ID_CLIENT_DATA => {
            let client_data = msg as *mut SIMCONNECT_RECV_CLIENT_DATA;
            let request_id = (*client_data)._base.dwRequestID;
            // Accept responses from both the ON_SET subscription and the
            // one-shot ONCE read. Using separate request IDs prevents
            // SimConnect internal state corruption.
            if request_id == REQUEST_ID_PONG
                || request_id == REQUEST_ID_PONG_ONCE
            {
                // Data starts at the dwData field of the struct.
                let recv_data = &*client_data;
                let payload_ptr = std::ptr::addr_of!(recv_data._base.dwData) as *const u8;

                // Calculate actual payload size from the cb (total bytes) returned
                // by GetNextDispatch. This prevents reading beyond the SimConnect
                // buffer, which was causing STATUS_HEAP_CORRUPTION.
                let header_end = payload_ptr as usize;
                let msg_start = msg as usize;
                let payload_size = (cb as usize).saturating_sub(header_end - msg_start);

                handle_pong_response(
                    app,
                    payload_ptr as *const std::ffi::c_void,
                    payload_size,
                    ping_state,
                    state,
                );
            }
        }

        SIMCONNECT_RECV_ID_EXCEPTION => {
            let exc = msg as *mut SIMCONNECT_RECV_EXCEPTION;
            let dw_exception = std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwException));
            let dw_send_id = std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwSendID));
            let dw_index = std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwIndex));
            tracing::warn!(
                "[SimConnect] Exception: dwException={} dwSendID={} dwIndex={}",
                dw_exception,
                dw_send_id,
                dw_index
            );
        }

        SIMCONNECT_RECV_ID_QUIT => {
            tracing::info!("[SimConnect] Simulator quit message received — exiting dispatch loop");
            return false;
        }

        _ => {
            tracing::trace!("[SimConnect] Unhandled message ID: {}", msg_id);
        }
    }

    true // Continue the dispatch loop
}
