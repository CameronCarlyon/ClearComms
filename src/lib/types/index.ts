/**
 * TypeScript interfaces for ClearComms application
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

/** Memory info from performance API (Chromium only) */
export interface MemoryInfo {
  jsHeapSizeLimit?: number;
  totalJSHeapSize?: number;
  usedJSHeapSize?: number;
}

/** Debug configuration options */
export interface DebugConfig {
  ENABLED: boolean;
  FORCE_BOOT_SCREEN: boolean;
  FORCE_BOOT_ERROR: boolean;
  FORCE_CLOSE_CONFIRMATION: boolean;
  FORCE_MAIN_APP: boolean;
  BOOT_STATUS_TEXT: string;
  BOOT_ERROR_MESSAGE: string;
  FORCE_EDIT_MODE: boolean;
  FORCE_NO_SESSIONS: boolean;
  FORCE_ERROR_BANNER: boolean;
  ERROR_BANNER_TEXT: string;
  FORCE_MOCK_SESSIONS: boolean;
  MOCK_SESSIONS: AudioSession[];
  FORCE_BINDING_MODE: boolean;
  FORCE_BUTTON_BINDING_MODE: boolean;
  FORCE_AUDIO_NOT_INITIALISED: boolean;
}
