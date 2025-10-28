<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  console.log("[ClearComms] Component script loaded");

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
  let showCloseConfirmation = $state(false);

  const POLL_LOG_INTERVAL = 200;
  const BUTTON_CACHE_LOG_INTERVAL = 200;
  const LIVE_UPDATE_MIN_INTERVAL_MS = 40;
  let pollInFlight = false;
  let pollIterations = 0;
  let skippedPolls = 0;
  let buttonCachePruneCounter = 0;

  interface LiveVolumeState {
    inFlight: boolean;
    lastSent: number;
    queuedVolume?: number;
    timerId?: number;
  }

  const liveVolumeState = new Map<string, LiveVolumeState>();

  // Track display count and resize window when bindings change
  $effect(() => {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName)
    ]);
    
    let displayCount = boundProcessNames.size;
    
    if (isEditMode && displayCount >= 2) {
      displayCount += 1;
    }
    
    if (audioInitialised && displayCount !== previousDisplayCount) {
      previousDisplayCount = displayCount;
      resizeWindowToFit(displayCount);
    }
  });

  // Get bound sessions with placeholders for inactive apps
  function getBoundSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName)
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
    
    // Add placeholders for inactive bound apps
    const allMappings = [...axisMappings, ...buttonMappings];
    for (const mapping of allMappings) {
      if (!foundProcessNames.has(mapping.processName)) {
        sessions.push({
          session_id: `placeholder_${mapping.processName}`,
          display_name: mapping.sessionName,
          process_id: 0,
          process_name: mapping.processName,
          volume: 0,
          is_muted: true
        });
        foundProcessNames.add(mapping.processName);
      }
    }
    
    return sessions;
  }

  // Get unbound sessions for dropdown
  function getAvailableSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName)
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
  }

  function showCloseDialog() {
    showCloseConfirmation = true;
  }

  function cancelClose() {
    showCloseConfirmation = false;
  }

  async function confirmClose() {
    showCloseConfirmation = false;
    await invoke("quit_application");
  }

  async function minimiseToTray() {
    showCloseConfirmation = false;
    const window = (await import("@tauri-apps/api/window")).Window.getCurrent();
    await window.hide();
  }

  async function quitApplication() {
    await invoke("quit_application");
  }

  onMount(async () => {
    loadMappings();
    loadButtonMappings();
    await autoInitialise();

    // Exit edit mode when window loses focus (minimised or switched away)
    const handleBlur = () => {
      if (isEditMode) {
        isEditMode = false;
        isBindingMode = false;
        isButtonBindingMode = false;
        pendingBinding = null;
        pendingButtonBinding = null;
      }
    };

    window.addEventListener('blur', handleBlur);

    // Clean up event listener on component destroy
    return () => {
      window.removeEventListener('blur', handleBlur);
    };
  });

  onDestroy(() => {
    stopPolling();
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
  }
  
  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
    }
    isPolling = false;
    pollInFlight = false;
    stopAudioMonitoring();
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

  async function resizeWindowToFit(sessionCount: number) {
    try {
      await invoke<string>("resize_window_to_content", { sessionCount });
    } catch (error) {
      console.error("Error resizing window:", error);
    }
  }

  // Update volume immediately in UI (no backend call)
  function setSessionVolumeImmediate(sessionId: string, volume: number) {
    if (sessionId.startsWith('placeholder_')) return;
    
    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex !== -1) {
      audioSessions[sessionIndex].volume = volume;
      audioSessions[sessionIndex].is_muted = volume === 0;
    }
  }

  function scheduleLiveVolumeUpdate(sessionId: string, volume: number) {
    if (sessionId.startsWith('placeholder_')) return;

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
    if (sessionId.startsWith('placeholder_')) return false;
    
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
    if (sessionId.startsWith('placeholder_')) return;
    
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
  
  // Apply volume to backend after animation completes
  async function setSessionVolumeFinal(sessionId: string, volume: number) {
    if (sessionId.startsWith('placeholder_')) return;
    
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
    if (sessionId.startsWith('placeholder_')) return;
    
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
              await invoke("set_session_volume", { sessionId: session.session_id, volume: axisValue });
              await invoke("set_session_mute", { sessionId: session.session_id, muted: axisValue === 0 });
              
              const sessionIndex = audioSessions.findIndex(s => s.session_id === session.session_id);
              if (sessionIndex !== -1) {
                audioSessions[sessionIndex].volume = axisValue;
                audioSessions[sessionIndex].is_muted = axisValue === 0;
              }
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
              try {
                await invoke("set_session_mute", { sessionId: session.session_id, muted: newMuteState });
                const sessionIndex = audioSessions.findIndex(s => s.session_id === session.session_id);
                if (sessionIndex !== -1) {
                  audioSessions[sessionIndex].is_muted = newMuteState;
                }
              } catch (error) {
                console.error(`Error toggling mute for ${mapping.sessionName}:`, error);
              }
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
    if (buttonCachePruneCounter % BUTTON_CACHE_LOG_INTERVAL === 0) {
      console.debug(`[ClearComms] Button state cache size ${previousButtonStates.size}; active handles ${activeHandles.size}`);
    }
  }

  // Mappings persist even when apps are closed
  function cleanupStaleMappings() {
    return;
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
</script>

{#if initStatus === 'Ready'}
  <!-- Main Application -->
  <main class="container">
    <header class="app-header">
      <div class="header-right">
        <button 
          class="btn btn-round btn-icon" 
          class:active={isEditMode}
          onclick={toggleEditMode} 
          disabled={!audioInitialised}
          title={isEditMode ? 'Exit Edit Mode' : 'Edit Bindings'}
        >
          {isEditMode ? '✓' : '✏️'}
        </button>
        <button class="btn btn-round btn-close" onclick={showCloseDialog} title="Quit">
          ✕
        </button>
      </div>
    </header>

    {#if errorMsg}
      <div class="error-banner">{errorMsg}</div>
    {/if}

    <!-- Audio Management Section -->

    {#if audioInitialised}
      {@const boundSessions = getBoundSessions()}
      {@const availableSessions = getAvailableSessions()}
      
      {#if boundSessions.length > 0 || isEditMode}
        <div class="mixer-container">
          {#each boundSessions as session (session.session_id)}
            {@const mapping = axisMappings.find(m => m.processName === session.process_name)}
            {@const buttonMapping = buttonMappings.find(m => m.processName === session.process_name)}
            {@const isPlaceholder = session.session_id.startsWith('placeholder_')}
            
            <div class="channel-strip" class:has-mapping={!!mapping || !!buttonMapping} class:inactive={isPlaceholder} class:inactive-edit-mode={isPlaceholder && isEditMode}>
              <!-- Application Name -->
              <span class="app-name" title={session.display_name}>{formatProcessName(session.process_name)}</span>

              <!-- Horizontal Volume Bar -->
              <div class="volume-bar-container">
                <input
                  type="range"
                  class="volume-slider"
                  min="0"
                  max="1"
                  step="0.01"
                  value={session.volume}
                  style="--volume-percent: {session.volume * 100}%"
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
                  <div class="mapping-badge button" title="Mute: {buttonMapping.buttonName}">
                    <span>🔘</span>
                    <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeButtonMapping(session.process_name)}>✕</button>
                  </div>
                {:else if isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
                  <div class="binding-active">
                    <span class="pulse">⏺</span>
                    <button class="btn btn-round btn-badge-small btn-badge-cancel" onclick={cancelButtonBinding}>✕</button>
                  </div>
                {:else}
                  <button class="btn btn-round btn-channel btn-bind" onclick={() => startButtonBinding(session.session_id, session.display_name, session.process_id, session.process_name)} title="Bind Mute Button">
                    🔘
                  </button>
                {/if}
              {:else}
                <!-- Mute Button (Normal Mode) -->
                <button
                  class="btn btn-round btn-channel btn-mute"
                  class:muted={session.is_muted}
                  onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                  title={session.is_muted ? 'Unmute' : 'Mute'}
                >
                  {session.is_muted ? '🔇' : '🔊'}
                </button>
              {/if}

              <!-- Axis Binding Control (Edit Mode Only) -->
              {#if isEditMode}
                {#if mapping}
                  <div class="mapping-badge" title="Volume: {mapping.axisName}">
                    <span>🎮</span>
                    <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeMapping(session.process_name)}>✕</button>
                  </div>
                  <!-- Axis Inversion Toggle -->
                  <button 
                    class="btn btn-round btn-channel btn-invert" 
                    class:active={mapping.inverted}
                    onclick={() => toggleAxisInversion(session.process_name)} 
                    title={mapping.inverted ? 'Axis Inverted' : 'Normal Axis Direction'}
                  >
                    ↕️
                  </button>
                {:else if isBindingMode && pendingBinding?.sessionId === session.session_id}
                  <div class="binding-active">
                    <span class="pulse">⏺</span>
                    <button class="btn btn-round btn-badge-small btn-badge-cancel" onclick={cancelBinding}>✕</button>
                  </div>
                {:else}
                  <button class="btn btn-round btn-channel btn-bind" onclick={() => startAxisBinding(session.session_id, session.display_name, session.process_id, session.process_name)} title="Bind Volume Axis">
                    🎮
                  </button>
                {/if}

                <!-- Remove Application Button -->
                <button 
                  class="btn btn-round btn-channel btn-remove-app" 
                  onclick={() => removeApplication(session.process_name)} 
                  title="Remove Application"
                >
                  ✕
                </button>
              {/if}
            </div>
          {/each}

          <!-- Ghost Column (Add New Binding) - Only in Edit Mode -->
          {#if isEditMode}
            <div class="channel-strip ghost-column">
              <!-- Application Name -->
              <span class="app-name ghost">
                {#if availableSessions.length > 0}
                  <select class="app-dropdown-inline" onchange={(e) => {
                    const sessionId = (e.target as HTMLSelectElement).value;
                    if (sessionId) {
                      const session = audioSessions.find(s => s.session_id === sessionId);
                      if (session) {
                        startAxisBinding(session.session_id, session.display_name, session.process_id, session.process_name);
                      }
                      (e.target as HTMLSelectElement).value = '';
                    }
                  }}>
                    <option value="">Select App...</option>
                    {#each availableSessions as session}
                      <option value={session.session_id}>{formatProcessName(session.process_name)}</option>
                    {/each}
                  </select>
                {:else}
                  All Bound
                {/if}
              </span>

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

              <!-- Ghost Mute Button -->
              <button class="btn btn-round btn-channel btn-bind" disabled title="Select an app first">
                🔘
              </button>

              <!-- Ghost Axis Binding Button -->
              <button class="btn btn-round btn-channel btn-bind" disabled title="Select an app first">
                🎮
              </button>
            </div>
          {/if}
        </div>
      {:else}
        <p class="status-text">
          {#if isEditMode && availableSessions.length > 0}
            Click "Add Binding" to bind your first application
          {:else if isEditMode}
            No active audio sessions available
          {:else}
            No bound applications. Click "Edit" to add bindings.
          {/if}
        </p>
      {/if}
    {:else}
      <p class="status-text">Initialising...</p>
    {/if}

  <footer>
    <p style="font-size: 0.8rem; color: var(--text-muted); text-align: center;">
      ClearComms
    </p>
    <p style="font-size: 0.8rem; color: var(--text-muted); text-align: center;">
      Crafted by <a href="https://cameroncarlyon.com" onclick={async (e) => { e.preventDefault(); await invoke('open_url', { url: 'https://cameroncarlyon.com' }); }} style="color: var(--text-secondary); text-decoration: none; cursor: pointer;">Cameron Carlyon</a> | &copy; {new Date().getFullYear()}
    </p>
  </footer>

  <!-- Close Confirmation Dialog -->
  {#if showCloseConfirmation}
    <div class="modal-overlay" onclick={cancelClose}>
      <div class="modal-dialog" onclick={(e) => e.stopPropagation()}>
        <h2 class="modal-title">Close ClearComms</h2>
        <p class="modal-message">Are you sure you would like to close ClearComms?</p>
        <div class="modal-buttons">
          <button class="btn btn-modal btn-close-confirm" onclick={confirmClose}>
            Close
          </button>
          <button class="btn btn-modal btn-minimise" onclick={minimiseToTray}>
            Minimise to System Tray
          </button>
          <button class="btn btn-modal btn-cancel" onclick={cancelClose}>
            Nevermind
          </button>
        </div>
      </div>
    </div>
  {/if}
</main>
{:else}
  <!-- Boot Screen -->
  <div class="boot-screen">
    <h1 class="boot-title">ClearComms</h1>
    <p class="boot-status" class:error={initStatus === 'Failed'}>
      {initStatus === 'Failed' ? errorMsg : initStatus}
    </p>
    {#if initStatus === 'Failed'}
      <button class="btn btn-round btn-restart" onclick={() => window.location.reload()}>
        Restart Application
      </button>
    {/if}
  </div>
{/if}

<style>
  :root {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
    font-size: 14px;
    line-height: 1.4;
    
    /* Monochrome color palette */
    --bg-dark: #1a1a1a;
    --bg-medium: #2a2a2a;
    --bg-light: #3a3a3a;
    --text-primary: #ffffff;
    --text-secondary: #cccccc;
    --text-muted: #888888;
    --border-color: rgba(255, 255, 255, 0.1);
    --shadow-soft: rgba(0, 0, 0, 0.3);
  }

  * {
    box-sizing: border-box;
  }

  main {
    display: flex;
    gap: 1rem;
    padding: 1rem;
    flex-direction: column;
    height: 100vh;
    max-height: 100vh;
    justify-content: space-between;
    overflow: hidden;
  }

  .container {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    background: transparent;
    display: flex;
    flex-direction: column;
    position: relative;
    border-radius: 20px;
  }

  /* Main glass content - inset from edges */
  .container::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(26, 26, 26, 0.92);
    border-radius: 20px;
    box-shadow: 
      0 0 0 1px rgba(255, 255, 255, 0.08),
      0 20px 60px var(--shadow-soft);
    z-index: 0;
  }

  /* Clean overlay */
  .container::after {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    pointer-events: none;
    z-index: 1;
    border-radius: 20px;
  }

  /* Ensure content is above overlay */
  .app-header,
  .mixer-container,
  .status-text,
  .error-banner,
  footer {
    z-index: 2;
  }

  .app-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 12px;
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
    color: var(--bg-dark);
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

  .btn:active:not(:disabled) {
    transform: scale(0.95);
  }

  .btn-round {
    border-radius: 50%;
  }

  .btn-close {
    width: 32px;
    height: 32px;
    font-size: 1rem;
    font-weight: 600;
  }

  .error-banner {
    padding: 10px 14px;
    margin-bottom: 12px;
    background: var(--bg-medium);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    color: var(--text-primary);
    font-size: 0.85rem;
    font-weight: 500;
  }

  .btn-icon {
    width: 34px;
    height: 34px;
    font-size: 1rem;
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

  /* ===== MIXER LAYOUT ===== */
  .mixer-container {
    display: flex;
    flex-direction: row;
    justify-content: center;
    gap: 14px;
    overflow-y: auto;
    overflow-x: hidden;
    flex: 1;
    min-height: 0;
    align-items: center;
  }

  /* ===== CHANNEL STRIP (Vertical Layout) ===== */
  .channel-strip {
    display: flex;
    height: 100%;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    padding: 0rem 1rem;
    min-width: 85px;
    max-width: 95px;
    transition: all 0.2s ease;
  }

  /* Inactive (placeholder) channel styling */
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

  /* ===== GHOST COLUMN ===== */
  .channel-strip.ghost-column {
    opacity: 0.5;
  }

  .channel-strip.ghost-column:hover {
    opacity: 0.7;
    border-color: var(--text-secondary);
  }

  .channel-strip.ghost-column .volume-slider {
    pointer-events: none;
  }

  .channel-strip.ghost-column .btn:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }

  .app-dropdown-inline {
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 700;
    cursor: pointer;
    outline: none;
    text-align: center;
    width: 100%;
    padding: 0;
    letter-spacing: -0.2px;
    appearance: none;
    -webkit-appearance: none;
  }

  .app-dropdown-inline:hover {
    color: var(--text-secondary);
  }

  .app-dropdown-inline option {
    background: var(--bg-dark);
    color: var(--text-primary);
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
    letter-spacing: -0.2px;
  }

  .app-name.ghost {
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
    -webkit-appearance: slider-vertical;
    appearance: slider-vertical;
    width: 46px;
    flex: 1;
    min-height: 0;
    background: transparent;
    outline: none;
    cursor: pointer;
    position: relative;
  }

  /* Track styling */
  .volume-slider::-webkit-slider-runnable-track {
    width: 46px;
    height: 100%;
    background: linear-gradient(
      to top,
      #fff 0%,
      #fff var(--volume-percent, 0%),
      var(--bg-light) var(--volume-percent, 0%),
      var(--bg-light) 100%
    );
    border-radius: 23px;
    cursor: pointer;
  }

  .volume-slider::-moz-range-track {
    width: 46px;
    height: 100%;
    background: var(--bg-light);
    border-radius: 23px;
    cursor: pointer;
  }

  /* Progress fill for Firefox */
  .volume-slider::-moz-range-progress {
    width: 46px;
    background: #fff;
    border-radius: 0 0 23px 23px;
  }

  /* Hide the thumb - we want just the fill effect */
  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 0;
    height: 0;
    opacity: 0;
  }

  .volume-slider::-moz-range-thumb {
    width: 0;
    height: 0;
    border: none;
    opacity: 0;
  }

  /* Hover effect */
  .volume-slider:hover:not(:disabled)::-webkit-slider-runnable-track {
    border-color: rgba(255, 255, 255, 0.25);
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
    width: 46px;
    height: 46px;
    aspect-ratio: 1 / 1;
    font-size: 1.3rem;
  }

  /* Mute button */
  .btn-mute {
    font-size: 1.4rem;
  }

  .btn-mute.muted {
    background: var(--bg-light);
    color: var(--text-primary);
    border: 2px solid var(--text-primary);
  }

  .btn-mute.muted:hover {
    background: var(--bg-medium);
  }

  .btn-invert {
    font-size: 1.2rem;
    background: var(--bg-light);
    color: var(--text-secondary);
    border: 2px solid var(--border-color);
  }

  .btn-invert.active {
    background: var(--text-primary);
    color: var(--bg-dark);
    border-color: var(--text-primary);
  }

  .btn-invert:hover {
    background: var(--bg-medium);
    border-color: var(--text-secondary);
  }

  .btn-invert.active:hover {
    background: var(--text-secondary);
  }

  .btn-remove-app {
    font-size: 1.2rem;
    background: #ff4444;
    color: white;
    border: 2px solid #ff4444;
  }

  .btn-remove-app:hover {
    background: #cc0000;
    border-color: #cc0000;
  }

  .mapping-badge {
    width: 46px;
    height: 46px;
    aspect-ratio: 1 / 1;
    position: relative;
    background: var(--bg-light);
    border: 2px solid var(--text-primary);
    border-radius: 50%;
    font-size: 1.3rem;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-primary);
  }

  .mapping-badge.button {
    background: var(--bg-light);
    border-color: var(--text-primary);
  }

  /* Small badge buttons */
  .btn-badge-small {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 20px;
    height: 20px;
    aspect-ratio: 1 / 1;
    font-size: 0.75rem;
    font-weight: bold;
  }

  .binding-active {
    width: 46px;
    height: 46px;
    aspect-ratio: 1 / 1;
    position: relative;
    background: var(--bg-light);
    border: 2px solid var(--text-primary);
    border-radius: 50%;
    font-size: 1.3rem;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: pulse-border 1.5s ease-in-out infinite;
    color: var(--text-primary);
  }

  .binding-active .pulse {
    color: var(--text-primary);
    animation: pulse-icon 1s ease-in-out infinite;
  }

  .btn-badge-cancel {
    background: var(--bg-light);
    border: 1px solid var(--text-secondary);
    color: var(--text-primary);
  }

  @keyframes pulse-border {
    0%, 100% { 
      border-color: var(--text-secondary);
    }
    50% { 
      border-color: var(--text-primary);
    }
  }

  @keyframes pulse-icon {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.7; transform: scale(1.15); }
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
    color: var(--bg-dark);
    border-radius: 8px;
    font-weight: 500;
  }

  .btn-restart:hover {
    background: var(--text-secondary);
    transform: translateY(-2px);
  }

  /* ===== MODAL DIALOG ===== */
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.8);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(4px);
  }

  .modal-dialog {
    background: var(--bg-dark);
    border: 2px solid var(--border-color);
    border-radius: 12px;
    padding: 2rem;
    max-width: 400px;
    width: 90%;
    box-shadow: 0 8px 32px var(--shadow-soft);
  }

  .modal-title {
    font-size: 1.5rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0 0 1rem 0;
    text-align: center;
  }

  .modal-message {
    font-size: 1rem;
    color: var(--text-secondary);
    margin: 0 0 1.5rem 0;
    text-align: center;
    line-height: 1.5;
  }

  .modal-buttons {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .btn-modal {
    padding: 12px 24px;
    font-size: 1rem;
    border-radius: 8px;
    font-weight: 500;
    transition: all 0.15s ease;
    cursor: pointer;
    border: 2px solid transparent;
  }

  .btn-close-confirm {
    background: #ff4444;
    color: white;
    border-color: #ff4444;
  }

  .btn-close-confirm:hover {
    background: #cc0000;
    border-color: #cc0000;
    transform: translateY(-1px);
  }

  .btn-minimise {
    background: var(--text-primary);
    color: var(--bg-dark);
    border-color: var(--text-primary);
  }

  .btn-minimise:hover {
    background: var(--text-secondary);
    border-color: var(--text-secondary);
    transform: translateY(-1px);
  }

  .btn-cancel {
    background: transparent;
    color: var(--text-secondary);
    border-color: var(--border-color);
  }

  .btn-cancel:hover {
    background: var(--bg-light);
    border-color: var(--text-secondary);
    color: var(--text-primary);
  }
</style>
