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
  }

  // Button-to-mute mapping types
  interface ButtonMapping {
    deviceHandle: string;
    deviceName: string;
    buttonName: string;
    sessionId: string;
    sessionName: string;
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
  let isPolling = $state(false);
  let initStatus = $state("Initialising...");
  let audioInitialised = $state(false);
  let isBindingMode = $state(false);
  let isButtonBindingMode = $state(false);
  let pendingBinding = $state<{ sessionId: string; sessionName: string } | null>(null);
  let pendingButtonBinding = $state<{ sessionId: string; sessionName: string } | null>(null);
  let previousAxisValues: Map<string, Record<string, number>> = new Map();
  let previousButtonStates: Map<string, Record<string, boolean>> = new Map();
  let errorMsg = $state("");
  let isEditMode = $state(false);

  // Computed: Get sessions with bindings (axis OR button mappings)
  $effect(() => {
    const boundSessionIds = new Set([
      ...axisMappings.map(m => m.sessionId),
      ...buttonMappings.map(m => m.sessionId)
    ]);
    
    // Calculate display count: bound sessions + ghost column (if in edit mode)
    const boundCount = boundSessionIds.size;
    const displayCount = isEditMode ? boundCount + 1 : boundCount;
    
    // Resize window to fit displayed columns (including ghost when in edit mode)
    if (audioInitialised) {
      resizeWindowToFit(displayCount);
    }
  });

  // Computed: Get bound audio sessions
  function getBoundSessions(): AudioSession[] {
    const boundSessionIds = new Set([
      ...axisMappings.map(m => m.sessionId),
      ...buttonMappings.map(m => m.sessionId)
    ]);
    
    return audioSessions.filter(s => boundSessionIds.has(s.session_id));
  }

  // Computed: Get available (unbound) audio sessions for dropdown
  function getAvailableSessions(): AudioSession[] {
    const boundSessionIds = new Set([
      ...axisMappings.map(m => m.sessionId),
      ...buttonMappings.map(m => m.sessionId)
    ]);
    
    return audioSessions.filter(s => !boundSessionIds.has(s.session_id));
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
  }
  
  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
      console.log("[ClearComms] Polling stopped");
    }
    isPolling = false;
  }

  async function refreshAudioSessions() {
    try {
      const sessions = await invoke<AudioSession[]>("get_audio_sessions");
      audioSessions = sessions;
      console.log(`[ClearComms] Found ${sessions.length} audio session(s):`);
      sessions.forEach(s => {
        console.log(`  - ${s.process_name} (${s.display_name}) [PID: ${s.process_id}]`);
      });
      
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

  function startAxisBinding(sessionId: string, sessionName: string) {
    isBindingMode = true;
    pendingBinding = { sessionId, sessionName };
    
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

  function startButtonBinding(sessionId: string, sessionName: string) {
    isButtonBindingMode = true;
    pendingButtonBinding = { sessionId, sessionName };
    
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

  function createMapping(deviceHandle: string, deviceName: string, axisName: string, sessionId: string, sessionName: string) {
    axisMappings = axisMappings.filter(m => m.sessionId !== sessionId);
    
    const newMapping: AxisMapping = { deviceHandle, deviceName, axisName, sessionId, sessionName };
    axisMappings = [...axisMappings, newMapping];
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${axisName} → ${sessionName}`);
    saveMappings();
  }

  function removeMapping(sessionId: string) {
    const mapping = axisMappings.find(m => m.sessionId === sessionId);
    if (mapping) {
      console.log(`[ClearComms] Removed mapping: ${mapping.deviceName} ${mapping.axisName} → ${mapping.sessionName}`);
    }
    axisMappings = axisMappings.filter(m => m.sessionId !== sessionId);
    saveMappings();
  }

  function createButtonMapping(deviceHandle: string, deviceName: string, buttonName: string, sessionId: string, sessionName: string) {
    // Remove existing button mapping for this session (one button per session)
    buttonMappings = buttonMappings.filter(m => m.sessionId !== sessionId);
    
    const newMapping: ButtonMapping = { deviceHandle, deviceName, buttonName, sessionId, sessionName };
    buttonMappings = [...buttonMappings, newMapping];
    
    console.log(`[ClearComms] ✓ Mapped ${deviceName} ${buttonName} → Mute ${sessionName}`);
    saveButtonMappings();
  }

  function removeButtonMapping(sessionId: string) {
    const mapping = buttonMappings.find(m => m.sessionId === sessionId);
    if (mapping) {
      console.log(`[ClearComms] Removed button mapping: ${mapping.deviceName} ${mapping.buttonName} → Mute ${mapping.sessionName}`);
    }
    buttonMappings = buttonMappings.filter(m => m.sessionId !== sessionId);
    saveButtonMappings();
  }

  async function applyAxisMappings() {
    if (isBindingMode && pendingBinding) {
      const movement = detectAxisMovement();
      if (movement) {
        createMapping(movement.deviceHandle, movement.deviceName, movement.axisName, pendingBinding.sessionId, pendingBinding.sessionName);
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
        const session = audioSessions.find(s => s.session_id === mapping.sessionId);
        
        if (session && Math.abs(session.volume - axisValue) > 0.01) {
          try {
            await invoke("set_session_volume", { sessionId: mapping.sessionId, volume: axisValue });
            const sessionIndex = audioSessions.findIndex(s => s.session_id === mapping.sessionId);
            if (sessionIndex !== -1) {
              audioSessions[sessionIndex].volume = axisValue;
            }
          } catch (error) {
            console.error(`[ClearComms] Error applying mapping for ${mapping.sessionName}:`, error);
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
        createButtonMapping(buttonPress.deviceHandle, buttonPress.deviceName, buttonPress.buttonName, pendingButtonBinding.sessionId, pendingButtonBinding.sessionName);
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
          const session = audioSessions.find(s => s.session_id === mapping.sessionId);
          if (session) {
            const newMuteState = !session.is_muted;
            try {
              await invoke("set_session_mute", { sessionId: mapping.sessionId, muted: newMuteState });
              const sessionIndex = audioSessions.findIndex(s => s.session_id === mapping.sessionId);
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

<main class="container">
  <header class="app-header">
    <h1>ClearComms</h1>
    <div class="header-right">
      <div class="status-indicator" class:ready={initStatus === 'Ready'} class:failed={initStatus === 'Failed'}>
        {initStatus}
      </div>
      <button class="btn btn-round btn-close" onclick={quitApplication} title="Quit">
        ✕
      </button>
    </div>
  </header>

  {#if errorMsg}
    <div class="error-banner">{errorMsg}</div>
  {/if}

  <!-- Audio Management Section -->
  <section class="audio-section">
    <div class="section-header">
      <h2>Audio Mixer</h2>
      <div class="header-actions">
        <button 
          class="btn btn-pill btn-edit" 
          class:active={isEditMode}
          onclick={toggleEditMode} 
          disabled={!audioInitialised}
          title={isEditMode ? 'Exit Edit Mode' : 'Edit Bindings'}
        >
          {isEditMode ? '✓ Done' : '✏️ Edit'}
        </button>
        <button class="btn btn-round btn-icon" onclick={refreshAudioSessions} disabled={!audioInitialised} title="Refresh Sessions">
          🔄
        </button>
      </div>
    </div>

    {#if audioInitialised}
      {@const boundSessions = getBoundSessions()}
      {@const availableSessions = getAvailableSessions()}
      
      {#if boundSessions.length > 0 || isEditMode}
        <div class="mixer-container">
          {#each boundSessions as session (session.session_id)}
            {@const mapping = axisMappings.find(m => m.sessionId === session.session_id)}
            {@const buttonMapping = buttonMappings.find(m => m.sessionId === session.session_id)}
            
            <div class="channel-strip" class:has-mapping={!!mapping || !!buttonMapping}>
              <!-- Application Name -->
              <span class="app-name" title={session.display_name}>{session.process_name}</span>

              <!-- Vertical Volume Slider -->
              <input
                type="range"
                class="vertical-slider"
                min="0"
                max="1"
                step="0.01"
                value={session.volume}
                disabled={!!mapping}
                onchange={(e) => setSessionVolume(session.session_id, parseFloat((e.target as HTMLInputElement).value))}
              />
              <span class="volume-readout">{(session.volume * 100).toFixed(0)}%</span>

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
                  <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeMapping(session.session_id)}>✕</button>
                </div>
              {:else if isBindingMode && pendingBinding?.sessionId === session.session_id}
                <div class="binding-active">
                  <span class="pulse">⏺</span>
                  <button class="btn btn-round btn-badge-small btn-badge-cancel" onclick={cancelBinding}>✕</button>
                </div>
              {:else}
                <button class="btn btn-round btn-channel btn-bind" onclick={() => startAxisBinding(session.session_id, session.display_name)} title="Bind Volume Axis">
                  🎮
                </button>
              {/if}

              <!-- Button Binding Control -->
              {#if buttonMapping}
                <div class="mapping-badge button" title="Mute: {buttonMapping.buttonName}">
                  <span>🔘</span>
                  <button class="btn btn-round btn-badge-small btn-badge-remove" onclick={() => removeButtonMapping(session.session_id)}>✕</button>
                </div>
              {:else if isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
                <div class="binding-active">
                  <span class="pulse">⏺</span>
                  <button class="btn btn-round btn-badge-small btn-badge-cancel" onclick={cancelButtonBinding}>✕</button>
                </div>
              {:else}
                <button class="btn btn-round btn-channel btn-bind" onclick={() => startButtonBinding(session.session_id, session.display_name)} title="Bind Mute Button">
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
                      startAxisBinding(session.session_id, session.display_name);
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
  </section>

  <footer>
    <p style="font-size: 0.8rem; color: var(--text-muted); text-align: center;">
      Crafted by Cameron Carlyon | &copy; 2025
    </p>
  </footer>
</main>

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

  .container {
    margin: 0;
    padding: 0;
    height: 100vh;
    max-width: 100vw;
    overflow: hidden;
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
  .audio-section,
  footer {
    position: relative;
    z-index: 2;
    padding-left: 16px;
    padding-right: 16px;
  }

  .app-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 12px;
    margin-bottom: 12px;
    padding-top: 12px;
    padding-bottom: 12px;
    background: var(--bg-medium);
    border-radius: 12px;
    border: 1px solid var(--border-color);
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

  .status-indicator {
    padding: 6px 14px;
    border-radius: 20px;
    font-size: 0.75rem;
    font-weight: 600;
    background: var(--bg-light);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
  }

  .status-indicator.ready {
    background: var(--text-primary);
    border-color: var(--text-primary);
    color: var(--bg-dark);
  }

  .status-indicator.failed {
    background: var(--bg-light);
    border-color: var(--text-muted);
    color: var(--text-muted);
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

  .audio-section {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    flex: 1;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
    padding: 12px 16px;
    background: var(--bg-medium);
    border-radius: 12px;
    border: 1px solid var(--border-color);
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: -0.2px;
  }

  .header-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* Edit mode button */
  .btn-edit {
    padding: 7px 14px;
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
    color: var(--text-secondary);
    font-size: 0.9rem;
    padding: 20px;
  }

  footer {
    display: flex;
    justify-content: center;
    align-items: center;
    padding: 16px;
    color: var(--text-muted);
  }

  /* ===== MIXER LAYOUT ===== */
  .mixer-container {
    display: flex;
    flex-direction: row;
    justify-content: center;
    gap: 14px;
    overflow: hidden;
    padding: 8px;
  }

  /* ===== CHANNEL STRIP (Vertical Layout) ===== */
  .channel-strip {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 16px 12px;
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

  /* ===== VERTICAL SLIDER ===== */
  .vertical-slider {
    -webkit-appearance: slider-vertical;
    appearance: slider-vertical;
    writing-mode: bt-lr;
    width: 6px;
    height: 180px;
    background: var(--bg-light);
    border-radius: 999px;
    outline: none;
    cursor: pointer;
    position: relative;
    margin: 10px 0;
  }

  /* Chrome/Safari/Edge Vertical Slider */
  .vertical-slider::-webkit-slider-runnable-track {
    width: 6px;
    height: 180px;
    background: var(--bg-light);
    border-radius: 999px;
  }

  .vertical-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    transition: all 0.2s ease;
    margin-left: -8px;
  }

  .vertical-slider::-webkit-slider-thumb:active {
    transform: scale(1.05);
  }

  /* Firefox Vertical Slider */
  .vertical-slider::-moz-range-track {
    width: 6px;
    height: 180px;
    background: var(--bg-light);
    border-radius: 999px;
    border: none;
  }

  .vertical-slider::-moz-range-thumb {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--text-primary);
    cursor: pointer;
    border: none;
    transition: all 0.2s ease;
  }

  .vertical-slider:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .vertical-slider:disabled::-webkit-slider-thumb {
    background: var(--text-muted);
    cursor: not-allowed;
  }

  .vertical-slider:disabled::-moz-range-thumb {
    background: var(--text-muted);
    cursor: not-allowed;
  }

  .volume-readout {
    font-size: 0.75rem;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.8);
    text-align: center;
    min-width: 40px;
    letter-spacing: -0.3px;
  }

  /* ===== CHANNEL BUTTONS ===== */
  .btn-channel {
    width: 46px;
    height: 46px;
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
    font-size: 0.75rem;
    font-weight: bold;
  }

  .btn-badge-remove {
    /* Uses .btn-badge-small styling */
  }

  .binding-active {
    width: 46px;
    height: 46px;
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

</style>
