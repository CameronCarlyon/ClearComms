//! SimConnect Connection Thread
//!
//! Runs the SimConnect connection lifecycle on a dedicated thread:
//! 1. Single connection attempt: SimConnect_Open is called once. If it fails,
//!    the thread exits and the lifecycle controller schedules a retry via the
//!    detection event channel.
//! 2. Dispatch loop: processes SimConnect messages (OPEN, SIMOBJECT_DATA,
//!    CLIENT_DATA, EXCEPTION, QUIT) until the simulator exits or shutdown is
//!    signalled.
//! 3. TITLE SimVar polling: requests the aircraft title with
//!    SIMCONNECT_DATA_REQUEST_FLAG_CHANGED so updates only arrive on change.
//! 4. MobiFlight ping/pong: health-checks the WASM module via ClientDataArea.
//!
//! All SimConnect API calls happen exclusively on this thread.
//!
//! ## Threading Model
//! The dispatch loop blocks in `WaitForMultipleObjects` on:
//! - SimConnect event handle: signalled when messages arrive
//! - Shutdown event handle: signalled when the app is shutting down
//! - LVar wake event: signalled when the frontend queues a command
//! - Simulator process handle: signalled when the simulator exits
//!
//! Every reason to wake is therefore an event. Nothing is polled, and LVars are
//! subscribed with CHANGED so the simulator itself withholds values that have
//! not moved: while connected and idle this thread does not run at all.

use std::ffi::CString;
use std::io::Write as _;
use std::sync::Arc;

use simconnect_sys::*;
use tauri::Emitter;

#[cfg(windows)]
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, WaitForMultipleObjects, WaitForSingleObject, PROCESS_SYNCHRONIZE,
};

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
use crate::simconnect::{update_state, LvarCommand, MAX_LVAR_SUBSCRIPTIONS};

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
// ClearComms MobiFlight Client (LVar read/write)
// ─────────────────────────────────────────────────────────────────────────────
//
// The MobiFlight WASM module supports multiple named clients, each with its own
// Command/Response/LVars client data areas. Registering our own client
// ("ClearComms") isolates our LVar subscriptions from the MobiFlight Connector
// application (or any other client) sharing the default "MobiFlight" client:
// in particular, `MF.SimVars.Clear` only clears the issuing client's vars.

/// Command sent on the default channel to register our dedicated client.
const MF_CLIENT_REGISTER_CMD: &str = "MF.Clients.Add.ClearComms";
/// Confirmation the module sends back on the default Response channel.
const MF_CLIENT_REGISTER_DONE: &str = "MF.Clients.Add.ClearComms.Finished";

const CC_COMMAND_NAME: &str = "ClearComms.Command";
const CC_LVARS_NAME: &str = "ClearComms.LVars";
const CC_RESPONSE_NAME: &str = "ClearComms.Response";

const CLIENT_DATA_ID_CC_COMMAND: u32 = 3;
const CLIENT_DATA_ID_CC_LVARS: u32 = 4;
const CLIENT_DATA_ID_CC_RESPONSE: u32 = 5;

const DEFINE_ID_CC_COMMAND: u32 = 12;
const DEFINE_ID_CC_RESPONSE: u32 = 13;

/// Request ID for the ClearComms response channel subscription.
const REQUEST_ID_CC_RESPONSE: u32 = 22;

/// Per-LVar definition/request IDs are derived from these bases by index.
/// Ranges are bounded by MAX_LVAR_SUBSCRIPTIONS.
const DEFINE_ID_LVAR_BASE: u32 = 100;
const REQUEST_ID_LVAR_BASE: u32 = 200;
/// One-shot reads used to prime each slot's current value at subscribe time.
/// Kept clear of REQUEST_ID_LVAR_BASE so the standing subscription and the
/// prime never share a request id.
const REQUEST_ID_LVAR_ONCE_BASE: u32 = 400;

/// Raw buffer size for MobiFlight command writes (matches the existing ping
/// buffer convention; commands are NUL-padded C strings).
const MF_COMMAND_BUFFER_SIZE: usize = 256;

/// Connection-thread state for the ClearComms MobiFlight client.
#[derive(Default)]
struct LvarClientState {
    /// `MF.Clients.Add.ClearComms` has been sent, awaiting confirmation.
    register_sent: bool,
    /// ClearComms client data areas are mapped and ready for use.
    client_ready: bool,
    /// Currently subscribed LVar names; the index is the module's registration
    /// order and therefore the float offset (index × 4) in the LVars area.
    subscribed: Vec<String>,
    /// Last value emitted for each subscribed slot, parallel to `subscribed`.
    /// Seeded with NaN so the first delivery for a slot always compares unequal
    /// and is therefore always forwarded to the frontend.
    last_values: Vec<f32>,
    /// Subscription requested before the client became ready.
    pending: Option<Vec<String>>,
}

/// Mutable state threaded through the dispatch loop's message handler, bundled
/// to keep the handler's arity reasonable.
#[derive(Default)]
struct DispatchState {
    setup_complete: bool,
    ping: PingState,
    lvar: LvarClientState,
}

