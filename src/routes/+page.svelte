<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import type {
    AudioSession,
    AxisMapping,
    ButtonMapping,
    AxisData,
    PendingBinding,
    PendingButtonBinding,
    LiveVolumeState,
    AnimationSignal,
    SimFunctionAssignment,
    SimFunctionCategory,
    LvarValueEvent
  } from "$lib/types";
  import {
    Mixer,
    Dock,
    BootScreen,
    Footer
  } from "$lib/components";
  import { formatProcessName, applyDisplayNameOverride, SYSTEM_VOLUME_ID, SYSTEM_VOLUME_PROCESS_NAME, SYSTEM_VOLUME_DISPLAY_NAME, isSystemVolume } from "$lib/stores/audioStore";
  import { initTheme, theme, applyTheme } from "$lib/stores/themeStore";
  import { startSimStatusListener, stopSimStatusListener, simStatus } from '$lib/stores/simStore.svelte';
  import {
    matchAircraftProfile,
    getFunctionDef,
    getSupportedCategories,
    normaliseVolume,
    denormaliseVolume,
    type SimFunctionDef
  } from "$lib/data/aircraftProfiles";


  // ─────────────────────────────────────────────────────────────────────────────
  // DEBUG CONFIGURATION
  // ─────────────────────────────────────────────────────────────────────────────
  // In development, the full debug config (including mock data) is loaded
  // dynamically from $lib/debug so it is not shipped in production builds.
  // ─────────────────────────────────────────────────────────────────────────────

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let DEBUG: any = { ENABLED: false };

  // ─────────────────────────────────────────────────────────────────────────────
  // STATE
  // ─────────────────────────────────────────────────────────────────────────────
  
  let axisData = $state<AxisData[]>([]);
  let audioSessions = $state<AudioSession[]>([]);
  let axisMappings = $state<AxisMapping[]>([]);
  let buttonMappings = $state<ButtonMapping[]>([]);
  let pinnedApps = $state<Set<string>>(new Set());
  let appFriendlyNames = $state<Map<string, string>>(new Map()); // processName -> friendlyName
  let windowPinned = $state(false);
  let pollingInterval: number | null = null;
  /** Unlisten handle for the audio-state-updated push event from the Rust backend */
  let unlistenAudioState: UnlistenFn | null = null;
  let isPolling = $state(false);
  let initStatus = $state("Initialising...");
  let audioInitialised = $state(false);
  let isBindingMode = $state(false);
  let isButtonBindingMode = $state(false);
  let pendingBinding = $state<PendingBinding | null>(null);
  let pendingButtonBinding = $state<PendingButtonBinding | null>(null);
  let previousAxisValues: Map<string, Record<string, number>> = new Map();
  let previousButtonStates: Map<string, Record<string, boolean>> = new Map();
  let lastHardwareAxisValues: Map<string, number> = new Map();
  let axisActivated: Map<string, boolean> = new Map(); // Track if axis has had user input
  let errorMsg = $state("");
  let isEditMode = $state(false);
  let previousDisplayCount = $state(-1);
  let animatingSliders = $state<Set<string>>(new Set());
  let animationSignals = $state<Map<string, AnimationSignal>>(new Map());
  let manuallyControlledSessions = $state<Set<string>>(new Set());
  let pinnedAppsLoaded = $state(false);

  // ─── Sim Function (LVar) State ───
  // Applications assigned to generic simulator functions (COM1, COM2, …). Each
  // assignment links the app's volume/mute to the corresponding aircraft LVars
  // via the MobiFlight WASM module, in both directions.
  let simAssignments = $state<SimFunctionAssignment[]>([]);
  /** Unlisten handle for the lvar-value-changed Tauri event */
  let unlistenLvarValue: UnlistenFn | null = null;
  /** Captain-side audio panel only for now: F/O support may follow later */
  const SIM_SEAT = 'captain' as const;

  /** Aircraft profile matched from the TITLE SimVar (null = unsupported aircraft) */
  const activeSimProfile = $derived(matchAircraftProfile(simStatus.aircraftTitle));

  /** All function categories available for assignment: always shown regardless of aircraft support.
   * The backend handles per-aircraft LVar mapping; the frontend picker is agnostic. */
  const supportedSimCategories = $derived(
    ['COM1', 'COM2', 'COM3', 'HF1', 'HF2', 'CAB', 'PA', 'INT'] as SimFunctionCategory[]
  );

  /** processName → function definition, for the active profile and assignments */
  const simFunctionByProcess = $derived.by(() => {
    const map = new Map<string, SimFunctionDef>();
    if (!activeSimProfile) return map;
    for (const assignment of simAssignments) {
      const def = getFunctionDef(activeSimProfile, SIM_SEAT, assignment.category);
      if (def) map.set(assignment.processName, def);
    }
    return map;
  });

