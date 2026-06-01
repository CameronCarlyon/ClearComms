/**
 * Theme store
 * Manages the application theme, synchronised with the backend.
 */
import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export type ThemeMode = 'automatic' | 'light' | 'dark' | 'seasonal';
export type ResolvedTheme = 'light' | 'dark';

// The user-selected theme mode
const themeMode = writable<ThemeMode>('automatic');

// The resolved theme (what's actually being used)
const resolvedTheme = writable<ResolvedTheme>('dark');

// Derived store that combines both
export const theme = derived(
  [themeMode, resolvedTheme],
  ([$themeMode, $resolvedTheme]) => ({
    mode: $themeMode,
    resolved: $resolvedTheme,
    isLight: $resolvedTheme === 'light',
    isDark: $resolvedTheme === 'dark',
    cssClass: $resolvedTheme === 'light' ? 'light' : 'dark',
  })
);

/**
 * Initialise the theme by querying the backend.
 * Call this during app startup.
 */
export async function initTheme() {
  try {
    const modeStr = await invoke<string>('get_theme_mode');
    const mode: ThemeMode = JSON.parse(modeStr);
    themeMode.set(mode);

    const resolved = await invoke<string>('get_resolved_theme_name');
    resolvedTheme.set(resolved as ResolvedTheme);
  } catch (error) {
    console.error('Failed to initialise theme:', error);
  }
}

/**
 * Set the theme mode. This updates both the local state and the backend.
 */
export async function setThemeMode(mode: ThemeMode) {
  try {
    await invoke('set_theme_mode', { mode });
    themeMode.set(mode);

    // Update resolved theme from backend
    const resolved = await invoke<string>('get_resolved_theme_name');
    resolvedTheme.set(resolved as ResolvedTheme);
  } catch (error) {
    console.error('Failed to set theme mode:', error);
  }
}

/**
 * Apply the current theme to the document.
 * Call this when the resolved theme changes.
 */
export function applyTheme(resolved: ResolvedTheme) {
  const root = document.documentElement;
  root.classList.remove('light', 'dark');
  root.classList.add(resolved);
}
