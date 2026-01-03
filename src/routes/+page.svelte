<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  console.log("[ClearComms] Component script loaded");

  // ─────────────────────────────────────────────────────────────────────────────
  // DEBUG CONFIGURATION - Set these to preview different UI states
  // ─────────────────────────────────────────────────────────────────────────────
  
  const DEBUG = {
    // Master toggle - set to true to enable debug overrides
    ENABLED: true,
    
    // Force specific application states (only one should be true at a time)
    FORCE_BOOT_SCREEN: false,           // Show boot/loading screen indefinitely
    FORCE_BOOT_ERROR: false,            // Show boot screen with error state
    FORCE_CLOSE_CONFIRMATION: false,    // Show close confirmation dialog
    FORCE_MAIN_APP: false,              // Skip boot and show main app immediately
    
    // Boot screen options (when FORCE_BOOT_SCREEN or FORCE_BOOT_ERROR is true)
    BOOT_STATUS_TEXT: "Initialising...",  // Custom boot status text
    BOOT_ERROR_MESSAGE: "Failed to initialise audio subsystem: Device not found", // Error message when FORCE_BOOT_ERROR is true
    
    // Main app state overrides (when main app is shown)
    FORCE_EDIT_MODE: false,             // Start in edit mode
    FORCE_NO_SESSIONS: false,           // Show empty state (no audio sessions)
    FORCE_ERROR_BANNER: false,          // Show error banner in main app
    ERROR_BANNER_TEXT: "Hardware device disconnected", // Custom error banner text
    
    // Mock data (when main app is shown with FORCE_MOCK_SESSIONS)
    FORCE_MOCK_SESSIONS: false,         // Use mock audio sessions instead of real ones
    MOCK_SESSIONS: [
      { session_id: "mock_1", display_name: "Discord", process_id: 1234, process_name: "Discord.exe", volume: 0.75, is_muted: false },
      { session_id: "mock_2", display_name: "Spotify", process_id: 5678, process_name: "Spotify.exe", volume: 0.50, is_muted: false },
      { session_id: "mock_3", display_name: "Microsoft Flight Simulator", process_id: 9012, process_name: "FlightSimulator.exe", volume: 1.0, is_muted: true },
    ] as AudioSession[],
    
    // Binding state previews
    FORCE_BINDING_MODE: false,          // Show axis binding in progress
    FORCE_BUTTON_BINDING_MODE: false,   // Show button binding in progress
    
    // Misc UI states
    FORCE_AUDIO_NOT_INITIALISED: false, // Disable edit button (audio not ready)
  };

  // Audio session types
  interface AudioSession {
    session_id: string;
    display_name: string;
    process_id: number;
    process_name: string;
    volume: number;
    is_muted: boolean;
  }

  // Axis-to-audio mapping types
  interface AxisMapping {
    deviceHandle: string;
    deviceName: string;
    axisName: string;
    sessionId: string;
    sessionName: string;
    processId: number; // For re-matching after device changes
    processName: string;
    inverted: boolean; // Reverse axis direction
  }

  // Button-to-mute mapping types
  interface ButtonMapping {
    deviceHandle: string;
    deviceName: string;
    buttonName: string;
    sessionId: string;
    sessionName: string;
    processId: number; // For re-matching after device changes
    processName: string;
  }

  interface AxisData {
    device_handle: string;
    device_name: string;
    manufacturer: string;
    product_id: number;
    vendor_id: number;
    axes: Record<string, number>;
    buttons: Record<string, boolean>;
  }
  
  let axisData = $state<AxisData[]>([]);
  let audioSessions = $state<AudioSession[]>([]);
  let axisMappings = $state<AxisMapping[]>([]);
  let buttonMappings = $state<ButtonMapping[]>([]);
  let pinnedApps = $state<Set<string>>(new Set());
  let pollingInterval: number | null = null;
  let audioMonitorInterval: number | null = null;
  let isPolling = $state(false);
  let initStatus = $state("Initialising...");
  let audioInitialised = $state(false);
  let isBindingMode = $state(false);
  let isButtonBindingMode = $state(false);
  let pendingBinding = $state<{ sessionId: string; sessionName: string; processId: number; processName: string } | null>(null);
  let pendingButtonBinding = $state<{ sessionId: string; sessionName: string; processId: number; processName: string } | null>(null);
  let previousAxisValues: Map<string, Record<string, number>> = new Map();
  let previousButtonStates: Map<string, Record<string, boolean>> = new Map();
  let lastHardwareAxisValues: Map<string, number> = new Map(); // Track last hardware axis values
  let errorMsg = $state("");
  let isEditMode = $state(false);
  let previousDisplayCount = $state(-1);
  let preMuteVolumes = $state<Map<string, number>>(new Map());
  let animatingSliders = $state<Set<string>>(new Set());
  let animationSignals = $state<Map<string, { cancelled: boolean; resolve?: (completed: boolean) => void; frameId?: number }>>(new Map());
  let dragTargets = $state<Map<string, number>>(new Map());
  let dragAnimationFrames = $state<Map<string, number>>(new Map());
  let manuallyControlledSessions = $state<Set<string>>(new Set());
  
  // "Add Application" list expansion state in edit mode
  let addAppListExpanded = $state(false);
  
  // "Getting Started" expansion state in onboarding mode
  let gettingStartedExpanded = $state(false);
  
  // "Help" expansion state in main app
  let helpExpanded = $state(false);
  
  // "Close" expansion state for quit confirmation
  let closeExpanded = $state(false);

  // Hover/open state for controls bar
  let controlsOpen = $state(false);

  const handleControlsEnter = () => {
    controlsOpen = true;
  };

  const handleControlsLeave = () => {
    if (!helpExpanded && !closeExpanded) {
      controlsOpen = false;
    }
  };

  const POLL_LOG_INTERVAL = 200;
  const BUTTON_CACHE_LOG_INTERVAL = 200;
  const LIVE_UPDATE_MIN_INTERVAL_MS = 40;
  const HARDWARE_VOLUME_SMOOTHING = 0.3; // Interpolation factor (0-1, higher = faster)
  let pollInFlight = false;
  let pollIterations = 0;
  let skippedPolls = 0;
  let buttonCachePruneCounter = 0;

  // Hardware volume interpolation state
  const hardwareVolumeTargets = new Map<string, number>();
  const hardwareVolumeAnimations = new Map<string, number>();

  interface LiveVolumeState {
    inFlight: boolean;
    lastSent: number;
    queuedVolume?: number;
    timerId?: number;
  }

  const liveVolumeState = new Map<string, LiveVolumeState>();
  
  // Memory monitoring variables
  let memoryMonitorInterval: number | null = null;
  let lastMemoryCleanup = Date.now();
  const MEMORY_CLEANUP_INTERVAL = 300000; // 5 minutes
  const MAX_CACHE_SIZE = 1000;

  // ─────────────────────────────────────────────────────────────────────────────
  // Memory Profiling (Dev Mode)
  // ─────────────────────────────────────────────────────────────────────────────
  
  const IS_DEV = import.meta.env.DEV;
  let memoryProfilerInterval: number | null = null;
  let memorySnapshots: { timestamp: number; heapUsed: number; heapTotal: number }[] = [];
  const MEMORY_PROFILER_INTERVAL = 60000; // Log every 60 seconds
  const MAX_MEMORY_SNAPSHOTS = 120; // Keep 2 hours of data at 60s intervals
  
  interface MemoryInfo {
    jsHeapSizeLimit?: number;
    totalJSHeapSize?: number;
    usedJSHeapSize?: number;
  }
  
  function getMemoryUsage(): MemoryInfo | null {
    // Chrome/Chromium-based browsers expose memory info
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
    
    // Initial snapshot
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
    if (!memory || !memory.usedJSHeapSize || !memory.totalJSHeapSize) {
      return;
    }
    
    const snapshot = {
      timestamp: Date.now(),
      heapUsed: memory.usedJSHeapSize,
      heapTotal: memory.totalJSHeapSize
    };
    
    memorySnapshots.push(snapshot);
    
    // Keep only recent snapshots to prevent the profiler itself from leaking
    if (memorySnapshots.length > MAX_MEMORY_SNAPSHOTS) {
      memorySnapshots = memorySnapshots.slice(-MAX_MEMORY_SNAPSHOTS);
    }
    
    // Log current state
    console.log(
      `[MemoryProfiler] Heap: ${formatBytes(snapshot.heapUsed)} / ${formatBytes(snapshot.heapTotal)} | ` +
      `Caches: axis=${previousAxisValues.size}, btn=${previousButtonStates.size}, hw=${lastHardwareAxisValues.size}, ` +
      `live=${liveVolumeState.size}, anim=${animatingSliders.size}, drag=${dragAnimationFrames.size}`
    );
  }
  
  function checkForMemoryLeaks() {
    if (memorySnapshots.length < 10) return; // Need enough data points
    
    // Compare first and last 5 snapshots to detect growth trend
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
    console.log(`liveVolumeState: ${liveVolumeState.size} entries`);
    console.log(`hardwareVolumeTargets: ${hardwareVolumeTargets.size} entries`);
    console.log(`hardwareVolumeAnimations: ${hardwareVolumeAnimations.size} entries`);
    console.log(`animatingSliders: ${animatingSliders.size} entries`);
    console.log(`animationSignals: ${animationSignals.size} entries`);
    console.log(`dragTargets: ${dragTargets.size} entries`);
    console.log(`dragAnimationFrames: ${dragAnimationFrames.size} entries`);
    console.log(`manuallyControlledSessions: ${manuallyControlledSessions.size} entries`);
    console.log(`preMuteVolumes: ${preMuteVolumes.size} entries`);
    console.log(`audioSessions: ${audioSessions.length} entries`);
    console.log(`axisData: ${axisData.length} entries`);
    console.log(`axisMappings: ${axisMappings.length} entries`);
    console.log(`buttonMappings: ${buttonMappings.length} entries`);
    console.groupEnd();
  }
  
  // Expose debug functions to window in dev mode
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
        // Attempt to trigger garbage collection (may not work in all browsers)
        cleanupAllCaches();
        console.log("[MemoryProfiler] Caches cleared, GC should run soon");
        setTimeout(logMemorySnapshot, 1000);
      }
    };
    console.log("[MemoryProfiler] Debug functions available: window.clearCommsDebug.{logMemory, logCaches, getSnapshots, forceCleanup, forceGC}");
  }

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
    
    // Add active sessions
    for (const session of audioSessions) {
      if (boundProcessNames.has(session.process_name) && !foundProcessNames.has(session.process_name)) {
        sessions.push(session);
        foundProcessNames.add(session.process_name);
      }
    }
    
    // Add inactive session entries for bound/pinned apps that aren't running
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
    
    // Add inactive session entries for pinned apps without mappings
    for (const processName of pinnedApps) {
      if (!foundProcessNames.has(processName)) {
        // Try to find session info from audio sessions
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

  // Get unbound sessions for dropdown
  function getAvailableSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName),
      ...pinnedApps
    ]);
    
    return audioSessions.filter(s => !boundProcessNames.has(s.process_name));
  }

  // Format process name to be more user-friendly
  function formatProcessName(processName: string): string {
    // Custom name mappings for specific applications
    const customNames: Record<string, string> = {
      'vpilot.exe': 'vPilot',
      'couatl.exe': 'GSX'
    };
    
    // Check for custom name first (case-insensitive)
    const lowerProcessName = processName.toLowerCase();
    if (customNames[lowerProcessName]) {
      return customNames[lowerProcessName];
    }
    
    // Remove .exe extension
    let name = processName.replace(/\.exe$/i, '');
    
    // Capitalize first letter of each word
    name = name.split(/[-_\s]/).map(word => 
      word.charAt(0).toUpperCase() + word.slice(1).toLowerCase()
    ).join(' ');
    
    return name;
  }

  function toggleEditMode() {
    isEditMode = !isEditMode;
    // Collapse add-app list when exiting edit mode
    if (!isEditMode) {
      addAppListExpanded = false;
    }
  }

  onMount(() => {
    // Apply debug overrides if enabled
    if (DEBUG.ENABLED) {
      console.log("[DEBUG] Debug mode enabled - applying overrides");
      applyDebugOverrides();
      return; // Skip normal initialisation if forcing a specific state
    }
    
    loadMappings();
    loadButtonMappings();
    loadPinnedApps();
    autoInitialise();

    // Exit edit mode when window loses focus (minimised or switched away)
    const handleBlur = () => {
      if (isEditMode) {
        isEditMode = false;
        isBindingMode = false;
        isButtonBindingMode = false;
        pendingBinding = null;
        pendingButtonBinding = null;
        addAppListExpanded = false;
      }
    };

    window.addEventListener('blur', handleBlur);

    // Clean up event listener on component destroy
    return () => {
      window.removeEventListener('blur', handleBlur);
    };
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Debug Overrides
  // ─────────────────────────────────────────────────────────────────────────────
  
  function applyDebugOverrides() {
    // Boot screen with error
    if (DEBUG.FORCE_BOOT_ERROR) {
      initStatus = "Failed";
      errorMsg = DEBUG.BOOT_ERROR_MESSAGE;
      console.log("[DEBUG] Forcing boot error screen");
      return;
    }
    
    // Boot screen (loading)
    if (DEBUG.FORCE_BOOT_SCREEN) {
      initStatus = DEBUG.BOOT_STATUS_TEXT;
      console.log("[DEBUG] Forcing boot screen with status:", DEBUG.BOOT_STATUS_TEXT);
      return;
    }
    
    // Main application
    if (DEBUG.FORCE_MAIN_APP) {
      initStatus = "Ready";
      
      // Audio initialisation state
      audioInitialised = !DEBUG.FORCE_AUDIO_NOT_INITIALISED;
      
      // Edit mode
      if (DEBUG.FORCE_EDIT_MODE) {
        isEditMode = true;
      }
      
      // Error banner
      if (DEBUG.FORCE_ERROR_BANNER) {
        errorMsg = DEBUG.ERROR_BANNER_TEXT;
      }
      
      // Mock sessions
      if (DEBUG.FORCE_MOCK_SESSIONS && !DEBUG.FORCE_NO_SESSIONS) {
        audioSessions = DEBUG.MOCK_SESSIONS;
        console.log("[DEBUG] Using mock audio sessions:", audioSessions.length);
      } else if (DEBUG.FORCE_NO_SESSIONS) {
        audioSessions = [];
        console.log("[DEBUG] Forcing no audio sessions (empty state)");
      }
      
      // Binding modes
      if (DEBUG.FORCE_BINDING_MODE && audioSessions.length > 0) {
        isBindingMode = true;
        pendingBinding = {
          sessionId: audioSessions[0].session_id,
          sessionName: audioSessions[0].display_name,
          processId: audioSessions[0].process_id,
          processName: audioSessions[0].process_name
        };
        console.log("[DEBUG] Forcing axis binding mode");
      }
      
      if (DEBUG.FORCE_BUTTON_BINDING_MODE && audioSessions.length > 0) {
        isButtonBindingMode = true;
        pendingButtonBinding = {
          sessionId: audioSessions[0].session_id,
          sessionName: audioSessions[0].display_name,
          processId: audioSessions[0].process_id,
          processName: audioSessions[0].process_name
        };
        console.log("[DEBUG] Forcing button binding mode");
      }
      
      console.log("[DEBUG] Forcing main app view");
      return;
    }
    
    // If no specific state forced, run normal init
    console.log("[DEBUG] No specific state forced, running normal initialisation");
    loadMappings();
    loadButtonMappings();
    loadPinnedApps();
    autoInitialise();
  }

  onDestroy(() => {
    console.log("[ClearComms] Component destroying, cleaning up resources...");
    
    // Stop all polling and intervals
    stopPolling();
    
    // Clean up all animation frames
    cleanupAllAnimations();
    
    // Clean up live volume state timers
    cleanupAllLiveVolumeStates();
    
    // Clear all Maps and Sets to free memory
    cleanupAllCaches();
    
    // Clean up backend resources
    // cleanupBackendResources(); // Commented out to prevent HMR crashes
    
    console.log("[ClearComms] Component cleanup complete");
  });

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
        // Prevent counter overflow after extended runs (reset at 1 million)
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
    }, 3000);
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
      
      // Update sessions but handle special cases
      for (const newSession of sessions) {
        const existingIndex = audioSessions.findIndex(s => s.session_id === newSession.session_id);
        
        if (existingIndex !== -1) {
          const existing = audioSessions[existingIndex];
          
          // If user is manually dragging, preserve the current volume completely
          if (manuallyControlledSessions.has(newSession.session_id)) {
            newSession.volume = existing.volume;
            newSession.is_muted = existing.is_muted;
          }
          // If currently animating, preserve the animated volume
          else if (animatingSliders.has(newSession.session_id)) {
            newSession.volume = existing.volume;
          }
          // If muted with volume at 0, keep it at 0 (don't restore from backend)
          else if (newSession.is_muted && existing.volume === 0) {
            newSession.volume = 0;
          }
        }
      }
      
      audioSessions = sessions;
      cleanupStaleMappings();
    } catch (error) {
      console.error("Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  // Clean up mappings for sessions that are no longer active
  function cleanupStaleMappings() {
    // Note: We intentionally keep mappings for inactive applications
    // so they appear as inactive sessions and automatically reconnect
    // when the applications start again. This preserves user bindings.
    
    // Only perform minimal cleanup if needed (e.g., remove duplicate mappings)
    // For now, this is a no-op to preserve the intended functionality
    return;
  }

  async function resizeWindowToFit(sessionCount: number) {
    try {
      await invoke<string>("resize_window_to_content", { sessionCount });
    } catch (error) {
      console.error("Error resizing window:", error);
    }
  }

  // Update volume immediately in UI (no backend call)
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
      if (!currentState) {
        return;
      }

      const queued = currentState.queuedVolume;
      if (queued === undefined) {
        return;
      }

      if (currentState.inFlight) {
        return;
      }

      const now = performance.now();
      const elapsed = now - currentState.lastSent;

      if (elapsed < LIVE_UPDATE_MIN_INTERVAL_MS) {
        if (currentState.timerId !== undefined) {
          clearTimeout(currentState.timerId);
        }

        const delay = Math.max(0, LIVE_UPDATE_MIN_INTERVAL_MS - elapsed);
        currentState.timerId = window.setTimeout(() => {
          const refreshedState = liveVolumeState.get(sessionId);
          if (!refreshedState) {
            return;
          }
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
          await invoke("set_session_volume", { sessionId, volume: volumeToSend });
          await invoke("set_session_mute", { sessionId, muted: volumeToSend === 0 });
        } catch (error) {
          console.error(`Error applying live volume for ${sessionId}:`, error);
        } finally {
          const finalState = liveVolumeState.get(sessionId);
          if (!finalState) {
            return;
          }
          finalState.inFlight = false;
          attemptSend();
        }
      })();
    };

    attemptSend();
  }

  function clearLiveVolumeState(sessionId: string) {
    const state = liveVolumeState.get(sessionId);
    if (!state) {
      return;
    }

    if (state.timerId !== undefined) {
      clearTimeout(state.timerId);
    }

    liveVolumeState.delete(sessionId);
  }

  function cancelVolumeAnimation(sessionId: string) {
    const signal = animationSignals.get(sessionId);
    if (!signal) {
      return;
    }

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

  // Animate volume change in UI, then apply to backend
  async function animateVolumeTo(sessionId: string, targetVolume: number, durationMs: number = 200): Promise<boolean> {
    if (sessionId.startsWith('inactive_')) return false;
    
    const session = audioSessions.find(s => s.session_id === sessionId);
    if (!session) return false;

    cancelVolumeAnimation(sessionId);
    
    const startVolume = session.volume;
    const startTime = Date.now();
    animatingSliders.add(sessionId);

    return new Promise<boolean>((resolve) => {
      const signal = { cancelled: false, resolve, frameId: undefined as number | undefined };
      animationSignals.set(sessionId, signal);

      const animate = () => {
        if (signal.cancelled) {
          return;
        }

        const elapsed = Date.now() - startTime;
        const progress = Math.min(elapsed / durationMs, 1);

        // Ease-out cubic for smooth deceleration
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
  
  // Smooth drag following - continuously animate toward target
  function startDragAnimation(sessionId: string, targetVolume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    dragTargets.set(sessionId, targetVolume);
    
    // Start animation loop if not already running
    if (!dragAnimationFrames.has(sessionId)) {
      const session = audioSessions.find(s => s.session_id === sessionId);
      if (!session) return;
      
      const animate = () => {
        const target = dragTargets.get(sessionId);
        if (target === undefined) {
          dragAnimationFrames.delete(sessionId);
          return;
        }
        
        const currentSession = audioSessions.find(s => s.session_id === sessionId);
        if (!currentSession) {
          dragAnimationFrames.delete(sessionId);
          dragTargets.delete(sessionId);
          return;
        }
        
        const current = currentSession.volume;
        const diff = target - current;
        
        // Smooth interpolation - move 25% of the way to target each frame
        const smoothingFactor = 0.25;
        const newVolume = current + (diff * smoothingFactor);
        
        // Stop if very close to target
        if (Math.abs(diff) < 0.001) {
          setSessionVolumeImmediate(sessionId, target);
          dragAnimationFrames.delete(sessionId);
          dragTargets.delete(sessionId);
          return;
        }
        
        setSessionVolumeImmediate(sessionId, newVolume);
        const frameId = requestAnimationFrame(animate);
        dragAnimationFrames.set(sessionId, frameId);
      };
      
      const frameId = requestAnimationFrame(animate);
      dragAnimationFrames.set(sessionId, frameId);
    }
  }
  
  function stopDragAnimation(sessionId: string) {
    const frameId = dragAnimationFrames.get(sessionId);
    if (frameId !== undefined) {
      cancelAnimationFrame(frameId);
    }
    dragAnimationFrames.delete(sessionId);
    dragTargets.delete(sessionId);
  }

  // Smooth hardware volume interpolation (front-end only)
  function startHardwareVolumeInterpolation(sessionId: string, targetVolume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    hardwareVolumeTargets.set(sessionId, targetVolume);
    
    // Start animation loop if not already running
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
        
        // Smooth interpolation
        const newVolume = current + (diff * HARDWARE_VOLUME_SMOOTHING);
        
        // Stop if very close to target
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
      
      // Periodic memory cleanup
      if (now - lastMemoryCleanup > MEMORY_CLEANUP_INTERVAL) {
        performPeriodicCleanup();
        lastMemoryCleanup = now;
      }
      
      // Monitor cache sizes
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
      }
      
      // Check liveVolumeState Map size
      if (liveVolumeState.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Live volume state cache size exceeded limit, clearing");
        cleanupAllLiveVolumeStates();
      }
      
      // Check hardwareVolumeTargets Map size
      if (hardwareVolumeTargets.size > MAX_CACHE_SIZE) {
        console.warn("[ClearComms] Hardware volume targets cache size exceeded limit, clearing");
        for (const [_, frameId] of hardwareVolumeAnimations) {
          cancelAnimationFrame(frameId);
        }
        hardwareVolumeAnimations.clear();
        hardwareVolumeTargets.clear();
      }
    }, 30000); // Check every 30 seconds
  }
  
  function stopMemoryMonitoring() {
    if (memoryMonitorInterval) {
      clearInterval(memoryMonitorInterval);
      memoryMonitorInterval = null;
    }
  }
  
  function performPeriodicCleanup() {
    // Clean up any stale animation states
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
    
    // Clean up stale pre-mute volumes
    for (const [sessionId] of preMuteVolumes) {
      if (!activeSessionIds.has(sessionId)) {
        preMuteVolumes.delete(sessionId);
      }
    }
    
    // Clean up stale hardware volume interpolations
    for (const [sessionId, frameId] of hardwareVolumeAnimations) {
      if (!activeSessionIds.has(sessionId)) {
        cancelAnimationFrame(frameId);
        hardwareVolumeAnimations.delete(sessionId);
        hardwareVolumeTargets.delete(sessionId);
      }
    }
    
    // Clean up stale drag animations
    for (const [sessionId, frameId] of dragAnimationFrames) {
      if (!activeSessionIds.has(sessionId)) {
        cancelAnimationFrame(frameId);
        dragAnimationFrames.delete(sessionId);
        dragTargets.delete(sessionId);
      }
    }
    
    // Clean up stale live volume states
    for (const [sessionId] of liveVolumeState) {
      if (!activeSessionIds.has(sessionId)) {
        clearLiveVolumeState(sessionId);
      }
    }
    
    console.log("[ClearComms] Periodic memory cleanup completed");
  }
  
  // Apply volume to backend after animation completes
  async function setSessionVolumeFinal(sessionId: string, volume: number) {
    if (sessionId.startsWith('inactive_')) return;
    
    try {
      await invoke("set_session_volume", { sessionId, volume });
      await invoke("set_session_mute", { sessionId, muted: volume === 0 });
      await refreshAudioSessions();
    } catch (error) {
      console.error("Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function setSessionMute(sessionId: string, muted: boolean) {
    if (sessionId.startsWith('inactive_')) return;
    
    try {
      const session = audioSessions.find(s => s.session_id === sessionId);
      
      if (muted && session && session.volume > 0) {
        preMuteVolumes.set(sessionId, session.volume);
        await animateVolumeTo(sessionId, 0);
        await invoke("set_session_mute", { sessionId, muted: true });
        
        const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
        if (sessionIndex !== -1) {
          audioSessions[sessionIndex].is_muted = true;
          audioSessions[sessionIndex].volume = 0;
        }
      } else if (!muted) {
        const previousVolume = preMuteVolumes.get(sessionId) ?? 0.5;
        
        // Unmute backend first, then animate slider up
        await invoke("set_session_volume", { sessionId, volume: previousVolume });
        await invoke("set_session_mute", { sessionId, muted: false });
        
        const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
        if (sessionIndex !== -1) {
          audioSessions[sessionIndex].is_muted = false;
        }
        
        await animateVolumeTo(sessionId, previousVolume, 200);
        preMuteVolumes.delete(sessionId);
      }
    } catch (error) {
      console.error("Error setting mute:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

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

  // Detect significant axis movement (>5% change)
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

  // Detect button press (false → true transition)
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
    
    // Pin the app so it remains visible even if bindings are removed
    pinnedApps = new Set([...pinnedApps, processName]);
    savePinnedApps();
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${axisName} → ${sessionName}`);
    saveMappings();
  }

  function toggleAxisInversion(processName: string) {
    const mapping = axisMappings.find(m => m.processName === processName);
    if (mapping) {
      mapping.inverted = !mapping.inverted;
      axisMappings = [...axisMappings]; // Trigger reactivity
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
    // Remove existing button mapping for this process (one button per process)
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    
    const newMapping: ButtonMapping = { deviceHandle, deviceName, buttonName, sessionId, sessionName, processId, processName };
    buttonMappings = [...buttonMappings, newMapping];
    
    // Pin the app so it remains visible even if bindings are removed
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
    // Remove axis mapping
    const axisMapping = axisMappings.find(m => m.processName === processName);
    if (axisMapping) {
      console.log(`[ClearComms] Removed axis mapping for ${axisMapping.sessionName}`);
    }
    axisMappings = axisMappings.filter(m => m.processName !== processName);
    
    // Remove button mapping
    const btnMapping = buttonMappings.find(m => m.processName === processName);
    if (btnMapping) {
      console.log(`[ClearComms] Removed button mapping for ${btnMapping.sessionName}`);
    }
    buttonMappings = buttonMappings.filter(m => m.processName !== processName);
    
    // Unpin the app so it's removed from the mixer
    const newPinnedApps = new Set(pinnedApps);
    newPinnedApps.delete(processName);
    pinnedApps = newPinnedApps;
    savePinnedApps();
    
    // Exit edit mode if no pinned apps remain
    if (pinnedApps.size === 0) {
      isEditMode = false;
    }
    
    // Clear any cached pre-mute volumes
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
        
        // 1% deadzone at each end
        const deadzoneThreshold = 0.01;
        if (axisValue < deadzoneThreshold) {
          axisValue = 0.0;
        } else if (axisValue > (1.0 - deadzoneThreshold)) {
          axisValue = 1.0;
        }
        
        const mappingKey = `${mapping.deviceHandle}-${mapping.axisName}-${mapping.processName}`;
        const lastHardwareValue = lastHardwareAxisValues.get(mappingKey);
        
        // Only update if hardware value changed by >1%
        if (lastHardwareValue === undefined || Math.abs(lastHardwareValue - axisValue) > 0.01) {
          const session = audioSessions.find(s => s.process_name === mapping.processName);
          
          // Skip if user is manually controlling this session
          if (session && !manuallyControlledSessions.has(session.session_id)) {
            try {
              // Update backend immediately (real control)
              await invoke("set_session_volume", { sessionId: session.session_id, volume: axisValue });
              await invoke("set_session_mute", { sessionId: session.session_id, muted: axisValue === 0 });
              
              // Smoothly interpolate UI display (cosmetic)
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
    // Handle button binding mode
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
          
          // Button press = false → true transition
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

    // Update previous button states for next poll
    for (const device of axisData) {
      previousButtonStates.set(device.device_handle, { ...device.buttons });
    }

    for (const handle of Array.from(previousButtonStates.keys())) {
      if (!activeHandles.has(handle)) {
        previousButtonStates.delete(handle);
      }
    }

    buttonCachePruneCounter += 1;
    // Prevent counter overflow
    if (buttonCachePruneCounter > 1000000) {
      buttonCachePruneCounter = 0;
    }
    if (buttonCachePruneCounter % BUTTON_CACHE_LOG_INTERVAL === 0) {
      console.debug(`[ClearComms] Button state cache size ${previousButtonStates.size}; active handles ${activeHandles.size}`);
    }
  }

  // Mappings persist even when apps are closed
  function cleanupAllAnimations() {
    // Cancel all volume animations
    for (const [sessionId] of animationSignals) {
      cancelVolumeAnimation(sessionId);
    }
    animationSignals.clear();
    animatingSliders.clear();
    
    // Cancel all drag animations
    for (const [sessionId, frameId] of dragAnimationFrames) {
      cancelAnimationFrame(frameId);
    }
    dragAnimationFrames.clear();
    dragTargets.clear();
    
    // Cancel all hardware volume animations
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
    // Clear all Maps
    previousAxisValues.clear();
    previousButtonStates.clear();
    lastHardwareAxisValues.clear();
    preMuteVolumes.clear();
    manuallyControlledSessions.clear();
    hardwareVolumeTargets.clear();
    hardwareVolumeAnimations.clear();
    dragTargets.clear();
    dragAnimationFrames.clear();
    
    // Clear memory profiler snapshots
    memorySnapshots = [];
    
    // Clear arrays to release memory
    axisData = [];
    audioSessions = [];
    axisMappings = [];
    buttonMappings = [];
    
    console.log("[ClearComms] All caches cleared");
  }
  
  async function cleanupBackendResources() {
    try {
      await invoke("cleanup_audio_manager");
      await invoke("cleanup_input_manager");
      console.log("[ClearComms] Backend resources cleaned up");
    } catch (error) {
      console.warn("[ClearComms] Backend cleanup failed (non-critical):", error);
    }
  }

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
    }
  }
</script>

{#if initStatus === 'Ready'}
  <!-- Main Application -->
  <main role="application" aria-label="ClearComms Audio Mixer">
    <a href="#main-content" class="skip-link">Skip to main content</a>

    {#if errorMsg}
      <div class="error-banner" role="alert" aria-live="assertive">{errorMsg}</div>
    {/if}

    <!-- Audio Management Section -->

    {#if audioInitialised}
      {@const boundSessions = getBoundSessions()}
      {@const availableSessions = getAvailableSessions()}
      
      {#if boundSessions.length > 0 || isEditMode}
        <div class="mixer-container" id="main-content">
          {#each boundSessions as session (session.session_id)}
            {@const mapping = axisMappings.find(m => m.processName === session.process_name)}
            {@const buttonMapping = buttonMappings.find(m => m.processName === session.process_name)}
            {@const isInactiveSession = session.session_id.startsWith('inactive_')}
            
            <div class="channel-strip" class:has-mapping={!!mapping || !!buttonMapping} class:inactive={isInactiveSession} class:inactive-edit-mode={isInactiveSession && isEditMode} role="group" aria-label="Audio controls for {session.display_name}">

              <!-- Vertical Volume Bar -->
              <div class="volume-bar-container">
                <input
                  type="range"
                  class="volume-slider"
                  min="0"
                  max="1"
                  step="0.01"
                  value={session.volume}
                  aria-label="Volume for {session.display_name}"
                  aria-valuemin="0"
                  aria-valuemax="100"
                  aria-valuenow={Math.round(session.volume * 100)}
                  aria-valuetext="{Math.round(session.volume * 100)} percent"
                  style="--volume-percent: {session.volume * 100}%"
                  disabled={isInactiveSession}
                  onpointerdown={(e) => {
                    animatingSliders.delete(session.session_id);
                    manuallyControlledSessions.add(session.session_id);
                    cancelVolumeAnimation(session.session_id);
                    clearLiveVolumeState(session.session_id);
                    
                    const slider = e.currentTarget as HTMLInputElement;
                    slider.dataset.isDragging = 'pending';
                    slider.dataset.startVolume = session.volume.toString();
                    delete slider.dataset.wasTrackClick;
                    try {
                      slider.setPointerCapture(e.pointerId);
                    } catch (captureError) {
                      // Ignore if pointer capture not available
                    }
                  }}
                  onpointermove={(e) => {
                    if (e.buttons !== 1) {
                      return;
                    }

                    const slider = e.currentTarget as HTMLInputElement;
                    if (slider.dataset.isDragging !== 'true') {
                      slider.dataset.isDragging = 'true';
                      cancelVolumeAnimation(session.session_id);
                      delete slider.dataset.wasTrackClick;
                    }
                  }}
                  oninput={(e) => {
                    const slider = e.currentTarget as HTMLInputElement;
                    const newValue = parseFloat(slider.value);
                    const startVolume = parseFloat(slider.dataset.startVolume ?? session.volume.toString());
                    
                    if (slider.dataset.wasTrackClick === 'true' && slider.dataset.isDragging !== 'true') {
                      return;
                    }

                    if (slider.dataset.isDragging !== 'true') {
                      slider.dataset.wasTrackClick = 'true';
                      slider.value = startVolume.toString();

                      (async () => {
                        const completed = await animateVolumeTo(session.session_id, newValue, 250);
                        if (completed) {
                          await setSessionVolumeFinal(session.session_id, newValue);
                          manuallyControlledSessions.delete(session.session_id);
                          delete slider.dataset.wasTrackClick;
                          delete slider.dataset.startVolume;
                          delete slider.dataset.isDragging;
                        }
                      })();
                      return;
                    }
                    
                    const sessionIndex = audioSessions.findIndex(s => s.session_id === session.session_id);
                    if (sessionIndex !== -1) {
                      audioSessions[sessionIndex].volume = newValue;
                      audioSessions[sessionIndex].is_muted = newValue === 0;
                    }

                    scheduleLiveVolumeUpdate(session.session_id, newValue);
                  }}
                  onpointerup={async (e) => {
                    const slider = e.currentTarget as HTMLInputElement;
                    
                    if (slider.dataset.isDragging === 'true') {
                      const finalValue = parseFloat(slider.value);
                      await setSessionVolumeFinal(session.session_id, finalValue);
                    }
                    
                    manuallyControlledSessions.delete(session.session_id);
                    clearLiveVolumeState(session.session_id);
                    delete slider.dataset.wasTrackClick;
                    delete slider.dataset.startVolume;
                    delete slider.dataset.isDragging;
                    if (slider.hasPointerCapture?.(e.pointerId)) {
                      try {
                        slider.releasePointerCapture(e.pointerId);
                      } catch (releaseError) {
                        // Ignore if pointer capture not available
                      }
                    }
                  }}
                  onwheel={async (e) => {
                    e.preventDefault();
                    const delta = e.deltaY > 0 ? -0.05 : 0.05;
                    const newVolume = Math.max(0, Math.min(1, session.volume + delta));
                    const completed = await animateVolumeTo(session.session_id, newVolume, 150);
                    if (completed) {
                      await setSessionVolumeFinal(session.session_id, newVolume);
                    }
                  }}
                />
              </div>

              <!-- Mute Button / Button Binding Control -->
              {#if isEditMode}
                <!-- Button Binding Control (Edit Mode) -->
                {#if buttonMapping}
                  <button
                    class="mapping-badge button mapping-removable"
                    aria-label="Remove mute button binding for {session.display_name}: {buttonMapping.buttonName}"
                    title="Mute Button: {buttonMapping.buttonName}"
                    onclick={() => removeButtonMapping(session.process_name)}
                    type="button"
                  >
                    <span class="mapping-icon default" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416zM399 239C389.6 248.4 389.6 263.6 399 272.9L446 319.9L399 366.9C389.6 376.3 389.6 391.5 399 400.8C408.4 410.1 423.6 410.2 432.9 400.8L479.9 353.8L526.9 400.8C536.3 410.2 551.5 410.2 560.8 400.8C570.1 391.4 570.2 376.2 560.8 366.9L513.8 319.9L560.8 272.9C570.2 263.5 570.2 248.3 560.8 239C551.4 229.7 536.2 229.6 526.9 239L479.9 286L432.9 239C423.5 229.6 408.3 229.6 399 239z"/>
                      </svg>
                    </span>
                    <span class="mapping-icon remove" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
                      </svg>
                    </span>
                  </button>
                {:else if isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
                  <!-- Binding in progress: Cancel binding -->
                  <button
                    class="btn btn-channel btn-disabled"
                    aria-label="Cancel mute button binding for {session.display_name}"
                    title="Cancel Mute Binding"
                    onclick={cancelButtonBinding}
                    type="button"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                      <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z" />
                    </svg>
                  </button>
                {:else}
                  <button class="btn btn-channel btn-bind btn-disabled" onclick={() => startButtonBinding(session.session_id, session.display_name, session.process_id, session.process_name)} aria-label="Bind hardware button to mute {session.display_name}" title="Bind Mute Button">
                    <span class="bind-icon default" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416zM399 239C389.6 248.4 389.6 263.6 399 272.9L446 319.9L399 366.9C389.6 376.3 389.6 391.5 399 400.8C408.4 410.1 423.6 410.2 432.9 400.8L479.9 353.8L526.9 400.8C536.3 410.2 551.5 410.2 560.8 400.8C570.1 391.4 570.2 376.2 560.8 366.9L513.8 319.9L560.8 272.9C570.2 263.5 570.2 248.3 560.8 239C551.4 229.7 536.2 229.6 526.9 239L479.9 286L432.9 239C423.5 229.6 408.3 229.6 399 239z"/>
                      </svg>
                    </span>
                    <span class="bind-icon hover" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
                      </svg>
                    </span>
                  </button>
                {/if}
              {:else}
                <!-- Mute Button (Normal Mode) -->
                <button
                  class="btn btn-channel {isInactiveSession ? 'btn-unavail' : (session.is_muted ? 'btn-disabled' : 'btn-enabled')}"
                  onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                  aria-label="{session.is_muted ? 'Unmute' : 'Mute'} {session.display_name}"
                  aria-pressed={session.is_muted}
                  title={session.is_muted ? 'Unmute' : 'Mute'}
                >
                  {#if session.is_muted || session.volume === 0}
                    <!-- Muted or 0% volume -->
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                      <path d="M80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416zM399 239C389.6 248.4 389.6 263.6 399 272.9L446 319.9L399 366.9C389.6 376.3 389.6 391.5 399 400.8C408.4 410.1 423.6 410.2 432.9 400.8L479.9 353.8L526.9 400.8C536.3 410.2 551.5 410.2 560.8 400.8C570.1 391.4 570.2 376.2 560.8 366.9L513.8 319.9L560.8 272.9C570.2 263.5 570.2 248.3 560.8 239C551.4 229.7 536.2 229.6 526.9 239L479.9 286L432.9 239C423.5 229.6 408.3 229.6 399 239z"/>
                    </svg>
                  {:else if session.volume === 1}
                    <!-- 100% volume -->
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                      <path d="M533.6 96.5C523.3 88.1 508.2 89.7 499.8 100C491.4 110.3 493 125.4 503.3 133.8C557.5 177.8 592 244.8 592 320C592 395.2 557.5 462.2 503.3 506.3C493 514.7 491.5 529.8 499.8 540.1C508.1 550.4 523.3 551.9 533.6 543.6C598.5 490.7 640 410.2 640 320C640 229.8 598.5 149.2 533.6 96.5zM473.1 171C462.8 162.6 447.7 164.2 439.3 174.5C430.9 184.8 432.5 199.9 442.8 208.3C475.3 234.7 496 274.9 496 320C496 365.1 475.3 405.3 442.8 431.8C432.5 440.2 431 455.3 439.3 465.6C447.6 475.9 462.8 477.4 473.1 469.1C516.3 433.9 544 380.2 544 320.1C544 260 516.3 206.3 473.1 171.1zM412.6 245.5C402.3 237.1 387.2 238.7 378.8 249C370.4 259.3 372 274.4 382.3 282.8C393.1 291.6 400 305 400 320C400 335 393.1 348.4 382.3 357.3C372 365.7 370.5 380.8 378.8 391.1C387.1 401.4 402.3 402.9 412.6 394.6C434.1 376.9 448 350.1 448 320C448 289.9 434.1 263.1 412.6 245.5zM80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416z"/>
                    </svg>
                  {:else}
                    <!-- Between 1% and 99% volume -->
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                      <path d="M144 416L192 416L326.1 535.2C332.5 540.9 340.7 544 349.2 544C368.4 544 384 528.4 384 509.2L384 130.8C384 111.6 368.4 96 349.2 96C340.7 96 332.5 99.1 326.1 104.8L192 224L144 224C117.5 224 96 245.5 96 272L96 368C96 394.5 117.5 416 144 416zM476.6 245.5C466.3 237.1 451.2 238.7 442.8 249C434.4 259.3 436 274.4 446.3 282.8C457.1 291.6 464 305 464 320C464 335 457.1 348.4 446.3 357.3C436 365.7 434.5 380.8 442.8 391.1C451.1 401.4 466.3 402.9 476.6 394.6C498.1 376.9 512 350.1 512 320C512 289.9 498.1 263.1 476.5 245.5z"/>
                    </svg>
                  {/if}
                </button>
              {/if}

              <!-- Axis Binding Control (Edit Mode Only) -->
              {#if isEditMode}
                {#if mapping}
                  <!-- Bound: Show mapping badge -->
                  <button
                    class="mapping-badge mapping-removable"
                    aria-label="Remove volume axis binding for {session.display_name}: {mapping.axisName}"
                    title="Volume Axis: {mapping.axisName}"
                    onclick={() => removeMapping(session.process_name)}
                    type="button"
                  >
                    <span class="mapping-icon default" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M448 128C554 128 640 214 640 320C640 426 554 512 448 512L192 512C86 512 0 426 0 320C0 214 86 128 192 128L448 128zM192 240C178.7 240 168 250.7 168 264L168 296L136 296C122.7 296 112 306.7 112 320C112 333.3 122.7 344 136 344L168 344L168 376C168 389.3 178.7 400 192 400C205.3 400 216 389.3 216 376L216 344L248 344C261.3 344 272 333.3 272 320C272 306.7 261.3 296 248 296L216 296L216 264C216 250.7 205.3 240 192 240zM432 336C414.3 336 400 350.3 400 368C400 385.7 414.3 400 432 400C449.7 400 464 385.7 464 368C464 350.3 449.7 336 432 336zM496 240C478.3 240 464 254.3 464 272C464 289.7 478.3 304 496 304C513.7 304 528 289.7 528 272C528 254.3 513.7 240 496 240z"/>
                      </svg>
                    </span>
                    <span class="mapping-icon remove" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
                      </svg>
                    </span>
                  </button>
                {:else if isBindingMode && pendingBinding?.sessionId === session.session_id}
                  <!-- Binding in progress: Cancel binding -->
                  <button
                    class="btn btn-channel btn-disabled"
                    aria-label="Cancel axis binding for {session.display_name}"
                    title="Cancel Axis Binding"
                    onclick={cancelBinding}
                    type="button"
                  >
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                      <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z" />
                    </svg>
                  </button>
                {:else}
                  <!-- Unbound: Show bind button -->
                  <button class="btn btn-channel btn-bind btn-disabled" onclick={() => startAxisBinding(session.session_id, session.display_name, session.process_id, session.process_name)} aria-label="Bind hardware axis to control volume for {session.display_name}" title="Bind Volume Axis">
                    <span class="bind-icon default" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M448 128C554 128 640 214 640 320C640 426 554 512 448 512L192 512C86 512 0 426 0 320C0 214 86 128 192 128L448 128zM192 240C178.7 240 168 250.7 168 264L168 296L136 296C122.7 296 112 306.7 112 320C112 333.3 122.7 344 136 344L168 344L168 376C168 389.3 178.7 400 192 400C205.3 400 216 389.3 216 376L216 344L248 344C261.3 344 272 333.3 272 320C272 306.7 261.3 296 248 296L216 296L216 264C216 250.7 205.3 240 192 240zM432 336C414.3 336 400 350.3 400 368C400 385.7 414.3 400 432 400C449.7 400 464 385.7 464 368C464 350.3 449.7 336 432 336zM496 240C478.3 240 464 254.3 464 272C464 289.7 478.3 304 496 304C513.7 304 528 289.7 528 272C528 254.3 513.7 240 496 240z"/>
                      </svg>
                    </span>
                    <span class="bind-icon hover" aria-hidden="true">
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
                        <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
                      </svg>
                    </span>
                  </button>
                {/if}
                
                <!-- Axis Inversion Toggle (Always visible in edit mode) -->
                <button 
                  class="btn btn-channel {mapping ? 'btn-enabled' : 'btn-unavail'}" 
                  class:active={mapping?.inverted}
                  disabled={!mapping}
                  onclick={() => mapping && toggleAxisInversion(session.process_name)} 
                  aria-label="{mapping ? (mapping.inverted ? 'Disable' : 'Enable') : 'No axis binding'} axis inversion for {session.display_name}"
                  aria-pressed={mapping?.inverted ?? false}
                  title={mapping ? 'Reverse Axis Direction' : 'Bind an axis to enable inversion'}
                >
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                    <path d="M342.6 41.4C330.1 28.9 309.8 28.9 297.3 41.4L201.3 137.4C188.8 149.9 188.8 170.2 201.3 182.7C213.8 195.2 234.1 195.2 246.6 182.7L288 141.3L288 498.7L246.6 457.4C234.1 444.9 213.8 444.9 201.3 457.4C188.8 469.9 188.8 490.2 201.3 502.7L297.3 598.7C303.3 604.7 311.4 608.1 319.9 608.1C328.4 608.1 336.5 604.7 342.5 598.7L438.5 502.7C451 490.2 451 469.9 438.5 457.4C426 444.9 405.7 444.9 393.2 457.4L351.8 498.8L351.8 141.3L393.2 182.7C405.7 195.2 426 195.2 438.5 182.7C451 170.2 451 149.9 438.5 137.4L342.5 41.4z"/>
                  </svg>
                </button>

                <!-- Remove Application Button -->
                <button 
                  class="btn btn-channel btn-close" 
                  onclick={() => removeApplication(session.process_name)} 
                  aria-label="Remove {session.display_name} from mixer"
                  title="Remove Application"
                >
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                    <path d="M232.7 69.9L224 96L128 96C110.3 96 96 110.3 96 128C96 145.7 110.3 160 128 160L512 160C529.7 160 544 145.7 544 128C544 110.3 529.7 96 512 96L416 96L407.3 69.9C402.9 56.8 390.7 48 376.9 48L263.1 48C249.3 48 237.1 56.8 232.7 69.9zM512 208L128 208L149.1 531.1C150.7 556.4 171.7 576 197 576L443 576C468.3 576 489.3 556.4 490.9 531.1L512 208z"/>
                  </svg>
                </button>
              {/if}

              <!-- Application Name -->
              <span class="app-name" title={session.display_name}>{formatProcessName(session.process_name)}</span>

            </div>
          {/each}
    
              <!-- Add Application Column - Only in Edit Mode -->
              {#if isEditMode}
                {@const isBindingNewApp = isBindingMode && pendingBinding !== null && !boundSessions.some(s => s.process_name === pendingBinding?.processName)}
                {#if isBindingNewApp && pendingBinding}
                  <!-- Binding in Progress for NEW App (not already in boundSessions) -->
                  <div class="channel-strip add-app-column" role="group" aria-label="Binding in progress for {pendingBinding.sessionName}">
                    <!-- Application Name -->
                    <span class="app-name inactive">{formatProcessName(pendingBinding.processName)}</span>
    
                    <!-- Horizontal Volume Bar (Disabled) -->
                    <div class="volume-bar-container">
                      <input
                        type="range"
                        class="volume-slider"
                        min="0"
                        max="1"
                        step="0.01"
                        value={0.5}
                        style="--volume-percent: 50%"
                        disabled
                      />
                    </div>
    
                    <!-- Binding Active (Mute) -->
                    <button
                      class="btn btn-channel btn-disabled"
                      aria-label="Cancel mute binding"
                      title="Cancel Mute Binding"
                      onclick={cancelButtonBinding}
                      type="button"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z" />
                      </svg>
                    </button>
    
                    <!-- Binding Active (Axis) -->
                    <button
                      class="btn btn-channel btn-disabled"
                      aria-label="Cancel axis binding"
                      title="Cancel Axis Binding"
                      onclick={cancelBinding}
                      type="button"
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
                        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z" />
                      </svg>
                    </button>
                  </div>
                {:else}
                  <!-- Add Application Button (Expandable) -->
                  <div 
                    class="btn-add-app-container"
                    class:expanded={addAppListExpanded}
                  >
                    <!-- Application List (visible when expanded) -->
                    {#if addAppListExpanded}
                      <div class="add-app-list" role="listbox">
                        {#each availableSessions as session}
                          <button 
                            class="add-app-list-item"
                            role="option"
                            aria-selected="false"
                            onclick={(e) => {
                              e.stopPropagation();
                              // Add to pinned apps so it persists even without bindings
                              pinnedApps = new Set([...pinnedApps, session.process_name]);
                              savePinnedApps();
                              // Collapse list - the app will now appear as a regular channel strip
                              addAppListExpanded = false;
                            }}
                            aria-label="Select {formatProcessName(session.process_name)}"
                          >
                            {formatProcessName(session.process_name)}
                          </button>
                        {/each}
                      </div>
                    {/if}
                    
                    <button 
                      class="btn btn-add-app"
                      onclick={() => { addAppListExpanded = !addAppListExpanded; }}
                      disabled={availableSessions.length === 0}
                      aria-label={availableSessions.length > 0 ? (addAppListExpanded ? "Close application list" : "Add application") : "No applications available"}
                      title={availableSessions.length > 0 ? (addAppListExpanded ? "Close" : "Add Application") : "No applications available"}
                      aria-expanded={addAppListExpanded}
                    >
                      <!-- Plus Icon (rotates to X when expanded) -->
                      <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor" aria-hidden="true">
                        <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
                      </svg>
                    </button>
                  </div>
                {/if}
              {/if}
            </div>
    
            {#if pinnedApps.size > 0}
          <!-- Bottom hover zone for controls -->
          <div 
            class="controls-hover-zone" 
            class:expanded={helpExpanded || closeExpanded}
            onmouseenter={handleControlsEnter}
            onmouseleave={handleControlsLeave}
            role="region"
            aria-label="Application controls"
          >
            <div 
              class="controls-bar" 
              class:open={controlsOpen}
              class:expanded={helpExpanded || closeExpanded}
            >
              <!-- Help Button Container (same pattern as btn-add-app-container) -->
              <div 
                class="btn-add-app-container controls"
                class:expanded={helpExpanded}
                class:hidden={closeExpanded}
              >
                {#if helpExpanded}
                  <div class="add-app-list">
                    <p class="help-text"><strong>1.</strong> Click the + button to add an audio application</p>
                    <p class="help-text"><strong>2.</strong> Move a hardware axis to bind volume control</p>
                    <p class="help-text"><strong>3.</strong> Press a hardware button to bind mute toggle</p>
                  </div>
                {/if}
                <button 
                  class="btn btn-add-app"
                  onclick={() => {
                    helpExpanded = !helpExpanded;
                    if (helpExpanded) closeExpanded = false;
                  }}
                  aria-label={helpExpanded ? "Close help" : "Open help"}
                  title={helpExpanded ? "Close" : "Help"}
                  aria-expanded={helpExpanded}
                >
                  <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
                    {#if helpExpanded}
                      <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
                    {:else}
                      <path d="M224 224C224 171 267 128 320 128C373 128 416 171 416 224C416 266.7 388.1 302.9 349.5 315.4C321.1 324.6 288 350.7 288 392L288 416C288 433.7 302.3 448 320 448C337.7 448 352 433.7 352 416L352 392C352 390.3 352.6 387.9 355.5 384.7C358.5 381.4 363.4 378.2 369.2 376.3C433.5 355.6 480 295.3 480 224C480 135.6 408.4 64 320 64C231.6 64 160 135.6 160 224C160 241.7 174.3 256 192 256C209.7 256 224 241.7 224 224zM320 576C342.1 576 360 558.1 360 536C360 513.9 342.1 496 320 496C297.9 496 280 513.9 280 536C280 558.1 297.9 576 320 576z"/>
                    {/if}
                  </svg>
                </button>
              </div>
              
              <!-- Edit Button (not expandable, just a button) -->
              <div class="btn-add-app-container controls" class:hidden={helpExpanded || closeExpanded}>
                <button 
                  class="btn btn-add-app {isEditMode ? 'btn-enabled' : ''}"
                  onclick={toggleEditMode} 
                  disabled={!audioInitialised}
                  aria-label={isEditMode ? 'Exit edit mode' : 'Enter edit mode to configure bindings'}
                  title={isEditMode ? 'Exit Edit Mode' : 'Edit Bindings'}
                >
                  <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
                    <path d="M416.9 85.2L372 130.1L509.9 268L554.8 223.1C568.4 209.6 576 191.2 576 172C576 152.8 568.4 134.4 554.8 120.9L519.1 85.2C505.6 71.6 487.2 64 468 64C448.8 64 430.4 71.6 416.9 85.2zM338.1 164L122.9 379.1C112.2 389.8 104.4 403.2 100.3 417.8L64.9 545.6C62.6 553.9 64.9 562.9 71.1 569C77.3 575.1 86.2 577.5 94.5 575.2L222.3 539.7C236.9 535.6 250.2 527.9 261 517.1L476 301.9L338.1 164z"/>
                  </svg>
                </button>
              </div>
              
              <!-- Close Button Container (same pattern as btn-add-app-container) -->
              <div 
                class="btn-add-app-container controls"
                class:expanded={closeExpanded}
                class:hidden={helpExpanded}
              >
                {#if closeExpanded}
                  <div class="add-app-list">
                    <button 
                      class="btn-add-app"
                      onclick={async () => { await invoke('quit_application'); }}
                    >
                      Quit ClearComms
                    </button>
                    <button 
                      class="btn-add-app"
                      onclick={async () => { const window = (await import('@tauri-apps/api/window')).Window.getCurrent(); await window.hide(); closeExpanded = false; }}
                    >
                      Minimise to Tray
                    </button>
                  </div>
                {/if}
                <button 
                  class="btn btn-add-app"
                  onclick={() => {
                    closeExpanded = !closeExpanded;
                    if (closeExpanded) helpExpanded = false;
                  }}
                  aria-label={closeExpanded ? "Cancel" : "Close application"}
                  title={closeExpanded ? "Cancel" : "Quit"}
                  aria-expanded={closeExpanded}
                >
                  {#if closeExpanded}
                    <span>Return</span>
                  {:else}
                    <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
                      <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
                    </svg>
                  {/if}
                </button>
              </div>
            </div>
          </div>
      {/if}
      {:else}
        <!-- Onboarding View -->
        <div class="onboarding-container" id="main-content">
          
          <!-- Add Application Button (Circular when collapsed, centered) -->
          <div 
            class="btn-add-app-container onboarding"
            class:expanded={addAppListExpanded}
            class:hidden={gettingStartedExpanded}
          >
            <!-- Application List (visible when expanded) -->
            {#if addAppListExpanded}
              <div class="add-app-list" role="listbox">
                {#each availableSessions as session}
                  <button 
                    class="add-app-list-item"
                    role="option"
                    aria-selected="false"
                    onclick={(e) => {
                      e.stopPropagation();
                      pinnedApps = new Set([...pinnedApps, session.process_name]);
                      savePinnedApps();
                      addAppListExpanded = false;
                      isEditMode = true;
                    }}
                    aria-label="Select {formatProcessName(session.process_name)}"
                  >
                    {formatProcessName(session.process_name)}
                  </button>
                {/each}
              </div>
            {/if}
            
            <button 
              class="btn btn-add-app"
              onclick={() => { 
                addAppListExpanded = !addAppListExpanded;
                if (addAppListExpanded) gettingStartedExpanded = false;
              }}
              disabled={availableSessions.length === 0}
              aria-label={availableSessions.length > 0 ? (addAppListExpanded ? "Close application list" : "Add application") : "No applications available"}
              title={availableSessions.length > 0 ? (addAppListExpanded ? "Close" : "Add Application") : "No applications available"}
              aria-expanded={addAppListExpanded}
            >
              <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor" aria-hidden="true">
                <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
              </svg>
            </button>
          </div>
        </div>
      {/if}
    {:else}
      <p class="status-text">Initialising...</p>
    {/if}

  <!-- Getting Started Button at Bottom (only in onboarding) -->
  {#if !isEditMode && pinnedApps.size === 0}
    <!-- <div class="onboarding-bottom-controls">
      <div 
        class="btn-getting-started-container"
        class:expanded={gettingStartedExpanded}
      >
        {#if gettingStartedExpanded}
          <div class="getting-started-content">
            <p><strong>1.</strong> Click the + button to add an audio application</p>
            <p><strong>2.</strong> Move a hardware axis to bind volume control</p>
            <p><strong>3.</strong> Press a hardware button to bind mute toggle</p>
          </div>
        {/if}

        <button 
          class="btn btn-getting-started"
          onclick={() => { 
            gettingStartedExpanded = !gettingStartedExpanded;
            if (gettingStartedExpanded) addAppListExpanded = false;
          }}
          aria-expanded={gettingStartedExpanded}
          aria-label={gettingStartedExpanded ? "Close getting started guide" : "Open getting started guide"}
        >
          <span class="getting-started-text">Getting Started</span>
        </button>
      </div>
    </div> -->
  {/if}

  <footer>
    <p style="font-size: 0.8rem; color: var(--text-muted); text-align: center; margin: 0;">
      Crafted by <a href="https://cameroncarlyon.com" onclick={async (e) => { e.preventDefault(); await invoke('open_url', { url: 'https://cameroncarlyon.com' }); }} class="author-link" aria-label="Visit Cameron Carlyon's website (opens in external browser)">Cameron Carlyon</a>
    </p>
  </footer>
</main>
{:else}
  <!-- Boot Screen -->
  <div class="boot-screen" role="status" aria-live="polite">
    <h1 class="boot-title">ClearComms</h1>
    <p class="boot-status" class:error={initStatus === 'Failed'} role={initStatus === 'Failed' ? 'alert' : 'status'}>
      {initStatus === 'Failed' ? errorMsg : initStatus}
    </p>
    {#if initStatus === 'Failed'}
      <button class="btn btn-restart" onclick={() => window.location.reload()} aria-label="Restart application">
        Restart Application
      </button>
    {/if}
  </div>
{/if}

<style>
  /* Prevent text selection across the app for a cleaner control surface */
  :global(body) {
    -webkit-user-select: none;
    -ms-user-select: none;
    user-select: none;
  }

  * {
    box-sizing: border-box;
  }

  /* Skip link for keyboard navigation accessibility */
  .skip-link {
    position: absolute;
    top: -40px;
    left: 0;
    background: var(--text-primary);
    color: var(--bg-primary);
    padding: 8px 16px;
    text-decoration: none;
    border-radius: 0 0 4px 0;
    z-index: 100;
    font-weight: 600;
  }

  .skip-link:focus {
    top: 0;
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
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

  /* Ensure content is above overlay */
  .mixer-container,
  .status-text,
  .error-banner,
  footer {
    z-index: 2;
  }

  h1 {
    margin: 0;
    font-size: 1.3rem;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.3px;
  }

  .btn {
    padding: 0;
    background: var(--text-primary);
    border: none;
    color: var(--bg-primary);
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .btn:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }

  .btn:active:not(:disabled) {
    transform: scale(0.98);
  }

  .btn-close {
    width: 46px;
    height: 46px;
    border-radius: 50%;
    font-size: 1.3rem;
    font-weight: 600;
    background: #ff4444;
    color: white;
    border: 2px solid #ff4444;
    transition: background 0.2s ease, color 0.2s ease, box-shadow 0.2s ease;
  }

  .btn-close:hover:not(:disabled) {
    box-shadow: 0 0 100px rgba(255, 68, 68, 0.35);
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

  footer {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    color: var(--text-muted);
  }

  .author-link {
    color: var(--text-muted);
    text-decoration: none;
    cursor: pointer;
    transition: color 0.2s ease, filter 0.2s ease;
    display: inline-block;
  }

  .author-link:hover {
    color: var(--text-primary);
    filter: drop-shadow(0 0 30px rgba(255, 255, 255, 1)) drop-shadow(0 0 60px rgba(255, 255, 255, 0.8)) drop-shadow(0 0 100px rgba(255, 255, 255, 0.6)) drop-shadow(0 0 140px rgba(255, 255, 255, 0.4));
  }

  /* ===== MIXER LAYOUT ===== */
  .mixer-container {
    display: flex;
    flex-direction: row;
    justify-content: center;
    gap: 3rem;
    overflow: visible;
    flex: 1;
    min-height: 0;
    align-items: center;
    transition: opacity 0.3s ease, transform 0.3s ease;
  }

  /* ===== CHANNEL STRIP (Vertical Layout) ===== */
  .channel-strip {
    display: flex;
    height: 100%;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    transition: all 0.2s ease;
  }

  /* Inactive session channel styling */
  .channel-strip.inactive {
    opacity: 0.5;
  }

  .channel-strip.inactive-edit-mode {
    opacity: 1;
  }

  .channel-strip.inactive .volume-slider {
    pointer-events: none;
  }

  .channel-strip.inactive .app-name {
    color: var(--text-muted);
  }

  /* ===== ADD APPLICATION COLUMN ===== */
  .channel-strip.add-app-column {
    opacity: 0.6;
    justify-content: center;
  }

  .channel-strip.add-app-column .volume-slider {
    pointer-events: none;
  }

  .channel-strip.add-app-column .btn:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .controls-bar {
    display: flex;
    flex-direction: row;
    justify-content: center;
    align-items: flex-end;
    gap: 1rem;
    width: 100%;
    height: 0;
    max-height: 0;
    overflow: visible;
    transition: height 0.3s ease, max-height 0.3s ease, padding 0.3s ease, gap 0.3s ease;
    position: relative;
  }

  .controls-bar.open {
    height: 60px;
    max-height: 60px;
    overflow: visible;
  }

  /* When any menu is expanded, controls-bar expands to accommodate */
  .controls-bar.expanded {
    height: 200px;
    max-height: 200px;
    overflow: visible;
    gap: 0;
  }

  /* Controls variant of btn-add-app-container - matches standard btn-add-app-container */
  .btn-add-app-container.controls {
    height: 46px;
    width: 46px;
    justify-content: flex-end;
    transition: width 0.3s ease, height 0.3s ease, background 0.3s ease, border-color 0.3s ease, transform 0.3s ease, opacity 0.3s ease;
    transform: scale(0);
    align-self: flex-end;
  }

  .btn-add-app-container.controls.hidden {
    transform: scale(0) !important;
    opacity: 0;
    pointer-events: none;
    width: 0 !important;
    margin: 0;
    overflow: hidden;
  }

  .btn-add-app-container.controls .btn-add-app {
    height: 46px;
  }

  /* Expanded state - fills available space in controls-bar */
  .btn-add-app-container.controls.expanded {
    height: fit-content;
    width: 100%;
    flex: 1;
    transform: scale(1) !important;
    background: var(--bg-card);
    border-color: var(--text-muted);
    justify-content: flex-start;
  }

  .btn-add-app-container.controls .btn-add-app.btn-enabled {
    border-color: var(--text-primary);
    color: var(--text-primary);
  }

  /* Help text styling */
  .help-text {
    padding: 0.75rem 1rem;
    margin: 0;
    font-size: 0.8rem;
    line-height: 1.4;
    text-align: left;
  }

  .help-text strong {
    color: var(--text-primary);
  }

  .controls-hover-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    width: 100%;
    flex-shrink: 0;
    min-height: 12px;
    cursor: pointer;
    transition: min-height 0.3s ease;
  }

  .controls-hover-zone.expanded {
    min-height: auto;
    cursor: default;
  }

  .controls-hover-zone::before {
    content: '';
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--text-muted);
    opacity: 0.3;
    transition: opacity 0.3s ease;
    margin: 4px 0;
  }

  .controls-hover-zone.expanded::before {
    display: none;
  }

  .controls-hover-zone:hover::before {
    opacity: 0.6;
  }

  .controls-hover-zone.expanded .controls-bar {
    max-height: 50vh;
  }

  .controls-hover-zone:hover .btn-add-app-container.controls {
    transform: scale(1);
  }

  .controls-hover-zone:hover .btn-add-app-container.controls.hidden {
    transform: scale(0) !important;
  }

  .controls-hover-zone:hover .btn-add-app-container.controls.expanded {
    transform: scale(1) !important;
  }

  /* ===== ONBOARDING VIEW ===== */
  .onboarding-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    min-height: 0;
    gap: 1.5rem;
  }

  /* Onboarding variant: circular when collapsed, expands upward */
  .btn-add-app-container.onboarding {
    height: 46px;
    width: 46px;
    justify-content: flex-end;
    transition: width 0.3s ease, height 0.3s ease, background 0.3s ease, border-color 0.3s ease, transform 0.3s ease, opacity 0.3s ease;
  }

  .btn-add-app-container.onboarding.hidden {
    transform: scale(0);
    opacity: 0;
    pointer-events: none;
  }

  .btn-add-app-container.onboarding .btn-add-app {
    height: 46px;
  }

  .btn-add-app-container.onboarding.expanded {
    height: 100%;
    width: 180px;
  }

  /* ===== ADD APP BUTTON CONTAINER ===== */
  .btn-add-app-container {
    position: relative;
    width: 46px;
    height: 100%;
    border-radius: 29px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    transition: width 0.3s ease, background 0.3s ease, border-color 0.3s ease;
    background: transparent;
    border: 1px solid transparent;
  }

  .btn-add-app-container.expanded {
    width: 180px;
    background: var(--bg-card);
    border-color: var(--text-muted);
    justify-content: flex-start;
  }

  /* ===== ADD APP BUTTON (Expandable) ===== */
  .btn-add-app {
    width: 46px;
    height: 100%;
    min-width: 46px;
    border-radius: 23px;
    background: var(--bg-card);
    border: 1px solid var(--text-muted);
    color: white;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.2s ease, box-shadow 0.2s ease, height 0.3s ease, background 0.3s ease;
    flex-shrink: 0;
  }

  .btn-add-app-container.expanded .btn-add-app {
    width: calc(100% - 12px);
    height: 46px;
    min-height: 46px;
    margin: 6px;
    background: transparent;
    border-color: transparent;
    border-radius: 23px;
  }

  .btn-add-app:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    color: var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.1);
  }

  .btn-add-app-container.expanded .btn-add-app:hover:not(:disabled) {
    border-color: transparent;
    box-shadow: none;
    background: var(--bg-card-hover);
  }

  .btn-add-app:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  /* Plus icon */
  .btn-add-app .add-app-icon {
    width: 24px;
    height: 24px;
    opacity: 1;
    transition: opacity 0.2s ease, transform 0.3s ease;
  }

  .btn-add-app-container.expanded .add-app-icon {
    opacity: 1;
    transform: rotate(45deg);
  }

  /* Do not rotate icons for controls-bar expandables */
  .btn-add-app-container.controls.expanded .add-app-icon {
    transform: none;
  }

  /* Application list container */
  .add-app-list {
    display: flex;
    flex-direction: column;
    width: 100%;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
  }

  .add-app-list::-webkit-scrollbar {
    display: none;
  }

  /* List items */
  .add-app-list-item {
    padding: 1rem;
    background: transparent;
    border: none;
    border-radius: 23px;
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
  }

  .add-app-list-item:hover {
    background: var(--bg-card-hover);
  }

  .add-app-list-item:active {
    background: var(--text-primary);
    color: var(--bg-primary);
  }

  /* ===== APP NAME ===== */
  .app-name {
    text-align: center;
    font-size: 0.8rem;
    font-weight: 700;
    color: var(--text-primary);
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 3rem;
  }

  .app-name.inactive {
    color: var(--text-muted);
    font-weight: 500;
  }

  /* ===== VOLUME BAR ===== */
  .volume-bar-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 100%;
    flex: 1;
    min-height: 0;
  }

  .volume-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 46px;
    flex: 1;
    min-height: 0;
    background: var(--bg-card);
    border: 0.5px solid var(--text-muted);
    cursor: pointer;
    position: relative;
    writing-mode: vertical-lr;
    direction: rtl;
    border-radius: 2rem;
  }

  /* Track styling */
  .volume-slider::-webkit-slider-runnable-track {
    width: 46px;
    height: 100%;
    background: linear-gradient(
      to top,
      var(--text-primary) 5.7%,
      var(--text-primary) calc(5.7% + var(--volume-percent, 0%) * 0.886),
      var(--bg-card) calc(5.7% + var(--volume-percent, 0%) * 0.886),
      var(--bg-card) 94.3%
    );
    border-radius: 2rem;
    cursor: pointer;
  }

  .volume-slider::-moz-range-track {
    width: 46px;
    height: 100%;
    background: var(--bg-card);
    border-radius: 23px;
    cursor: pointer;
    /* box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2); */
  }

  /* Progress fill for Firefox */
  .volume-slider::-moz-range-progress {
    width: 46px;
    background: var(--text-primary);
    border-radius: 0 0 23px 23px;
  }

  /* Thumb styling - responsive circle */
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 46px;
    height: 46px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    border: none;
  }

  .volume-slider::-moz-range-thumb {
    width: 46px;
    height: 46px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    border: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
  }

  /* Hover effect */
  .volume-slider:hover:not(:disabled)::-webkit-slider-runnable-track {
    border-color: rgba(255, 255, 255, 0.25);
  }

  .volume-slider:hover:not(:disabled) {
    filter: drop-shadow(0 0 40px rgba(255, 255, 255, 0.25));
  }

  .volume-slider {
    transition: filter 0.2s ease;
  }

  /* Focus state for accessibility */
  .volume-slider:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 4px;
    border-radius: 2rem;
  }

  /* Disabled state */
  .volume-slider:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .volume-slider:disabled::-webkit-slider-runnable-track {
    cursor: not-allowed;
  }

  /* ===== CHANNEL BUTTONS ===== */
  .btn-channel {
    box-sizing: border-box;
    width: 46px;
    height: 46px;
    min-width: 46px;
    min-height: 46px;
    max-width: 46px;
    max-height: 46px;
    border-radius: 50%;
    font-size: 1.3rem;
    transition: background 0.2s ease, color 0.2s ease, border 0.2s ease, box-shadow 0.2s ease;
    flex-shrink: 0;
    flex-grow: 0;
    /* box-shadow: 0 2px 10px rgba(0, 0, 0, 0.2); */
  }

  /* === Consolidated Button States === */
  
  /* btn-enabled: Solid white button with dark icon (appears as cutout) */
  .btn-enabled {
    border-radius: 50%;
    background: var(--text-primary);
    color: var(--bg-primary);
    border: 2px solid var(--text-primary);
  }

  .btn-enabled svg {
    fill: #181818;
  }

  .btn-enabled:hover:not(:disabled) {
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }

  /* btn-disabled: Outline button (empty state) with white box-shadow and border on hover */
  .btn-disabled {
    border-radius: 50%;
    background: var(--bg-card);
    color: var(--text-primary);
    border: 0.5px solid var(--text-muted);
    transition: border 0.2s ease, box-shadow 0.2s ease;
  }

  .btn-disabled:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.45);
  }

  /* btn-unavail: Unavailable/disabled appearance - no hover effects, no pointer */
  .btn-unavail {
    background: var(--bg-card);
    color: var(--text-primary);
    border: 0.5px solid var(--text-muted);
    cursor: not-allowed;
    pointer-events: none;
  }

  .btn-unavail:hover {
    box-shadow: none;
  }

  /* === Bind Button Icon Animation === */
  .btn-channel.btn-bind {
    position: relative;
  }

  .btn-bind .bind-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    inset: 0;
    transition: opacity 0.2s ease;
  }

  .btn-bind .bind-icon.default {
    opacity: 1;
  }

  .btn-bind .bind-icon.hover {
    opacity: 0;
    font-size: 1.8rem;
    font-weight: 300;
  }

  .btn-bind:hover .bind-icon.default {
    opacity: 0;
  }

  .btn-bind:hover .bind-icon.hover {
    opacity: 1;
  }

  /* === Button-Specific Overrides === */
  
  /* Invert button SVG animation */
  .btn-channel.active svg {
    transform: scaleY(-1);
  }

  .btn-channel svg {
    transition: transform 0.3s ease;
  }

  .mapping-badge {
    width: 46px;
    height: 46px;
    aspect-ratio: 1 / 1;
    position: relative;
    background: var(--text-primary);
    border: 2px solid var(--text-primary);
    border-radius: 50%;
    font-size: 1.3rem;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--bg-primary);
    cursor: pointer;
    transition: background 0.2s ease, color 0.2s ease, border-color 0.2s ease, box-shadow 0.2s ease;
    flex-shrink: 0;
  }

  .mapping-badge.button {
    background: var(--text-primary);
    color: var(--bg-primary);
  }

  .mapping-badge .mapping-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    inset: 0;
    transition: opacity 0.2s ease;
  }

  .mapping-badge .mapping-icon svg {
    width: 20px;
    height: 20px;
  }

  .mapping-badge .mapping-icon.remove {
    font-size: 1.8rem;
    font-weight: 200;
    opacity: 0;
  }

  .mapping-badge:hover {
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }

  .mapping-badge:hover .mapping-icon.default {
    opacity: 0;
  }

  .mapping-badge:hover .mapping-icon.remove {
    opacity: 1;
  }

  /* ===== BOOT SCREEN ===== */
  .boot-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100vw;
    background: transparent;
    gap: 1.5rem;
    padding: 2rem;
  }

  .boot-title {
    font-size: 2.5rem;
    font-weight: 700;
    color: var(--text-primary);
    margin: 0;
    letter-spacing: -0.5px;
  }

  .boot-status {
    font-size: 1rem;
    color: var(--text-secondary);
    margin: 0;
    text-align: center;
    max-width: 300px;
  }

  .boot-status.error {
    color: #ff4444;
  }

  .btn-restart {
    margin-top: 1rem;
    padding: 12px 24px;
    font-size: 1rem;
    background: var(--text-primary);
    color: var(--bg-primary);
    border-radius: 8px;
    font-weight: 500;
  }

  .btn-restart:hover {
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }
</style>