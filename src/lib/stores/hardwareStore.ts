/**
 * Hardware State Store
 * Manages hardware input device data and axis/button detection
 */
import { writable, get } from 'svelte/store';
import type { AxisData } from '$lib/types';

// Hardware input device data
export const axisData = writable<AxisData[]>([]);

// Previous state caches for detecting changes
export const previousAxisValues = writable<Map<string, Record<string, number>>>(new Map());
export const previousButtonStates = writable<Map<string, Record<string, boolean>>>(new Map());
export const lastHardwareAxisValues = writable<Map<string, number>>(new Map());

// Hardware volume interpolation state
export const hardwareVolumeTargets = writable<Map<string, number>>(new Map());
export const hardwareVolumeAnimations = writable<Map<string, number>>(new Map());

// Drag animation state
export const dragTargets = writable<Map<string, number>>(new Map());
export const dragAnimationFrames = writable<Map<string, number>>(new Map());

/**
 * Detect significant axis movement (>5% change)
 * Used during axis binding to detect which axis the user moved
 */
export function detectAxisMovement(): { deviceHandle: string; deviceName: string; axisName: string } | null {
  const devices = get(axisData);
  const prevValues = get(previousAxisValues);
  
  for (const device of devices) {
    const previousValues = prevValues.get(device.device_handle);
    if (!previousValues) continue;

    for (const [axisName, currentValue] of Object.entries(device.axes)) {
      const previousValue = previousValues[axisName];
      if (previousValue === undefined) continue;

      const change = Math.abs(currentValue - previousValue);
      if (change > 0.05) {
        return { deviceHandle: device.device_handle, deviceName: device.device_name, axisName };
      }
    }
  }
  return null;
}

/**
 * Detect button press (false → true transition)
 * Used during button binding to detect which button the user pressed
 */
export function detectButtonPress(): { deviceHandle: string; deviceName: string; buttonName: string } | null {
  const devices = get(axisData);
  const prevStates = get(previousButtonStates);
  
  for (const device of devices) {
    const previousStates = prevStates.get(device.device_handle);
    if (!previousStates) continue;

    for (const [buttonName, currentState] of Object.entries(device.buttons)) {
      const previousState = previousStates[buttonName];
      if (previousState === undefined) continue;

      if (!previousState && currentState) {
        return { deviceHandle: device.device_handle, deviceName: device.device_name, buttonName };
      }
    }
  }
  return null;
}

/**
 * Capture current axis values for change detection
 */
export function captureAxisValues(): void {
  const devices = get(axisData);
  const newValues = new Map<string, Record<string, number>>();
  
  for (const device of devices) {
    newValues.set(device.device_handle, { ...device.axes });
  }
  
  previousAxisValues.set(newValues);
}

/**
 * Capture current button states for change detection
 */
export function captureButtonStates(): void {
  const devices = get(axisData);
  const newStates = new Map<string, Record<string, boolean>>();
  
  for (const device of devices) {
    newStates.set(device.device_handle, { ...device.buttons });
  }
  
  previousButtonStates.set(newStates);
}

/**
 * Clear all hardware caches
 */
export function clearHardwareCaches(): void {
  previousAxisValues.set(new Map());
  previousButtonStates.set(new Map());
  lastHardwareAxisValues.set(new Map());
  
  // Cancel any running hardware animations
  const animations = get(hardwareVolumeAnimations);
  for (const [, frameId] of animations) {
    cancelAnimationFrame(frameId);
  }
  hardwareVolumeAnimations.set(new Map());
  hardwareVolumeTargets.set(new Map());
  
  // Cancel any running drag animations
  const dragFrames = get(dragAnimationFrames);
  for (const [, frameId] of dragFrames) {
    cancelAnimationFrame(frameId);
  }
  dragAnimationFrames.set(new Map());
  dragTargets.set(new Map());
}
