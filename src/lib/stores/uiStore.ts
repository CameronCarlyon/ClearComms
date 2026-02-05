/**
 * UI State Store
 * Manages application-wide UI state such as edit mode, menu expansion states, and dock visibility
 */
import { writable, derived } from 'svelte/store';

// Edit mode state
export const isEditMode = writable(false);

// Menu expansion states
export const addAppListExpanded = writable(false);
export const gettingStartedExpanded = writable(false);
export const settingsMenuExpanded = writable(false);
export const closeMenuExpanded = writable(false);

// Dock state (single source of truth)
export const dockOpen = writable(false);

// Window state
export const windowPinned = writable(false);

// Boot/initialisation state
export const initStatus = writable('Initialising...');
export const errorMsg = writable('');
export const audioInitialised = writable(false);

// Binding mode states
export const isBindingMode = writable(false);
export const isButtonBindingMode = writable(false);

// Derived state: any menu is open
export const anyMenuOpen = derived(
  [settingsMenuExpanded, closeMenuExpanded],
  ([$settings, $close]) => $settings || $close
);

// Helper functions for UI state management
export function toggleEditMode(): void {
  isEditMode.update(mode => {
    if (mode) {
      // Exiting edit mode - collapse menus
      addAppListExpanded.set(false);
      settingsMenuExpanded.set(false);
    }
    return !mode;
  });
}

export function closeAllMenus(): void {
  addAppListExpanded.set(false);
  settingsMenuExpanded.set(false);
  closeMenuExpanded.set(false);
  dockOpen.set(false);
}

export function exitEditModeAndCloseMenus(): void {
  isEditMode.set(false);
  isBindingMode.set(false);
  isButtonBindingMode.set(false);
  closeAllMenus();
}