/** One application channel driven by an inbound LVar update */
  type LvarRoute = { processName: string; kind: 'volume' | 'mute' };

  /**
   * Sim function definition: the running applications bound to it.
   *
   * Built once per change rather than filtered per call. The consumers below run
   * on every animation frame of a gesture and on every inbound LVar event, and
   * a filter in that position allocates a fresh array each time.
   */
  const sessionsBySimFunction = $derived.by(() => {
    const map = new Map<SimFunctionDef, AudioSession[]>();

    for (const session of audioSessions) {
      if (session.session_id.startsWith('inactive_')) continue;

      const def = simFunctionByProcess.get(session.process_name);
      if (!def) continue;

      const bound = map.get(def);
      if (bound) bound.push(session);
      else map.set(def, [session]);
    }
    return map;
  });

  /**
   * session id: the session. Reads only session_id, so the per-property
   * tracking that keeps sessionsBySimFunction stable applies here too: a volume
   * or mute write does not rebuild it.
   */
  const sessionById = $derived.by(() => {
    const map = new Map<string, AudioSession>();
    for (const session of audioSessions) map.set(session.session_id, session);
    return map;
  });

  /** LVar name → every route bound to it. A sim function can be shared by any
   *  number of applications, so one LVar drives all of them. */
  const lvarRouteByName = $derived.by(() => {
    const map = new Map<string, LvarRoute[]>();
    const add = (lvar: string, route: LvarRoute) => {
      const existing = map.get(lvar);
      if (existing) existing.push(route);
      else map.set(lvar, [route]);
    };

    for (const [processName, def] of simFunctionByProcess) {
      add(def.volume.lvar, { processName, kind: 'volume' });
      if (def.mute) add(def.mute.lvar, { processName, kind: 'mute' });
    }
    return map;
  });

  // ─── LVar Subscription Orchestration ───
  // Keeps the backend's subscription set in sync with the active profile and
  // assignments. The dedupe key is reset whenever the WASM module drops so a
  // reconnect always re-sends the full set.
  let lastLvarSubscriptionKey = '';
  /** Bumped to re-run the effect after a failed attempt. */
  let lvarSubscriptionAttempt = $state(0);
  /** Backoff between failed subscription attempts, so a persistent failure
   *  settles into an occasional poll instead of a once-a-second loop. */
  let lvarSubscriptionBackoffMs = 1000;

  /**
   * LVars that have reported a non-zero value since the module became ready.
   *
   * While the simulator loads, the module publishes every registered LVar as
   * exactly 0 before the aircraft powers up. Applied literally that drags each
   * bound application to silence, repeatedly, with nobody having touched
   * anything. Until an LVar has shown a real value at least once, a reading of
   * exactly 0 is treated as "not initialised yet" rather than as a setting.
   * Afterwards 0 is meaningful: it means the knob was turned down.
   */
  const lvarsSeenNonZero = new Set<string>();

  $effect(() => {
    // Read so a retry scheduled below actually re-runs this effect.
    lvarSubscriptionAttempt;

    const wasmReady = simStatus.wasmPresent;
    if (!wasmReady) {
      lastLvarSubscriptionKey = '';
      lvarsSeenNonZero.clear();
      return;
    }

    const names: string[] = [];
    for (const def of simFunctionByProcess.values()) {
      if (!names.includes(def.volume.lvar)) names.push(def.volume.lvar);
      if (def.mute && !names.includes(def.mute.lvar)) names.push(def.mute.lvar);
    }

    const key = `${activeSimProfile?.id ?? 'none'}|${names.join(',')}`;
    if (key === lastLvarSubscriptionKey) return;

    // A new subscription set means new baselines. The LVar names change with
    // the aircraft, so entries carried over would let a genuinely-zero level
    // through as though that variable had already proven itself.
    lvarsSeenNonZero.clear();

    // The key is recorded only once the backend accepts the set. Caching it up
    // front meant one transient failure (the command channel is published a
    // moment after the connection reports ready) left the backend subscribed to
    // nothing, with no path back until the assignments changed.

    invoke('subscribe_lvars', { names })
      .then(() => {
        lastLvarSubscriptionKey = key;
        lvarSubscriptionBackoffMs = 1000;
      })
      .catch((e) => {
        console.warn('[Sim] Failed to update LVar subscriptions, retrying:', e);
        setTimeout(() => { lvarSubscriptionAttempt++; }, lvarSubscriptionBackoffMs);
        lvarSubscriptionBackoffMs = Math.min(lvarSubscriptionBackoffMs * 2, 30000);
      });
  });

  // Menu expansion states
  let addAppListExpanded = $state(false);
  let settingsMenuExpanded = $state(false);
  let closeMenuExpanded = $state(false);
  let dockOpen = $state(false);
  let addAppComponentKey = $state(0);
  let stopSimStatusListenerFn: (() => void) | null = null;


  // ─────────────────────────────────────────────────────────────────────────────
  // DERIVED STATE
  // ─────────────────────────────────────────────────────────────────────────────

  $effect(() => {
    // Apply theme when resolved
    applyTheme($theme.resolved);
  });

  $effect(() => {
    // Enforce edit mode when no pinned applications (onboarding mode)
    if (!pinnedAppsLoaded || initStatus !== "Ready") {
      return;
    }
    if (pinnedApps.size === 0 && !isEditMode) {
      isEditMode = true;
    }
  });

  $effect(() => {
    // Keep windowPinned state in sync when settings menu is opened
    if (settingsMenuExpanded || dockOpen) {
      fetchWindowPinnedState();
    }
  });

  $effect(() => {
    // Re-measure layout when channels are rendered in case of styling changes
    // Only measure if we have pinned applications to measure against
    if (pinnedApps.size > 0 && initStatus === "Ready" && audioSessions.length > 0) {
      measureLayoutDimensions();
    }
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // CONSTANTS
  // ─────────────────────────────────────────────────────────────────────────────

  const POLL_LOG_INTERVAL = 200;
  const BUTTON_CACHE_LOG_INTERVAL = 200;
  const LIVE_UPDATE_MIN_INTERVAL_MS = 40;
  const HARDWARE_VOLUME_SMOOTHING = 0.3;
  let pollInFlight = false;
  let pollIterations = 0;
  let skippedPolls = 0;
  let buttonCachePruneCounter = 0;

  const hardwareVolumeTargets = new Map<string, number>();
  const hardwareVolumeAnimations = new Map<string, number>();
  const liveVolumeState = new Map<string, LiveVolumeState>();
  
  let memoryMonitorInterval: number | null = null;
  let lastMemoryCleanup = Date.now();
  const MEMORY_CLEANUP_INTERVAL = 300000;
  const MAX_CACHE_SIZE = 1000;

  // ─────────────────────────────────────────────────────────────────────────────
  // MEMORY PROFILING (Dev Mode)
  // ─────────────────────────────────────────────────────────────────────────────
  
  /** Chromium memory info (may not be available in WebView2) */
  interface MemoryInfo {
    usedJSHeapSize?: number;
    totalJSHeapSize?: number;
    jsHeapSizeLimit?: number;
  }
  
  const IS_DEV = typeof (import.meta as any).hot !== 'undefined';
  let memoryProfilerInterval: number | null = null;
  let memorySnapshots: { timestamp: number; heapUsed: number; heapTotal: number }[] = [];
  const MEMORY_PROFILER_INTERVAL = 60000;
  const MAX_MEMORY_SNAPSHOTS = 120;
  
  function getMemoryUsage(): MemoryInfo | null {
    const perf = performance as Performance & { memory?: MemoryInfo };
    return perf.memory || null;
  }
  
  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }
  
  function startMemoryProfiler() {
    if (!IS_DEV || memoryProfilerInterval) return;
    logMemorySnapshot();
    memoryProfilerInterval = setInterval(() => {
      logMemorySnapshot();
      checkForMemoryLeaks();
    }, MEMORY_PROFILER_INTERVAL);
  }
  
  function stopMemoryProfiler() {
    if (memoryProfilerInterval) {
      clearInterval(memoryProfilerInterval);
      memoryProfilerInterval = null;
    }
  }
  
  function logMemorySnapshot() {
    const memory = getMemoryUsage();
    if (!memory || !memory.usedJSHeapSize || !memory.totalJSHeapSize) return;
    
    const snapshot = {
      timestamp: Date.now(),
      heapUsed: memory.usedJSHeapSize,
      heapTotal: memory.totalJSHeapSize
    };
    
    memorySnapshots.push(snapshot);
    if (memorySnapshots.length > MAX_MEMORY_SNAPSHOTS) {
      memorySnapshots = memorySnapshots.slice(-MAX_MEMORY_SNAPSHOTS);
    }
    
  }
  
  function checkForMemoryLeaks() {
    if (memorySnapshots.length < 10) return;
    
    const earlySnapshots = memorySnapshots.slice(0, 5);
    const recentSnapshots = memorySnapshots.slice(-5);
    
    const earlyAvg = earlySnapshots.reduce((sum, s) => sum + s.heapUsed, 0) / earlySnapshots.length;
    const recentAvg = recentSnapshots.reduce((sum, s) => sum + s.heapUsed, 0) / recentSnapshots.length;
    
    const growthPercent = ((recentAvg - earlyAvg) / earlyAvg) * 100;
    const growthBytes = recentAvg - earlyAvg;
    
    if (growthPercent > 50) {
      console.warn(
        `[MemoryProfiler] ⚠️ MEMORY GROWTH DETECTED: +${formatBytes(growthBytes)} (+${growthPercent.toFixed(1)}%) ` +
        `over ${memorySnapshots.length} snapshots`
      );
      logDetailedCacheStats();
    }
  }
  
  function logDetailedCacheStats() {
    // Detailed cache statistics available via window.clearCommsDebug in dev mode
  }
  
  if (IS_DEV && typeof window !== 'undefined') {
    (window as any).clearCommsDebug = {
      logMemory: logMemorySnapshot,
      logCaches: logDetailedCacheStats,
      getSnapshots: () => memorySnapshots,
      forceCleanup: () => {
        performPeriodicCleanup();
        logMemorySnapshot();
      },
      forceGC: () => {
        cleanupAllCaches();
        setTimeout(logMemorySnapshot, 1000);
      }
    };
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // DERIVED STATE
  // ─────────────────────────────────────────────────────────────────────────────

  // Track display count and resize window when bindings change
  $effect(() => {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName),
      ...pinnedApps
    ]);
    
    let displayCount = boundProcessNames.size;
    
    if (isEditMode && displayCount >= 1) {
      displayCount += 2;
    }
    
    if (audioInitialised && displayCount !== previousDisplayCount) {
      previousDisplayCount = displayCount;
      resizeWindowToFit(displayCount);
    }
  });

  // Get bound sessions with inactive session entries for apps not currently running
  function getBoundSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName),
      ...pinnedApps
    ]);
    
    const sessions: AudioSession[] = [];
    const foundProcessNames = new Set<string>();
    
    // Handle system volume specially if it's bound
    if (boundProcessNames.has(SYSTEM_VOLUME_PROCESS_NAME)) {
      // System volume will be fetched and updated separately
      const existingSystemSession = audioSessions.find(s => s.process_name === SYSTEM_VOLUME_PROCESS_NAME);
      if (existingSystemSession) {
        sessions.push(existingSystemSession);
      } else {
        // Placeholder until actual state is fetched
        sessions.push({
          session_id: SYSTEM_VOLUME_ID,
          display_name: SYSTEM_VOLUME_DISPLAY_NAME,
          process_id: 0,
          process_name: SYSTEM_VOLUME_PROCESS_NAME,
          volume: 1.0,
          is_muted: false
        });
      }
      foundProcessNames.add(SYSTEM_VOLUME_PROCESS_NAME);
    }
    
    for (const session of audioSessions) {
      if (boundProcessNames.has(session.process_name) && !foundProcessNames.has(session.process_name)) {
        sessions.push(session);
        foundProcessNames.add(session.process_name);
      }
    }
    
    const allMappings = [...axisMappings, ...buttonMappings];
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
    
    for (const processName of pinnedApps) {
      if (!foundProcessNames.has(processName)) {
        const activeSession = audioSessions.find(s => s.process_name === processName);
        if (activeSession) {
          sessions.push(activeSession);
        } else {
          // For inactive apps, use cached friendly name if available; otherwise format the process name
          const cachedFriendlyName = appFriendlyNames.get(processName);
          const displayName = cachedFriendlyName || processName.replace(/\.exe$/i, '');
          sessions.push({
            session_id: `inactive_${processName}`,
            display_name: displayName,
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

  function getAvailableSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName),
      ...pinnedApps
    ]);
    
    // Filter out bound processes and deduplicate by process_name
    // (keep only the first session for each process to avoid duplicate app entries)
    const seenProcesses = new Set<string>();
    const sessions = audioSessions
      .filter(s => {
        if (boundProcessNames.has(s.process_name)) {
          return false; // Skip bound processes
        }
        if (seenProcesses.has(s.process_name)) {
          return false; // Skip duplicate process entries
        }
        seenProcesses.add(s.process_name);
        return true;
      });
    
    // Add system volume option if not already bound
    if (!boundProcessNames.has(SYSTEM_VOLUME_PROCESS_NAME)) {
      sessions.unshift({
        session_id: SYSTEM_VOLUME_ID,
        display_name: SYSTEM_VOLUME_DISPLAY_NAME,
        process_id: 0,
        process_name: SYSTEM_VOLUME_PROCESS_NAME,
        volume: 1.0,
        is_muted: false
      });
    }
    
    return sessions;
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // KEYBOARD NAVIGATION
  // ─────────────────────────────────────────────────────────────────────────────

  const isElementVisible = (el: HTMLElement) => {
    const style = window.getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") return false;
    return el.offsetParent !== null || el.getClientRects().length > 0;
  };

  const getAppFocusables = () => {
    return Array.from(
      document.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"]), [role="button"]'
      )
    ).filter((el) => !el.hasAttribute('disabled') && el.getAttribute('aria-hidden') !== 'true' && isElementVisible(el));
  };

  const handleGlobalTab = (event: KeyboardEvent) => {
    if (event.key !== "Tab" || event.defaultPrevented) return;

    const focusables = getAppFocusables();
    if (focusables.length === 0) return;

    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement as HTMLElement | null;

    if (!event.shiftKey) {
      if (active === last || active === document.body || active === null) {
        event.preventDefault();
        first.focus();
      }
    } else {
      if (active === first || active === document.body || active === null) {
        event.preventDefault();
        last.focus();
      }
    }
  };

  // ─────────────────────────────────────────────────────────────────────────────
  // UI ACTIONS
  // ─────────────────────────────────────────────────────────────────────────────

  function toggleEditMode() {
    isEditMode = !isEditMode;
    if (!isEditMode) {
      addAppListExpanded = false;
      settingsMenuExpanded = false;
      // Cancel any active binding modes when exiting edit mode
      if (isBindingMode) {
        cancelBinding();
      }
      if (isButtonBindingMode) {
        cancelButtonBinding();
      }
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // LIFECYCLE
  // ─────────────────────────────────────────────────────────────────────────────

  onMount(() => {
    const init = async () => {
      // Dynamically import debug config in development only
      if (import.meta.env.DEV) {
        const debugMod = await import('$lib/debug');
        DEBUG = debugMod.DEBUG;
      }

      if (DEBUG.ENABLED) {
        // Debug mode: apply overrides without backend initialisation
        applyDebugOverrides();
        return;
      }

      // Normal initialisation path
      await initTheme();

      await Promise.all([
        loadMappings(),
        loadButtonMappings(),
        loadPinnedApps(),
        loadAppFriendlyNames(),
        loadSimAssignments()
      ]);

      await fetchWindowPinnedState();
      await autoInitialise();

      // Measure layout dimensions once on mount
      // This ensures the backend knows the actual rendered widths for all DPI scales
      measureLayoutDimensions();
    };

    init();

    // Listen for pin state changes from the backend (e.g., from context menu)
    // Store the promise so cleanup can unlisten even if it hasn't resolved yet
    const unlistenPromise = listen('window-pin-changed', (event: { payload: boolean }) => {
      windowPinned = event.payload;
    });


    const handleBlur = async () => {
      // Fetch current pinned state to ensure we have the latest value
      await fetchWindowPinnedState();
      
      // Only close menus and disable edit mode if window is NOT pinned on top
      if (!windowPinned) {
        if (isEditMode) {
          isEditMode = false;
          isBindingMode = false;
          isButtonBindingMode = false;
          pendingBinding = null;
          pendingButtonBinding = null;
          addAppListExpanded = false;
        }
        
        dockOpen = false;
        settingsMenuExpanded = false;
        closeMenuExpanded = false;
      }
    };

    const handleFocus = async () => {
      // Fetch current pinned state to ensure we have the latest value
      await fetchWindowPinnedState();
      
      // Close menus when window regains focus (dock may be opened by focus events)
      // But only if window is not pinned on top
      if (!windowPinned) {
        settingsMenuExpanded = false;
        closeMenuExpanded = false;
        addAppListExpanded = false;
      }
    };

    window.addEventListener('blur', handleBlur);
    window.addEventListener('focus', handleFocus);

    return () => {
      window.removeEventListener('blur', handleBlur);
      window.removeEventListener('focus', handleFocus);
      unlistenPromise.then(fn => fn()).catch(() => {});
    };
  });

  function applyDebugOverrides() {
    // Boot screen overrides: these prevent the app from reaching the main UI
    if (DEBUG.FORCE_BOOT_ERROR) {
      initStatus = "Failed";
      errorMsg = "Debug: Forced boot error";
      return;
    }
    
    if (DEBUG.FORCE_BOOT_SCREEN) {
      // initStatus remains "Initialising...": app stays on boot screen
      return;
    }
    
    // Default path: transition past boot screen into the main application
    initStatus = "Ready";
    pinnedAppsLoaded = true;
    
    // Audio subsystem state
    if (DEBUG.FORCE_AUDIO_NOT_INITIALISED) {
      audioInitialised = false;
    } else {
      audioInitialised = true;
    }
    
    // Session data
    if (DEBUG.FORCE_MOCK_SESSIONS && !DEBUG.FORCE_NO_SESSIONS) {
      audioSessions = DEBUG.MOCK_SESSIONS;
      pinnedApps = new Set(DEBUG.MOCK_SESSIONS.map((s: AudioSession) => s.process_name));
    } else if (DEBUG.FORCE_NO_SESSIONS) {
      audioSessions = [];
    }
    
    // UI state overrides
    if (DEBUG.FORCE_EDIT_MODE) {
      isEditMode = true;
    }
    
    // Banner overrides
    if (DEBUG.FORCE_ERROR_BANNER) {
      errorMsg = DEBUG.ERROR_BANNER_TEXT;
    }
  }

  onDestroy(() => {
    stopPolling();
    if (stopSimStatusListenerFn) {
      stopSimStatusListenerFn();
    }
    cleanupAllAnimations();
    cleanupAllLiveVolumeStates();
    cleanupAllCaches();
    for (const entry of simVolumeWrites.values()) {
      if (entry.timerId !== undefined) clearTimeout(entry.timerId);
    }
    simVolumeWrites.clear();
    if (IS_DEV && typeof window !== 'undefined') {
      delete (window as any).clearCommsDebug;
    }
  });


  // ─────────────────────────────────────────────────────────────────────────────
  // INITIALISATION & POLLING
  // ─────────────────────────────────────────────────────────────────────────────

  async function autoInitialise() {
    try {
      initStatus = "Initialising subsystems...";

      // Input and audio are independent: start both immediately
      const [inputResult, audioResult] = await Promise.allSettled([
        invoke<string>("init_input"),
        invoke<string>("init_audio_manager"),
      ]);

      if (inputResult.status === "rejected") {
        throw new Error(`Input failed: ${inputResult.reason}`);
      }

      if (audioResult.status === "fulfilled") {
        audioInitialised = true;
        // The audio thread emits the initial session list immediately after init,
        // but fetch once here as a guarantee in case the event arrives before the
        // listener is registered below.
        await refreshAudioSessions();
      } else {
        console.warn("Audio manager failed (non-critical):", audioResult.reason);
      }

      initStatus = "Starting real-time monitoring...";
      startPolling();
      
      // Start listening for simulator status change events
      stopSimStatusListenerFn = await startSimStatusListener();

      initStatus = "Ready";
      errorMsg = "";
    } catch (error) {
      const errorMessage = `Initialisation failed: ${error}`;
      errorMsg = errorMessage;
      initStatus = "Failed";
      console.error("Initialisation error:", error);
    }
  }

  /** Unlisten handle for the input-axis-data Tauri event */
  let unlistenInputAxis: (() => void) | null = null;

  /** Process incoming axis data from the dedicated input polling thread */
  function handleAxisData(data: AxisData[]) {
    axisData = data;
    // Apply mappings synchronously since data arrives at the polling thread's cadence
    applyAxisMappings();
    applyButtonMappings();
    pollIterations += 1;
    if (pollIterations > 1000000) {
      pollIterations = 0;
    }
  }
  
  function startPolling() {
    if (pollingInterval) return;
    
    isPolling = true;

    // Listen for axis data events emitted by the dedicated Rust input thread.
    // The Rust thread only emits when values have changed, so this fires at
    // most at the poll cadence and only when hardware is being moved.
    listen<AxisData[]>('input-axis-data', (event) => {
      handleAxisData(event.payload);
    }).then((unlisten) => {
      unlistenInputAxis = unlisten;
    });

    // Listen for LVar value changes pushed by the SimConnect thread whenever a
    // subscribed simulator function's LVar changes in the simulator.
    listen<LvarValueEvent>('lvar-value-changed', (event) => {
      handleLvarValueChanged(event.payload);
    }).then((unlisten) => {
      unlistenLvarValue = unlisten;
    }).catch((e) => {
      console.error("Failed to subscribe to lvar-value-changed:", e);
    });

    // Listen for audio session push events from the Rust audio COM thread.
    // The backend emits this when it detects topology changes (device add/remove,
    // session start/stop, external volume change): no frontend polling required.
    startAudioMonitoring();
    startMemoryMonitoring();
    startMemoryProfiler();
  }

  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
    }
    // Clean up the input axis event listener
    if (unlistenInputAxis) {
      unlistenInputAxis();
      unlistenInputAxis = null;
    }
    // Clean up the LVar value event listener
    if (unlistenLvarValue) {
      unlistenLvarValue();
      unlistenLvarValue = null;
    }
    isPolling = false;
    pollInFlight = false;
    stopAudioMonitoring();
    stopMemoryMonitoring();
    stopMemoryProfiler();
  }

  /**
   * Subscribe to the `audio-state-updated` push event emitted by the Rust audio
   * COM thread whenever the session topology or volumes change. Replaces the
   * previous setInterval-based approach which polled every 1 second.
   */
  function startAudioMonitoring() {
    if (unlistenAudioState) return;

    listen<AudioSession[]>('audio-state-updated', async (event) => {
      await handleAudioStateUpdate(event.payload);
    }).then((unlisten) => {
      unlistenAudioState = unlisten;
    }).catch((e) => {
      console.error("Failed to subscribe to audio-state-updated:", e);
    });
  }

  function stopAudioMonitoring() {
    if (unlistenAudioState) {
      unlistenAudioState();
      unlistenAudioState = null;
    }
  }

  /**
   * Fetch the latest audio sessions from the backend and apply them.
   * Used on startup and as a manual force-refresh. Ongoing updates arrive
   * via the `audio-state-updated` push event handled by `handleAudioStateUpdate`.
   */
  async function refreshAudioSessions() {
    try {
      const sessions = await invoke<AudioSession[]>("get_audio_sessions");
      await handleAudioStateUpdate(sessions);
    } catch (error) {
      console.error("Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  /**
   * Process an incoming session list: either from a manual `get_audio_sessions`
   * call or from the `audio-state-updated` push event. Merges new data with
   * existing UI state (preserving manual control, animations) and optionally
   * augments the list with the system volume session when it is bound.
   */
  async function handleAudioStateUpdate(sessions: AudioSession[]) {
      // Build a lookup map once to avoid O(n²) findIndex scans
      const existingById = new Map(audioSessions.map(s => [s.session_id, s]));
      
      // Track volume changes for smooth animation
      const volumeChanges = new Map<string, { from: number; to: number }>();
      
      // If system volume is bound (pinned or has mappings), fetch and add it to the sessions
      const hasSystemVolume = pinnedApps.has(SYSTEM_VOLUME_PROCESS_NAME) ||
        axisMappings.some(m => m.processName === SYSTEM_VOLUME_PROCESS_NAME) ||
        buttonMappings.some(m => m.processName === SYSTEM_VOLUME_PROCESS_NAME);
      
      if (hasSystemVolume) {
        try {
          const systemVolume = await invoke<number>("get_system_volume");
          const systemMuted = await invoke<boolean>("get_system_mute");
          
          const systemSession: AudioSession = {
            session_id: SYSTEM_VOLUME_ID,
            display_name: SYSTEM_VOLUME_DISPLAY_NAME,
            process_id: 0,
            process_name: SYSTEM_VOLUME_PROCESS_NAME,
            volume: systemVolume,
            is_muted: systemMuted
          };
          
          // Preserve manual control or animation state
          const existing = existingById.get(SYSTEM_VOLUME_ID);
          if (existing) {
            if (manuallyControlledSessions.has(SYSTEM_VOLUME_ID)) {
              systemSession.volume = existing.volume;
              systemSession.is_muted = existing.is_muted;
            } else if (animatingSliders.has(SYSTEM_VOLUME_ID)) {
              systemSession.volume = existing.volume;
              systemSession.is_muted = existing.is_muted;
            } else {
              // Handle mute state transitions
              if (systemSession.is_muted && !existing.is_muted) {
                // Just muted externally: no volume animation needed, display derives to 0
              } else if (!systemSession.is_muted && existing.is_muted) {
                // Just unmuted externally: no volume animation needed, display derives from real volume
              } else if (!systemSession.is_muted) {
                // Not muted: detect external volume changes and queue animation
                const volumeDiff = Math.abs(systemSession.volume - existing.volume);
                if (volumeDiff > 0.01) {
                  volumeChanges.set(SYSTEM_VOLUME_ID, { from: existing.volume, to: systemSession.volume });
                  systemSession.volume = existing.volume;
                }
              }
              // When muted, always accept the real Windows volume (display is derived as 0)
            }
          }
          
          sessions.push(systemSession);
        } catch (error) {
          console.error("Error fetching system volume:", error);
        }
      }
      
      for (const newSession of sessions) {
        const existing = existingById.get(newSession.session_id);
        
        if (existing) {
          if (manuallyControlledSessions.has(newSession.session_id)
            || isLocallyDriven(newSession.session_id)) {
            newSession.volume = existing.volume;
            newSession.is_muted = existing.is_muted;
            newSession.displayVolumeOverride = existing.displayVolumeOverride;
          } else if (animatingSliders.has(newSession.session_id)) {
            newSession.volume = existing.volume;
            newSession.is_muted = existing.is_muted;
            newSession.displayVolumeOverride = existing.displayVolumeOverride;
          } else {
            // Handle mute state transitions
            if (newSession.is_muted && !existing.is_muted) {
              // Just muted externally: no volume animation needed, display derives to 0
            } else if (!newSession.is_muted && existing.is_muted) {
              // Just unmuted externally: no volume animation needed, display derives from real volume
            } else if (!newSession.is_muted) {
              // Not muted: detect external volume changes and queue animation
              const volumeDiff = Math.abs(newSession.volume - existing.volume);
              if (volumeDiff > 0.01) {
                volumeChanges.set(newSession.session_id, { from: existing.volume, to: newSession.volume });
                newSession.volume = existing.volume;
              }
            }
            // When muted, always accept the real Windows volume (display is derived as 0)
          }
        }
      }
      
      audioSessions = sessions;
      
      // Capture friendly names from active sessions for persistence.
      // Only write to disk if a name actually changed (dirty-flag pattern).
      let friendlyNamesDirty = false;
      for (const session of sessions) {
        if (session.display_name && session.display_name !== formatProcessName(session.process_name)) {
          const existing = appFriendlyNames.get(session.process_name);
          if (existing !== session.display_name) {
            appFriendlyNames.set(session.process_name, session.display_name);
            friendlyNamesDirty = true;
          }
        }
      }
      if (friendlyNamesDirty) {
        saveAppFriendlyNames();
      }
      
      // Trigger smooth animations for external changes using requestAnimationFrame
      for (const [sessionId, change] of volumeChanges) {
        // Animation frames no longer write through, so an externally driven
        // change (Windows mixer, another app) still needs its settled value
        // forwarded to the cockpit once the slider has caught up.
        animateVolumeTo(sessionId, change.to, 200, 'windows').then((completed) => {
          if (completed) writeSimVolumeFinal(sessionId, change.to);
        });
      }
      
      cleanupStaleMappings();
  }

  function cleanupStaleMappings() {
    // Intentionally kept empty - we preserve mappings for inactive apps
    return;
  }

  async function measureLayoutDimensions() {
    try {
      // Wait for the next paint to ensure elements are fully rendered
      await new Promise(resolve => requestAnimationFrame(resolve));
      
      // Find the mixer container and first channel
      const mixer = document.querySelector<HTMLElement>('.mixer-container');
      const firstChannel = document.querySelector<HTMLElement>('.application-channel');
      
      if (!mixer || !firstChannel) {
        return;
      }
      
      // Get actual rendered dimensions in logical pixels
      const channelWidth = Math.round(firstChannel.clientWidth);
      
      // Get the gap from the mixer's computed style
      const computedStyle = window.getComputedStyle(mixer);
      const gapStr = computedStyle.gap;
      const channelGap = parseInt(gapStr) || 50; // Fallback to 50px if can't parse
      
      // Measure the main container's horizontal padding (one side)
      // Expected: 100px total (50px per side)
      const mainEl = document.querySelector<HTMLElement>('main');
      let padding = 50; // Sensible default (100px total)
      if (mainEl) {
        const mainStyle = window.getComputedStyle(mainEl);
        padding = parseInt(mainStyle.paddingLeft) || 50;
      }
      
      // Send measurements to backend
      // The Rust formula is: (n × channelWidth) + ((n-1) × channelGap) + (2 × padding)
      const result = await invoke<string>('update_layout_measurements', {
        channelWidth: channelWidth,
        channelGap: channelGap,
        padding: padding,
      });
      
    } catch (error) {
      console.error("[Layout] Failed to measure and report layout dimensions:", error);
      // Non-fatal error - window sizing will use defaults
    }
  }

  async function resizeWindowToFit(sessionCount: number) {
    try {
      await invoke<string>("resize_window_to_content", { sessionCount });
    } catch (error) {
      console.error("Error resizing window:", error);
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // VOLUME CONTROL
  // ─────────────────────────────────────────────────────────────────────────────

  // Helper: Invoke set volume for either regular session or system volume
  async function invokeSetVolume(sessionId: string, volume: number): Promise<void> {
    if (sessionId === SYSTEM_VOLUME_ID) {
      await invoke("set_system_volume", { volume });
    } else {
      await invoke("set_session_volume", { sessionId, volume });
    }
  }

  // Helper: Invoke set mute for either regular session or system volume
  async function invokeSetMute(sessionId: string, muted: boolean): Promise<void> {
    if (sessionId === SYSTEM_VOLUME_ID) {
      await invoke("set_system_mute", { muted });
    } else {
      await invoke("set_session_mute", { sessionId, muted });
    }
  }

  /**
   * Who caused a volume change.
   *
   * Stated positively on purpose. This used to be a `fromLvar` boolean, and
   * `!fromLvar` was read as "the user did this": but it only ever meant "not
   * the simulator", so a change pushed by Windows impersonated user input. That
   * let an inbound LVar update, applied to Windows, come back as an audio push
   * that claimed the sim function and blocked every further LVar read.
   */
  type VolumeSource = 'user' | 'lvar' | 'windows';

  function setSessionVolumeImmediate(sessionId: string, volume: number, source: VolumeSource = 'user', writeSim: boolean = true) {
    if (sessionId.startsWith('inactive_')) return;

    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex !== -1) {
      audioSessions[sessionIndex].volume = volume;
      // Mute follows volume across zero. Only the user's own input does this:
      // the cockpit's volume and mute are separate controls, and Windows
      // reporting a level back is not a gesture.
      if (source === 'user' && applyAutoMute(audioSessions[sessionIndex], volume)) {
        // The mute LVar is part of the same gesture, so it moves with it.
        writeSimMute(sessionId, audioSessions[sessionIndex].is_muted);
      }
    }

    if (source === 'user') {
      // This gesture now owns the sim function; its LVar stops being read until
      // the movement settles. Every user volume path reaches this point, so
      // hardware axes are covered as well as the mouse.
      markSimFunctionLocalInput(sessionId);

      // Applications sharing a sim function follow each other here, in the app,
      // instead of waiting for the LVar to echo back.
      syncSimFunctionSiblings(sessionId, volume);

      // Two-way sim sync: local gestures write through to the function's volume
      // LVar (throttled). LVar-driven changes must not write back, and neither
      // may synthetic animation frames: only real input and settled values.
      if (writeSim) {
        writeSimVolume(sessionId, volume);
      }
    }
  }

  /**
   * How long after our last write to Windows a push is still assumed to be
   * answering that write rather than reporting someone else's change.
   */
  const LOCAL_DRIVE_SETTLE_MS = 300;

  /**
   * True while we are pushing volume to Windows for this session, or just did.
   *
   * A push generated before our write landed carries a stale value; treating it
   * as an external change makes the slider animate backwards to where it used
   * to be. This is deliberately separate from manuallyControlledSessions: that
   * flag additionally suppresses inbound cockpit state, which is right for a
   * control the user is holding but wrong for a channel we are merely driving.
   */
  function isLocallyDriven(sessionId: string): boolean {
    const state = liveVolumeState.get(sessionId);
    if (!state) return false;
    return state.inFlight || performance.now() - state.lastSent < LOCAL_DRIVE_SETTLE_MS;
  }

  function scheduleLiveVolumeUpdate(sessionId: string, volume: number) {
    if (sessionId.startsWith('inactive_')) return;

    let state = liveVolumeState.get(sessionId);
    if (!state) {
      state = { inFlight: false, lastSent: 0 };
      liveVolumeState.set(sessionId, state);
    }

    state.queuedVolume = volume;

    const attemptSend = () => {
      const currentState = liveVolumeState.get(sessionId);
      if (!currentState) return;

      const queued = currentState.queuedVolume;
      if (queued === undefined) return;
      if (currentState.inFlight) return;

      const now = performance.now();
      const elapsed = now - currentState.lastSent;

      if (elapsed < LIVE_UPDATE_MIN_INTERVAL_MS) {
        if (currentState.timerId !== undefined) {
          clearTimeout(currentState.timerId);
        }

        const delay = Math.max(0, LIVE_UPDATE_MIN_INTERVAL_MS - elapsed);
        currentState.timerId = window.setTimeout(() => {
          const refreshedState = liveVolumeState.get(sessionId);
          if (!refreshedState) return;
          refreshedState.timerId = undefined;
          attemptSend();
        }, delay);

        return;
      }

      currentState.inFlight = true;
      currentState.lastSent = now;
      currentState.queuedVolume = undefined;
      if (currentState.timerId !== undefined) {
        clearTimeout(currentState.timerId);
        currentState.timerId = undefined;
      }

      const volumeToSend = queued;

      (async () => {
        try {
          await invokeSetVolume(sessionId, volumeToSend);
          await invokeSetMute(sessionId, volumeToSend === 0);
        } catch (error) {
          console.error(`Error applying live volume for ${sessionId}:`, error);
        } finally {
          const finalState = liveVolumeState.get(sessionId);
          if (!finalState) return;
          finalState.inFlight = false;
          attemptSend();
        }
      })();
    };

    attemptSend();
  }

  function clearLiveVolumeState(sessionId: string) {
    const state = liveVolumeState.get(sessionId);
    if (!state) return;
    if (state.timerId !== undefined) {
      clearTimeout(state.timerId);
    }
    liveVolumeState.delete(sessionId);
  }

  function cancelVolumeAnimation(sessionId: string) {
    const signal = animationSignals.get(sessionId);
    if (!signal) return;

    signal.cancelled = true;
    if (signal.frameId !== undefined) {
      cancelAnimationFrame(signal.frameId);
    }

    const resolve = signal.resolve;
    signal.resolve = undefined;
    animationSignals.delete(sessionId);
    animatingSliders.delete(sessionId);
    resolve?.(false);
  }

  async function animateVolumeTo(sessionId: string, targetVolume: number, durationMs: number = 200, source: VolumeSource = 'user'): Promise<boolean> {
    if (sessionId.startsWith('inactive_')) return false;

    const session = audioSessions.find(s => s.session_id === sessionId);
    if (!session) return false;

    cancelVolumeAnimation(sessionId);

    const startVolume = session.volume;
    const startTime = Date.now();
    animatingSliders.add(sessionId);

    return new Promise<boolean>((resolve) => {
      const signal: AnimationSignal = { cancelled: false, resolve, frameId: undefined };
      animationSignals.set(sessionId, signal);

      const animate = () => {
        if (signal.cancelled) return;

        const elapsed = Date.now() - startTime;
        const progress = Math.min(elapsed / durationMs, 1);
        const eased = 1 - Math.pow(1 - progress, 3);
        const currentVolume = startVolume + (targetVolume - startVolume) * eased;

        // Animation frames are interpolation, not input: they never write to
        // the sim. Every caller forwards the settled value itself.
        setSessionVolumeImmediate(sessionId, currentVolume, source, false);

        if (progress < 1) {
          signal.frameId = requestAnimationFrame(animate);
        } else {
          animationSignals.delete(sessionId);
          animatingSliders.delete(sessionId);
          resolve(true);
        }
      };

      animate();
    });
  }
  
  function startHardwareVolumeInterpolation(sessionId: string, targetVolume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    hardwareVolumeTargets.set(sessionId, targetVolume);
    
    if (!hardwareVolumeAnimations.has(sessionId)) {
      // Geometric convergence reaches the target in about twenty frames. The
      // cap exists because this loop calls setSessionVolumeImmediate, which
      // claims the sim function: were anything ever to stop it converging, the
      // channel would go deaf to its LVar silently and for good. Bounding it
      // makes that impossible rather than merely unlikely.
      const maxFrames = 60;
      let frame = 0;

      const animate = () => {
        const target = hardwareVolumeTargets.get(sessionId);
        if (target === undefined) {
          hardwareVolumeAnimations.delete(sessionId);
          return;
        }
        
        const session = audioSessions.find(s => s.session_id === sessionId);
        if (!session) {
          hardwareVolumeAnimations.delete(sessionId);
          hardwareVolumeTargets.delete(sessionId);
          return;
        }
        
        const current = session.volume;
        const diff = target - current;
        const newVolume = current + (diff * HARDWARE_VOLUME_SMOOTHING);
        
        frame += 1;
        if (Math.abs(diff) < 0.001 || frame >= maxFrames) {
          setSessionVolumeImmediate(sessionId, target);
          hardwareVolumeAnimations.delete(sessionId);
          hardwareVolumeTargets.delete(sessionId);
          return;
        }
        
        setSessionVolumeImmediate(sessionId, newVolume);
        const frameId = requestAnimationFrame(animate);
        hardwareVolumeAnimations.set(sessionId, frameId);
      };
      
      const frameId = requestAnimationFrame(animate);
      hardwareVolumeAnimations.set(sessionId, frameId);
    }
  }

  function startMemoryMonitoring() {
    if (memoryMonitorInterval) return;
    
    memoryMonitorInterval = setInterval(() => {
      const now = Date.now();
      
      if (now - lastMemoryCleanup > MEMORY_CLEANUP_INTERVAL) {
        performPeriodicCleanup();
        lastMemoryCleanup = now;
      }
      
      if (previousAxisValues.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Axis cache size exceeded limit, clearing");
        previousAxisValues.clear();
      }
      
      if (previousButtonStates.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Button cache size exceeded limit, clearing");
        previousButtonStates.clear();
      }
      
      if (lastHardwareAxisValues.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Hardware axis cache size exceeded limit, clearing");
        lastHardwareAxisValues.clear();
        axisActivated.clear();
      }

      if (liveVolumeState.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Live volume state cache size exceeded limit, clearing");
        cleanupAllLiveVolumeStates();
      }
      
      if (hardwareVolumeTargets.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Hardware volume targets cache size exceeded limit, clearing");
        for (const [_, frameId] of hardwareVolumeAnimations) {
          cancelAnimationFrame(frameId);
        }
        hardwareVolumeAnimations.clear();
        hardwareVolumeTargets.clear();
      }
    }, 30000);
  }
  
  function stopMemoryMonitoring() {
    if (memoryMonitorInterval) {
      clearInterval(memoryMonitorInterval);
      memoryMonitorInterval = null;
    }
  }
  
  function performPeriodicCleanup() {
    const activeSessionIds = new Set(audioSessions.map(s => s.session_id));
    
    for (const sessionId of animatingSliders) {
      if (!activeSessionIds.has(sessionId)) {
        animatingSliders.delete(sessionId);
      }
    }
    
    for (const sessionId of manuallyControlledSessions) {
      if (!activeSessionIds.has(sessionId)) {
        manuallyControlledSessions.delete(sessionId);
      }
    }

    for (const sessionId of manualControlClaims.keys()) {
      if (!activeSessionIds.has(sessionId)) {
        const timerId = manualControlReleaseTimers.get(sessionId);
        if (timerId !== undefined) clearTimeout(timerId);
        manualControlReleaseTimers.delete(sessionId);
        manualControlClaims.delete(sessionId);
      }
    }
    
    for (const [sessionId, frameId] of muteAnimationFrames) {
      if (!activeSessionIds.has(sessionId)) {
        cancelAnimationFrame(frameId);
        muteAnimationFrames.delete(sessionId);
      }
    }
    
    for (const [sessionId, frameId] of hardwareVolumeAnimations) {
      if (!activeSessionIds.has(sessionId)) {
        cancelAnimationFrame(frameId);
        hardwareVolumeAnimations.delete(sessionId);
        hardwareVolumeTargets.delete(sessionId);
      }
    }
    
    for (const [sessionId] of liveVolumeState) {
      if (!activeSessionIds.has(sessionId)) {
        clearLiveVolumeState(sessionId);
      }
    }

    // Clean up stale axis activation and hardware values
    const activeMappingKeys = new Set(
      axisMappings.map(m => `${m.deviceHandle}-${m.axisName}-${m.processName}`)
    );
    for (const key of Array.from(lastHardwareAxisValues.keys())) {
      if (!activeMappingKeys.has(key)) {
        lastHardwareAxisValues.delete(key);
        axisActivated.delete(key);
      }
    }

  }
  
  async function setSessionVolumeFinal(sessionId: string, volume: number) {
    if (sessionId.startsWith('inactive_')) return;

    try {
      await invokeSetVolume(sessionId, volume);
      // Only called from local gestures, so always write the final value to the sim
      writeSimVolumeFinal(sessionId, volume);
      syncSimFunctionSiblings(sessionId, volume);
      await refreshAudioSessions();
    } catch (error) {
      console.error("Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  /**
   * `fromSync` marks a change driven from outside this channel: an LVar update
   * or a sibling sharing the same sim function. Such a change is applied
   * locally but never propagated onward, which is what stops two shared
   * channels muting each other in a loop.
   */
  async function setSessionMute(sessionId: string, muted: boolean, fromSync: boolean = false) {
    if (sessionId.startsWith('inactive_')) return;

    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex === -1) return;
    const session = audioSessions[sessionIndex];

    // Cancel any ongoing volume animation (e.g. hardware input) before toggling mute
    cancelVolumeAnimation(sessionId);
    cancelMuteAnimation(sessionId);

    try {
      // Two-way sim sync: local mute gestures write through to the function's
      // mute LVar and to any channel sharing that function.
      if (!fromSync) {
        // Same rule as volume: this gesture owns the sim function, so its LVars
        // stop being read while it settles. Mute needs this more than volume
        // does: is_muted is only committed once the 200ms animation finishes,
        // so an echo arriving mid-animation still sees the old state, applies
        // the change again and restarts the animation on every bound channel.
        markSimFunctionLocalInput(sessionId);

        writeSimMute(sessionId, muted);
        syncSimFunctionMuteSiblings(sessionId, muted);
      }

      if (muted) {
        // Muting: visually animate slider from current volume to 0, then set mute flag
        const startVolume = session.is_muted ? 0 : session.volume;

        // Call Windows to mute immediately (audio silences now, volume preserved natively)
        await invokeSetMute(sessionId, true);

        // Animate the slider visually from current position to 0
        await animateMuteVisual(sessionId, startVolume, 0, 200);

        // After animation completes, apply final state
        const idx = audioSessions.findIndex(s => s.session_id === sessionId);
        if (idx !== -1) {
          audioSessions[idx].is_muted = true;
          audioSessions[idx].displayVolumeOverride = undefined;
        }
      } else {
        // Unmuting: call Windows first (restores audio), then animate slider from 0 to real volume
        await invokeSetMute(sessionId, false);

        // Briefly refresh to get the real Windows volume after unmute
        let targetVolume = session.volume;
        try {
          const freshSessions: AudioSession[] = await invoke('get_audio_sessions');
          const fresh = freshSessions.find(s => s.session_id === sessionId);
          if (fresh) targetVolume = fresh.volume;
        } catch { /* use existing volume as fallback */ }
        if (targetVolume <= 0) targetVolume = 0.5; // Safety fallback

        // Set unmuted state immediately so display derives correctly after animation
        audioSessions[sessionIndex].is_muted = false;

        // Animate the slider visually from 0 to the real volume
        await animateMuteVisual(sessionId, 0, targetVolume, 200);

        // After animation completes, clear override and set real volume
        const idx = audioSessions.findIndex(s => s.session_id === sessionId);
        if (idx !== -1) {
          audioSessions[idx].volume = targetVolume;
          audioSessions[idx].displayVolumeOverride = undefined;
        }
      }
    } catch (error) {
      console.error("Error setting mute:", error);
      errorMsg = `Audio error: ${error}`;
      // Clean up override on error
      const idx = audioSessions.findIndex(s => s.session_id === sessionId);
      if (idx !== -1) audioSessions[idx].displayVolumeOverride = undefined;
      muteAnimationFrames.delete(sessionId);
    }
  }

  // Tracks active mute animation frame IDs so they can be cancelled
  const muteAnimationFrames = new Map<string, number>();

  function cancelMuteAnimation(sessionId: string) {
    const frameId = muteAnimationFrames.get(sessionId);
    if (frameId !== undefined) {
      cancelAnimationFrame(frameId);
      muteAnimationFrames.delete(sessionId);
    }
    // Clear any lingering override
    const idx = audioSessions.findIndex(s => s.session_id === sessionId);
    if (idx !== -1) audioSessions[idx].displayVolumeOverride = undefined;
  }

  /** Purely visual animation for mute/unmute: only updates displayVolumeOverride, no Windows API calls */
  function animateMuteVisual(sessionId: string, fromVolume: number, toVolume: number, durationMs: number): Promise<void> {
    return new Promise<void>((resolve) => {
      const startTime = Date.now();
      // Mark as animating so the poll doesn't overwrite our values
      animatingSliders.add(sessionId);

      const animate = () => {
        const elapsed = Date.now() - startTime;
        const progress = Math.min(elapsed / durationMs, 1);
        const eased = 1 - Math.pow(1 - progress, 3); // Cubic ease-out
        const currentVolume = fromVolume + (toVolume - fromVolume) * eased;

        // Update only the display override: no Windows API call
        const idx = audioSessions.findIndex(s => s.session_id === sessionId);
        if (idx !== -1) {
          audioSessions[idx].displayVolumeOverride = currentVolume;
        }

        if (progress < 1) {
          const frameId = requestAnimationFrame(animate);
          muteAnimationFrames.set(sessionId, frameId);
        } else {
          muteAnimationFrames.delete(sessionId);
          animatingSliders.delete(sessionId);
          resolve();
        }
      };

      animate();
    });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // SIM FUNCTION SYNC (LVar ⇄ application volume/mute)
  // ─────────────────────────────────────────────────────────────────────────────

  /**
   * Handle an inbound LVar value change from the simulator (sim → app).
   * Loop prevention is value-based: an event whose raw value equals what our
   * current app state maps to is the echo of our own write and is ignored, so
   * genuine cockpit changes always apply while our own writes never bounce back.
   */
  // TEMPORARY: names the filter that swallowed an inbound LVar update, so a
  // dead sim-to-app chain can be diagnosed in one run. Dev builds only.
  function traceLvarDrop(payload: LvarValueEvent, reason: string, detail?: unknown) {
    console.log(`[Sim] LVar dropped (${reason})`, payload.name, payload.value, detail ?? '');
  }

  function handleLvarValueChanged(payload: LvarValueEvent) {
    const routes = lvarRouteByName.get(payload.name);
    if (!routes || routes.length === 0) {
      if (IS_DEV) traceLvarDrop(payload, 'no route: LVar not in the active assignment set');
      return;
    }

    // A sim function may be shared, so one update can drive several channels.
    for (const route of routes) {
      applyLvarToRoute(payload, route);
    }
  }

  /** Apply one inbound LVar update to a single application channel. */
  function applyLvarToRoute(payload: LvarValueEvent, route: LvarRoute) {
    const session = audioSessions.find(s => s.process_name === route.processName);
    if (!session) {
      if (IS_DEV) traceLvarDrop(payload, 'app not running', { processName: route.processName });
      return;
    }

    const def = simFunctionByProcess.get(route.processName);
    if (!def) {
      if (IS_DEV) traceLvarDrop(payload, 'no function definition', { processName: route.processName });
      return;
    }

    // Local input owns the channel: while a gesture is in progress every other
    // source is ignored. Checked once for the whole route rather than per kind,
    // so mute is covered too: an inbound mute forces displayVolume to 0, which
    // would tear the slider out from under the cursor mid-drag.
    if (manuallyControlledSessions.has(session.session_id)) {
      if (IS_DEV) {
        traceLvarDrop(payload, 'session under local control', {
          claimId: manualControlClaims.get(session.session_id),
        });
      }
      return;
    }

    // One source at a time: while a gesture is driving this sim function, no
    // channel bound to it reads the LVar.
    if (isSimFunctionLocallyDriven(def)) {
      if (IS_DEV) traceLvarDrop(payload, 'sim function is under local input');
      return;
    }

    if (route.kind === 'volume') {
      // See lvarsSeenNonZero: a bare 0 before the aircraft has initialised is
      // not a level anyone selected.
      if (payload.value !== 0) {
        lvarsSeenNonZero.add(payload.name);
      } else if (!lvarsSeenNonZero.has(payload.name)) {
        if (IS_DEV) traceLvarDrop(payload, 'LVar has not reported a non-zero value yet');
        return;
      }

      // f32 round-trip means float LVars need an epsilon rather than ===
      const epsilon = (def.volume.max - def.volume.min) * LVAR_ECHO_EPSILON_RATIO;

      // Already where the cockpit is asking us to be: nothing to apply.
      // Compared in the shared orientation so a reversed channel is not read as
      // permanently out of step with the LVar.
      const currentValue = denormaliseVolume(session.volume, def.volume);
      if (Math.abs(currentValue - payload.value) < epsilon) {
        if (IS_DEV) traceLvarDrop(payload, 'already at this value', { currentValue, epsilon });
        return;
      }

      if (IS_DEV) console.log('[Sim] LVar applied', payload.name, payload.value);
      applyLvarVolume(session.session_id, normaliseVolume(payload.value, def.volume));
    } else if (def.mute) {
      // Whichever of the two states the reading is nearer to. Exact equality
      // against mutedValue/unmutedValue looked safe for a two-state latch, but
      // any f32 round-trip noise: or a switch that animates through
      // intermediate values: matched neither and was discarded in silence.
      const toMuted = Math.abs(payload.value - def.mute.mutedValue);
      const toUnmuted = Math.abs(payload.value - def.mute.unmutedValue);
      const muted = toMuted <= toUnmuted;

      if (muted === session.is_muted) {
        if (IS_DEV) traceLvarDrop(payload, 'already in this mute state', { muted });
        return;
      }

      if (IS_DEV) console.log('[Sim] LVar mute applied', payload.name, payload.value, muted);
      applyLvarMute(session.session_id, muted);
    }
  }

  /** Apply a simulator-driven volume change without writing back to the sim */
  async function applyLvarVolume(sessionId: string, unit: number) {
    const completed = await animateVolumeTo(sessionId, unit, 150, 'lvar');
    if (completed && !manuallyControlledSessions.has(sessionId)) {
      await invokeSetVolume(sessionId, unit).catch((e) => {
        console.error("[Sim] Error applying LVar-driven volume:", e);
      });
    }
  }

  /** Apply a simulator-driven mute change without writing back to the sim */
  function applyLvarMute(sessionId: string, muted: boolean) {
    setSessionMute(sessionId, muted, true).catch((e) => {
      console.error("[Sim] Error applying LVar-driven mute:", e);
    });
  }

  // ─── Write-Through (app → sim) ───
  // Volume writes are throttled with a trailing send so a slider drag produces
  // a steady but modest command rate; the final value of a gesture is always
  // sent immediately via writeSimVolumeFinal. Mute writes are single shots.

  const LVAR_WRITE_INTERVAL_MS = 120;
  const simVolumeWrites = new Map<string, {
    timerId?: number;
    pending?: number;
    lastSent?: number;
    lastSentAt?: number;
  }>();

  /** Echo tolerance as a fraction of an LVar's full range: comfortably above
   *  f32 round-trip error, far below any real knob movement. */
  const LVAR_ECHO_EPSILON_RATIO = 1e-4;

  /**
   * How long a write stays eligible to be recognised as its own echo. A round
   * trip is ~100 ms, so anything later is the cockpit moving on its own: and
   * an unbounded guard would suppress a genuine knob movement that happened to
   * land on the last value we wrote, permanently killing sim→app sync for it.
   */
  const LVAR_ECHO_WINDOW_MS = 500;

  function sendSimVolume(lvar: string, unit: number, def: SimFunctionDef, processName: string) {
    const value = denormaliseVolume(unit, def.volume);

    // Skip a write that would restate the value we just sent. Now that sharing
    // makes other channels follow an echo, a redundant write is not free: it
    // would nudge every application bound to this function. A stale record is
    // never skipped: the cockpit may have moved since.
    const previous = simVolumeWrites.get(lvar);
    if (previous?.lastSentAt !== undefined
      && Date.now() - previous.lastSentAt < LVAR_ECHO_WINDOW_MS
      && previous.lastSent !== undefined
      && Math.abs(previous.lastSent - value) < (def.volume.max - def.volume.min) * LVAR_ECHO_EPSILON_RATIO) {
      return;
    }

    // Remember what we sent so the inbound echo can be recognised even when it
    // is an intermediate throttled value rather than the final one.
    let entry = simVolumeWrites.get(lvar);
    if (!entry) {
      entry = {};
      simVolumeWrites.set(lvar, entry);
    }
    entry.lastSent = value;
    entry.lastSentAt = Date.now();

    invoke('set_sim_lvar', { name: lvar, value })
      .catch((e) => console.warn(`[Sim] Failed to write ${lvar}:`, e));
  }

  function writeSimVolume(sessionId: string, unit: number) {
    const session = sessionById.get(sessionId);
    if (!session) return;
    const def = simFunctionByProcess.get(session.process_name);
    if (!def) return;

    const lvar = def.volume.lvar;
    let entry = simVolumeWrites.get(lvar);
    if (!entry) {
      entry = {};
      simVolumeWrites.set(lvar, entry);
    }
    entry.pending = unit;
    if (entry.timerId !== undefined) return; // trailing send already scheduled

    entry.timerId = window.setTimeout(() => {
      const current = simVolumeWrites.get(lvar);
      if (!current) return;
      current.timerId = undefined;
      const pending = current.pending;
      current.pending = undefined;
      if (pending === undefined) return;
      sendSimVolume(lvar, pending, def, session.process_name);
    }, LVAR_WRITE_INTERVAL_MS);
  }

  /** Send the final value of a volume gesture immediately, cancelling any pending throttled write */
  function writeSimVolumeFinal(sessionId: string, unit: number) {
    const session = sessionById.get(sessionId);
    if (!session) return;
    const def = simFunctionByProcess.get(session.process_name);
    if (!def) return;

    const lvar = def.volume.lvar;
    const entry = simVolumeWrites.get(lvar);
    if (entry) {
      if (entry.timerId !== undefined) clearTimeout(entry.timerId);
      entry.timerId = undefined;
      entry.pending = undefined;
    }
    sendSimVolume(lvar, unit, def, session.process_name);
  }

  /**
   * When local input last drove each sim function, keyed by the definition
   * object the profile hands out per category.
   */
  const simFunctionLocalInputAt = new Map<SimFunctionDef, number>();

  /**
   * How long a sim function keeps ignoring its LVar after local input.
   * Comfortably longer than the 120ms write throttle plus a round trip, so the
   * echo of a gesture cannot land back on the sliders while it is still going.
   */
  const SIM_FUNCTION_LOCAL_HOLD_MS = 500;

  /** Record that a gesture is driving the sim function this session is bound to. */
  function markSimFunctionLocalInput(sessionId: string) {
    const session = sessionById.get(sessionId);
    if (!session) return;

    const def = simFunctionByProcess.get(session.process_name);
    if (!def) return;

    simFunctionLocalInputAt.set(def, performance.now());
  }

  /**
   * True while local input owns this sim function. One source at a time: a
   * drag, the wheel or a hardware axis owns the whole function, so no channel
   * bound to it reads the LVar until the gesture settles: including the
   * siblings the gesture is driving, which would otherwise be moved twice.
   */
  function isSimFunctionLocallyDriven(def: SimFunctionDef): boolean {
    // A pointer gesture owns the function for as long as the button is down:
    // including while held perfectly still, which produces no events at all and
    // so cannot be expressed as a timestamp.
    if (isSimFunctionHeld(def)) return true;

    // The wheel and the hardware axis have no gesture end, so they own it for a
    // short window after each movement instead.
    const at = simFunctionLocalInputAt.get(def);
    return at !== undefined && performance.now() - at < SIM_FUNCTION_LOCAL_HOLD_MS;
  }

  /** True while a pointer gesture is holding any channel bound to this function. */
  function isSimFunctionHeld(def: SimFunctionDef): boolean {
    const bound = sessionsBySimFunction.get(def);
    if (!bound) return false;

    return bound.some(s => manuallyControlledSessions.has(s.session_id));
  }

  /**
   * True when a pointer gesture is holding this channel, or any channel sharing
   * its sim function. Dragging a slider outranks every other source.
   */
  function isHeldByPointer(sessionId: string): boolean {
    if (manuallyControlledSessions.has(sessionId)) return true;

    const session = sessionById.get(sessionId);
    if (!session) return false;

    const def = simFunctionByProcess.get(session.process_name);
    return def !== undefined && isSimFunctionHeld(def);
  }

  /**
   * Every other running application bound to the same sim function. A profile
   * holds one definition object per category, so two channels share a function
   * exactly when they resolve to the same object: identity is the whole test.
   */
  /**
   * Every running application bound to the same sim function as this session,
   * the session itself included. Callers skip the origin inline rather than
   * filtering, which would allocate a second array on every call.
   */
  function simFunctionPeers(session: AudioSession): AudioSession[] {
    const def = simFunctionByProcess.get(session.process_name);
    if (!def) return [];

    return sessionsBySimFunction.get(def) ?? [];
  }

  /**
   * Mute follows volume across zero: muted at 0, unmuted above it. Returns
   * whether the state actually changed.
   *
   * The flag is flipped synchronously, before anything async, so a repeat call
   * within the same gesture sees the new state and cannot re-enter. Every
   * channel decides from its own level, which is what stops a reversed sibling
   *: sitting at full scale while the origin is at zero: being silenced.
   */
  function applyAutoMute(session: AudioSession, volume: number): boolean {
    const shouldMute = volume === 0;
    if (session.is_muted === shouldMute) return false;

    session.is_muted = shouldMute;
    invokeSetMute(session.session_id, shouldMute).catch(e =>
      console.error("Error applying auto-mute:", e));
    return true;
  }

  /**
   * Mirror a local volume change onto the sibling channels, so shared functions
   * move together immediately rather than one LVar round trip apart. The LVar
   * itself is still written once, by the originating channel: siblings only
   * need their app and Windows state.
   */
  function syncSimFunctionSiblings(originSessionId: string, unit: number) {
    const origin = sessionById.get(originSessionId);
    if (!origin) return;

    for (const session of simFunctionPeers(origin)) {
      if (session.session_id === originSessionId) continue;

      // The user's own grip wins: a channel being dragged is never repositioned
      // by a sibling, whatever is driving that sibling.
      if (manuallyControlledSessions.has(session.session_id)) continue;

      session.volume = unit;

      // The shared mute LVar is written once, by the originating gesture, so
      // this only brings the sibling's own state into line.
      applyAutoMute(session, unit);

      // Deliberately no manual-control claim: that flag also suppresses inbound
      // cockpit state, which would leave a shared channel deaf to its own LVar.
      // Routing every sibling write through the same throttle instead records
      // it in liveVolumeState, which is what lets isLocallyDriven recognise the
      // resulting audio push as our own rather than an external change.
      scheduleLiveVolumeUpdate(session.session_id, unit);
    }
  }

  /**
   * Mirror a local mute change onto the sibling channels. These go through
   * setSessionMute so they get the same animation and Windows call as the
   * origin; passing fromSync stops them propagating onward, which would
   * otherwise loop straight back here.
   */
  function syncSimFunctionMuteSiblings(originSessionId: string, muted: boolean) {
    const origin = sessionById.get(originSessionId);
    if (!origin) return;

    for (const session of simFunctionPeers(origin)) {
      if (session.session_id === originSessionId) continue;

      void setSessionMute(session.session_id, muted, true);
    }
  }

  function writeSimMute(sessionId: string, muted: boolean) {
    const session = sessionById.get(sessionId);
    if (!session) return;
    const def = simFunctionByProcess.get(session.process_name);
    if (!def?.mute) return;

    const raw = muted ? def.mute.mutedValue : def.mute.unmutedValue;
    invoke('set_sim_lvar', { name: def.mute.lvar, value: raw })
      .catch((e) => console.warn(`[Sim] Failed to write ${def.mute!.lvar}:`, e));
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // BINDING MANAGEMENT
  // ─────────────────────────────────────────────────────────────────────────────

  function startAxisBinding(sessionId: string, sessionName: string, processId: number, processName: string) {
    isBindingMode = true;
    pendingBinding = { sessionId, sessionName, processId, processName };
    
    previousAxisValues.clear();
    for (const device of axisData) {
      previousAxisValues.set(device.device_handle, { ...device.axes });
    }
  }

  function cancelBinding() {
    isBindingMode = false;
    pendingBinding = null;
    previousAxisValues.clear();
  }

  function startButtonBinding(sessionId: string, sessionName: string, processId: number, processName: string) {
    isButtonBindingMode = true;
    pendingButtonBinding = { sessionId, sessionName, processId, processName };
    
    previousButtonStates.clear();
    for (const device of axisData) {
      previousButtonStates.set(device.device_handle, { ...device.buttons });
    }
  }

  function cancelButtonBinding() {
    isButtonBindingMode = false;
    pendingButtonBinding = null;
    previousButtonStates.clear();
  }

  function detectAxisMovement(): { deviceHandle: string; deviceName: string; axisName: string } | null {
    for (const device of axisData) {
      const previousValues = previousAxisValues.get(device.device_handle);
      if (!previousValues) {
        // Device appeared after binding started (hot-plugged); seed a baseline
        // so movement can be detected on the next poll tick.
        previousAxisValues.set(device.device_handle, { ...device.axes });
        continue;
      }

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

  function detectButtonPress(): { deviceHandle: string; deviceName: string; buttonName: string } | null {
    for (const device of axisData) {
      const previousStates = previousButtonStates.get(device.device_handle);
      if (!previousStates) {
        // Device appeared after binding started (hot-plugged); seed a baseline
        // so button presses can be detected on the next poll tick.
        previousButtonStates.set(device.device_handle, { ...device.buttons });
        continue;
      }

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

  function createMapping(deviceHandle: string, deviceName: string, axisName: string, sessionId: string, sessionName: string, processId: number, processName: string) {
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    
    const newMapping: AxisMapping = { deviceHandle, deviceName, axisName, sessionId, sessionName, processId, processName, inverted: false };
    axisMappings = [...axisMappings, newMapping];
    
    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    
    saveMappings();
  }

  function toggleAxisInversion(processName: string) {
    const mapping = axisMappings.find(m => m.processName === processName);
    if (!mapping) return;

    const inverted = !mapping.inverted;

    // Reverse belongs to the sim function, not to one application: channels
    // sharing a function share a level, so they have to read their axes the
    // same way round or they would fight each other on every movement.
    const def = simFunctionByProcess.get(processName);
    const sharing = new Set<string>([processName]);
    for (const session of (def && sessionsBySimFunction.get(def)) ?? []) {
      sharing.add(session.process_name);
    }

    for (const m of axisMappings) {
      if (sharing.has(m.processName)) m.inverted = inverted;
    }
    axisMappings = [...axisMappings];
    saveMappings();

    // Reversing flips this channel's current level, and that flip travels
    // exactly as far as dragging the slider there would: to Windows, to the
    // sim function's LVar, and to any application sharing that function.
    // Inversion itself is purely a hardware-axis property from here on; it does
    // not sit between the channel and its LVar, so the two stay in step.
    const session = audioSessions.find(s => s.process_name === processName);
    if (!session || session.session_id.startsWith('inactive_')) return;

    const mirrored = 1 - session.volume;
    setSessionVolumeImmediate(session.session_id, mirrored);
    void setSessionVolumeFinal(session.session_id, mirrored);
  }

  function removeMapping(processName: string) {
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    saveMappings();
  }

  function createButtonMapping(deviceHandle: string, deviceName: string, buttonName: string, sessionId: string, sessionName: string, processId: number, processName: string) {
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    
    const newMapping: ButtonMapping = { deviceHandle, deviceName, buttonName, sessionId, sessionName, processId, processName };
    buttonMappings = [...buttonMappings, newMapping];
    
    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    
    saveButtonMappings();
  }

  function removeButtonMapping(processName: string) {
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    saveButtonMappings();
  }

  function removeApplication(processName: string) {
    axisMappings = axisMappings.filter(m => m.processName !== processName);

    buttonMappings = buttonMappings.filter(m => m.processName !== processName);

    if (simAssignments.some(a => a.processName === processName)) {
      simAssignments = simAssignments.filter(a => a.processName !== processName);
      saveSimAssignments();
    }

    const newPinnedApps = new Set(pinnedApps);
    newPinnedApps.delete(processName);
    pinnedApps = newPinnedApps;
    savePinnedApps();

    if (pinnedApps.size === 0) {
      isEditMode = false;
    }

    const sessionsToClean = audioSessions.filter(s => s.process_name === processName);
    for (const session of sessionsToClean) {
      animatingSliders.delete(session.session_id);
      manuallyControlledSessions.delete(session.session_id);
      cancelVolumeAnimation(session.session_id);
      cancelMuteAnimation(session.session_id);
    }

    saveMappings();
    saveButtonMappings();
  }

  /** Assign (or clear) an application's simulator function category */
  function handleSetSimCategory(e: CustomEvent<{ processName: string; category: SimFunctionCategory | null }>) {
    const { processName, category } = e.detail;
    // A category may be shared by any number of applications, so only this
    // application's own assignment is replaced.
    simAssignments = simAssignments.filter(a => a.processName !== processName);
    if (category) {
      simAssignments = [...simAssignments, { processName, category }];
    }
    saveSimAssignments();
  }

  async function applyAxisMappings() {
    if (isBindingMode && pendingBinding) {
      const movement = detectAxisMovement();
      if (movement) {
        createMapping(
          movement.deviceHandle,
          movement.deviceName,
          movement.axisName,
          pendingBinding.sessionId,
          pendingBinding.sessionName,
          pendingBinding.processId,
          pendingBinding.processName
        );
        isBindingMode = false;
        pendingBinding = null;
      }
      return;
    }

    if (!audioInitialised || axisMappings.length === 0) return;

    for (const mapping of axisMappings) {
      const device = axisData.find(d => d.device_handle === mapping.deviceHandle);
      if (device && device.axes[mapping.axisName] !== undefined) {
        let axisValue = device.axes[mapping.axisName];

        if (mapping.inverted) {
          axisValue = 1.0 - axisValue;
        }

        const deadzoneThreshold = 0.01;
        if (axisValue < deadzoneThreshold) {
          axisValue = 0.0;
        } else if (axisValue > (1.0 - deadzoneThreshold)) {
          axisValue = 1.0;
        }

        const mappingKey = `${mapping.deviceHandle}-${mapping.axisName}-${mapping.processName}`;
        const lastHardwareValue = lastHardwareAxisValues.get(mappingKey);
        const isActivated = axisActivated.get(mappingKey);

        // First time seeing this axis - store initial position but don't apply
        if (lastHardwareValue === undefined) {
          lastHardwareAxisValues.set(mappingKey, axisValue);
          axisActivated.set(mappingKey, false);
          continue;
        }

        // Axis not yet activated - check for significant user movement (>5% change)
        if (!isActivated) {
          const movement = Math.abs(axisValue - lastHardwareValue);
          if (movement > 0.05) {
            // User has moved the axis - activate it and apply
            axisActivated.set(mappingKey, true);
          } else {
            // Not enough movement yet - don't apply
            continue;
          }
        }

        // Apply axis value if it has changed and axis is activated
        if (Math.abs(lastHardwareValue - axisValue) > 0.01) {
          const session = audioSessions.find(s => s.process_name === mapping.processName);

          // A drag outranks the hardware axis: on this channel, or on any
          // channel sharing its sim function.
          if (session && !isHeldByPointer(session.session_id)) {
            // Routed through the same throttled write a drag uses, rather than
            // calling Windows directly. A direct write leaves no trace in
            // liveVolumeState, so isLocallyDriven cannot recognise the audio
            // push it causes: the push is read as an external change and starts
            // a second animation that fights the interpolation below, both of
            // them reading and writing the same session.volume.
            scheduleLiveVolumeUpdate(session.session_id, axisValue);

            // Auto-mute at the bottom of the axis is handled once the
            // interpolation settles, in setSessionVolumeImmediate.
            startHardwareVolumeInterpolation(session.session_id, axisValue);
            lastHardwareAxisValues.set(mappingKey, axisValue);
          }
        }
      }
    }
  }

  async function applyButtonMappings() {
    if (isButtonBindingMode && pendingButtonBinding) {
      const buttonPress = detectButtonPress();
      if (buttonPress) {
        createButtonMapping(
          buttonPress.deviceHandle, 
          buttonPress.deviceName, 
          buttonPress.buttonName, 
          pendingButtonBinding.sessionId, 
          pendingButtonBinding.sessionName,
          pendingButtonBinding.processId,
          pendingButtonBinding.processName
        );
        isButtonBindingMode = false;
        pendingButtonBinding = null;
      }
      for (const device of axisData) {
        previousButtonStates.set(device.device_handle, { ...device.buttons });
      }
      return;
    }

    if (!audioInitialised) return;

    const activeHandles = new Set(axisData.map(d => d.device_handle));

    if (buttonMappings.length > 0) {
      for (const mapping of buttonMappings) {
        const device = axisData.find(d => d.device_handle === mapping.deviceHandle);
        if (device && device.buttons[mapping.buttonName] !== undefined) {
          const currentState = device.buttons[mapping.buttonName];
          const previousState = previousButtonStates.get(device.device_handle)?.[mapping.buttonName];
          
          if (!previousState && currentState) {
            const session = audioSessions.find(s => s.process_name === mapping.processName);
            // Same rule as the axis path below: a pointer gesture owns the
            // channel, and the sim function it belongs to.
            if (session && !isHeldByPointer(session.session_id)) {
              const newMuteState = !session.is_muted;
              setSessionMute(session.session_id, newMuteState);
            }
          }
        }
      }
    }

    for (const device of axisData) {
      previousButtonStates.set(device.device_handle, { ...device.buttons });
    }

    for (const handle of Array.from(previousButtonStates.keys())) {
      if (!activeHandles.has(handle)) {
        previousButtonStates.delete(handle);
      }
    }

    buttonCachePruneCounter += 1;
    if (buttonCachePruneCounter > 1000000) {
      buttonCachePruneCounter = 0;
    }
    if (buttonCachePruneCounter % BUTTON_CACHE_LOG_INTERVAL === 0) {
      console.debug(`[ClearComms] Button state cache size ${previousButtonStates.size}; active handles ${activeHandles.size}`);
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // CLEANUP
  // ─────────────────────────────────────────────────────────────────────────────

  function cleanupAllAnimations() {
    for (const [sessionId] of animationSignals) {
      cancelVolumeAnimation(sessionId);
    }
    animationSignals.clear();
    animatingSliders.clear();

    for (const [sessionId, frameId] of hardwareVolumeAnimations) {
      cancelAnimationFrame(frameId);
    }
    hardwareVolumeAnimations.clear();
    hardwareVolumeTargets.clear();

  }
  
  function cleanupAllLiveVolumeStates() {
    for (const [sessionId] of liveVolumeState) {
      clearLiveVolumeState(sessionId);
    }
    liveVolumeState.clear();
  }
  
  function cleanupAllCaches() {
    previousAxisValues.clear();
    previousButtonStates.clear();
    lastHardwareAxisValues.clear();
    axisActivated.clear();
    manuallyControlledSessions.clear();
    for (const timerId of manualControlReleaseTimers.values()) clearTimeout(timerId);
    manualControlReleaseTimers.clear();
    manualControlClaims.clear();
    hardwareVolumeTargets.clear();
    hardwareVolumeAnimations.clear();
    muteAnimationFrames.clear();
    memorySnapshots = [];
    axisData = [];
    audioSessions = [];
    axisMappings = [];
    buttonMappings = [];
    
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // PERSISTENCE
  // ─────────────────────────────────────────────────────────────────────────────

  const PERSIST_KEYS = {
    axisMappings: 'clearcomms_axis_mappings',
    buttonMappings: 'clearcomms_button_mappings',
    pinnedApps: 'clearcomms_pinned_apps',
    appFriendlyNames: 'clearcomms_app_friendly_names',
    simAssignments: 'clearcomms_sim_assignments'
  } as const;

  async function saveConfigValue(key: string, value: unknown) {
    await invoke('save_config_value', { key, value });
  }

  async function loadConfigValue<T>(key: string): Promise<T | null> {
    const value = await invoke<T | null>('load_config_value', { key });
    return value;
  }

  function saveMappings() {
    void saveConfigValue(PERSIST_KEYS.axisMappings, axisMappings).catch((error) => {
      console.error("Error saving mappings:", error);
    });
  }

  async function loadMappings() {
    try {
      const saved = await loadConfigValue<AxisMapping[]>(PERSIST_KEYS.axisMappings);
      if (saved) {
        axisMappings = saved;
      }
    } catch (error) {
      console.error("Error loading mappings:", error);
    }
  }

  function saveButtonMappings() {
    void saveConfigValue(PERSIST_KEYS.buttonMappings, buttonMappings).catch((error) => {
      console.error("Error saving button mappings:", error);
    });
  }

  async function loadButtonMappings() {
    try {
      const saved = await loadConfigValue<ButtonMapping[]>(PERSIST_KEYS.buttonMappings);
      if (saved) {
        buttonMappings = saved;
      }
    } catch (error) {
      console.error("Error loading button mappings:", error);
    }
  }

  function savePinnedApps() {
    void saveConfigValue(PERSIST_KEYS.pinnedApps, [...pinnedApps]).catch((error) => {
      console.error("Error saving pinned apps:", error);
    });
  }

  async function loadPinnedApps() {
    try {
      const saved = await loadConfigValue<string[]>(PERSIST_KEYS.pinnedApps);
      if (saved) {
        pinnedApps = new Set(saved);
      }
    } catch (error) {
      console.error("Error loading pinned apps:", error);
    } finally {
      pinnedAppsLoaded = true;
    }
  }

  function saveAppFriendlyNames() {
    const obj: Record<string, string> = {};
    appFriendlyNames.forEach((value, key) => {
      obj[key] = value;
    });

    void saveConfigValue(PERSIST_KEYS.appFriendlyNames, obj).catch((error) => {
      console.error("Error saving app friendly names:", error);
    });
  }

  function saveSimAssignments() {
    void saveConfigValue(PERSIST_KEYS.simAssignments, simAssignments).catch((error) => {
      console.error("Error saving sim assignments:", error);
    });
  }

  async function loadSimAssignments() {
    try {
      const saved = await loadConfigValue<SimFunctionAssignment[]>(PERSIST_KEYS.simAssignments);
      if (saved) {
        simAssignments = saved;
      }
    } catch (error) {
      console.error("Error loading sim assignments:", error);
    }
  }

  async function loadAppFriendlyNames() {
    try {
      const saved = await loadConfigValue<Record<string, string>>(PERSIST_KEYS.appFriendlyNames);
      if (saved) {
        appFriendlyNames = new Map(Object.entries(saved));
      }
    } catch (error) {
      console.error("Error loading app friendly names:", error);
    }
  }

  async function fetchWindowPinnedState() {
    try {
      windowPinned = await invoke<boolean>('is_window_pinned');
    } catch (error) {
      console.error("Error fetching window pin state:", error);
    }
  }

  async function toggleWindowPinned() {
    try {
      const newState = await invoke<boolean>('toggle_pin_window');
      windowPinned = newState;
    } catch (error) {
      console.error("Error toggling window pin:", error);
    }
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // MANUAL CONTROL CLAIMS
  // ─────────────────────────────────────────────────────────────────────────────
  //
  // A session under local control ignores both inbound cockpit state and the
  // Windows audio push, so our own writes cannot come back as "external"
  // changes and fight the slider. A drag brackets this naturally with
  // pointerdown/pointerup, but the wheel has no gesture end: so it holds the
  // claim for a short settle window after the settled value is sent, long
  // enough for the LVar echo (~50 ms per hop) and the audio push to arrive.

  const MANUAL_CONTROL_SETTLE_MS = 300;

  /**
   * Longest a claim survives without further input. A drag refreshes it on
   * every move, so it only ever expires when the releasing event failed to
   * arrive: a cancelled pointer, or an awaited command that never settles.
   * Without it, one missed release blocks inbound sync for that channel for
   * the rest of the session with nothing able to recover it.
   */
  const MANUAL_CONTROL_MAX_HOLD_MS = 5000;

  const manualControlReleaseTimers = new Map<string, number>();
  /** sessionId → id of the most recent claim, so a superseded release is a no-op */
  const manualControlClaims = new Map<string, number>();
  let manualControlClaimSeq = 0;

  /**
   * Schedule a claim to lapse after `delayMs`, replacing any pending expiry.
   * Every claim always has one of these armed, so control is never held
   * indefinitely no matter which events go missing.
   */
  function armManualControlExpiry(sessionId: string, claimId: number, delayMs: number) {
    const existing = manualControlReleaseTimers.get(sessionId);
    if (existing !== undefined) clearTimeout(existing);

    manualControlReleaseTimers.set(sessionId, window.setTimeout(() => {
      manualControlReleaseTimers.delete(sessionId);
      if (manualControlClaims.get(sessionId) !== claimId) return;
      manualControlClaims.delete(sessionId);
      manuallyControlledSessions.delete(sessionId);
    }, delayMs));
  }

  /**
   * Take local control of a session, cancelling any pending expiry.
   *
   * `held` marks a gesture with a real end: a pointer press, which finishes on
   * pointerup, pointercancel or lost capture. Those arm no watchdog at all:
   * holding a slider still is a legitimate thing to do, produces no events
   * while it lasts, and must not quietly hand the channel back to the
   * simulator. Everything else (the wheel) has no end event, so it keeps the
   * watchdog and lapses on its own.
   */
  function claimManualControl(sessionId: string, held: boolean = false): number {
    const claimId = ++manualControlClaimSeq;
    manualControlClaims.set(sessionId, claimId);
    manuallyControlledSessions.add(sessionId);

    if (held) {
      const existing = manualControlReleaseTimers.get(sessionId);
      if (existing !== undefined) clearTimeout(existing);
      manualControlReleaseTimers.delete(sessionId);
    } else {
      armManualControlExpiry(sessionId, claimId, MANUAL_CONTROL_MAX_HOLD_MS);
    }

    return claimId;
  }

  /**
   * Release a claim once the settle window passes. A claim that has since been
   * superseded: by the next notch of a rapid scroll, or by a drag starting
   * before the previous gesture settled: releases nothing, so control is held
   * continuously across a burst of input instead of flickering between notches.
   */
  function releaseManualControl(sessionId: string, claimId: number) {
    if (manualControlClaims.get(sessionId) !== claimId) return;
    armManualControlExpiry(sessionId, claimId, MANUAL_CONTROL_SETTLE_MS);
  }

  /** The claim covering an in-progress gesture, starting one if none is held. */
  function currentManualControl(sessionId: string): number {
    return manualControlClaims.get(sessionId) ?? claimManualControl(sessionId);
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // EVENT HANDLERS (from components)
  // ─────────────────────────────────────────────────────────────────────────────

  function handleVolumeDragStart(e: CustomEvent<{ sessionId: string }>) {
    const { sessionId } = e.detail;
    animatingSliders.delete(sessionId);
    claimManualControl(sessionId, true);
    cancelVolumeAnimation(sessionId);
    clearLiveVolumeState(sessionId);
  }

  function handleVolumeDragMove(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;
    cancelVolumeAnimation(sessionId);
    setSessionVolumeImmediate(sessionId, volume);
    scheduleLiveVolumeUpdate(sessionId, volume);
  }

  async function handleVolumeDragEnd(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;
    const claimId = currentManualControl(sessionId);
    try {
      await setSessionVolumeFinal(sessionId, volume);
    } finally {
      clearLiveVolumeState(sessionId);
      releaseManualControl(sessionId, claimId);
    }
  }

  /**
   * A press on the slider that neither dragged nor moved the value. dragstart
   * already claimed local control, so release it without writing anything.
   */
  function handleVolumeDragCancel(e: CustomEvent<{ sessionId: string }>) {
    const { sessionId } = e.detail;
    clearLiveVolumeState(sessionId);
    const claimId = manualControlClaims.get(sessionId);
    if (claimId !== undefined) releaseManualControl(sessionId, claimId);
  }

  async function handleVolumeTrackClick(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;

    // The hold belongs to the pointer gesture: pointerdown claimed it and
    // pointerup releases it. This animation merely runs inside that gesture:
    // the button may still be down, and the user may yet drag away before the
    // thumb arrives: so it deliberately releases nothing itself.
    const completed = await animateVolumeTo(sessionId, volume, 250);
    if (completed) {
      await setSessionVolumeFinal(sessionId, volume);
    }
  }

  async function handleVolumeWheel(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;
    // The wheel has no pointerdown to claim control for it, so claim here: or
    // the LVar echo and the Windows push both read this as an external change.
    const claimId = claimManualControl(sessionId);
    try {
      const completed = await animateVolumeTo(sessionId, volume, 150);
      if (completed) {
        await setSessionVolumeFinal(sessionId, volume);
      }
    } finally {
      releaseManualControl(sessionId, claimId);
    }
  }

  function handleMuteToggle(e: CustomEvent<{ sessionId: string; muted: boolean }>) {
    const { sessionId, muted } = e.detail;
    setSessionMute(sessionId, muted);
  }

  function handleStartAxisBinding(e: CustomEvent<{ session: AudioSession }>) {
    const { session } = e.detail;
    startAxisBinding(session.session_id, applyDisplayNameOverride(session.display_name, session.process_name), session.process_id, session.process_name);
  }

  function handleStartButtonBinding(e: CustomEvent<{ session: AudioSession }>) {
    const { session } = e.detail;
    startButtonBinding(session.session_id, applyDisplayNameOverride(session.display_name, session.process_name), session.process_id, session.process_name);
  }

  function handleRemoveAxisMapping(e: CustomEvent<{ processName: string }>) {
    removeMapping(e.detail.processName);
  }

  function handleRemoveButtonMapping(e: CustomEvent<{ processName: string }>) {
    removeButtonMapping(e.detail.processName);
  }

  function handleToggleInversion(e: CustomEvent<{ processName: string }>) {
    toggleAxisInversion(e.detail.processName);
  }

  function handleRemoveApplication(e: CustomEvent<{ processName: string }>) {
    removeApplication(e.detail.processName);
  }

  async function handleSelectApp(e: CustomEvent<{ processName: string }>) {
    const { processName } = e.detail;

    // For system volume, fetch the real volume/mute state before pinning
    // so the UI never flashes a 100% placeholder
    if (processName === SYSTEM_VOLUME_PROCESS_NAME) {
      try {
        const [systemVolume, systemMuted] = await Promise.all([
          invoke<number>("get_system_volume"),
          invoke<boolean>("get_system_mute")
        ]);
        const systemSession: AudioSession = {
          session_id: SYSTEM_VOLUME_ID,
          display_name: SYSTEM_VOLUME_DISPLAY_NAME,
          process_id: 0,
          process_name: SYSTEM_VOLUME_PROCESS_NAME,
          volume: systemVolume,
          is_muted: systemMuted
        };
        // Inject (or replace) the system session so getBoundSessions() uses the real values
        const existingIndex = audioSessions.findIndex(s => s.session_id === SYSTEM_VOLUME_ID);
        if (existingIndex !== -1) {
          audioSessions[existingIndex] = systemSession;
        } else {
          audioSessions = [...audioSessions, systemSession];
        }
      } catch (err) {
        console.error("Failed to pre-fetch system volume:", err);
      }
    }

    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    addAppListExpanded = false;
    addAppComponentKey += 1; // Force ButtonAddApplication to recreate
    if (!isEditMode) {
      isEditMode = true;
    }
  }
</script>

<svelte:window on:keydown={handleGlobalTab} />

{#if initStatus === 'Ready'}
  <!-- Main Application -->
  <main role="application" aria-label="ClearComms">
    {#if errorMsg}
      <div class="error-banner" role="alert" aria-live="assertive">{errorMsg}</div>
    {/if}

    {#if audioInitialised}
      {@const boundSessions = getBoundSessions()}
      {@const availableSessions = getAvailableSessions()}

      <div class="mixer-dock-wrapper" class:settings-open={settingsMenuExpanded}>
        <Mixer
          {boundSessions}
          {availableSessions}
          axisMappings={axisMappings}
          buttonMappings={buttonMappings}
          {simAssignments}
          {supportedSimCategories}
          {isEditMode}
          isBindingMode={isBindingMode}
          isButtonBindingMode={isButtonBindingMode}
          pendingBinding={pendingBinding}
          pendingButtonBinding={pendingButtonBinding}
          bind:addAppListExpanded
          {addAppComponentKey}
          settingsOpen={settingsMenuExpanded}
          on:volumedragstart={handleVolumeDragStart}
          on:volumedragmove={handleVolumeDragMove}
          on:volumedragend={handleVolumeDragEnd}
          on:volumetrackclick={handleVolumeTrackClick}
          on:volumedragcancel={handleVolumeDragCancel}
          on:volumewheel={handleVolumeWheel}
          on:mutetoggle={handleMuteToggle}
          on:startaxisbinding={handleStartAxisBinding}
          on:startbuttonbinding={handleStartButtonBinding}
          on:cancelaxisbinding={cancelBinding}
          on:cancelbuttonbinding={cancelButtonBinding}
          on:removeaxismapping={handleRemoveAxisMapping}
          on:removebuttonmapping={handleRemoveButtonMapping}
          on:toggleinversion={handleToggleInversion}
          on:removeapplication={handleRemoveApplication}
          on:setsimcategory={handleSetSimCategory}
          on:select={handleSelectApp}
        />

        {#if pinnedApps.size > 0}
          <Dock
            bind:dockOpen
            bind:settingsMenuExpanded
            bind:closeMenuExpanded
            {isEditMode}
            {audioInitialised}
            {windowPinned}
            on:toggleeditmode={toggleEditMode}
            on:togglewindowpinned={toggleWindowPinned}
          />
        {/if}
      </div>
    {:else}
      <p class="status-text">Initialising...</p>
    {/if}

    <Footer />

  </main>
{:else}
  <BootScreen status={initStatus} errorMessage={errorMsg} />
{/if}

<style>
  :global(body) {
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
  }

  * {
    box-sizing: border-box;
  }

  main {
    display: flex;
    gap: 1rem;
    flex-direction: column;
    height: 100vh;
    max-height: 100vh;
    width: 100vw;
    justify-content: space-between;
    overflow: hidden;
    box-sizing: border-box;
    padding: 50px 0rem 1rem 0rem;
    position: relative;
  }

  .mixer-dock-wrapper {
    display: flex;
    justify-content: space-between;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    gap: 1rem;
    transition: gap 0.3s ease;
  }

  .mixer-dock-wrapper.settings-open {
    gap: 0;
  }

  .status-text,
  .error-banner {
    z-index: 2;
  }

  .error-banner {
    padding: 10px 14px;
    margin-bottom: 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    color: var(--text-primary);
    font-size: 0.85rem;
    font-weight: 500;
  }


  .status-text {
    text-align: center;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-secondary);
    font-size: 0.9rem;
    height: 100%;
  }
</style>
