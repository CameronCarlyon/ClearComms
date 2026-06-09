/**
 * SimConnect status store
 *
 * Provides reactive state for the simulator connection status.
 * Listens for `sim-status-changed` events from the Tauri backend.
 * Uses Svelte 5 runes for reactive state management.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SimStatus } from "$lib/types";

/** Reactive SimConnect status state */
export const simStatus = $state<SimStatus>({
  connected: false,
  wasmPresent: false,
  aircraftTitle: null,
  simVersion: "unknown",
});

let unlistenFn: UnlistenFn | null = null;

/**
 * Start listening for simulator status change events from the backend.
 * Registers the event listener BEFORE fetching initial state so that no
 * `sim-status-changed` event can fire in the gap between the fetch and the
 * listen call. The subsequent `get_sim_status` fetch catches any state that
 * was set before we started listening.
 *
 * @returns A cleanup function that stops listening
 */
export async function startSimStatusListener(): Promise<() => void> {
  if (unlistenFn) {
    return () => {};
  }

  // Register the listener first — any event fired between now and the
  // initial-state fetch below will be received rather than silently dropped.
  unlistenFn = await listen<SimStatus>("sim-status-changed", (event) => {
    simStatus.connected = event.payload.connected;
    simStatus.wasmPresent = event.payload.wasmPresent;
    simStatus.aircraftTitle = event.payload.aircraftTitle;
    simStatus.simVersion = event.payload.simVersion;
  });

  // Fetch current state to catch anything that happened before we registered.
  // This is the primary mechanism for showing connection state when the
  // sim was already running before ClearComms launched.
  try {
    const status = await invoke<SimStatus>("get_sim_status");
    simStatus.connected = status.connected;
    simStatus.wasmPresent = status.wasmPresent;
    simStatus.aircraftTitle = status.aircraftTitle;
    simStatus.simVersion = status.simVersion;
  } catch (error) {
    console.error("[SimStore] Failed to get initial sim status:", error);
  }

  return () => {
    if (unlistenFn) {
      unlistenFn();
      unlistenFn = null;
    }
  };
}

/**
 * Stop listening for simulator status change events.
 */
export function stopSimStatusListener(): void {
  if (unlistenFn) {
    unlistenFn();
    unlistenFn = null;
  }
}