/// Payload emitted to the frontend when a subscribed LVar value changes.
///
/// Borrows the name from the subscription list rather than owning it: this is
/// constructed per delivered value, and the clone it replaces was an allocation
/// on the hot path for a string that already exists and outlives the emit.
#[derive(serde::Serialize, Clone)]
struct LvarValueEvent<'a> {
    name: &'a str,
    value: f32,
}

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
// Defensive message validation
// ─────────────────────────────────────────────────────────────────────────────

/// Validates that `msg` is non-null and that the supplied buffer size `cb` is
/// large enough to contain the target SimConnect message type.  This defends
/// against struct-layout mismatches in the FFI bindings and corrupted messages.
///
/// # Safety
/// `msg` must have been returned by a successful `SimConnect_GetNextDispatch`
/// call (i.e. the pointer is non-null and backed by SimConnect memory).
#[inline]
unsafe fn validate_message<T>(msg: *mut SIMCONNECT_RECV, cb: u32) -> Option<*mut T> {
    if msg.is_null() {
        return None;
    }
    let base_size = std::mem::size_of::<SIMCONNECT_RECV>();
    if (cb as usize) < base_size {
        tracing::warn!(
            "[SimConnect] Message buffer too small for base header: {} bytes",
            cb
        );
        return None;
    }
    // SimConnect sets dwSize to the total bytes of this individual message.
    // It should always match cb.  If it does not, the stream is corrupted.
    let dw_size = (*msg).dwSize;
    if dw_size != cb {
        tracing::warn!(
            "[SimConnect] Message size mismatch: dwSize={} but cb={}",
            dw_size,
            cb
        );
        // dwSize must never be larger than the buffer we were given.
        if (cb as usize) < (dw_size as usize) {
            return None;
        }
    }
    let needed = std::mem::size_of::<T>();
    if (cb as usize) < needed {
        tracing::warn!(
            "[SimConnect] Message buffer too small for expected type (need {} got {}): skipping",
            needed,
            cb
        );
        return None;
    }
    Some(msg as *mut T)
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
    // Do NOT use SIMCONNECT_DATATYPE_STRING256 here: MobiFlight expects a raw byte buffer.
    let add_result1 = SimConnect_AddToClientDataDefinition(
        handle,
        DEFINE_ID_PING,
        0,   // dwOffset
        256, // dwSizeOrType: raw byte size
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
        256, // dwSizeOrType: raw byte size
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
    // data only when it writes: no timer-based polling.
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
// MobiFlight Command Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Write a NUL-padded command string to a MobiFlight client data area.
///
/// # Safety
/// Must be called on the thread that owns the SimConnect handle, with data
/// areas already mapped and defined.
unsafe fn write_mf_command(
    handle: *mut std::ffi::c_void,
    area_id: u32,
    define_id: u32,
    command: &str,
) -> Result<(), String> {
    let bytes = command.as_bytes();
    if bytes.len() >= MF_COMMAND_BUFFER_SIZE {
        return Err(format!("MobiFlight command too long: {} bytes", bytes.len()));
    }

    let mut buffer = [0u8; MF_COMMAND_BUFFER_SIZE];
    buffer[..bytes.len()].copy_from_slice(bytes);

    write_mf_buffer(handle, area_id, define_id, &mut buffer)
}

/// Send an already-formatted, NUL-padded command buffer.
///
/// Split out so a caller that builds its command in place can avoid the
/// intermediate `String`: volume writes go out on every throttle tick of a
/// gesture, and formatting one onto the heap each time is avoidable.
///
/// # Safety
/// Same contract as `write_mf_command`.
unsafe fn write_mf_buffer(
    handle: *mut std::ffi::c_void,
    area_id: u32,
    define_id: u32,
    buffer: &mut [u8; MF_COMMAND_BUFFER_SIZE],
) -> Result<(), String> {
    let result = SimConnect_SetClientData(
        handle,
        area_id,
        define_id,
        SIMCONNECT_CLIENT_DATA_SET_FLAG_DEFAULT as u32,
        0,
        MF_COMMAND_BUFFER_SIZE as u32,
        buffer.as_mut_ptr() as *mut std::ffi::c_void,
    );

    if result != 0 {
        return Err(format!(
            "SimConnect_SetClientData failed with HRESULT: {:#010x}",
            result
        ));
    }
    Ok(())
}

/// Send a command on the default "MobiFlight" client channel.
unsafe fn send_mf_command(handle: *mut std::ffi::c_void, command: &str) -> Result<(), String> {
    write_mf_command(handle, CLIENT_DATA_ID_COMMAND, DEFINE_ID_PING, command)
}

/// Send a command on our dedicated "ClearComms" client channel.
unsafe fn send_cc_command(handle: *mut std::ffi::c_void, command: &str) -> Result<(), String> {
    write_mf_command(handle, CLIENT_DATA_ID_CC_COMMAND, DEFINE_ID_CC_COMMAND, command)
}

// ─────────────────────────────────────────────────────────────────────────────
// ClearComms Client Setup & LVar Subscriptions
// ─────────────────────────────────────────────────────────────────────────────

/// Map the ClearComms client data areas and define the command write buffer.
/// Called once the WASM module confirms our client registration.
///
/// # Safety
/// Must be called on the thread that owns the SimConnect handle.
unsafe fn setup_clearcomms_client_data(handle: *mut std::ffi::c_void) -> Result<(), String> {
    let command_name = CString::new(CC_COMMAND_NAME)
        .map_err(|e| format!("Failed to create ClearComms command CString: {}", e))?;
    let result = SimConnect_MapClientDataNameToID(
        handle,
        command_name.as_ptr(),
        CLIENT_DATA_ID_CC_COMMAND,
    );
    if result != 0 {
        return Err(format!(
            "MapClientDataNameToID (ClearComms.Command) failed: {:#010x}",
            result
        ));
    }

    let lvars_name = CString::new(CC_LVARS_NAME)
        .map_err(|e| format!("Failed to create ClearComms LVars CString: {}", e))?;
    let result = SimConnect_MapClientDataNameToID(
        handle,
        lvars_name.as_ptr(),
        CLIENT_DATA_ID_CC_LVARS,
    );
    if result != 0 {
        return Err(format!(
            "MapClientDataNameToID (ClearComms.LVars) failed: {:#010x}",
            result
        ));
    }

    let result = SimConnect_AddToClientDataDefinition(
        handle,
        DEFINE_ID_CC_COMMAND,
        0,
        MF_COMMAND_BUFFER_SIZE as u32,
        0.0,
        0,
    );
    if result != 0 {
        return Err(format!(
            "AddToClientDataDefinition (ClearComms command) failed: {:#010x}",
            result
        ));
    }

    // Subscribe to our own response channel. The module answers every command
    // sent on ClearComms.Command here, so without this subscription a rejected
    // LVar name or an unprocessed command is silently discarded and the only
    // evidence of a broken subscription is that values never arrive.
    let response_name = CString::new(CC_RESPONSE_NAME)
        .map_err(|e| format!("Failed to create ClearComms response CString: {}", e))?;
    let result = SimConnect_MapClientDataNameToID(
        handle,
        response_name.as_ptr(),
        CLIENT_DATA_ID_CC_RESPONSE,
    );
    if result != 0 {
        return Err(format!(
            "MapClientDataNameToID (ClearComms.Response) failed: {:#010x}",
            result
        ));
    }

    let result = SimConnect_AddToClientDataDefinition(
        handle,
        DEFINE_ID_CC_RESPONSE,
        0,
        MF_COMMAND_BUFFER_SIZE as u32,
        0.0,
        0,
    );
    if result != 0 {
        return Err(format!(
            "AddToClientDataDefinition (ClearComms response) failed: {:#010x}",
            result
        ));
    }

    let result = SimConnect_RequestClientData(
        handle,
        CLIENT_DATA_ID_CC_RESPONSE,
        REQUEST_ID_CC_RESPONSE,
        DEFINE_ID_CC_RESPONSE,
        SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET as i32,
        SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_DEFAULT as u32,
        0, // origin
        0, // interval
        0, // limit
    );
    if result != 0 {
        return Err(format!(
            "RequestClientData (ClearComms.Response ON_SET) failed: {:#010x}",
            result
        ));
    }

    tracing::info!("[SimConnect] ClearComms MobiFlight client ready");
    Ok(())
}

/// Map a client-data request id back to its LVar slot. Both the standing
/// CHANGED subscription and the one-shot prime target the same slot, so either
/// range resolves to the same index.
fn lvar_slot_for_request(request_id: u32) -> Option<usize> {
    let max = MAX_LVAR_SUBSCRIPTIONS as u32;

    if request_id >= REQUEST_ID_LVAR_BASE && request_id < REQUEST_ID_LVAR_BASE + max {
        Some((request_id - REQUEST_ID_LVAR_BASE) as usize)
    } else if request_id >= REQUEST_ID_LVAR_ONCE_BASE
        && request_id < REQUEST_ID_LVAR_ONCE_BASE + max
    {
        Some((request_id - REQUEST_ID_LVAR_ONCE_BASE) as usize)
    } else {
        None
    }
}

/// Log a reply from the WASM module on our dedicated ClearComms response
/// channel. Purely diagnostic: nothing in the state machine depends on it.
///
/// # Safety
/// `data_ptr` must point to the variable-length payload of a valid
/// `SIMCONNECT_RECV_CLIENT_DATA`, and `data_bytes` must be the number of
/// payload bytes actually available.
unsafe fn handle_cc_response(data_ptr: *const std::ffi::c_void, data_bytes: usize) {
    if data_bytes == 0 || data_ptr.is_null() {
        return;
    }

    let len = data_bytes.min(MF_COMMAND_BUFFER_SIZE);
    let slice = std::slice::from_raw_parts(data_ptr as *const u8, len);
    let str_len = slice.iter().position(|&b| b == 0).unwrap_or(len);
    if str_len == 0 {
        return;
    }

    tracing::debug!(
        "[SimConnect] ClearComms channel response: {}",
        String::from_utf8_lossy(&slice[..str_len])
    );
}

/// Replace the LVar subscription set: clears the module's registrations for
/// our client, then re-registers each name as `(L:<name>)` and subscribes to
/// its float slot in the ClearComms.LVars area with ON_SET.
///
/// The CHANGED flag is deliberately NOT used: the module only writes when a
/// value changes anyway, but the initial registration write must always be
/// delivered so the frontend learns the current state (a cleared-then-rewritten
/// 0 would otherwise be suppressed).
///
/// # Safety
/// Must be called on the thread that owns the SimConnect handle.
unsafe fn apply_lvar_subscriptions(
    handle: *mut std::ffi::c_void,
    lvar_state: &mut LvarClientState,
    names: Vec<String>,
) {
    if !lvar_state.client_ready {
        // The client registers right after the WASM pong; a subscription that
        // arrives earlier is applied once registration confirms.
        lvar_state.pending = Some(names);
        return;
    }

    if let Err(e) = send_cc_command(handle, "MF.SimVars.Clear") {
        tracing::warn!("[SimConnect] Failed to clear LVar subscriptions: {}", e);
    }

    // Tear down every slot the previous subscription used before redefining it.
    // Both the request and the definition persist for the life of the
    // connection, and re-adding the same datum to an existing definition raises
    // SIMCONNECT_EXCEPTION_DUPLICATE_ID: which would leave that slot unusable
    // for the rest of the session. Requests are cancelled before their
    // definition is cleared so no live request ever references a freed define.
    for i in 0..lvar_state.subscribed.len() {
        SimConnect_RequestClientData(
            handle,
            CLIENT_DATA_ID_CC_LVARS,
            REQUEST_ID_LVAR_BASE + i as u32,
            DEFINE_ID_LVAR_BASE + i as u32,
            SIMCONNECT_CLIENT_DATA_PERIOD_NEVER as i32,
            SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_DEFAULT as u32,
            0,
            0,
            0,
        );
        SimConnect_ClearClientDataDefinition(handle, DEFINE_ID_LVAR_BASE + i as u32);
    }

    for (i, name) in names.iter().enumerate() {
        // Each LVar is a 4-byte float at offset index × 4 in the LVars area.
        let result = SimConnect_AddToClientDataDefinition(
            handle,
            DEFINE_ID_LVAR_BASE + i as u32,
            (i * std::mem::size_of::<f32>()) as u32,
            std::mem::size_of::<f32>() as u32,
            0.0,
            0,
        );
        if result != 0 {
            tracing::warn!(
                "[SimConnect] AddToClientDataDefinition failed for LVar {}: {:#010x}",
                name,
                result
            );
            continue;
        }

        // CHANGED lets SimConnect drop unchanged values inside the simulator
        // process. The module rewrites every registered variable on each tick
        // whether or not it moved, so without this the connection thread is
        // woken tens of times a second purely to discard identical data.
        let result = SimConnect_RequestClientData(
            handle,
            CLIENT_DATA_ID_CC_LVARS,
            REQUEST_ID_LVAR_BASE + i as u32,
            DEFINE_ID_LVAR_BASE + i as u32,
            SIMCONNECT_CLIENT_DATA_PERIOD_ON_SET as i32,
            SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_CHANGED as u32,
            0,
            0,
            0,
        );
        if result != 0 {
            tracing::warn!(
                "[SimConnect] RequestClientData failed for LVar {}: {:#010x}",
                name,
                result
            );
            continue;
        }

        if let Err(e) = send_cc_command(handle, &format!("MF.SimVars.Add.(L:{})", name)) {
            tracing::warn!("[SimConnect] Failed to subscribe LVar {}: {}", name, e);
            continue;
        }

        // CHANGED only reports movement, so the current level has to be asked
        // for explicitly: otherwise a channel stays wherever it happened to be
        // until someone touches the knob. Delivered on its own request id and
        // folded into the same slot by lvar_slot_for_request.
        //
        // Skipped when this slot previously held a different variable. The read
        // targets an offset in the shared LVars area, and the module may not
        // have written the newly registered variable there yet: priming then
        // would apply the previous variable's value to this channel. In that
        // case the CHANGED subscription delivers the real value on the module's
        // next write instead, one tick later.
        let slot_held_another_lvar = lvar_state
            .subscribed
            .get(i)
            .is_some_and(|previous| previous != name);

        if slot_held_another_lvar {
            continue;
        }

        let result = SimConnect_RequestClientData(
            handle,
            CLIENT_DATA_ID_CC_LVARS,
            REQUEST_ID_LVAR_ONCE_BASE + i as u32,
            DEFINE_ID_LVAR_BASE + i as u32,
            SIMCONNECT_CLIENT_DATA_PERIOD_ONCE as i32,
            SIMCONNECT_CLIENT_DATA_REQUEST_FLAG_DEFAULT as u32,
            0,
            0,
            0,
        );
        if result != 0 {
            tracing::warn!(
                "[SimConnect] Initial read failed for LVar {}: {:#010x}",
                name,
                result
            );
        }
    }

    tracing::info!("[SimConnect] Subscribed to {} LVar(s)", names.len());
    lvar_state.last_values = vec![f32::NAN; names.len()];
    lvar_state.subscribed = names;
}

// ─────────────────────────────────────────────────────────────────────────────
// Active Poll (ONCE Read)
// ─────────────────────────────────────────────────────────────────────────────

/// Actively request the current value of the MobiFlight response area as a
/// one-shot read (`SIMCONNECT_CLIENT_DATA_PERIOD_ONCE`).
///
/// This is required when MSFS is already running at ClearComms startup. In
/// that case the `MobiFlight.Response` area may already contain `MF.Pong` from
/// a prior session. When we send a new `MF.Ping`, the WASM module writes the
/// same `MF.Pong` value back: but SimConnect suppresses the `ON_SET`
/// notification because the data did not change. The ONCE read bypasses
/// value-change deduplication and delivers the current buffer contents
/// unconditionally, allowing us to confirm WASM presence.
///
/// The response arrives as a `SIMCONNECT_RECV_ID_CLIENT_DATA` message with
/// `dwRequestID == REQUEST_ID_PONG_ONCE`. `handle_pong_response` discards
/// anything other than `MF.Pong` on that path: the retained buffer is just as
/// likely to hold our own `MF.Clients.Add.ClearComms.Finished` from a previous
/// run, and acting on that would register the client twice.
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
/// `once_read` marks a payload delivered by the one-shot `PERIOD_ONCE` read
/// rather than the `ON_SET` subscription. Those payloads are whatever the
/// shared response area happened to be holding, so only `MF.Pong` is trusted
/// from them: see `request_response_once`.
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
    handle: *mut std::ffi::c_void,
    data_ptr: *const std::ffi::c_void,
    data_bytes: usize,
    once_read: bool,
    ping_state: &mut PingState,
    lvar_state: &mut LvarClientState,
    state: &std::sync::Arc<SimStateHandle>,
) {
    // Guard against empty or null payloads: SimConnect should never deliver
    // such data for a valid CLIENT_DATA message, but defend against it anyway.
    if data_bytes == 0 || data_ptr.is_null() {
        tracing::warn!("[SimConnect] MobiFlight response delivered null or zero-byte payload");
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

    // The ONCE read returns whatever the shared response area currently holds,
    // which may be a message left over from an earlier ClearComms session: the
    // WASM module retains the buffer for the whole simulator session, so a
    // restart against a running sim finds our own last reply still sitting
    // there. Its sole purpose is to detect a retained MF.Pong, so anything else
    // is stale and must not drive the client-registration state machine.
    if once_read && !response.starts_with("MF.Pong") {
        tracing::debug!(
            "[SimConnect] Ignoring stale response from ONCE read: {}",
            response
        );
        return;
    }

    if response.starts_with("MF.Pong") {
        tracing::info!("[SimConnect] MobiFlight WASM module responded to ping");
        ping_state.awaiting_pong = false;
        crate::simconnect::update_state_and_emit(app, state, |s| s.wasm = WasmState::Present);

        // Register our dedicated client so LVar subscriptions are isolated
        // from other MobiFlight clients. The module confirms on this channel.
        if !lvar_state.register_sent {
            match send_mf_command(handle, MF_CLIENT_REGISTER_CMD) {
                Ok(()) => lvar_state.register_sent = true,
                Err(e) => tracing::warn!("[SimConnect] Failed to register ClearComms client: {}", e),
            }
        }
    } else if response.starts_with(MF_CLIENT_REGISTER_DONE) {
        // Only act on a confirmation for a registration we actually sent this
        // session, and only once. Running the setup twice re-issues the same
        // MapClientDataNameToID / AddToClientDataDefinition calls, each of which
        // raises SIMCONNECT_EXCEPTION_DUPLICATE_ID: the mappings are per
        // connection, so a repeat can only ever be a duplicate.
        if !lvar_state.register_sent || lvar_state.client_ready {
            tracing::debug!(
                "[SimConnect] Ignoring unsolicited ClearComms registration confirmation"
            );
            return;
        }
        match setup_clearcomms_client_data(handle) {
            Ok(()) => {
                lvar_state.client_ready = true;
                // Flush any subscription that arrived before we were ready.
                if let Some(pending) = lvar_state.pending.take() {
                    apply_lvar_subscriptions(handle, lvar_state, pending);
                }
            }
            Err(e) => tracing::error!("[SimConnect] ClearComms client setup failed: {}", e),
        }
    } else {
        // Other MobiFlight clients (e.g. the Connector app) share the default
        // response channel: their traffic is expected and not an error.
        tracing::debug!("[SimConnect] Ignoring MobiFlight response: {}", response);
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
    // String SimVars have no units: pass null for UnitsName.
    let title_name = CString::new("TITLE").map_err(|e| {
        format!("Failed to create TITLE CString: {}", e)
    })?;
    let add_result = SimConnect_AddToDataDefinition(
        handle,
        DEFINE_ID_TITLE,
        title_name.as_ptr(),
        std::ptr::null(), // UnitsName: null for string SimVars (no units)
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
// COM Apartment RAII Guard
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(windows)]
struct ComApartment;

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public Entry Point
// ─────────────────────────────────────────────────────────────────────────────

/// Entry point for the SimConnect background thread.
///
/// Performs a single connection attempt. If `SimConnect_Open` succeeds, enters
/// the dispatch loop until the simulator exits or the shutdown event is
/// signalled. If the connection fails, returns the error immediately: the caller
/// is responsible for retry logic via the detection module.
///
/// # Arguments
/// * `app`: Tauri AppHandle for emitting events to the frontend
/// * `shutdown_event`: a Win32 event handle (stored as `isize`) that, when
///   signalled via `SetEvent`, causes the thread to exit immediately.
/// * `state`: shared `SimState` handle for publishing connection status.
/// * `version`: the simulator version detected by the detection module. This is
///   set in `SimState` immediately so the frontend does not wait for OPEN.
pub fn run_simconnect_loop(
    app: tauri::AppHandle,
    shutdown_event: isize,
    state: Arc<SimStateHandle>,
    version: SimVersion,
    lvar_rx: std::sync::mpsc::Receiver<LvarCommand>,
    lvar_wake_event: isize,
) {
    // Initialise COM on this dedicated thread: required for SimConnect
    #[cfg(windows)]
    let _com = unsafe {
        match CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            Ok(_) => Some(ComApartment),
            Err(e) => {
                tracing::error!("[SimConnect] CoInitializeEx failed: {}", e);
                return;
            }
        }
    };

    let shutdown_handle = HANDLE(shutdown_event as *mut std::ffi::c_void);

    // Set the version immediately: the detection module already knows it
    update_state(&state, |s| {
        s.sim_version = version;
    });

    // Check shutdown before attempting connection
    match unsafe { WaitForSingleObject(shutdown_handle, 0) } {
        WAIT_OBJECT_0 => {
            tracing::info!("[SimConnect] Shutdown signal received before connection attempt");
            return;
        }
        _ => {}
    }

    update_state(&state, |s| {
        s.connection = ConnectionState::Connecting;
        s.last_error = None;
    });
    tracing::info!("[SimConnect] Attempting connection to simulator...");

    match try_connect_and_run(&app, &shutdown_handle, &state, &lvar_rx, lvar_wake_event) {
        Ok(()) => {
            tracing::info!("[SimConnect] Connection closed cleanly");
        }
        Err(e) => {
            tracing::warn!("[SimConnect] Connection error: {}", e);
            update_state(&state, |s| {
                s.last_error = Some(e);
            });
        }
    }

    // This thread owns the connection, so by the time it leaves nothing is
    // connected: however it got here. Publishing that unconditionally, and
    // emitting it, is what stops SimState advertising a live connection whose
    // command channel the manager is about to drop: the frontend would see
    // "connected", try to subscribe to LVars, and be told the channel is gone,
    // forever, with no event to tell it otherwise.
    crate::simconnect::update_state_and_emit(&app, &state, |s| {
        s.connection = ConnectionState::Disconnected;
        s.wasm = WasmState::Absent;
        s.aircraft_title = None;
    });

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
    lvar_rx: &std::sync::mpsc::Receiver<LvarCommand>,
    lvar_wake_event: isize,
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

    let mut dispatch_state = DispatchState::default();
    let mut quit_received = false;

    // Consecutive health checks that found no simulator process. One negative
    // scan is not trusted: process enumeration can transiently fail, and acting
    // on it tears the connection down, then re-opens it, re-registers the
    // MobiFlight client and re-sends the entire SimVar subscription: churn
    // inside the simulator process that is worth not causing by accident.
    let mut missed_sim_scans: u32 = 0;
    const MISSED_SCANS_BEFORE_EXIT: u32 = 2;
    let lvar_wake_handle = HANDLE(lvar_wake_event as *mut std::ffi::c_void);

    // Waiting on the simulator's own process handle makes its exit an event
    // rather than something to poll for. When the handle is available the loop
    // needs no periodic work at all, so the timeout is only a long backstop
    // against an orphaned SimConnect event handle. Without it (OpenProcess can
    // fail) the loop falls back to the previous scan on a short timeout.
    const WAIT_TIMEOUT_WATCHED_MS: u32 = 60_000;
    const WAIT_TIMEOUT_POLLED_MS: u32 = 5_000;

    // SimConnect_GetNextDispatch signals an empty message queue by returning
    // E_FAIL: it is the drain loop's normal exit, not an error. A genuinely
    // dead connection produces no dispatch wakeups at all, so it surfaces via
    // the timeout branch's health check rather than here.
    const E_FAIL: i32 = 0x80004005_u32 as i32;

    // A SYNCHRONIZE handle is signalled the moment the process exits, so the
    // simulator going away wakes this loop directly instead of being noticed up
    // to five seconds later by a process-table snapshot.
    let sim_process: Option<HANDLE> = crate::sim_detection::scan_for_running_sim_with_pid()
        .and_then(|(_, pid)| unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok());

    if sim_process.is_none() {
        tracing::debug!(
            "[SimConnect] Could not open the simulator process: falling back to periodic scans"
        );
    }

    let mut handles = vec![simconnect_event, *shutdown_event, lvar_wake_handle];
    if let Some(process) = sim_process {
        handles.push(process);
    }
    let wait_timeout_ms = if sim_process.is_some() {
        WAIT_TIMEOUT_WATCHED_MS
    } else {
        WAIT_TIMEOUT_POLLED_MS
    };

    loop {
        let wait_result = unsafe {
            WaitForMultipleObjects(&handles, false, wait_timeout_ms)
        };

        // WAIT_OBJECT_0 is the base value (0). The windows crate wraps the
        // return value in a WAIT_EVENT newtype. We compare the inner u32.
        let result_code = wait_result.0;

        if result_code == WAIT_OBJECT_0.0 {
            if quit_received {
                tracing::info!("[SimConnect] Quit already processed: discarding stale messages");
                break;
            }
            // SimConnect has messages: drain the queue until it reports empty.
            loop {
                let mut msg: *mut SIMCONNECT_RECV = std::ptr::null_mut();
                let mut cb: u32 = 0;

                let result = unsafe { SimConnect_GetNextDispatch(handle, &mut msg, &mut cb) };

                match result {
                    0 if !msg.is_null() => {
                        // handle_message returns false when the dispatch loop should
                        // exit (e.g. SIMCONNECT_RECV_ID_QUIT received).
                        let should_continue = unsafe {
                            handle_message(app, msg, cb, state, &mut dispatch_state, handle)
                        };
                        if !should_continue {
                            quit_received = true;
                            break;
                        }
                    }
                    // Queue drained: the expected end of every drain pass.
                    0 | E_FAIL => break,
                    other => {
                        tracing::debug!(
                            "[SimConnect] GetNextDispatch returned {:#010x}: treating as queue empty",
                            other
                        );
                        break;
                    }
                }
            }
        } else if result_code == WAIT_OBJECT_0.0 + 2 {
            // LVar command(s) from the frontend: drain the command queue.
            while let Ok(command) = lvar_rx.try_recv() {
                match command {
                    LvarCommand::Subscribe(names) => unsafe {
                        apply_lvar_subscriptions(handle, &mut dispatch_state.lvar, names);
                    },
                    LvarCommand::Set { name, value } => unsafe {
                        if dispatch_state.lvar.client_ready {
                            // Formatted straight into the command buffer, which
                            // is zero-filled and therefore already NUL-padded.
                            let mut buffer = [0u8; MF_COMMAND_BUFFER_SIZE];
                            let formatted = {
                                let mut cursor = &mut buffer[..];
                                write!(cursor, "MF.SimVars.Set.{} (>L:{})", value, name)
                            };

                            if formatted.is_err() {
                                tracing::warn!(
                                    "[SimConnect] LVar set command too long: {}",
                                    name
                                );
                            } else if let Err(e) = write_mf_buffer(
                                handle,
                                CLIENT_DATA_ID_CC_COMMAND,
                                DEFINE_ID_CC_COMMAND,
                                &mut buffer,
                            ) {
                                tracing::warn!("[SimConnect] Failed to set LVar {}: {}", name, e);
                            }
                        } else {
                            // Dropped rather than queued: the UI will resend on
                            // the next state change if the write still matters.
                            tracing::debug!(
                                "[SimConnect] LVar set dropped: client not ready: {}",
                                name
                            );
                        }
                    },
                }
            }
        } else if result_code == WAIT_OBJECT_0.0 + 1 {
            // Shutdown signalled: break cleanly
            tracing::info!("[SimConnect] Shutdown signal received in dispatch loop");
            break;
        } else if sim_process.is_some() && result_code == WAIT_OBJECT_0.0 + 3 {
            tracing::info!("[SimConnect] Simulator process exited: closing connection");
            break;
        } else if result_code == WAIT_TIMEOUT.0 {
            if quit_received {
                tracing::info!(
                    "[SimConnect] Quit already received but event still waking: forcing exit"
                );
                break;
            }

            // Only reached when the process handle could not be opened. Two
            // scans must agree before tearing down, since a single failed
            // enumeration would otherwise cost a full reconnect.
            if sim_process.is_none()
                && crate::sim_detection::scan_for_running_sim().is_none()
            {
                missed_sim_scans += 1;
                if missed_sim_scans >= MISSED_SCANS_BEFORE_EXIT {
                    tracing::info!(
                        "[SimConnect] Simulator no longer running: exiting dispatch loop"
                    );
                    break;
                }
                tracing::debug!(
                    "[SimConnect] Simulator scan found nothing ({}/{}): rechecking",
                    missed_sim_scans,
                    MISSED_SCANS_BEFORE_EXIT
                );
            } else {
                missed_sim_scans = 0;
            }
        } else {
            // Unexpected result: log and break
            tracing::warn!(
                "[SimConnect] WaitForMultipleObjects returned unexpected result: {:#010x}",
                result_code
            );
            break;
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Cleanup
    // ─────────────────────────────────────────────────────────────────────────

    unsafe {
        SimConnect_Close(handle);
        CloseHandle(simconnect_event).ok();
        if let Some(process) = sim_process {
            CloseHandle(process).ok();
        }
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
    dispatch_state: &mut DispatchState,
    handle: *mut std::ffi::c_void,
) -> bool {
    let msg_id = (*msg).dwID as i32;

    match msg_id {
        SIMCONNECT_RECV_ID_OPEN => {
            // Only perform setup once per connection
            if !dispatch_state.setup_complete {
                let open_msg = match validate_message::<SIMCONNECT_RECV_OPEN>(msg, cb) {
                    Some(m) => m,
                    None => return true,
                };
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

                // A connection was actually established, so the reconnect
                // backoff starts from scratch next time.
                crate::simconnect::note_connection_established();

                // Now that OPEN has been received, perform all registrations
                if let Err(e) = register_simvars_and_client_data(handle) {
                    tracing::error!("[SimConnect] Failed to register SimVars: {}", e);
                    return true; // Continue loop: will exit when sim disconnects
                }

                // Give the sim time to process the registration commands before
                // the next GetNextDispatch call. Without this, the IPC layer
                // may still be processing the registrations when GetNextDispatch
                // is called again, causing E_FAIL.
                std::thread::sleep(std::time::Duration::from_millis(100));

                dispatch_state.setup_complete = true;

                // Send a single ping to confirm the WASM module is present.
                // The ON_SET subscription will deliver the pong when the module
                // responds: no repeating timer is needed.
                unsafe {
                    if let Err(e) = send_ping(handle, &mut dispatch_state.ping) {
                        tracing::warn!("[SimConnect] Failed to send initial MobiFlight ping: {}", e);
                    }
                    // Also request a one-shot read of the current response area.
                    // This handles the case where MSFS was already running at
                    // startup and the response area already contains MF.Pong from
                    // a prior session: the ON_SET subscription won't fire because
                    // the value hasn't changed, so the ONCE read bypasses the
                    // value-change deduplication.
                    if let Err(e) = request_response_once(handle) {
                        tracing::warn!("[SimConnect] Failed to request MobiFlight response ONCE read: {}", e);
                    }
                }
            }
        }

        SIMCONNECT_RECV_ID_SIMOBJECT_DATA => {
            let data_msg = match validate_message::<SIMCONNECT_RECV_SIMOBJECT_DATA>(msg, cb) {
                Some(m) => m,
                None => return true,
            };
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
                    crate::simconnect::update_state_and_emit(app, state, |s| {
                        s.aircraft_title = Some(title);
                    });
                }
            }
        }

        SIMCONNECT_RECV_ID_CLIENT_DATA => {
            let client_data = match validate_message::<SIMCONNECT_RECV_CLIENT_DATA>(msg, cb) {
                Some(m) => m,
                None => return true,
            };
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
                    handle,
                    payload_ptr as *const std::ffi::c_void,
                    payload_size,
                    request_id == REQUEST_ID_PONG_ONCE,
                    &mut dispatch_state.ping,
                    &mut dispatch_state.lvar,
                    state,
                );
            } else if request_id == REQUEST_ID_CC_RESPONSE {
                let recv_data = &*client_data;
                let payload_ptr = std::ptr::addr_of!(recv_data._base.dwData) as *const u8;

                let header_end = payload_ptr as usize;
                let msg_start = msg as usize;
                let payload_size = (cb as usize).saturating_sub(header_end - msg_start);

                handle_cc_response(payload_ptr as *const std::ffi::c_void, payload_size);
            } else if let Some(index) = lvar_slot_for_request(request_id) {
                // Subscribed LVar value update: a single f32 payload.
                let recv_data = &*client_data;
                let payload_ptr = std::ptr::addr_of!(recv_data._base.dwData) as *const u8;

                let header_end = payload_ptr as usize;
                let msg_start = msg as usize;
                let payload_size = (cb as usize).saturating_sub(header_end - msg_start);
                if payload_size < std::mem::size_of::<f32>() {
                    tracing::warn!("[SimConnect] LVar payload too small: {} bytes", payload_size);
                    return true;
                }

                let value = std::ptr::read_unaligned(payload_ptr as *const f32);

                // The WASM module rewrites every subscribed LVar on each sim
                // tick whether or not it changed: measured at 10 Hz per LVar
                // with the simulator backgrounded and ~25 Hz with it focused:
                // so most deliveries carry a value the frontend already has.
                // Forwarding them all floods the webview with redundant IPC and
                // restarts the frontend's volume animation faster than it can
                // finish. Emit only on genuine change.
                {
                    match dispatch_state.lvar.last_values.get_mut(index) {
                        Some(last) if *last == value => return true,
                        Some(last) => *last = value,
                        // Slot outside the current subscription set: a stale
                        // request that has not been cancelled yet.
                        None => return true,
                    }
                }

                // Map the slot index back to the LVar name for the frontend.
                if let Some(name) = dispatch_state.lvar.subscribed.get(index) {
                    tracing::trace!("[SimConnect] LVar {} = {}", name, value);
                    if let Err(e) = app.emit(
                        "lvar-value-changed",
                        LvarValueEvent { name, value },
                    ) {
                        tracing::warn!("[SimConnect] Failed to emit lvar-value-changed: {}", e);
                    }
                }
            }
        }

        SIMCONNECT_RECV_ID_EXCEPTION => {
            let exc = match validate_message::<SIMCONNECT_RECV_EXCEPTION>(msg, cb) {
                Some(m) => m,
                None => return true,
            };
            // read_unaligned is kept as a defensive measure in case the
            // simconnect_sys crate uses packed structs.
            let dw_exception =
                std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwException));
            let dw_send_id =
                std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwSendID));
            let dw_index =
                std::ptr::read_unaligned(std::ptr::addr_of!((*exc).dwIndex));
            tracing::warn!(
                "[SimConnect] Exception: dwException={} dwSendID={} dwIndex={}",
                dw_exception,
                dw_send_id,
                dw_index
            );
        }

        SIMCONNECT_RECV_ID_QUIT => {
            tracing::info!("[SimConnect] Simulator quit message received: exiting dispatch loop");
            return false;
        }

        _ => {
            tracing::trace!("[SimConnect] Unhandled message ID: {}", msg_id);
        }
    }

    true // Continue the dispatch loop
}
