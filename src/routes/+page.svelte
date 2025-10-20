<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

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

  // Computed: Get sessions with bindings (axis OR button mappings)
  $effect(() => {
    // Only count mappings for sessions that actually exist
    const activeSessionIds = new Set(audioSessions.map(s => s.session_id));
    
    const boundSessionIds = new Set([
      ...axisMappings.filter(m => activeSessionIds.has(m.sessionId)).map(m => m.sessionId),
      ...buttonMappings.filter(m => activeSessionIds.has(m.sessionId)).map(m => m.sessionId)
    ]);
    
    // Calculate display count based only on bound sessions (not edit mode)
    // Window width should only change when bindings change, not when toggling edit mode
    const displayCount = boundSessionIds.size;
    
    console.log(`[ClearComms] Resize effect: axisMappings=${axisMappings.length}, buttonMappings=${buttonMappings.length}, activeBindings=${displayCount}`);
    
    // Resize window to fit bound sessions only
    if (audioInitialised) {
      resizeWindowToFit(displayCount);
    }
  });

  // Computed: Get bound audio sessions
  function getBoundSessions(): AudioSession[] {
    const boundProcessIds = new Set([
      ...axisMappings.map(m => m.processId),
      ...buttonMappings.map(m => m.processId)
    ]);
    
    return audioSessions.filter(s => boundProcessIds.has(s.process_id));
  }

  // Computed: Get available (unbound) audio sessions for dropdown
  function getAvailableSessions(): AudioSession[] {
    const boundProcessIds = new Set([
      ...axisMappings.map(m => m.processId),
      ...buttonMappings.map(m => m.processId)
    ]);
    
    return audioSessions.filter(s => !boundProcessIds.has(s.process_id));
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
      console.log(`[ClearComms] Found ${sessions.length} audio session(s):`);
      sessions.forEach(s => {
        console.log(`  - ${s.process_name} (${s.display_name}) [PID: ${s.process_id}]`);
      });
      
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

  async function setSessionVolume(sessionId: string, volume: number) {
    try {
      await invoke("set_session_volume", { sessionId, volume });
      console.log(`[ClearComms] Set volume for ${sessionId} to ${volume.toFixed(2)}`);
      await refreshAudioSessions();
    } catch (error) {
      console.error("[ClearComms] Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function setSessionMute(sessionId: string, muted: boolean) {
    try {
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
    axisMappings = axisMappings.filter(m => m.processId !== processId);
    
    const newMapping: AxisMapping = { deviceHandle, deviceName, axisName, sessionId, sessionName, processId, processName };
    axisMappings = [...axisMappings, newMapping];
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${axisName} → ${sessionName}`);
    saveMappings();
  }

  function removeMapping(processId: number) {
    const mapping = axisMappings.find(m => m.processId === processId);
    if (mapping) {
      console.log(`[ClearComms] Removed mapping: ${mapping.deviceName} ${mapping.axisName} → ${mapping.sessionName}`);
    }
    axisMappings = axisMappings.filter(m => m.processId !== processId);
    saveMappings();
  }

  function createButtonMapping(deviceHandle: string, deviceName: string, buttonName: string, sessionId: string, sessionName: string, processId: number, processName: string) {
    // Remove existing button mapping for this process (one button per process)
    buttonMappings = buttonMappings.filter(m => m.processId !== processId);
    
    const newMapping: ButtonMapping = { deviceHandle, deviceName, buttonName, sessionId, sessionName, processId, processName };
    buttonMappings = [...buttonMappings, newMapping];
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${buttonName} → Mute ${sessionName}`);
    saveButtonMappings();
  }

  function removeButtonMapping(processId: number) {
    const mapping = buttonMappings.find(m => m.processId === processId);
    if (mapping) {
      console.log(`[ClearComms] Removed button mapping: ${mapping.deviceName} ${mapping.buttonName} → Mute ${mapping.sessionName}`);
    }
    buttonMappings = buttonMappings.filter(m => m.processId !== processId);
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
        const axisValue = device.axes[mapping.axisName];
        const mappingKey = `${mapping.deviceHandle}-${mapping.axisName}-${mapping.processId}`;
        const lastHardwareValue = lastHardwareAxisValues.get(mappingKey);
        
        // Only update if the hardware axis value has actually changed
        if (lastHardwareValue === undefined || Math.abs(lastHardwareValue - axisValue) > 0.01) {
          // Find session by process ID (survives device changes)
          const session = audioSessions.find(s => s.process_id === mapping.processId);
          
          if (session) {
            try {
              await invoke("set_session_volume", { sessionId: session.session_id, volume: axisValue });
              // Update local state immediately for responsive UI
              const sessionIndex = audioSessions.findIndex(s => s.process_id === mapping.processId);
              if (sessionIndex !== -1) {
                audioSessions[sessionIndex].volume = axisValue;
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
          // Find session by process ID (survives device changes)
          const session = audioSessions.find(s => s.process_id === mapping.processId);
          if (session) {
            const newMuteState = !session.is_muted;
            try {
              await invoke("set_session_mute", { sessionId: session.session_id, muted: newMuteState });
              const sessionIndex = audioSessions.findIndex(s => s.process_id === mapping.processId);
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
    const activeProcessIds = new Set(audioSessions.map(s => s.process_id));
    
    const oldAxisCount = axisMappings.length;
    const oldButtonCount = buttonMappings.length;
    
    // Remove mappings for processes that no longer have active audio sessions
    axisMappings = axisMappings.filter(m => activeProcessIds.has(m.processId));
    buttonMappings = buttonMappings.filter(m => activeProcessIds.has(m.processId));
    
    const removedAxis = oldAxisCount - axisMappings.length;
    const removedButton = oldButtonCount - buttonMappings.length;
    
    if (removedAxis > 0 || removedButton > 0) {
      console.log(`[ClearComms] Cleaned up ${removedAxis} stale axis mapping(s) and ${removedButton} stale button mapping(s)`);
      saveMappings();
      saveButtonMappings();
    }
  }

  function saveMappings() {
    try {
      localStorage.setItem('clearcomms_axis_mappings', JSON.stringify(axisMappings));
      console.log("[ClearComms] Mappings saved");
    } catch (error) {
      console.error("[ClearComms] Error saving mappings:", error);
    }
  }

  function loadMappings() {
    try {
      const saved = localStorage.getItem('clearcomms_axis_mappings');
      if (saved) {
        axisMappings = JSON.parse(saved);
        console.log(`[ClearComms] Loaded ${axisMappings.length} axis mapping(s) from storage`);
      }
    } catch (error) {
      console.error("[ClearComms] Error loading mappings:", error);
    }
  }

  function saveButtonMappings() {
    try {
      localStorage.setItem('clearcomms_button_mappings', JSON.stringify(buttonMappings));
      console.log("[ClearComms] Button mappings saved");
    } catch (error) {
      console.error("[ClearComms] Error saving button mappings:", error);
    }
  }

  function loadButtonMappings() {
    try {
      const saved = localStorage.getItem('clearcomms_button_mappings');
      if (saved) {
        buttonMappings = JSON.parse(saved);
        console.log(`[ClearComms] Loaded ${buttonMappings.length} button mapping(s) from storage`);
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
            {@const mapping = axisMappings.find(m => m.processId === session.process_id)}
            {@const buttonMapping = buttonMappings.find(m => m.processId === session.process_id)}
            
            <div class="channel-strip" class:has-mapping={!!mapping || !!buttonMapping}>
              <!-- Application Name -->
              <span class="app-name" title={session.display_name}>{session.process_name}</span>

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
                  oninput={(e) => setSessionVolume(session.session_id, parseFloat((e.target as HTMLInputElement).value))}
                  onchange={(e) => setSessionVolume(session.session_id, parseFloat((e.target as HTMLInputElement).value))}
                />
                <span class="volume-readout">{(session.volume * 100).toFixed(0)}%</span>
              </div>

              <!-- Mute Button -->
              <button
                class="btn btn-round btn-channel btn-mute"
                class:muted={session.is_muted}
                onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                title={session.is_muted ? 'Unmute' : 'Mute'}
              >
                {session.is_muted ? '🔇' : '🔊'}
              </button>

              <!-- Axis Binding Control -->
              {#if mapping}
                <div class="mapping-badge" title="Volume: {mapping.axisName}">
                  <span>🎮</span>
                  <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeMapping(session.process_id)}>✕</button>
                </div>
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

              <!-- Button Binding Control -->
              {#if buttonMapping}
                <div class="mapping-badge button" title="Mute: {buttonMapping.buttonName}">
                  <span>🔘</span>
                  <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeButtonMapping(session.process_id)}>✕</button>
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
            </div>
          {/each}

          <!-- Ghost Column (Add New Binding) - Only in Edit Mode -->
          {#if isEditMode}
            <div class="channel-strip ghost-column">
              <span class="app-name ghost">Add Binding</span>

              {#if availableSessions.length > 0}
                <select class="app-dropdown" onchange={(e) => {
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
                    <option value={session.session_id}>{session.process_name}</option>
                  {/each}
                </select>
                <p class="ghost-hint">Select an app to bind volume control</p>
              {:else}
                <p class="ghost-hint empty">All apps are bound</p>
              {/if}
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
      Crafted by Cameron Carlyon | &copy; {new Date().getFullYear()}
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

  /* ===== GHOST COLUMN ===== */
  .channel-strip.ghost-column {
    background: transparent;
    border: 2px dashed var(--border-color);
    min-height: 320px;
    justify-content: flex-start;
  }

  .channel-strip.ghost-column:hover {
    background: var(--bg-medium);
    border-color: var(--text-secondary);
  }

  .app-dropdown {
    width: 100%;
    padding: 10px;
    margin-top: 12px;
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

  .ghost-hint {
    font-size: 0.65rem;
    color: var(--text-muted);
    text-align: center;
    margin: 0;
    padding: 0 8px;
    line-height: 1.4;
  }

  .ghost-hint.empty {
    color: var(--border-color);
  }

  /* ===== APP NAME ===== */
  .app-name {
    width: 100%;
    text-align: center;
    margin-bottom: 12px;
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

  .volume-readout {
    font-size: 0.7rem;
    font-weight: 700;
    color: var(--text-secondary);
    text-align: center;
    min-width: 40px;
    letter-spacing: -0.3px;
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
