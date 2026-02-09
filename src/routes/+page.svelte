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
    AnimationSignal,
    MemoryInfo 
  } from "$lib/types";
  import { 
    Mixer,
    Dock, 
    BootScreen, 
    Footer
  } from "$lib/components";
  import { formatProcessName, SYSTEM_VOLUME_ID, SYSTEM_VOLUME_PROCESS_NAME, SYSTEM_VOLUME_DISPLAY_NAME, isSystemVolume } from "$lib/stores/audioStore";

  console.log("[ClearComms] Component script loaded");

  // ─────────────────────────────────────────────────────────────────────────────
  // DEBUG CONFIGURATION - Set these to preview different UI states
  // ─────────────────────────────────────────────────────────────────────────────
  
  const DEBUG = {
    ENABLED: false,
    FORCE_BOOT_SCREEN: false,
    FORCE_BOOT_ERROR: false,
    FORCE_CLOSE_CONFIRMATION: false,
    FORCE_MAIN_APP: false,
    BOOT_STATUS_TEXT: "Initialising...",
    BOOT_ERROR_MESSAGE: "Failed to initialise audio subsystem: Device not found",
    FORCE_EDIT_MODE: false,
    FORCE_NO_SESSIONS: false,
    FORCE_ERROR_BANNER: false,
    ERROR_BANNER_TEXT: "Hardware device disconnected",
    FORCE_MOCK_SESSIONS: false,
    MOCK_SESSIONS: [
      { session_id: "mock_1", display_name: "Discord", process_id: 1234, process_name: "Discord.exe", volume: 0.75, is_muted: false },
      { session_id: "mock_2", display_name: "Spotify", process_id: 5678, process_name: "Spotify.exe", volume: 0.50, is_muted: false },
      { session_id: "mock_3", display_name: "Microsoft Flight Simulator", process_id: 9012, process_name: "FlightSimulator.exe", volume: 1.0, is_muted: true },
    ] as AudioSession[],
    FORCE_BUTTON_BINDING_MODE: false,
    FORCE_AUDIO_NOT_INITIALISED: false,
  };

  // ─────────────────────────────────────────────────────────────────────────────
  // STATE
  // ─────────────────────────────────────────────────────────────────────────────
  
  let axisData = $state<AxisData[]>([]);
  let audioSessions = $state<AudioSession[]>([]);
  let axisMappings = $state<AxisMapping[]>([]);
  let buttonMappings = $state<ButtonMapping[]>([]);
  let pinnedApps = $state<Set<string>>(new Set());
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
  let preMuteVolumes = $state<Map<string, number>>(new Map());
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
    console.log("[MemoryProfiler] Starting memory profiler (dev mode)");
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
    
    console.log(
      `[MemoryProfiler] Heap: ${formatBytes(snapshot.heapUsed)} / ${formatBytes(snapshot.heapTotal)} | ` +
      `Caches: axis=${previousAxisValues.size}, btn=${previousButtonStates.size}, hw=${lastHardwareAxisValues.size}, ` +
      `live=${liveVolumeState.size}, anim=${animatingSliders.size}`
    );
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
    console.group("[MemoryProfiler] Detailed Cache Statistics");
    console.log(`previousAxisValues: ${previousAxisValues.size} entries`);
    console.log(`previousButtonStates: ${previousButtonStates.size} entries`);
    console.log(`lastHardwareAxisValues: ${lastHardwareAxisValues.size} entries`);
    console.log(`axisActivated: ${axisActivated.size} entries`);
    console.log(`liveVolumeState: ${liveVolumeState.size} entries`);
    console.log(`hardwareVolumeTargets: ${hardwareVolumeTargets.size} entries`);
    console.log(`hardwareVolumeAnimations: ${hardwareVolumeAnimations.size} entries`);
    console.log(`animatingSliders: ${animatingSliders.size} entries`);
    console.log(`animationSignals: ${animationSignals.size} entries`);
    console.log(`manuallyControlledSessions: ${manuallyControlledSessions.size} entries`);
    console.log(`preMuteVolumes: ${preMuteVolumes.size} entries`);
    console.log(`audioSessions: ${audioSessions.length} entries`);
    console.log(`axisData: ${axisData.length} entries`);
    console.log(`axisMappings: ${axisMappings.length} entries`);
    console.log(`buttonMappings: ${buttonMappings.length} entries`);
    console.groupEnd();
  }
  
  if (IS_DEV && typeof window !== 'undefined') {
    (window as any).clearCommsDebug = {
      logMemory: logMemorySnapshot,
      logCaches: logDetailedCacheStats,
      getSnapshots: () => memorySnapshots,
      forceCleanup: () => {
        performPeriodicCleanup();
        console.log("[MemoryProfiler] Forced cleanup completed");
        logMemorySnapshot();
      },
      forceGC: () => {
        cleanupAllCaches();
        console.log("[MemoryProfiler] Caches cleared, GC should run soon");
        setTimeout(logMemorySnapshot, 1000);
      }
    };
    console.log("[MemoryProfiler] Debug functions available: window.clearCommsDebug.{logMemory, logCaches, getSnapshots, forceCleanup, forceGC}");
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
      displayCount += 1;
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

  function getAvailableSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName),
      ...pinnedApps
    ]);
    
    const sessions = audioSessions.filter(s => !boundProcessNames.has(s.process_name));
    
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
      console.log("[DEBUG] Debug mode enabled - applying overrides");
      applyDebugOverrides();
      return;
    }
    
    loadMappings();
    loadButtonMappings();
    loadPinnedApps();
    fetchWindowPinnedState();
    autoInitialise();
    
    // Measure layout dimensions once on mount
    // This ensures the backend knows the actual rendered widths for all DPI scales
    measureLayoutDimensions();

    // Listen for pin state changes from the backend (e.g., from context menu)
    let unlisten: (() => void) | null = null;
    listen('window-pin-changed', (event: { payload: boolean }) => {
      windowPinned = event.payload;
      console.log(`[Window] Pin state changed: ${windowPinned}`);
    }).then(fn => {
      unlisten = fn;
    }).catch(error => {
      console.error("Failed to set up pin state listener:", error);
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
      if (unlisten) {
        unlisten();
      }
    };
  });

  function applyDebugOverrides() {
    // Set boot screen state based on FORCE_* flags
    if (DEBUG.FORCE_BOOT_ERROR) {
      initStatus = "Failed";
      errorMsg = DEBUG.BOOT_ERROR_MESSAGE;
      return;
    }
    
    if (DEBUG.FORCE_BOOT_SCREEN) {
      initStatus = DEBUG.BOOT_STATUS_TEXT;
      return;
    }
    
    if (DEBUG.FORCE_MAIN_APP) {
      initStatus = "Ready";
    }
    
    // Apply individual flags (work independently)
    if (DEBUG.FORCE_AUDIO_NOT_INITIALISED) {
      initStatus = "Failed";
      errorMsg = DEBUG.BOOT_ERROR_MESSAGE;
    } else {
      audioInitialised = true;
    }
    
    if (DEBUG.FORCE_EDIT_MODE) {
      isEditMode = true;
    }
    
    if (DEBUG.FORCE_ERROR_BANNER) {
      errorMsg = DEBUG.ERROR_BANNER_TEXT;
    }
    
    if (DEBUG.FORCE_MOCK_SESSIONS && !DEBUG.FORCE_NO_SESSIONS) {
      audioSessions = DEBUG.MOCK_SESSIONS;
    } else if (DEBUG.FORCE_NO_SESSIONS) {
      audioSessions = [];
    }
    
    if (DEBUG.FORCE_BUTTON_BINDING_MODE && audioSessions.length > 0) {
      isButtonBindingMode = true;
      pendingButtonBinding = {
        sessionId: audioSessions[0].session_id,
        sessionName: audioSessions[0].display_name,
        processId: audioSessions[0].process_id,
        processName: audioSessions[0].process_name
      };
    }
  }

  onDestroy(() => {
    console.log("[ClearComms] Component destroying, cleaning up resources...");
    stopPolling();
    cleanupAllAnimations();
    cleanupAllLiveVolumeStates();
    cleanupAllCaches();
    console.log("[ClearComms] Component cleanup complete");
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // INITIALISATION & POLLING
  // ─────────────────────────────────────────────────────────────────────────────

  async function autoInitialise() {
    try {
      initStatus = "Initialising input system...";
      await invoke<string>("init_direct_input");

      initStatus = "Enumerating devices...";
      await invoke<string[]>("enumerate_input_devices");

      initStatus = "Reading axis values...";
      await getAxisValues();

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

  async function getAxisValues() {
    try {
      const data = await invoke<AxisData[]>("get_all_axis_values");
      axisData = data;
    } catch (error) {
      console.error("Error getting axis values:", error);
      errorMsg = `Error: ${error}`;
      axisData = [];
    }
  }
  
  function startPolling() {
    if (pollingInterval) return;
    
    isPolling = true;
    pollingInterval = setInterval(async () => {
      if (pollInFlight) {
        skippedPolls += 1;
        if (skippedPolls % POLL_LOG_INTERVAL === 0) {
          console.debug(`[ClearComms] Polling skipped ${skippedPolls} times due to in-flight iteration`);
        }
        return;
      }

      pollInFlight = true;
      try {
        await getAxisValues();
        await applyAxisMappings();
        await applyButtonMappings();
        pollIterations += 1;
        if (pollIterations > 1000000) {
          pollIterations = 0;
          skippedPolls = 0;
        }
        if (pollIterations % POLL_LOG_INTERVAL === 0) {
          console.debug(`[ClearComms] Polling iteration ${pollIterations}; cached sessions ${audioSessions.length}; button cache size ${previousButtonStates.size}`);
        }
      } catch (error) {
        console.error("Polling error:", error);
      } finally {
        pollInFlight = false;
      }
    }, 50);
    
    startAudioMonitoring();
    startMemoryMonitoring();
    startMemoryProfiler();
  }
  
  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
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
        if (deviceChanged) {
          console.log("Audio device changed - refreshing sessions");
        }
        await refreshAudioSessions();
      } catch (error) {
        console.error("Audio monitoring error:", error);
      }
    }, 200); // Poll every 200ms for responsive external changes
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
              // Detect external changes and queue animation
              const volumeDiff = Math.abs(systemSession.volume - existing.volume);
              if (volumeDiff > 0.01) {
                volumeChanges.set(SYSTEM_VOLUME_ID, { from: existing.volume, to: systemSession.volume });
                systemSession.volume = existing.volume;
              }
              
              if (systemSession.is_muted && existing.volume === 0) {
                systemSession.volume = 0;
              }
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
          } else if (animatingSliders.has(newSession.session_id)) {
            newSession.volume = existing.volume;
            newSession.is_muted = existing.is_muted;
          } else {
            // Detect external changes and queue animation
            const volumeDiff = Math.abs(newSession.volume - existing.volume);
            if (volumeDiff > 0.01) {
              volumeChanges.set(newSession.session_id, { from: existing.volume, to: newSession.volume });
              newSession.volume = existing.volume;
            }
            
            if (newSession.is_muted && existing.volume === 0) {
              newSession.volume = 0;
            }
          }
        }
      }
      
      audioSessions = sessions;
      
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
        console.log("[Layout] Not yet ready for measurement - mixer or channel not found");
        return;
      }
      
      // Get actual rendered dimensions in logical pixels
      const channelWidth = Math.round(firstChannel.clientWidth);
      
      // Get the gap from the mixer's computed style
      const computedStyle = window.getComputedStyle(mixer);
      const gapStr = computedStyle.gap;
      const channelGap = parseInt(gapStr) || 48; // Fallback to 48px if can't parse
      
      // The base width accounts for the mixer padding and the first channel
      // Calculate from main container padding: main { padding: 2.5rem } = 40px per side
      const mainPadding = 80; // 40px left + 40px right
      const baseWidth = channelWidth + mainPadding;
      
      // Send measurements to backend
      const result = await invoke<string>('update_layout_measurements', {
        channel_width: channelWidth,
        channel_gap: channelGap,
        base_width: baseWidth,
      });
      
      console.log(`[Layout] ${result}`);
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
      audioSessions[sessionIndex].is_muted = volume === 0;
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
    
    for (const [sessionId] of preMuteVolumes) {
      if (!activeSessionIds.has(sessionId)) {
        preMuteVolumes.delete(sessionId);
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

    console.log("[ClearComms] Periodic memory cleanup completed");
  }
  
  async function setSessionVolumeFinal(sessionId: string, volume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    try {
      await invokeSetVolume(sessionId, volume);
      await invokeSetMute(sessionId, volume === 0);
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

    // Cancel any ongoing animation before starting mute/unmute
    cancelVolumeAnimation(sessionId);

    try {
      if (muted && session.volume > 0) {
        // Preserve original pre-mute volume (don't overwrite if mid-toggle)
        if (!preMuteVolumes.has(sessionId)) {
          preMuteVolumes.set(sessionId, session.volume);
        }

        // Set muted state immediately for instant UI feedback
        audioSessions[sessionIndex].is_muted = true;

        // Fire-and-forget backend calls so animation starts without delay
        invokeSetVolume(sessionId, 0).catch(e => console.error("Error setting mute volume:", e));
        invokeSetMute(sessionId, true).catch(e => console.error("Error setting mute state:", e));

        // Animate visual slider to 0; if cancelled (superseded), return early
        const completed = await animateVolumeTo(sessionId, 0);
        if (!completed) return;
        audioSessions[sessionIndex].volume = 0;
      } else if (!muted) {
        const previousVolume = preMuteVolumes.get(sessionId) ?? 0.5;

        // Set unmuted state immediately for instant UI feedback
        audioSessions[sessionIndex].is_muted = false;

        // Fire-and-forget backend calls so animation starts without delay
        invokeSetVolume(sessionId, previousVolume).catch(e => console.error("Error setting unmute volume:", e));
        invokeSetMute(sessionId, false).catch(e => console.error("Error setting unmute state:", e));

        // Animate visual slider to previous volume; if cancelled, return early
        const completed = await animateVolumeTo(sessionId, previousVolume, 200);
        if (!completed) return;
        preMuteVolumes.delete(sessionId);
      }
    } catch (error) {
      console.error("Error setting mute:", error);
      errorMsg = `Audio error: ${error}`;
    }
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

  function detectButtonPress(): { deviceHandle: string; deviceName: string; buttonName: string } | null {
    for (const device of axisData) {
      const previousStates = previousButtonStates.get(device.device_handle);
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

  function createMapping(deviceHandle: string, deviceName: string, axisName: string, sessionId: string, sessionName: string, processId: number, processName: string) {
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    
    const newMapping: AxisMapping = { deviceHandle, deviceName, axisName, sessionId, sessionName, processId, processName, inverted: false };
    axisMappings = [...axisMappings, newMapping];
    
    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${axisName} → ${sessionName}`);
    saveMappings();
  }

  function toggleAxisInversion(processName: string) {
    const mapping = axisMappings.find(m => m.processName === processName);
    if (mapping) {
      mapping.inverted = !mapping.inverted;
      axisMappings = [...axisMappings];
      console.log(`[ClearComms] Axis inversion ${mapping.inverted ? 'enabled' : 'disabled'} for ${mapping.sessionName}`);
      saveMappings();
    }
  }

  function removeMapping(processName: string) {
    const mapping = axisMappings.find(m => m.processName === processName);
    if (mapping) {
      console.log(`[ClearComms] Removed mapping: ${mapping.deviceName} ${mapping.axisName} → ${mapping.sessionName}`);
    }
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    saveMappings();
  }

  function createButtonMapping(deviceHandle: string, deviceName: string, buttonName: string, sessionId: string, sessionName: string, processId: number, processName: string) {
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    
    const newMapping: ButtonMapping = { deviceHandle, deviceName, buttonName, sessionId, sessionName, processId, processName };
    buttonMappings = [...buttonMappings, newMapping];
    
    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${buttonName} → Mute ${sessionName}`);
    saveButtonMappings();
  }

  function removeButtonMapping(processName: string) {
    const mapping = buttonMappings.find(m => m.processName === processName);
    if (mapping) {
      console.log(`[ClearComms] Removed button mapping: ${mapping.deviceName} ${mapping.buttonName} → Mute ${mapping.sessionName}`);
    }
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    saveButtonMappings();
  }

  function removeApplication(processName: string) {
    const axisMapping = axisMappings.find(m => m.processName === processName);
    if (axisMapping) {
      console.log(`[ClearComms] Removed axis mapping for ${axisMapping.sessionName}`);
    }
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    
    const btnMapping = buttonMappings.find(m => m.processName === processName);
    if (btnMapping) {
      console.log(`[ClearComms] Removed button mapping for ${btnMapping.sessionName}`);
    }
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
      preMuteVolumes.delete(session.session_id);
      animatingSliders.delete(session.session_id);
      manuallyControlledSessions.delete(session.session_id);
      cancelVolumeAnimation(session.session_id);
    }
    
    console.log(`[ClearComms] ✓ Completely removed application: ${processName}`);
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
            console.log(`[ClearComms] Axis activated: ${mapping.deviceName} ${mapping.axisName} → ${mapping.sessionName}`);
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

    console.log("[ClearComms] All animations cleaned up");
  }
  
  function cleanupAllLiveVolumeStates() {
    for (const [sessionId] of liveVolumeState) {
      clearLiveVolumeState(sessionId);
    }
    liveVolumeState.clear();
    console.log("[ClearComms] Live volume states cleaned up");
  }
  
  function cleanupAllCaches() {
    previousAxisValues.clear();
    previousButtonStates.clear();
    lastHardwareAxisValues.clear();
    axisActivated.clear();
    preMuteVolumes.clear();
    manuallyControlledSessions.clear();
    hardwareVolumeTargets.clear();
    hardwareVolumeAnimations.clear();
    memorySnapshots = [];
    axisData = [];
    audioSessions = [];
    axisMappings = [];
    buttonMappings = [];
    
    console.log("[ClearComms] All caches cleared");
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // PERSISTENCE
  // ─────────────────────────────────────────────────────────────────────────────

  function saveMappings() {
    try {
      localStorage.setItem('clearcomms_axis_mappings', JSON.stringify(axisMappings));
    } catch (error) {
      console.error("Error saving mappings:", error);
    }
  }

  function loadMappings() {
    try {
      const saved = localStorage.getItem('clearcomms_axis_mappings');
      if (saved) {
        axisMappings = JSON.parse(saved);
      }
    } catch (error) {
      console.error("Error loading mappings:", error);
    }
  }

  function saveButtonMappings() {
    try {
      localStorage.setItem('clearcomms_button_mappings', JSON.stringify(buttonMappings));
    } catch (error) {
      console.error("Error saving button mappings:", error);
    }
  }

  function loadButtonMappings() {
    try {
      const saved = localStorage.getItem('clearcomms_button_mappings');
      if (saved) {
        buttonMappings = JSON.parse(saved);
      }
    } catch (error) {
      console.error("Error loading button mappings:", error);
    }
  }

  function savePinnedApps() {
    try {
      localStorage.setItem('clearcomms_pinned_apps', JSON.stringify([...pinnedApps]));
    } catch (error) {
      console.error("Error saving pinned apps:", error);
    }
  }

  function loadPinnedApps() {
    try {
      const saved = localStorage.getItem('clearcomms_pinned_apps');
      if (saved) {
        pinnedApps = new Set(JSON.parse(saved));
      }
    } catch (error) {
      console.error("Error loading pinned apps:", error);
    } finally {
      pinnedAppsLoaded = true;
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
    startAxisBinding(session.session_id, session.display_name, session.process_id, session.process_name);
  }

  function handleStartButtonBinding(e: CustomEvent<{ session: AudioSession }>) {
    const { session } = e.detail;
    startButtonBinding(session.session_id, session.display_name, session.process_id, session.process_name);
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

  function handleSelectApp(e: CustomEvent<{ processName: string }>) {
    const { processName } = e.detail;
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
    padding: 2.5rem;
    padding-bottom: 1rem;
    position: relative;
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
