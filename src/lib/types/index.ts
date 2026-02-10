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
}

/** Animation signal for volume animations */
export interface AnimationSignal {
  cancelled: boolean;
  resolve?: (completed: boolean) => void;
  frameId?: number;
}