/**
 * Audio State Store
 * Manages audio sessions, mappings, pinned applications, and volume control state
 */
import { writable, derived, get } from 'svelte/store';
import type { AudioSession, AxisMapping, ButtonMapping, PendingBinding, PendingButtonBinding } from '$lib/types';

// Audio sessions from Windows Audio API
export const audioSessions = writable<AudioSession[]>([]);

// Axis and button mappings
export const axisMappings = writable<AxisMapping[]>([]);
export const buttonMappings = writable<ButtonMapping[]>([]);

// Pinned applications (persist even without bindings)
export const pinnedApps = writable<Set<string>>(new Set());

// Pending bindings (when user is binding a control)
export const pendingBinding = writable<PendingBinding | null>(null);
export const pendingButtonBinding = writable<PendingButtonBinding | null>(null);

// Volume animation state
export const animatingSliders = writable<Set<string>>(new Set());
export const manuallyControlledSessions = writable<Set<string>>(new Set());
export const preMuteVolumes = writable<Map<string, number>>(new Map());

// Derived: Get all bound process names
export const boundProcessNames = derived(
  [axisMappings, buttonMappings, pinnedApps],
  ([$axisMappings, $buttonMappings, $pinnedApps]) => {
    return new Set([
      ...$axisMappings.map(m => m.processName),
      ...$buttonMappings.map(m => m.processName),
      ...$pinnedApps
    ]);
  }
);

// Derived: Get sessions that should be displayed (bound or pinned)
export const boundSessions = derived(
  [audioSessions, axisMappings, buttonMappings, pinnedApps],
  ([$audioSessions, $axisMappings, $buttonMappings, $pinnedApps]) => {
    const boundNames = new Set([
      ...$axisMappings.map(m => m.processName),
      ...$buttonMappings.map(m => m.processName),
      ...$pinnedApps
    ]);
    
    const sessions: AudioSession[] = [];
    const foundProcessNames = new Set<string>();
    
    // Add active sessions
    for (const session of $audioSessions) {
      if (boundNames.has(session.process_name) && !foundProcessNames.has(session.process_name)) {
        sessions.push(session);
        foundProcessNames.add(session.process_name);
      }
    }
    
    // Add inactive session entries for bound/pinned apps that aren't running
    const allMappings = [...$axisMappings, ...$buttonMappings];
    for (const mapping of allMappings) {
      if (!foundProcessNames.has(mapping.processName)) {
        sessions.push({
          session_id: `inactive_${mapping.processName}`,
          display_name: mapping.sessionName,
          process_id: 0,
          process_name: mapping.processName,
          volume: 0,
          is_muted: true
        });
        foundProcessNames.add(mapping.processName);
      }
    }
    
    // Add inactive entries for pinned apps without mappings
    for (const processName of $pinnedApps) {
      if (!foundProcessNames.has(processName)) {
        const activeSession = $audioSessions.find(s => s.process_name === processName);
        if (activeSession) {
          sessions.push(activeSession);
        } else {
          sessions.push({
            session_id: `inactive_${processName}`,
            display_name: processName,
            process_id: 0,
            process_name: processName,
            volume: 0,
            is_muted: true
          });
        }
        foundProcessNames.add(processName);
      }
    }
    
    return sessions;
  }
);

// Derived: Get available sessions (not bound/pinned) for dropdown
export const availableSessions = derived(
  [audioSessions, boundProcessNames],
  ([$audioSessions, $boundNames]) => {
    return $audioSessions.filter(s => !$boundNames.has(s.process_name));
  }
);

// Persistence functions
const AXIS_MAPPINGS_KEY = 'clearcomms_axis_mappings';
const BUTTON_MAPPINGS_KEY = 'clearcomms_button_mappings';
const PINNED_APPS_KEY = 'clearcomms_pinned_apps';

export function saveMappings(): void {
  try {
    const mappings = get(axisMappings);
    localStorage.setItem(AXIS_MAPPINGS_KEY, JSON.stringify(mappings));
  } catch (error) {
    console.error('Error saving axis mappings:', error);
  }
}

export function loadMappings(): void {
  try {
    const saved = localStorage.getItem(AXIS_MAPPINGS_KEY);
    if (saved) {
      axisMappings.set(JSON.parse(saved));
    }
  } catch (error) {
    console.error('Error loading axis mappings:', error);
  }
}

export function saveButtonMappings(): void {
  try {
    const mappings = get(buttonMappings);
    localStorage.setItem(BUTTON_MAPPINGS_KEY, JSON.stringify(mappings));
  } catch (error) {
    console.error('Error saving button mappings:', error);
  }
}

export function loadButtonMappings(): void {
  try {
    const saved = localStorage.getItem(BUTTON_MAPPINGS_KEY);
    if (saved) {
      buttonMappings.set(JSON.parse(saved));
    }
  } catch (error) {
    console.error('Error loading button mappings:', error);
  }
}

export function savePinnedApps(): void {
  try {
    const apps = get(pinnedApps);
    localStorage.setItem(PINNED_APPS_KEY, JSON.stringify([...apps]));
  } catch (error) {
    console.error('Error saving pinned apps:', error);
  }
}

export function loadPinnedApps(): void {
  try {
    const saved = localStorage.getItem(PINNED_APPS_KEY);
    if (saved) {
      pinnedApps.set(new Set(JSON.parse(saved)));
    }
  } catch (error) {
    console.error('Error loading pinned apps:', error);
  }
}

// Helper: Format process name for display
export function formatProcessName(processName: string): string {
  const customNames: Record<string, string> = {
    'vpilot.exe': 'vPilot',
    'couatl.exe': 'GSX'
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
