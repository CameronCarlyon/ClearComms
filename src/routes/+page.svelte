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
    } catch (error) {
      console.error("[ClearComms] Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
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
      <h2>Audio Control</h2>
      <button class="icon-btn" onclick={refreshAudioSessions} disabled={!audioInitialised} title="Refresh Sessions">
        🔄
      </button>
    </div>

    {#if audioInitialised}
      {#if audioSessions.length > 0}
        <div class="audio-sessions">
          {#each audioSessions as session (session.session_id)}
            {@const mapping = axisMappings.find(m => m.sessionId === session.session_id)}
            {@const buttonMapping = buttonMappings.find(m => m.sessionId === session.session_id)}
            <div class="audio-session-card" class:has-mapping={!!mapping || !!buttonMapping}>
              <div class="session-info">
                <div class="session-title">
                  <span class="app-name">{session.process_name}</span>
                  {#if session.display_name !== session.process_name && session.display_name !== `Process ${session.process_id}`}
                    <span class="session-subtitle">{session.display_name}</span>
                  {/if}
                </div>
              </div>

              {#if mapping}
                <div class="mapping-indicator">
                  <span class="mapping-icon">🎮</span>
                  <span class="mapping-text">Volume: {mapping.axisName}</span>
                  <button class="remove-mapping-btn" onclick={() => removeMapping(session.session_id)}>✕</button>
                </div>
              {:else if isBindingMode && pendingBinding?.sessionId === session.session_id}
                <div class="binding-indicator">
                  <span class="binding-pulse">⏺</span>
                  <span>Move axis...</span>
                  <button class="small-btn" onclick={cancelBinding}>Cancel</button>
                </div>
              {:else}
                <button class="bind-btn" onclick={() => startAxisBinding(session.session_id, session.display_name)}>Bind Volume</button>
              {/if}

              {#if buttonMapping}
                <div class="mapping-indicator button-mapping">
                  <span class="mapping-icon">🔘</span>
                  <span class="mapping-text">Mute: {buttonMapping.buttonName}</span>
                  <button class="remove-mapping-btn" onclick={() => removeButtonMapping(session.session_id)}>✕</button>
                </div>
              {:else if isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
                <div class="binding-indicator">
                  <span class="binding-pulse">⏺</span>
                  <span>Press button...</span>
                  <button class="small-btn" onclick={cancelButtonBinding}>Cancel</button>
                </div>
              {:else}
                <button class="bind-btn button-bind" onclick={() => startButtonBinding(session.session_id, session.display_name)}>Bind Mute</button>
              {/if}
              
              <div class="audio-controls">
                <label class="volume-control">
                  <span class="volume-label">{(session.volume * 100).toFixed(0)}%</span>
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={session.volume}
                    disabled={!!mapping}
                    onchange={(e) => setSessionVolume(session.session_id, parseFloat((e.target as HTMLInputElement).value))}
                  />
                </label>
                
                <button
                  class="mute-btn"
                  class:muted={session.is_muted}
                  onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                  title={session.is_muted ? 'Unmute' : 'Mute'}
                >
                  {session.is_muted ? '🔇' : '🔊'}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="status-text">No active audio sessions</p>
      {/if}
    {:else}
      <p class="status-text">Initialising...</p>
    {/if}
  </section>
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

  .audio-sessions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .audio-session-card {
    background: var(--bg-card);
    border: 1px solid rgba(36, 200, 219, 0.2);
    border-radius: 8px;
    padding: 10px;
    transition: all 0.2s;
  }

  .audio-session-card:hover {
    background: var(--bg-card-hover);
    border-color: rgba(36, 200, 219, 0.4);
  }

  .audio-session-card.has-mapping {
    background: var(--bg-card-mapped);
    border-color: rgba(36, 200, 219, 0.6);
    box-shadow: 0 0 10px rgba(36, 200, 219, 0.1);
  }

  .session-info {
    margin-bottom: 8px;
  }

  .session-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .app-name {
    font-weight: 600;
    color: var(--accent-primary);
    font-size: 0.95rem;
  }

  .session-subtitle {
    font-size: 0.75rem;
    color: var(--text-muted);
    font-style: italic;
  }

  .mapping-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: rgba(36, 200, 219, 0.1);
    border: 1px solid rgba(36, 200, 219, 0.3);
    border-radius: 6px;
    margin-bottom: 8px;
    font-size: 0.85rem;
  }

  .mapping-indicator.button-mapping {
    background: rgba(147, 51, 234, 0.1);
    border-color: rgba(147, 51, 234, 0.3);
  }

  .mapping-indicator.button-mapping .mapping-text {
    color: #9333ea;
  }

  .mapping-icon {
    font-size: 1rem;
  }

  .mapping-text {
    flex: 1;
    color: var(--accent-primary);
    font-weight: 500;
  }

  .remove-mapping-btn {
    padding: 2px 6px;
    background: rgba(255, 62, 0, 0.2);
    border: 1px solid rgba(255, 62, 0, 0.4);
    color: var(--status-error);
    font-size: 0.9rem;
    font-weight: bold;
    border-radius: 4px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .remove-mapping-btn:hover {
    background: rgba(255, 62, 0, 0.3);
  }

  .binding-indicator {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: rgba(57, 108, 216, 0.1);
    border: 1px solid rgba(57, 108, 216, 0.4);
    border-radius: 6px;
    margin-bottom: 8px;
    font-size: 0.85rem;
    color: var(--accent-secondary);
    animation: pulse 1.5s ease-in-out infinite;
  }

  .binding-pulse {
    font-size: 1rem;
    color: var(--status-error);
    animation: pulse-icon 1s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { background: rgba(57, 108, 216, 0.1); }
    50% { background: rgba(57, 108, 216, 0.2); }
  }

  @keyframes pulse-icon {
    0%, 100% { opacity: 1; transform: scale(1); }
    50% { opacity: 0.6; transform: scale(1.2); }
  }

  .small-btn {
    padding: 2px 8px;
    background: var(--button-bg);
    border: 1px solid var(--border-color-strong);
    border-radius: 4px;
    color: var(--text-primary);
    font-size: 0.75rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .small-btn:hover {
    background: var(--button-hover);
  }

  .bind-btn {
    width: 100%;
    padding: 6px;
    background: rgba(36, 200, 219, 0.1);
    border: 1px solid rgba(36, 200, 219, 0.3);
    color: var(--accent-primary);
    margin-bottom: 8px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 0.85rem;
    transition: all 0.2s;
  }

  .bind-btn:hover {
    background: rgba(36, 200, 219, 0.2);
    transform: translateY(-1px);
  }

  .bind-btn.button-bind {
    background: rgba(147, 51, 234, 0.1);
    border-color: rgba(147, 51, 234, 0.3);
    color: #9333ea;
  }

  .bind-btn.button-bind:hover {
    background: rgba(147, 51, 234, 0.2);
  }

  .audio-controls {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .volume-control {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .volume-label {
    font-size: 0.8rem;
    color: var(--text-secondary);
    font-weight: 500;
  }

  .volume-control input[type="range"] {
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: var(--slider-bg);
    outline: none;
    -webkit-appearance: none;
    appearance: none;
  }

  .volume-control input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent-primary);
    cursor: pointer;
    transition: all 0.2s;
  }

  .volume-control input[type="range"]::-webkit-slider-thumb:hover {
    background: var(--accent-secondary);
    transform: scale(1.1);
  }

  .volume-control input[type="range"]::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent-primary);
    cursor: pointer;
    border: none;
    transition: all 0.2s;
  }

  .volume-control input[type="range"]::-moz-range-thumb:hover {
    background: var(--accent-secondary);
    transform: scale(1.1);
  }

  .volume-control input[type="range"]:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .volume-control input[type="range"]:disabled::-webkit-slider-thumb {
    cursor: not-allowed;
    background: var(--text-muted);
  }

  .volume-control input[type="range"]:disabled::-moz-range-thumb {
    cursor: not-allowed;
    background: var(--text-muted);
  }

  .mute-btn {
    padding: 8px;
    background: var(--button-bg);
    border: 1px solid var(--border-color-strong);
    border-radius: 6px;
    font-size: 1.2rem;
    cursor: pointer;
    transition: all 0.2s;
    min-width: 38px;
  }

  .mute-btn:hover {
    background: var(--button-hover);
  }

  .mute-btn.muted {
    background: rgba(255, 62, 0, 0.2);
    border-color: rgba(255, 62, 0, 0.4);
  }

  .mute-btn.muted:hover {
    background: rgba(255, 62, 0, 0.3);
  }

  /* Remove old dark mode override since we're using variables now */

</style>
