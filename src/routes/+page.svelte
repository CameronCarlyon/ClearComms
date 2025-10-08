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
    
    // Trigger window resize based on bound session count + edit mode ghost column
    const displayCount = isEditMode ? boundSessionIds.size + 1 : boundSessionIds.size;
    if (audioInitialised && displayCount > 0) {
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
      
      // Resize window to fit sessions
      await resizeWindowToFit(sessions.length);
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
    <div class="status-indicator" class:ready={initStatus === 'Ready'} class:failed={initStatus === 'Failed'}>
      {initStatus}
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
          class="edit-mode-btn" 
          class:active={isEditMode}
          onclick={toggleEditMode} 
          disabled={!audioInitialised}
          title={isEditMode ? 'Exit Edit Mode' : 'Edit Bindings'}
        >
          {isEditMode ? '✓ Done' : '✏️ Edit'}
        </button>
        <button class="icon-btn" onclick={refreshAudioSessions} disabled={!audioInitialised} title="Refresh Sessions">
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
              <div class="channel-name">
                <span class="app-name" title={session.display_name}>{session.process_name}</span>
              </div>

              <!-- Vertical Volume Slider -->
              <div class="fader-container">
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
              </div>

              <!-- Mute Button -->
              <button
                class="channel-mute-btn"
                class:muted={session.is_muted}
                onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                title={session.is_muted ? 'Unmute' : 'Mute'}
              >
                {session.is_muted ? '🔇' : '🔊'}
              </button>

              <!-- Axis Binding Control -->
              <div class="channel-binding">
                {#if mapping}
                  <div class="mapping-badge" title="Volume: {mapping.axisName}">
                    <span>🎮</span>
                    <button class="remove-badge-btn" onclick={() => removeMapping(session.session_id)}>✕</button>
                  </div>
                {:else if isBindingMode && pendingBinding?.sessionId === session.session_id}
                  <div class="binding-active">
                    <span class="pulse">⏺</span>
                    <button class="cancel-badge-btn" onclick={cancelBinding}>✕</button>
                  </div>
                {:else}
                  <button class="bind-badge-btn" onclick={() => startAxisBinding(session.session_id, session.display_name)} title="Bind Volume Axis">
                    🎮
                  </button>
                {/if}
              </div>

              <!-- Button Binding Control -->
              <div class="channel-binding">
                {#if buttonMapping}
                  <div class="mapping-badge button" title="Mute: {buttonMapping.buttonName}">
                    <span>🔘</span>
                    <button class="remove-badge-btn" onclick={() => removeButtonMapping(session.session_id)}>✕</button>
                  </div>
                {:else if isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
                  <div class="binding-active">
                    <span class="pulse">⏺</span>
                    <button class="cancel-badge-btn" onclick={cancelButtonBinding}>✕</button>
                  </div>
                {:else}
                  <button class="bind-badge-btn button" onclick={() => startButtonBinding(session.session_id, session.display_name)} title="Bind Mute Button">
                    🔘
                  </button>
                {/if}
              </div>
            </div>
          {/each}

          <!-- Ghost Column (Add New Binding) - Only in Edit Mode -->
          {#if isEditMode}
            <div class="channel-strip ghost-column">
              <div class="channel-name">
                <span class="app-name ghost">Add Binding</span>
              </div>

              <div class="ghost-content">
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
    <p style="font-size: 0.8rem; color: var(--text-muted); text-align: center; margin-top: 20px;">
      Crafted by Cameron Carlyon | &copy; 2025
    </p>
  </footer>
</main>

<style>
  :root {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
    font-size: 14px;
    line-height: 1.4;
  }

  .container {
    margin: 0;
    padding: 12px;
    min-height: 100vh;
    max-width: 100%;
    overflow-x: hidden; /* Hide horizontal overflow since window resizes to fit */
    overflow-y: auto;
    background: var(--bg-primary);
  }

  .app-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
    padding-bottom: 12px;
    border-bottom: 1px solid rgba(36, 200, 219, 0.2);
  }

  h1 {
    margin: 0;
    font-size: 1.4rem;
    font-weight: 600;
    color: var(--accent-primary);
  }

  .status-indicator {
    padding: 4px 12px;
    border-radius: 12px;
    font-size: 0.75rem;
    font-weight: 500;
    background: rgba(136, 136, 136, 0.2);
    color: var(--text-muted);
  }

  .status-indicator.ready {
    background: rgba(36, 200, 219, 0.2);
    color: var(--accent-primary);
  }

  .status-indicator.failed {
    background: rgba(255, 62, 0, 0.2);
    color: var(--status-error);
  }

  .error-banner {
    padding: 8px 12px;
    margin-bottom: 12px;
    background: rgba(255, 62, 0, 0.1);
    border: 1px solid rgba(255, 62, 0, 0.3);
    border-radius: 6px;
    color: var(--status-error);
    font-size: 0.85rem;
  }

  .audio-section {
    margin-bottom: 16px;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .header-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .edit-mode-btn {
    padding: 6px 12px;
    background: rgba(36, 200, 219, 0.1);
    border: 1px solid rgba(36, 200, 219, 0.3);
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--text-primary);
    transition: all 0.2s;
  }

  .edit-mode-btn:hover:not(:disabled) {
    background: rgba(36, 200, 219, 0.2);
    transform: translateY(-1px);
  }

  .edit-mode-btn.active {
    background: rgba(36, 200, 219, 0.25);
    border-color: rgba(36, 200, 219, 0.5);
    color: var(--accent-primary);
  }

  .edit-mode-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .icon-btn {
    padding: 6px;
    background: rgba(36, 200, 219, 0.1);
    border: 1px solid rgba(36, 200, 219, 0.3);
    border-radius: 6px;
    cursor: pointer;
    font-size: 1rem;
    transition: all 0.2s;
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(36, 200, 219, 0.2);
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .status-text {
    text-align: center;
    color: var(--text-muted);
    font-size: 0.9rem;
    padding: 20px;
  }

  /* ===== MIXER LAYOUT ===== */
  .mixer-container {
    display: flex;
    flex-direction: row;
    gap: 12px;
    padding: 8px;
    overflow-x: visible; /* No scroll needed since window resizes */
    overflow-y: visible;
  }

  /* ===== CHANNEL STRIP (Vertical Layout) ===== */
  .channel-strip {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 12px 10px;
    min-width: 80px;
    max-width: 90px;
    background: var(--bg-card);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    transition: all 0.2s;
  }

  .channel-strip:hover {
    background: var(--bg-card-hover);
    transform: translateY(-2px);
  }

  .channel-strip.has-mapping {
    background: var(--bg-card-mapped);
    border-color: rgba(36, 200, 219, 0.4);
    box-shadow: 0 0 12px rgba(36, 200, 219, 0.15);
  }

  /* ===== GHOST COLUMN ===== */
  .channel-strip.ghost-column {
    background: rgba(136, 136, 136, 0.05);
    border: 2px dashed rgba(136, 136, 136, 0.3);
    min-height: 300px;
    justify-content: flex-start;
  }

  .channel-strip.ghost-column:hover {
    background: rgba(136, 136, 136, 0.08);
    border-color: rgba(136, 200, 219, 0.4);
    transform: none;
  }

  .ghost-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 12px 0;
    width: 100%;
  }

  .app-dropdown {
    width: 100%;
    padding: 8px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    font-size: 0.75rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .app-dropdown:hover {
    border-color: rgba(36, 200, 219, 0.5);
    background: var(--bg-card);
  }

  .app-dropdown:focus {
    outline: none;
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 2px rgba(36, 200, 219, 0.2);
  }

  .ghost-hint {
    font-size: 0.65rem;
    color: var(--text-muted);
    text-align: center;
    margin: 0;
    padding: 0 8px;
    line-height: 1.3;
  }

  .ghost-hint.empty {
    color: rgba(136, 136, 136, 0.6);
  }

  .channel-name .app-name.ghost {
    color: var(--text-muted);
    font-weight: 500;
  }

  /* ===== CHANNEL NAME ===== */
  .channel-name {
    width: 100%;
    text-align: center;
    margin-bottom: 4px;
  }

  .channel-name .app-name {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--accent-primary);
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ===== VERTICAL SLIDER (Fader) ===== */
  .fader-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }

  .vertical-slider {
    -webkit-appearance: slider-vertical;
    appearance: slider-vertical;
    writing-mode: bt-lr; /* For Firefox */
    width: 8px;
    height: 180px;
    background: var(--slider-bg);
    border-radius: 999px; /* Pill shape */
    outline: none;
    cursor: pointer;
    position: relative;
  }

  /* Chrome/Safari/Edge Vertical Slider */
  .vertical-slider::-webkit-slider-runnable-track {
    width: 8px;
    height: 180px;
    background: var(--slider-bg);
    border-radius: 999px;
  }

  .vertical-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 20px;
    height: 20px;
    border-radius: 50%; /* Circle */
    background: var(--accent-primary);
    cursor: pointer;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
    transition: all 0.2s;
    margin-left: -6px; /* Center the thumb */
  }

  .vertical-slider::-webkit-slider-thumb:hover {
    background: var(--accent-secondary);
    transform: scale(1.15);
    box-shadow: 0 3px 6px rgba(0, 0, 0, 0.3);
  }

  .vertical-slider::-webkit-slider-thumb:active {
    transform: scale(1.05);
  }

  /* Firefox Vertical Slider */
  .vertical-slider::-moz-range-track {
    width: 8px;
    height: 180px;
    background: var(--slider-bg);
    border-radius: 999px;
    border: none;
  }

  .vertical-slider::-moz-range-thumb {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--accent-primary);
    cursor: pointer;
    border: none;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.2);
    transition: all 0.2s;
  }

  .vertical-slider::-moz-range-thumb:hover {
    background: var(--accent-secondary);
    transform: scale(1.15);
  }

  .vertical-slider:disabled {
    opacity: 0.5;
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
    font-size: 0.7rem;
    font-weight: 600;
    color: var(--text-secondary);
    text-align: center;
    min-width: 35px;
  }

  /* ===== MUTE BUTTON ===== */
  .channel-mute-btn {
    width: 42px;
    height: 42px;
    padding: 0;
    background: var(--button-bg);
    border: 2px solid var(--border-color-strong);
    border-radius: 8px;
    font-size: 1.3rem;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .channel-mute-btn:hover {
    background: var(--button-hover);
    transform: scale(1.05);
  }

  .channel-mute-btn.muted {
    background: rgba(255, 62, 0, 0.2);
    border-color: rgba(255, 62, 0, 0.5);
  }

  .channel-mute-btn.muted:hover {
    background: rgba(255, 62, 0, 0.3);
  }

  /* ===== BINDING BADGES ===== */
  .channel-binding {
    width: 42px;
    height: 42px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .bind-badge-btn {
    width: 100%;
    height: 100%;
    padding: 0;
    background: rgba(36, 200, 219, 0.1);
    border: 2px solid rgba(36, 200, 219, 0.3);
    border-radius: 8px;
    font-size: 1.2rem;
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .bind-badge-btn:hover {
    background: rgba(36, 200, 219, 0.2);
    transform: scale(1.05);
  }

  .bind-badge-btn.button {
    background: rgba(147, 51, 234, 0.1);
    border-color: rgba(147, 51, 234, 0.3);
  }

  .bind-badge-btn.button:hover {
    background: rgba(147, 51, 234, 0.2);
  }

  .mapping-badge {
    width: 100%;
    height: 100%;
    position: relative;
    background: rgba(36, 200, 219, 0.2);
    border: 2px solid rgba(36, 200, 219, 0.5);
    border-radius: 8px;
    font-size: 1.2rem;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .mapping-badge.button {
    background: rgba(147, 51, 234, 0.2);
    border-color: rgba(147, 51, 234, 0.5);
  }

  .remove-badge-btn {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 18px;
    height: 18px;
    padding: 0;
    background: rgba(255, 62, 0, 0.9);
    border: 1px solid rgba(255, 62, 0, 1);
    border-radius: 50%;
    color: white;
    font-size: 0.7rem;
    font-weight: bold;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .remove-badge-btn:hover {
    background: rgba(255, 62, 0, 1);
    transform: scale(1.1);
  }

  .binding-active {
    width: 100%;
    height: 100%;
    position: relative;
    background: rgba(57, 108, 216, 0.2);
    border: 2px solid rgba(57, 108, 216, 0.5);
    border-radius: 8px;
    font-size: 1.2rem;
    display: flex;
    align-items: center;
    justify-content: center;
    animation: pulse-border 1.5s ease-in-out infinite;
  }

  .binding-active .pulse {
    color: var(--status-error);
    animation: pulse-icon 1s ease-in-out infinite;
  }

  .cancel-badge-btn {
    position: absolute;
    top: -6px;
    right: -6px;
    width: 18px;
    height: 18px;
    padding: 0;
    background: rgba(136, 136, 136, 0.9);
    border: 1px solid rgba(136, 136, 136, 1);
    border-radius: 50%;
    color: white;
    font-size: 0.7rem;
    font-weight: bold;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .cancel-badge-btn:hover {
    background: rgba(136, 136, 136, 1);
    transform: scale(1.1);
  }

  @keyframes pulse-border {
    0%, 100% { 
      border-color: rgba(57, 108, 216, 0.3);
      box-shadow: 0 0 0 rgba(57, 108, 216, 0);
    }
    50% { 
      border-color: rgba(57, 108, 216, 0.8);
      box-shadow: 0 0 8px rgba(57, 108, 216, 0.4);
    }
  }

  @keyframes pulse-icon {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.6; transform: scale(1.2); }
  }

</style>
