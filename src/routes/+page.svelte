<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import type { 
    AudioSession, 
    AxisMapping, 
    ButtonMapping, 
    AxisData, 
    PendingBinding, 
    PendingButtonBinding,
    LiveVolumeState,
    AnimationSignal
  } from "$lib/types";
  import { 
    Mixer,
    Dock, 
    BootScreen, 
    Footer
  } from "$lib/components";
  import { formatProcessName, applyDisplayNameOverride, SYSTEM_VOLUME_ID, SYSTEM_VOLUME_PROCESS_NAME, SYSTEM_VOLUME_DISPLAY_NAME, isSystemVolume } from "$lib/stores/audioStore";


  // ─────────────────────────────────────────────────────────────────────────────
  // DEBUG CONFIGURATION - Set these to preview different UI states
  // ─────────────────────────────────────────────────────────────────────────────
  
  const DEBUG = {
    ENABLED: false, // Global debugging toggle
    FORCE_BOOT_SCREEN: false, // Force application to hang on boot screen
    FORCE_BOOT_ERROR: false, // Force application to display boot error with restart button
    FORCE_AUDIO_NOT_INITIALISED: false, // Force application to behave as if audio subsystem failed to initialise
    FORCE_EDIT_MODE: false, // Force application to start in edit mode
    FORCE_NO_SESSIONS: false, // Force application to start with no audio sessions
    FORCE_ERROR_BANNER: false, // Force application to display error banner
    ERROR_BANNER_TEXT: "Critical error", // Text to display in error banner
    FORCE_WARNING_BANNER: false, // Force application to display warning banner
    WARNING_BANNER_TEXT: "Update available", // Text to display in warning banner
    FORCE_MOCK_SESSIONS: false, // Force application to use mock audio sessions for testing UI without actual audio subsystem (overrides FORCE_NO_SESSIONS)
    MOCK_SESSIONS: [
      { session_id: "mock_1", display_name: "Discord", process_id: 1234, process_name: "discord.exe", volume: 0.75, is_muted: false },
      { session_id: "mock_2", display_name: "Spotify", process_id: 5678, process_name: "spotify.exe", volume: 0.50, is_muted: false },
      { session_id: "mock_3", display_name: "Microsoft Flight Simulator", process_id: 9012, process_name: "flightsim.exe", volume: 1.0, is_muted: true },
      { session_id: "mock_4", display_name: "Google Chrome", process_id: 3456, process_name: "chrome.exe", volume: 0.25, is_muted: false },
      { session_id: "mock_5", display_name: "System Sounds", process_id: 0, process_name: SYSTEM_VOLUME_PROCESS_NAME, volume: 0.80, is_muted: false },
      { session_id: "mock_6", display_name: "Game Launcher", process_id: 7890, process_name: "launcher1.exe", volume: 0.60, is_muted: false },
      { session_id: "mock_7", display_name: "Video Player", process_id: 2345, process_name: "videoplayer1.exe", volume: 0.90, is_muted: false },
      { session_id: "mock_8", display_name: "Communication App", process_id: 6789, process_name: "commapp1.exe", volume: 0.40, is_muted: true },
      { session_id: "mock_9", display_name: "Music Player", process_id: 1357, process_name: "musicplayer1.exe", volume: 0.55, is_muted: false },
      { session_id: "mock_10", display_name: "Video Conferencing", process_id: 2468, process_name: "videoconf1.exe", volume: 0.65, is_muted: false },
      { session_id: "mock_11", display_name: "Audio Editor", process_id: 1122, process_name: "audioeditor1.exe", volume: 0.85, is_muted: false },
      { session_id: "mock_12", display_name: "Streaming Software", process_id: 3344, process_name: "streamer1.exe", volume: 0.70, is_muted: false },
      { session_id: "mock_13", display_name: "Virtual Machine", process_id: 5566, process_name: "vm1.exe", volume: 0.30, is_muted: true },
      { session_id: "mock_14", display_name: "Browser Music Tab", process_id: 7788, process_name: "chrome1.exe", volume: 0.45, is_muted: false },
      { session_id: "mock_15", display_name: "Game Soundtrack", process_id: 9900, process_name: "gamesoundtrack1.exe", volume: 0.95, is_muted: false },
      { session_id: "mock_16", display_name: "Podcast App", process_id: 2233, process_name: "podcastapp1.exe", volume: 0.35, is_muted: false },
      { session_id: "mock_17", display_name: "Voice Chat", process_id: 4455, process_name: "voicechat1.exe", volume: 0.50, is_muted: true },
      { session_id: "mock_18", display_name: "System Notifications", process_id: 0, process_name: "sysnotif1.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_19", display_name: "Media Player", process_id: 6677, process_name: "mediaplayer1.exe", volume: 0.60, is_muted: false },
      { session_id: "mock_20", display_name: "Audio Books", process_id: 8899, process_name: "audiobooks1.exe", volume: 0.40, is_muted: true },
      { session_id: "mock_21", display_name: "System Volume 2", process_id: 0, process_name: "sysvolume2.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_22", display_name: "Background Music", process_id: 1010, process_name: "bgmusic1.exe", volume: 0.20, is_muted: false },
      { session_id: "mock_23", display_name: "Game Chat", process_id: 2020, process_name: "gamechat1.exe", volume: 0.75, is_muted: true },
      { session_id: "mock_24", display_name: "Video Call", process_id: 3030, process_name: "videocall1.exe", volume: 0.65, is_muted: false },
      { session_id: "mock_25", display_name: "Music Streaming", process_id: 4040, process_name: "musicstreaming1.exe", volume: 0.55, is_muted: false },
      { session_id: "mock_26", display_name: "Voice Assistant", process_id: 5050, process_name: "assistant1.exe", volume: 0.45, is_muted: true },
      { session_id: "mock_27", display_name: "System Sounds 2", process_id: 0, process_name: "sysnotif2.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_28", display_name: "Video Streaming", process_id: 6060, process_name: "videostream1.exe", volume: 0.85, is_muted: false },
      { session_id: "mock_29", display_name: "Audio Mixer", process_id: 7070, process_name: "audiomixer1.exe", volume: 0.70, is_muted: false },
      { session_id: "mock_30", display_name: "Game Audio", process_id: 8080, process_name: "gameaudio1.exe", volume: 0.90, is_muted: true },
      { session_id: "mock_31", display_name: "System Volume 3", process_id: 0, process_name: "sysvolume3.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_32", display_name: "Music Player 2", process_id: 9090, process_name: "musicplayer2.exe", volume: 0.60, is_muted: false },
      { session_id: "mock_33", display_name: "Video Editor", process_id: 1111, process_name: "videoeditor1.exe", volume: 0.75, is_muted: false },
      { session_id: "mock_34", display_name: "Communication App 2", process_id: 2222, process_name: "commapp2.exe", volume: 0.50, is_muted: true },
      { session_id: "mock_35", display_name: "System Sounds 3", process_id: 0, process_name: "sysnotif3.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_36", display_name: "Game Launcher 2", process_id: 3333, process_name: "launcher2.exe", volume: 0.65, is_muted: false },
      { session_id: "mock_37", display_name: "Video Player 2", process_id: 4444, process_name: "videoplayer2.exe", volume: 0.85, is_muted: false },
      { session_id: "mock_38", display_name: "Voice Chat 2", process_id: 5555, process_name: "voicechat2.exe", volume: 0.40, is_muted: true },
      { session_id: "mock_39", display_name: "Music Player 3", process_id: 6666, process_name: "musicplayer3.exe", volume: 0.55, is_muted: false },
      { session_id: "mock_40", display_name: "Video Conferencing 2", process_id: 7777, process_name: "videoconf2.exe", volume: 0.70, is_muted: false },
      { session_id: "mock_41", display_name: "Audio Editor 2", process_id: 8888, process_name: "audioeditor2.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_42", display_name: "Streaming Software 2", process_id: 9999, process_name: "streamer2.exe", volume: 0.75, is_muted: false },
      { session_id: "mock_43", display_name: "Virtual Machine 2", process_id: 1212, process_name: "vm2.exe", volume: 0.30, is_muted: true },
      { session_id: "mock_44", display_name: "Browser Music Tab 2", process_id: 1313, process_name: "chrome2.exe", volume: 0.45, is_muted: false },
      { session_id: "mock_45", display_name: "Game Soundtrack 2", process_id: 1414, process_name: "gamesoundtrack2.exe", volume: 0.95, is_muted: false },
      { session_id: "mock_46", display_name: "Podcast App 2", process_id: 1515, process_name: "podcastapp2.exe", volume: 0.35, is_muted: false },
      { session_id: "mock_47", display_name: "Voice Chat 3", process_id: 1616, process_name: "voicechat3.exe", volume: 0.50, is_muted: true },
      { session_id: "mock_48", display_name: "System Notifications 2", process_id: 0, process_name: "sysnotif4.exe", volume: 0.80, is_muted: false },
      { session_id: "mock_49", display_name: "Media Player 2", process_id: 1717, process_name: "mediaplayer2.exe", volume: 0.60, is_muted: false },
      { session_id: "mock_50", display_name: "Audio Books 2", process_id: 1818, process_name: "audiobooks2.exe", volume: 0.40, is_muted: true }
    ] as AudioSession[],
  };

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
  let audioMonitorInterval: number | null = null;
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
  
  // Menu expansion states
  let addAppListExpanded = $state(false);
  let settingsMenuExpanded = $state(false);
  let closeMenuExpanded = $state(false);
  let dockOpen = $state(false);
  let addAppComponentKey = $state(0);

  // ─────────────────────────────────────────────────────────────────────────────
  // DERIVED STATE
  // ─────────────────────────────────────────────────────────────────────────────

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
    if (DEBUG.ENABLED) {
      // Debug mode: apply overrides without backend initialisation
      applyDebugOverrides();
      return;
    }

    void (async () => {
      await Promise.all([
        loadMappings(),
        loadButtonMappings(),
        loadPinnedApps(),
        loadAppFriendlyNames()
      ]);

      await fetchWindowPinnedState();
      await autoInitialise();

      // Measure layout dimensions once on mount
      // This ensures the backend knows the actual rendered widths for all DPI scales
      measureLayoutDimensions();
    })();

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
    // Boot screen overrides — these prevent the app from reaching the main UI
    if (DEBUG.FORCE_BOOT_ERROR) {
      initStatus = "Failed";
      errorMsg = "Debug: Forced boot error";
      return;
    }
    
    if (DEBUG.FORCE_BOOT_SCREEN) {
      // initStatus remains "Initialising..." — app stays on boot screen
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
      pinnedApps = new Set(DEBUG.MOCK_SESSIONS.map(s => s.process_name));
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
    cleanupAllAnimations();
    cleanupAllLiveVolumeStates();
    cleanupAllCaches();
    if (IS_DEV && typeof window !== 'undefined') {
      delete (window as any).clearCommsDebug;
    }
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // INITIALISATION & POLLING
  // ─────────────────────────────────────────────────────────────────────────────

  async function autoInitialise() {
    try {
      initStatus = "Initialising input system...";
      await invoke<string>("init_input");

      initStatus = "Enumerating devices...";
      await invoke<string[]>("enumerate_input_devices");

      initStatus = "Initialising audio manager...";
      try {
        await invoke<string>("init_audio_manager");
        audioInitialised = true;
        await refreshAudioSessions();
      } catch (audioError) {
        console.warn("Audio manager failed (non-critical):", audioError);
      }

      initStatus = "Starting real-time polling...";
      startPolling();

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

    // Listen for axis data events emitted by the dedicated Rust input thread
    listen<AxisData[]>('input-axis-data', (event) => {
      handleAxisData(event.payload);
    }).then((unlisten) => {
      unlistenInputAxis = unlisten;
    });
    
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
    isPolling = false;
    pollInFlight = false;
    stopAudioMonitoring();
    stopMemoryMonitoring();
    stopMemoryProfiler();
  }

  function startAudioMonitoring() {
    if (audioMonitorInterval) return;
    
    audioMonitorInterval = setInterval(async () => {
      try {
        const deviceChanged = await invoke<boolean>("check_default_device_changed");
        await refreshAudioSessions();
      } catch (error) {
        console.error("Audio monitoring error:", error);
      }
    }, 1000); // Poll every 1s for external audio changes (reduces COM object pressure)
  }

  function stopAudioMonitoring() {
    if (audioMonitorInterval) {
      clearInterval(audioMonitorInterval);
      audioMonitorInterval = null;
    }
  }

  async function refreshAudioSessions() {
    try {
      const sessions = await invoke<AudioSession[]>("get_audio_sessions");
      
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
          const existingIndex = audioSessions.findIndex(s => s.session_id === SYSTEM_VOLUME_ID);
          if (existingIndex !== -1) {
            const existing = audioSessions[existingIndex];
            
            if (manuallyControlledSessions.has(SYSTEM_VOLUME_ID)) {
              systemSession.volume = existing.volume;
              systemSession.is_muted = existing.is_muted;
            } else if (animatingSliders.has(SYSTEM_VOLUME_ID)) {
              systemSession.volume = existing.volume;
              systemSession.is_muted = existing.is_muted;
            } else {
              // Handle mute state transitions
              if (systemSession.is_muted && !existing.is_muted) {
                // Just muted externally — no volume animation needed, display derives to 0
              } else if (!systemSession.is_muted && existing.is_muted) {
                // Just unmuted externally — no volume animation needed, display derives from real volume
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
        const existingIndex = audioSessions.findIndex(s => s.session_id === newSession.session_id);
        
        if (existingIndex !== -1) {
          const existing = audioSessions[existingIndex];
          
          if (manuallyControlledSessions.has(newSession.session_id)) {
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
              // Just muted externally — no volume animation needed, display derives to 0
            } else if (!newSession.is_muted && existing.is_muted) {
              // Just unmuted externally — no volume animation needed, display derives from real volume
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
        animateVolumeTo(sessionId, change.to, 200);
      }
      
      cleanupStaleMappings();
    } catch (error) {
      console.error("Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  function cleanupStaleMappings() {
    // Intentionally kept empty - we preserve mappings for inactive apps
    return;
  }

  async function measureLayoutDimensions() {
    try {
      // Wait a tick to ensure elements are fully rendered
      await new Promise(resolve => setTimeout(resolve, 100));
      
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

  function setSessionVolumeImmediate(sessionId: string, volume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex !== -1) {
      audioSessions[sessionIndex].volume = volume;
      // Auto-unmute when volume is adjusted above 0 (e.g. user drags a muted slider)
      if (volume > 0 && audioSessions[sessionIndex].is_muted) {
        audioSessions[sessionIndex].is_muted = false;
        invokeSetMute(sessionId, false).catch(e => console.error("Error auto-unmuting:", e));
      }
    }
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

  async function animateVolumeTo(sessionId: string, targetVolume: number, durationMs: number = 200): Promise<boolean> {
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

        setSessionVolumeImmediate(sessionId, currentVolume);

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
        
        if (Math.abs(diff) < 0.001) {
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
      await refreshAudioSessions();
    } catch (error) {
      console.error("Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function setSessionMute(sessionId: string, muted: boolean) {
    if (sessionId.startsWith('inactive_')) return;

    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex === -1) return;
    const session = audioSessions[sessionIndex];

    // Cancel any ongoing volume animation (e.g. hardware input) before toggling mute
    cancelVolumeAnimation(sessionId);
    cancelMuteAnimation(sessionId);

    try {
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

  /** Purely visual animation for mute/unmute — only updates displayVolumeOverride, no Windows API calls */
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

        // Update only the display override — no Windows API call
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
    if (mapping) {
      mapping.inverted = !mapping.inverted;
      axisMappings = [...axisMappings];
      saveMappings();
    }
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

          if (session && !manuallyControlledSessions.has(session.session_id)) {
            try {
              await invokeSetVolume(session.session_id, axisValue);
              await invokeSetMute(session.session_id, axisValue === 0);
              startHardwareVolumeInterpolation(session.session_id, axisValue);
              lastHardwareAxisValues.set(mappingKey, axisValue);
            } catch (error) {
              console.error(`Error applying mapping for ${mapping.sessionName}:`, error);
            }
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
            if (session) {
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
    appFriendlyNames: 'clearcomms_app_friendly_names'
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
  // EVENT HANDLERS (from components)
  // ─────────────────────────────────────────────────────────────────────────────

  function handleVolumeDragStart(e: CustomEvent<{ sessionId: string }>) {
    const { sessionId } = e.detail;
    animatingSliders.delete(sessionId);
    manuallyControlledSessions.add(sessionId);
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
    await setSessionVolumeFinal(sessionId, volume);
    manuallyControlledSessions.delete(sessionId);
    clearLiveVolumeState(sessionId);
  }

  async function handleVolumeTrackClick(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;
    const completed = await animateVolumeTo(sessionId, volume, 250);
    if (completed) {
      await setSessionVolumeFinal(sessionId, volume);
      manuallyControlledSessions.delete(sessionId);
    }
  }

  async function handleVolumeWheel(e: CustomEvent<{ sessionId: string; volume: number }>) {
    const { sessionId, volume } = e.detail;
    const completed = await animateVolumeTo(sessionId, volume, 150);
    if (completed) {
      await setSessionVolumeFinal(sessionId, volume);
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
