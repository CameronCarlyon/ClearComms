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
  let previousDisplayCount = $state(-1); // Track previous count to avoid unnecessary resizes
  let preMuteVolumes = $state<Map<string, number>>(new Map()); // Store volume before muting
  let isDraggingSlider = $state<Map<string, boolean>>(new Map()); // Track which sliders are being dragged
  let isAnimatingSlider = $state<Map<string, boolean>>(new Map()); // Track which sliders are animating

  // Computed: Get sessions with bindings (axis OR button mappings)
  $effect(() => {
    // Count all unique bound process names (includes placeholders for inactive apps)
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName)
    ]);
    
    // Calculate display count based on all bound processes (active + inactive)
    let displayCount = boundProcessNames.size;
    
    // Add ghost column width if edit mode is enabled with 2+ bound sessions
    if (isEditMode && displayCount >= 2) {
      displayCount += 1;
    }
    
    // Only resize if the count has actually changed
    if (audioInitialised && displayCount !== previousDisplayCount) {
      console.log(`[ClearComms] Display count changed: ${previousDisplayCount} → ${displayCount} (Edit mode: ${isEditMode}, Bound processes: ${boundProcessNames.size})`);
      previousDisplayCount = displayCount;
      resizeWindowToFit(displayCount);
    }
  });

  // Computed: Get bound audio sessions (deduplicated by process_name)
  function getBoundSessions(): AudioSession[] {
    const boundProcessNames = new Set([
      ...axisMappings.map(m => m.processName),
      ...buttonMappings.map(m => m.processName)
    ]);
    
    const sessions: AudioSession[] = [];
    const foundProcessNames = new Set<string>();
    
    // Add all active bound sessions (match by process_name, but only take first occurrence)
    for (const session of audioSessions) {
      if (boundProcessNames.has(session.process_name) && !foundProcessNames.has(session.process_name)) {
        sessions.push(session);
        foundProcessNames.add(session.process_name);
      }
    }
    
    // Add placeholder sessions for bound apps that aren't currently active
    for (const mapping of axisMappings) {
      if (!foundProcessNames.has(mapping.processName)) {
        console.log(`[ClearComms] Creating placeholder for ${mapping.processName} (axis binding)`);
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
    
    // Check button mappings too (in case app only has button binding, no axis)
    for (const mapping of buttonMappings) {
      if (!foundProcessNames.has(mapping.processName)) {
        console.log(`[ClearComms] Creating placeholder for ${mapping.processName} (button binding)`);
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

  // Computed: Get available (unbound) audio sessions for dropdown
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
    console.log(`[ClearComms] Edit mode: ${isEditMode ? 'ON' : 'OFF'}`);
  }

  async function quitApplication() {
    await invoke("quit_application");
  }

  // Auto-initialise on component mount
  onMount(async () => {
    loadMappings();
    loadButtonMappings();
    console.log("[ClearComms] Starting automatic initialisation...");
    await autoInitialise();
  });

  // Clean up polling on component destroy
  onDestroy(() => {
    stopPolling();
  });

  async function autoInitialise() {
    try {
      // Step 1: Initialise input system
      initStatus = "Initialising input system...";
      console.log("[ClearComms] Step 1: Initialising input system");
      const initResult = await invoke<string>("init_direct_input");
      console.log("[ClearComms] ✓ Input system initialised:", initResult);

      // Step 2: Enumerate devices
      initStatus = "Enumerating devices...";
      console.log("[ClearComms] Step 2: Enumerating devices");
      const deviceList = await invoke<string[]>("enumerate_input_devices");
      console.log(`[ClearComms] ✓ Found ${deviceList.length} device(s):`, deviceList);

      // Step 3: Get initial axis values
      initStatus = "Reading axis values...";
      console.log("[ClearComms] Step 3: Getting initial axis values");
      await getAxisValues();
      console.log("[ClearComms] ✓ Axis values retrieved");

      // Step 4: Initialise audio manager
      initStatus = "Initialising audio manager...";
      console.log("[ClearComms] Step 4: Initialising audio manager");
      try {
        const audioResult = await invoke<string>("init_audio_manager");
        console.log("[ClearComms] ✓", audioResult);
        audioInitialised = true;
        
        // Get initial audio sessions
        await refreshAudioSessions();
      } catch (audioError) {
        console.warn("[ClearComms] ⚠ Audio manager failed (non-critical):", audioError);
      }

      // Step 5: Start polling
      initStatus = "Starting real-time polling...";
      console.log("[ClearComms] Step 5: Starting real-time polling (20Hz)");
      startPolling();
      console.log("[ClearComms] ✓ Polling started");

      initStatus = "Ready";
      console.log("[ClearComms] === Initialisation complete ===");
      errorMsg = "";
    } catch (error) {
      const errorMessage = `Initialisation failed: ${error}`;
      errorMsg = errorMessage;
      initStatus = "Failed";
      console.error("[ClearComms] ✗ Initialisation error:", error);
    }
  }

  async function getAxisValues() {
    try {
      const data = await invoke<AxisData[]>("get_all_axis_values");
      axisData = data;
    } catch (error) {
      console.error("[ClearComms] Error getting axis values:", error);
      errorMsg = `Error: ${error}`;
      axisData = [];
    }
  }
  
  function startPolling() {
    if (pollingInterval) return;
    
    isPolling = true;
    pollingInterval = setInterval(async () => {
      try {
        await getAxisValues();
        await applyAxisMappings();
        await applyButtonMappings();
      } catch (error) {
        console.error("[ClearComms] Polling error:", error);
      }
    }, 50);
    
    // Start audio session monitoring (every 3 seconds)
    startAudioMonitoring();
  }
  
  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
      console.log("[ClearComms] Polling stopped");
    }
    isPolling = false;
    
    // Stop audio monitoring
    stopAudioMonitoring();
  }

  function startAudioMonitoring() {
    if (audioMonitorInterval) return;
    
    audioMonitorInterval = setInterval(async () => {
      try {
        // Check if default device changed
        const deviceChanged = await invoke<boolean>("check_default_device_changed");
        if (deviceChanged) {
          console.log("[ClearComms] Audio device changed - refreshing sessions");
        }
        
        // Always refresh sessions to pick up new/closed apps
        await refreshAudioSessions();
      } catch (error) {
        console.error("[ClearComms] Audio monitoring error:", error);
      }
    }, 3000); // Check every 3 seconds
    
    console.log("[ClearComms] Audio session monitoring started (3s interval)");
  }

  function stopAudioMonitoring() {
    if (audioMonitorInterval) {
      clearInterval(audioMonitorInterval);
      audioMonitorInterval = null;
      console.log("[ClearComms] Audio monitoring stopped");
    }
  }

  async function refreshAudioSessions() {
    try {
      const sessions = await invoke<AudioSession[]>("get_audio_sessions");
      audioSessions = sessions;
      
      // Clean up stale mappings for sessions that no longer exist
      cleanupStaleMappings();
      
      // Window will be resized by the $effect that watches axisMappings/buttonMappings
    } catch (error) {
      console.error("[ClearComms] Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function resizeWindowToFit(sessionCount: number) {
    try {
      const result = await invoke<string>("resize_window_to_content", { sessionCount });
      console.log(`[ClearComms] ${result}`);
    } catch (error) {
      console.error("[ClearComms] Error resizing window:", error);
    }
  }

  function setSessionVolumeImmediate(sessionId: string, volume: number) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    // Update local state immediately for responsive UI (no backend call)
    const sessionIndex = audioSessions.findIndex(s => s.session_id === sessionId);
    if (sessionIndex !== -1) {
      audioSessions[sessionIndex].volume = volume;
      audioSessions[sessionIndex].is_muted = volume === 0;
    }
  }
  
  async function setSessionVolumeAnimated(sessionId: string, targetVolume: number) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    // Find the current session
    const session = audioSessions.find(s => s.session_id === sessionId);
    if (!session) return;
    
    // Mark as animating
    isAnimatingSlider.set(sessionId, true);
    
    // Animate from current volume to target volume
    await animateVolumeUI(sessionId, session.volume, targetVolume);
    
    // Apply to backend after animation
    await setSessionVolumeFinal(sessionId, targetVolume);
    
    // Clear animating flag
    isAnimatingSlider.delete(sessionId);
  }
  
  async function setSessionVolumeFinal(sessionId: string, volume: number) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    // Called when user finishes dragging - apply to backend and refresh
    try {
      await invoke("set_session_volume", { sessionId, volume });
      
      // Auto-update mute state based on volume
      const shouldBeMuted = volume === 0;
      await invoke("set_session_mute", { sessionId, muted: shouldBeMuted });
      
      await refreshAudioSessions();
    } catch (error) {
      console.error("[ClearComms] Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }
  
  async function animateVolumeUI(sessionId: string, fromVolume: number, toVolume: number, durationMs: number = 300) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    const startTime = Date.now();
    const volumeDelta = toVolume - fromVolume;
    
    const animate = () => {
      const elapsed = Date.now() - startTime;
      const progress = Math.min(elapsed / durationMs, 1);
      
      // Ease-out curve for smooth deceleration
      const easeOut = 1 - Math.pow(1 - progress, 3);
      const currentVolume = fromVolume + (volumeDelta * easeOut);
      
      // Update UI only (no backend calls during animation)
      setSessionVolumeImmediate(sessionId, currentVolume);
      
      if (progress < 1) {
        requestAnimationFrame(animate);
      }
    };
    
    animate();
    
    // Wait for animation to complete
    await new Promise(resolve => setTimeout(resolve, durationMs));
  }

  async function animateVolume(sessionId: string, fromVolume: number, toVolume: number, durationMs: number = 300) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    const startTime = Date.now();
    const volumeDelta = toVolume - fromVolume;
    
    const animate = async () => {
      const elapsed = Date.now() - startTime;
      const progress = Math.min(elapsed / durationMs, 1);
      
      // Ease-out curve for smooth deceleration
      const easeOut = 1 - Math.pow(1 - progress, 3);
      const currentVolume = fromVolume + (volumeDelta * easeOut);
      
      try {
        await invoke("set_session_volume", { sessionId, volume: currentVolume });
        await refreshAudioSessions();
      } catch (error) {
        console.error("[ClearComms] Error animating volume:", error);
        return;
      }
      
      if (progress < 1) {
        requestAnimationFrame(animate);
      }
    };
    
    await animate();
  }

  async function setSessionMute(sessionId: string, muted: boolean) {
    // Skip placeholder sessions (inactive apps)
    if (sessionId.startsWith('placeholder_')) {
      return;
    }
    
    try {
      // Find the current session
      const session = audioSessions.find(s => s.session_id === sessionId);
      
      if (muted && session) {
        // Store current volume before muting (only if not already zero)
        if (session.volume > 0) {
          preMuteVolumes.set(sessionId, session.volume);
        }
        // Animate volume to 0
        await animateVolume(sessionId, session.volume, 0);
      } else if (!muted) {
        // Restore previous volume or default to 0.5
        const previousVolume = preMuteVolumes.get(sessionId) ?? 0.5;
        await animateVolume(sessionId, 0, previousVolume);
        preMuteVolumes.delete(sessionId);
      }
      
      await invoke("set_session_mute", { sessionId, muted });
      console.log(`[ClearComms] Set mute for ${sessionId} to ${muted}`);
      await refreshAudioSessions();
    } catch (error) {
      console.error("[ClearComms] Error setting mute:", error);
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
    
    console.log(`[ClearComms] Binding mode activated for ${sessionName}. Move an axis to bind it...`);
  }

  function cancelBinding() {
    isBindingMode = false;
    pendingBinding = null;
    previousAxisValues.clear();
    console.log("[ClearComms] Binding mode cancelled");
  }

  function startButtonBinding(sessionId: string, sessionName: string, processId: number, processName: string) {
    isButtonBindingMode = true;
    pendingButtonBinding = { sessionId, sessionName, processId, processName };
    
    // Store current button states
    previousButtonStates.clear();
    for (const device of axisData) {
      previousButtonStates.set(device.device_handle, { ...device.buttons });
    }
    
    console.log(`[ClearComms] Button binding mode activated for ${sessionName}. Press a button to bind it...`);
  }

  function cancelButtonBinding() {
    isButtonBindingMode = false;
    pendingButtonBinding = null;
    previousButtonStates.clear();
    console.log("[ClearComms] Button binding mode cancelled");
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
          console.log(`[ClearComms] Detected movement on ${device.device_name} ${axisName}: ${previousValue.toFixed(3)} → ${currentValue.toFixed(3)} (Δ ${change.toFixed(3)})`);
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

        // Detect button press (false → true transition)
        if (!previousState && currentState) {
          console.log(`[ClearComms] Detected button press on ${device.device_name} ${buttonName}`);
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
        
        // Apply inversion if enabled
        if (mapping.inverted) {
          axisValue = 1.0 - axisValue;
        }
        
        // Apply deadzones at extremes (snap to 0% or 100%)
        const deadzoneThreshold = 0.01; // 1% deadzone at each end
        if (axisValue < deadzoneThreshold) {
          axisValue = 0.0;
        } else if (axisValue > (1.0 - deadzoneThreshold)) {
          axisValue = 1.0;
        }
        
        const mappingKey = `${mapping.deviceHandle}-${mapping.axisName}-${mapping.processName}`;
        const lastHardwareValue = lastHardwareAxisValues.get(mappingKey);
        
        // Only update if the hardware axis value has actually changed
        if (lastHardwareValue === undefined || Math.abs(lastHardwareValue - axisValue) > 0.01) {
          // Find session by process name (survives app restarts)
          const session = audioSessions.find(s => s.process_name === mapping.processName);
          
          if (session) {
            try {
              await invoke("set_session_volume", { sessionId: session.session_id, volume: axisValue });
              
              // Auto-update mute state based on volume
              const shouldBeMuted = axisValue === 0;
              await invoke("set_session_mute", { sessionId: session.session_id, muted: shouldBeMuted });
              
              // Update local state immediately for responsive UI
              const sessionIndex = audioSessions.findIndex(s => s.session_id === session.session_id);
              if (sessionIndex !== -1) {
                audioSessions[sessionIndex].volume = axisValue;
                audioSessions[sessionIndex].is_muted = shouldBeMuted;
              }
              // Store the new hardware value
              lastHardwareAxisValues.set(mappingKey, axisValue);
            } catch (error) {
              console.error(`[ClearComms] Error applying mapping for ${mapping.sessionName}:`, error);
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
      // Update previous button states for next poll
      for (const device of axisData) {
        previousButtonStates.set(device.device_handle, { ...device.buttons });
      }
      return;
    }

    if (!audioInitialised || buttonMappings.length === 0) return;

    // Check for button presses and toggle mute
    for (const mapping of buttonMappings) {
      const device = axisData.find(d => d.device_handle === mapping.deviceHandle);
      if (device && device.buttons[mapping.buttonName] !== undefined) {
        const currentState = device.buttons[mapping.buttonName];
        const previousState = previousButtonStates.get(device.device_handle)?.[mapping.buttonName];
        
        // Detect button press (false → true transition)
        if (!previousState && currentState) {
          // Find session by process name (survives app restarts)
          const session = audioSessions.find(s => s.process_name === mapping.processName);
          if (session) {
            const newMuteState = !session.is_muted;
            try {
              await invoke("set_session_mute", { sessionId: session.session_id, muted: newMuteState });
              const sessionIndex = audioSessions.findIndex(s => s.session_id === session.session_id);
              if (sessionIndex !== -1) {
                audioSessions[sessionIndex].is_muted = newMuteState;
              }
              console.log(`[ClearComms] Button ${mapping.buttonName} toggled mute for ${mapping.sessionName}: ${newMuteState ? 'MUTED' : 'UNMUTED'}`);
            } catch (error) {
              console.error(`[ClearComms] Error toggling mute for ${mapping.sessionName}:`, error);
            }
          }
        }
      }
    }

    // Update previous button states for next poll
    for (const device of axisData) {
      previousButtonStates.set(device.device_handle, { ...device.buttons });
    }
  }

  function cleanupStaleMappings() {
    // DISABLED: We want mappings to persist even when apps are closed
    // This allows placeholders to show inactive bound apps
    // Mappings are now matched by processName which persists across app restarts
    return;
  }

  function saveMappings() {
    try {
      localStorage.setItem('clearcomms_axis_mappings', JSON.stringify(axisMappings));
      console.log("[ClearComms] Axis mappings saved:", axisMappings);
    } catch (error) {
      console.error("[ClearComms] Error saving mappings:", error);
    }
  }

  function loadMappings() {
    console.log("[ClearComms] loadMappings() called");
    try {
      const saved = localStorage.getItem('clearcomms_axis_mappings');
      console.log("[ClearComms] localStorage data:", saved);
      if (saved) {
        axisMappings = JSON.parse(saved);
        console.log(`[ClearComms] Loaded ${axisMappings.length} axis mapping(s):`);
        console.log(axisMappings);
      } else {
        console.log("[ClearComms] No saved axis mappings found");
      }
    } catch (error) {
      console.error("[ClearComms] Error loading mappings:", error);
    }
  }

  function saveButtonMappings() {
    try {
      localStorage.setItem('clearcomms_button_mappings', JSON.stringify(buttonMappings));
      console.log("[ClearComms] Button mappings saved:", buttonMappings);
    } catch (error) {
      console.error("[ClearComms] Error saving button mappings:", error);
    }
  }

  function loadButtonMappings() {
    console.log("[ClearComms] loadButtonMappings() called");
    try {
      const saved = localStorage.getItem('clearcomms_button_mappings');
      console.log("[ClearComms] localStorage data:", saved);
      if (saved) {
        buttonMappings = JSON.parse(saved);
        console.log(`[ClearComms] Loaded ${buttonMappings.length} button mapping(s):`);
        console.log(buttonMappings);
      } else {
        console.log("[ClearComms] No saved button mappings found");
      }
    } catch (error) {
      console.error("[ClearComms] Error loading button mappings:", error);
    }
  }
</script>

{#if initStatus === 'Ready'}
  <!-- Main Application -->
  <main class="container">
    <header class="app-header">
      <h1>ClearComms</h1>
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
        <button class="btn btn-round btn-close" onclick={quitApplication} title="Quit">
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
            
            <div class="channel-strip" class:has-mapping={!!mapping || !!buttonMapping} class:inactive={isPlaceholder}>
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
                    // Ignore if currently animating
                    if (isAnimatingSlider.get(session.session_id)) {
                      e.preventDefault();
                      return;
                    }
                    
                    // Store start position
                    isDraggingSlider.set(session.session_id, false);
                  }}
                  oninput={(e) => {
                    const slider = e.currentTarget as HTMLInputElement;
                    const newValue = parseFloat(slider.value);
                    const currentValue = session.volume;
                    
                    // Don't allow input during animation
                    if (isAnimatingSlider.get(session.session_id)) {
                      e.preventDefault();
                      slider.value = currentValue.toString();
                      return;
                    }
                    
                    // Check if this is the first input event (click) or a drag
                    const wasDragging = isDraggingSlider.get(session.session_id);
                    
                    if (!wasDragging && Math.abs(newValue - currentValue) > 0.02) {
                      // First input after mousedown with significant difference = track click
                      // Prevent the instant snap and animate instead
                      e.preventDefault();
                      slider.value = currentValue.toString();
                      console.log(`[Volume Click] Track click detected: from ${currentValue.toFixed(3)} to ${newValue.toFixed(3)}`);
                      setSessionVolumeAnimated(session.session_id, newValue);
                      // Don't set dragging flag - this was a click
                    } else {
                      // Mark as dragging for subsequent inputs
                      isDraggingSlider.set(session.session_id, true);
                      setSessionVolumeImmediate(session.session_id, newValue);
                    }
                  }}
                  onchange={async (e) => {
                    if (isDraggingSlider.get(session.session_id)) {
                      // User was dragging - just finalize without animation
                      const targetVolume = parseFloat((e.target as HTMLInputElement).value);
                      await setSessionVolumeFinal(session.session_id, targetVolume);
                      isDraggingSlider.delete(session.session_id);
                    }
                  }}
                  onwheel={async (e) => {
                    e.preventDefault();
                    const delta = e.deltaY > 0 ? -0.05 : 0.05;
                    const newVolume = Math.max(0, Math.min(1, session.volume + delta));
                    await setSessionVolumeAnimated(session.session_id, newVolume);
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
      Crafted by <a href="https://cameroncarlyon.com" onclick={async (e) => { e.preventDefault(); await invoke('open_url', { url: 'https://cameroncarlyon.com' }); }} style="color: var(--text-secondary); text-decoration: none; cursor: pointer;">Cameron Carlyon</a> | &copy; {new Date().getFullYear()}
    </p>
  </footer>
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

  /* ===== BASE BUTTON STYLES ===== */
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

  /* Round button variant */
  .btn-round {
    border-radius: 50%;
  }

  /* Pill button variant */
  .btn-pill {
    border-radius: 20px;
  }

  /* Window control button */
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

  /* Edit mode button */
  .btn-edit {
    font-size: 0.85rem;
    font-weight: 600;
  }

  /* Icon button */
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

  .channel-strip.inactive .volume-slider {
    pointer-events: none;
  }

  .channel-strip.inactive .app-name {
    color: var(--text-muted);
  }

  /* ===== GHOST COLUMN ===== */
  .channel-strip.ghost-column {
    opacity: 0.5;
    border: 2px dashed var(--border-color);
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

  .app-dropdown {
    width: 100%;
    padding: 8px;
    background: var(--bg-light);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    color: var(--text-primary);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
  }

  .app-dropdown:hover {
    border-color: var(--text-secondary);
    background: var(--bg-medium);
  }

  .app-dropdown:focus {
    outline: none;
    border-color: var(--text-primary);
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
    transition: background 0.3s ease-out;
  }

  .volume-slider::-moz-range-track {
    width: 46px;
    height: 100%;
    background: var(--bg-light);
    border-radius: 23px;
    cursor: pointer;
    transition: background 0.3s ease-out;
  }

  /* Progress fill for Firefox */
  .volume-slider::-moz-range-progress {
    width: 46px;
    background: #fff;
    border-radius: 0 0 23px 23px;
    transition: background 0.3s ease-out;
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
</style>
