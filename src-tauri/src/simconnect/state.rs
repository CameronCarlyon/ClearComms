use serde::{Deserialize, Serialize};

/// Detected simulator version from SIMCONNECT_RECV_OPEN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimVersion {
    /// dwApplicationVersionMajor == 11
    Msfs2020,
    /// dwApplicationVersionMajor == 12
    Msfs2024,
    /// Version could not be determined or is unexpected
    Unknown,
}

/// High-level connection state exposed to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// No active connection to the simulator
    Disconnected,
    /// Attempting to open a SimConnect connection
    Connecting,
    /// SimConnect connection is open and dispatching
    Connected,
}

/// MobiFlight WASM module presence state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WasmState {
    /// WASM module has not responded to ping
    Absent,
    /// WASM module responded to the last ping
    Present,
    /// A ping has been sent but no response yet
    Checking,
}

/// The complete state snapshot held in Tauri managed state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimState {
    pub connection: ConnectionState,
    pub wasm: WasmState,
    pub aircraft_title: Option<String>,
    pub sim_version: SimVersion,
    pub last_error: Option<String>,
}

impl Default for SimState {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Disconnected,
            wasm: WasmState::Absent,
            aircraft_title: None,
            sim_version: SimVersion::Unknown,
            last_error: None,
        }
    }
}

/// Thread-safe wrapper for Tauri managed state.
pub type SimStateHandle = std::sync::Mutex<SimState>;

/// Response type for the `get_sim_status` Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimStatusResponse {
    pub connected: bool,
    #[serde(rename = "wasmPresent")]
    pub wasm_present: bool,
    #[serde(rename = "aircraftTitle")]
    pub aircraft_title: Option<String>,
    #[serde(rename = "simVersion")]
    pub sim_version: String,
}
