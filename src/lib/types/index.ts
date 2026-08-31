/**
 * TypeScript interfaces for ClearComms
 * Centralises all type definitions for audio sessions, mappings, and hardware input
 */

/** Represents an audio session from the Windows Audio API */
export interface AudioSession {
  session_id: string;
  display_name: string;
  process_id: number;
  process_name: string;
  volume: number;
  is_muted: boolean;
  /** Temporary override for the displayed volume during mute/unmute animations */
  displayVolumeOverride?: number;
}

/** Axis-to-audio mapping configuration */
export interface AxisMapping {
  deviceHandle: string;
  deviceName: string;
  axisName: string;
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
  inverted: boolean;
}

/** Button-to-mute mapping configuration */
export interface ButtonMapping {
  deviceHandle: string;
  deviceName: string;
  buttonName: string;
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Hardware input device data */
export interface AxisData {
  device_handle: string;
  device_name: string;
  manufacturer: string;
  product_id: number;
  vendor_id: number;
  axes: Record<string, number>;
  buttons: Record<string, boolean>;
}

/** Pending axis binding state */
export interface PendingBinding {
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Pending button binding state */
export interface PendingButtonBinding {
  sessionId: string;
  sessionName: string;
  processId: number;
  processName: string;
}

/** Live volume update state for throttling backend calls */
export interface LiveVolumeState {
  inFlight: boolean;
  lastSent: number;
  queuedVolume?: number;
  timerId?: number;
  /** Last mute state written by this path, so unchanged values are not resent */
  lastSentMute?: boolean;
}

/** Animation signal for volume animations */
export interface AnimationSignal {
  cancelled: boolean;
  resolve?: (completed: boolean) => void;
  frameId?: number;
}

/** Display and work area information returned from the Rust backend.
 *  All spatial values are in physical pixels unless noted otherwise. */
export interface DisplayInfo {
  /** Full monitor width in physical pixels */
  monitorWidth: number;
  /** Full monitor height in physical pixels */
  monitorHeight: number;
  /** Work area left edge (non-zero if taskbar is on the left) */
  workAreaLeft: number;
  /** Work area top edge (non-zero if taskbar is on the top) */
  workAreaTop: number;
  /** Work area right edge */
  workAreaRight: number;
  /** Work area bottom edge */
  workAreaBottom: number;
  /** Usable work area width */
  workAreaWidth: number;
  /** Usable work area height */
  workAreaHeight: number;
  /** DPI scale factor (e.g. 1.0, 1.25, 1.5, 2.0) */
  scaleFactor: number;
  /** Edge padding in physical pixels */
  edgePadding: number;
  /** Maximum permissible window width in physical pixels */
  maxWindowWidth: number;
  /** Maximum permissible window height in physical pixels */
  maxWindowHeight: number;
}

/** Simulator connection status returned from the Rust backend */
export interface SimStatus {
  /** True when SimConnect is open and dispatching */
  connected: boolean;
  /** True when the MobiFlight WASM module responded to the last ping */
  wasmPresent: boolean;
  /** Currently loaded aircraft title, if known */
  aircraftTitle: string | null;
  /** Detected simulator version: "2020", "2024", or "unknown" */
  simVersion: string;
}

/** Generic simulator function categories an application channel can be assigned to */
export type SimFunctionCategory = 'COM1' | 'COM2' | 'COM3' | 'HF1' | 'HF2' | 'CAB' | 'PA' | 'INT';

/** Which cockpit seat's audio panel a sim function assignment follows */
export type SimSeat = 'captain' | 'firstOfficer';

/** Assignment of an application (keyed by process name) to a simulator function category */
export interface SimFunctionAssignment {
  processName: string;
  category: SimFunctionCategory;
}

/** Payload of the `lvar-value-changed` event emitted by the Rust backend */
export interface LvarValueEvent {
  /** LVar name (without the "L:" prefix) */
  name: string;
  /** Raw LVar value in the aircraft profile's native range */
  value: number;
}