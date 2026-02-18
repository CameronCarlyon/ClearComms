/**
 * Audio Utilities
 * Constants and helper functions for audio session display and identification
 */
import type { AudioSession } from '$lib/types';

// ─────────────────────────────────────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────────────────────────────────────

/** Special identifier for system volume control */
export const SYSTEM_VOLUME_ID = '__SYSTEM__';
export const SYSTEM_VOLUME_PROCESS_NAME = '__SYSTEM__';
export const SYSTEM_VOLUME_DISPLAY_NAME = 'System';

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/** Format process name for display */
export function formatProcessName(processName: string): string {
  // Special case for system volume
  if (processName === SYSTEM_VOLUME_PROCESS_NAME) {
    return SYSTEM_VOLUME_DISPLAY_NAME;
  }

  const customNames: Record<string, string> = {
    'couatl64_msfs2024': 'GSX',
  };

  const lowerProcessName = processName.toLowerCase();
  if (customNames[lowerProcessName]) {
    return customNames[lowerProcessName];
  }

  let name = processName.replace(/\.exe$/i, '');
  name = name.split(/[-_\s]/).map(word =>
    word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()
  ).join(' ');

  return name;
}

/** Apply display name overrides for friendly names returned from the backend */
export function applyDisplayNameOverride(displayName: string, processName: string = ''): string {
  const lowerProcessName = processName.toLowerCase();
  const lowerDisplayName = displayName.toLowerCase();

  // Check process name first (more reliable than dynamic window titles)
  if (lowerProcessName.includes('flightsimulator2024')) {
    return 'MSFS24';
  }
  if (lowerProcessName.includes('flightsimulator')) {
    return 'MSFS20';
  }

  // Check display name for other overrides
  if (lowerDisplayName.includes('couatl')) {
    return 'GSX';
  }

  return displayName;
}

/** Check if session is system volume */
export function isSystemVolume(session: AudioSession | null | undefined): boolean {
  return session?.process_name === SYSTEM_VOLUME_PROCESS_NAME || session?.session_id === SYSTEM_VOLUME_ID;
}
