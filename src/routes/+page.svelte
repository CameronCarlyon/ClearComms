<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, onDestroy } from "svelte";

  let name = $state("");
  let greetMsg = $state("");
  let rawInputStatus = $state("");
  let devices = $state<string[]>([]);
  let errorMsg = $state("");
  
  // Axis data types
  interface AxisData {
    device_handle: string;
    device_name: string;
    manufacturer: string;
    product_id: number;
    vendor_id: number;
    axes: Record<string, number>;
    buttons: Record<string, boolean>;
  }

  // Audio session types
  interface AudioSession {
    session_id: string;
    display_name: string;
    process_id: number;
    volume: number; // 0.0 to 1.0
    is_muted: boolean;
  }
  
  let axisData = $state<AxisData[]>([]);
  let audioSessions = $state<AudioSession[]>([]);
  let pollingInterval: number | null = null;
  let isPolling = $state(false);
  let initStatus = $state("Initialising...");
  let audioInitialised = $state(false);

  // Auto-initialise on component mount
  onMount(async () => {
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
      rawInputStatus = initResult;
      console.log("[ClearComms] ✓ Input system initialised:", initResult);

      // Step 2: Enumerate devices
      initStatus = "Enumerating devices...";
      console.log("[ClearComms] Step 2: Enumerating devices");
      const deviceList = await invoke<string[]>("enumerate_input_devices");
      devices = deviceList;
      console.log(`[ClearComms] ✓ Found ${deviceList.length} device(s):`, deviceList);

      // Step 3: Get initial axis values
      initStatus = "Reading axis values...";
      console.log("[ClearComms] Step 3: Getting initial axis values");
      await getAxisValues();
      console.log("[ClearComms] ✓ Axis values retrieved");

      // Step 4: Start polling
      initStatus = "Starting real-time polling...";
      console.log("[ClearComms] Step 4: Starting real-time polling (20Hz)");
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

  async function greet(event: Event) {
    event.preventDefault();
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    greetMsg = await invoke("greet", { name });
  }

  async function initRawInput() {
    try {
      errorMsg = "";
      const result = await invoke<string>("init_direct_input");
      rawInputStatus = result;
    } catch (error) {
      errorMsg = `Error: ${error}`;
    }
  }

  async function getStatus() {
    try {
      errorMsg = "";
      const status = await invoke<string>("get_direct_input_status");
      rawInputStatus = status;
    } catch (error) {
      errorMsg = `Error: ${error}`;
    }
  }

  async function enumerateDevices() {
    try {
      errorMsg = "";
      const deviceList = await invoke<string[]>("enumerate_input_devices");
      devices = deviceList;
      rawInputStatus = `Found ${deviceList.length} device(s)`;
    } catch (error) {
      errorMsg = `Error: ${error}`;
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
    // Poll every 50ms (20Hz) for responsive real-time tracking
    pollingInterval = setInterval(async () => {
      try {
        await getAxisValues();
      } catch (error) {
        console.error("[ClearComms] Polling error:", error);
        // Don't stop polling on individual errors
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
  
  async function updateTestAxis(deviceHandle: string, axisName: string, value: number) {
    try {
      await invoke("update_test_axis_value", {
        deviceHandle,
        axisName,
        value
      });
      await getAxisValues(); // Refresh display
    } catch (error) {
      errorMsg = `Error: ${error}`;
    }
  }

  // Audio management functions
  async function initAudioManager() {
    try {
      console.log("[ClearComms] Initialising audio manager...");
      const result = await invoke<string>("init_audio_manager");
      console.log("[ClearComms]", result);
      audioInitialised = true;
      await refreshAudioSessions();
    } catch (error) {
      console.error("[ClearComms] Error initialising audio manager:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function refreshAudioSessions() {
    try {
      const sessions = await invoke<AudioSession[]>("get_audio_sessions");
      audioSessions = sessions;
      console.log(`[ClearComms] Found ${sessions.length} audio session(s)`);
    } catch (error) {
      console.error("[ClearComms] Error getting audio sessions:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function setSessionVolume(sessionId: string, volume: number) {
    try {
      await invoke("set_session_volume", {
        sessionId,
        volume
      });
      console.log(`[ClearComms] Set volume for ${sessionId} to ${volume.toFixed(2)}`);
      await refreshAudioSessions(); // Update display
    } catch (error) {
      console.error("[ClearComms] Error setting volume:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }

  async function setSessionMute(sessionId: string, muted: boolean) {
    try {
      await invoke("set_session_mute", {
        sessionId,
        muted
      });
      console.log(`[ClearComms] Set mute for ${sessionId} to ${muted}`);
      await refreshAudioSessions(); // Update display
    } catch (error) {
      console.error("[ClearComms] Error setting mute:", error);
      errorMsg = `Audio error: ${error}`;
    }
  }
</script>

<main class="container">
  <h1>ClearComms - Raw Input Test</h1>

  <!-- Initialisation Status -->
  <div class="init-status">
    <p class:ready={initStatus === 'Ready'} class:failed={initStatus === 'Failed'}>
      Status: {initStatus}
    </p>
  </div>

  <div class="test-section">
    <h2>Raw Input Status</h2>
    <div class="button-row">
      <button onclick={initRawInput}>Initialise Raw Input</button>
      <button onclick={getStatus}>Get Status</button>
      <button onclick={enumerateDevices}>Enumerate Devices</button>
    </div>
    
    {#if rawInputStatus}
      <p class="status">{rawInputStatus}</p>
    {/if}
    
    {#if errorMsg}
      <p class="error">{errorMsg}</p>
    {/if}
    
    {#if devices.length > 0}
      <div class="devices">
        <h3>Discovered Devices:</h3>
        <ul>
          {#each devices as device}
            <li>{device}</li>
          {/each}
        </ul>
      </div>
    {/if}
  </div>

  <div class="test-section">
    <h2>Axis Values (Real-time Hardware Data)</h2>
    <div class="button-row">
      <button onclick={getAxisValues}>Get Axis Values</button>
      <button onclick={startPolling} disabled={isPolling}>Start Polling</button>
      <button onclick={stopPolling} disabled={!isPolling}>Stop Polling</button>
    </div>

    {#if axisData.length > 0}
      <div class="axis-display">
        {#each axisData as device (device.device_handle)}
          <div class="device-axes">
            <h3>{device.device_name}</h3>
            {#if device.manufacturer}
              <p class="device-manufacturer">{device.manufacturer}</p>
            {/if}
            <p class="device-handle">VID:{device.vendor_id.toString(16).toUpperCase().padStart(4, '0')} PID:{device.product_id.toString(16).toUpperCase().padStart(4, '0')}</p>
            
            <!-- Axes Section -->
            <h4>Axes</h4>
            <div class="axes-grid">
              {#each Object.entries(device.axes).sort((a, b) => a[0].localeCompare(b[0])) as [axisName, value] (axisName)}
                <div class="axis-item">
                  <div class="axis-header">
                    <span class="axis-name">{axisName}</span>
                    <span class="axis-value">{value.toFixed(3)}</span>
                  </div>
                  <div class="axis-bar">
                    <div class="axis-fill" style="width: {value * 100}%"></div>
                  </div>
                </div>
              {/each}
            </div>
            
            <!-- Buttons Section -->
            {#if Object.keys(device.buttons).length > 0}
              <h4>Buttons</h4>
              <div class="buttons-grid">
                {#each Object.entries(device.buttons).sort((a, b) => a[0].localeCompare(b[0])) as [buttonName, pressed] (buttonName)}
                  <div class="button-item" class:pressed={pressed}>
                    <span class="button-name">{buttonName}</span>
                    <span class="button-state">{pressed ? 'PRESSED' : 'Released'}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {:else if isPolling}
      <p class="status">Waiting for axis data...</p>
    {:else}
      <p class="status">No axis data available. Enumerate devices and start polling.</p>
    {/if}
  </div>

  <!-- Audio Management Section -->
  <div class="test-section">
    <h2>Audio Session Management</h2>
    <div class="button-row">
      <button onclick={initAudioManager} disabled={audioInitialised}>Initialise Audio</button>
      <button onclick={refreshAudioSessions} disabled={!audioInitialised}>Refresh Sessions</button>
    </div>

    {#if audioInitialised}
      {#if audioSessions.length > 0}
        <div class="audio-sessions">
          <h3>Active Audio Applications:</h3>
          {#each audioSessions as session (session.session_id)}
            <div class="audio-session-card">
              <div class="audio-session-header">
                <h4>{session.display_name}</h4>
                <span class="process-id">PID: {session.process_id}</span>
              </div>
              
              <div class="audio-controls">
                <label class="volume-control">
                  <span>Volume: {(session.volume * 100).toFixed(0)}%</span>
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={session.volume}
                    onchange={(e) => setSessionVolume(session.session_id, parseFloat((e.target as HTMLInputElement).value))}
                  />
                </label>
                
                <button
                  class:muted={session.is_muted}
                  onclick={() => setSessionMute(session.session_id, !session.is_muted)}
                >
                  {session.is_muted ? 'Unmute' : 'Mute'}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {:else}
        <p class="status">No active audio sessions found.</p>
      {/if}
    {:else}
      <p class="status">Initialise audio manager to view and control application volumes.</p>
    {/if}
  </div>

  <hr />

  <div class="row">
    <a href="https://vite.dev" target="_blank">
      <img src="/vite.svg" class="logo vite" alt="Vite Logo" />
    </a>
    <a href="https://tauri.app" target="_blank">
      <img src="/tauri.svg" class="logo tauri" alt="Tauri Logo" />
    </a>
    <a href="https://svelte.dev" target="_blank">
      <img src="/svelte.svg" class="logo svelte-kit" alt="SvelteKit Logo" />
    </a>
  </div>

  <form class="row" onsubmit={greet}>
    <input id="greet-input" placeholder="Enter a name..." bind:value={name} />
    <button type="submit">Greet</button>
  </form>
  <p>{greetMsg}</p>
</main>

<style>
.logo.vite:hover {
  filter: drop-shadow(0 0 2em #747bff);
}

.logo.svelte-kit:hover {
  filter: drop-shadow(0 0 2em #ff3e00);
}

:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding: 2rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.init-status {
  text-align: center;
  padding: 0.75rem;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 6px;
  border: 2px solid rgba(0, 0, 0, 0.1);
  margin-bottom: 1rem;
}

.init-status p {
  margin: 0;
  font-weight: 600;
  font-size: 1.1rem;
  color: #666;
}

.init-status p.ready {
  color: #24c8db;
}

.init-status p.failed {
  color: #ff3e00;
}

.test-section {
  background: rgba(255, 255, 255, 0.1);
  padding: 1.5rem;
  border-radius: 8px;
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.button-row {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  margin: 1rem 0;
}

.status {
  color: #24c8db;
  font-weight: 500;
  margin: 0.5rem 0;
}

.error {
  color: #ff3e00;
  font-weight: 500;
  margin: 0.5rem 0;
}

.devices {
  margin-top: 1rem;
  text-align: left;
}

.devices ul {
  list-style: none;
  padding: 0;
}

.devices li {
  background: rgba(0, 0, 0, 0.05);
  padding: 0.5rem 1rem;
  margin: 0.25rem 0;
  border-radius: 4px;
}

.axis-display {
  margin-top: 1rem;
}

.device-axes {
  background: rgba(0, 0, 0, 0.05);
  padding: 1rem;
  margin: 1rem 0;
  border-radius: 6px;
}

.device-axes h3 {
  margin-top: 0;
  margin-bottom: 0.5rem;
  color: #24c8db;
}

.device-manufacturer {
  font-size: 0.9rem;
  opacity: 0.8;
  margin: 0.25rem 0;
  font-weight: 500;
}

.device-handle {
  font-size: 0.85rem;
  opacity: 0.7;
  margin: 0.25rem 0 1rem 0;
}

.axes-grid {
  display: grid;
  gap: 1rem;
}

.axis-item {
  background: rgba(255, 255, 255, 0.5);
  padding: 0.75rem;
  border-radius: 4px;
}

.axis-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.5rem;
}

.axis-name {
  font-weight: 600;
  color: #333;
}

.axis-value {
  font-family: monospace;
  font-size: 0.95rem;
  color: #24c8db;
  font-weight: 600;
}

.axis-bar {
  width: 100%;
  height: 8px;
  background: rgba(0, 0, 0, 0.1);
  border-radius: 4px;
  overflow: hidden;
  margin-top: 0.5rem;
}

.axis-fill {
  height: 100%;
  background: linear-gradient(90deg, #24c8db, #396cd8);
  transition: width 0.1s ease;
  border-radius: 4px;
}

.buttons-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 0.5rem;
  margin-top: 1rem;
}

.button-item {
  background: rgba(255, 255, 255, 0.5);
  padding: 0.5rem 0.75rem;
  border-radius: 4px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  transition: all 0.15s ease;
  border: 2px solid transparent;
}

.button-item.pressed {
  background: linear-gradient(135deg, #24c8db, #396cd8);
  border-color: #396cd8;
  transform: scale(1.05);
  box-shadow: 0 0 10px rgba(36, 200, 219, 0.5);
}

.button-name {
  font-weight: 600;
  font-size: 0.9rem;
}

.button-item.pressed .button-name {
  color: white;
}

.button-state {
  font-size: 0.75rem;
  font-weight: 600;
  opacity: 0.7;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.button-item.pressed .button-state {
  color: white;
  opacity: 1;
}

h4 {
  margin: 1.5rem 0 0.5rem 0;
  color: #666;
  font-size: 0.9rem;
  text-transform: uppercase;
  letter-spacing: 1px;
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

button:disabled:hover {
  border-color: transparent;
}

.logo {
  height: 4em;
  padding: 1em;
  will-change: filter;
  transition: 0.75s;
}

.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
  gap: 0.5rem;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

h2 {
  margin-top: 0;
}

hr {
  border: none;
  border-top: 1px solid rgba(0, 0, 0, 0.1);
  margin: 2rem 0;
}

input,
button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #396cd8;
}
button:active {
  border-color: #396cd8;
  background-color: #e8e8e8;
}

input,
button {
  outline: none;
}

#greet-input {
  margin-right: 5px;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  a:hover {
    color: #24c8db;
  }

  .test-section {
    background: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.1);
  }

  .devices li {
    background: rgba(255, 255, 255, 0.05);
  }

  .axis-item {
    background: rgba(0, 0, 0, 0.3);
  }

  .axis-name {
    color: #f6f6f6;
  }

  .axis-bar {
    background: rgba(255, 255, 255, 0.1);
  }
  
  .init-status {
    background: rgba(0, 0, 0, 0.2);
    border-color: rgba(255, 255, 255, 0.1);
  }
  
  .init-status p {
    color: #aaa;
  }
  
  .button-item {
    background: rgba(0, 0, 0, 0.3);
  }
  
  .button-item.pressed {
    background: linear-gradient(135deg, #24c8db, #396cd8);
  }
  
  h4 {
    color: #aaa;
  }

  input,
  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
  button:active {
    background-color: #0f0f0f69;
  }

  hr {
    border-top-color: rgba(255, 255, 255, 0.1);
  }
}

/* Audio session styling */
.audio-sessions {
  margin-top: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.audio-session-card {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(36, 200, 219, 0.2);
  border-radius: 8px;
  padding: 1rem;
  transition: all 0.2s ease;
}

.audio-session-card:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(36, 200, 219, 0.4);
}

.audio-session-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
}

.audio-session-header h4 {
  margin: 0;
  color: #24c8db;
  font-size: 1rem;
}

.process-id {
  color: #888;
  font-size: 0.85rem;
  font-family: monospace;
}

.audio-controls {
  display: flex;
  gap: 1rem;
  align-items: center;
}

.volume-control {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.volume-control span {
  font-size: 0.9rem;
  color: #aaa;
}

.volume-control input[type="range"] {
  width: 100%;
  height: 6px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.1);
  outline: none;
  -webkit-appearance: none;
}

.volume-control input[type="range"]::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #24c8db;
  cursor: pointer;
  transition: all 0.2s ease;
}

.volume-control input[type="range"]::-webkit-slider-thumb:hover {
  background: #396cd8;
  transform: scale(1.1);
}

.volume-control input[type="range"]::-moz-range-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: #24c8db;
  cursor: pointer;
  border: none;
  transition: all 0.2s ease;
}

.volume-control input[type="range"]::-moz-range-thumb:hover {
  background: #396cd8;
  transform: scale(1.1);
}

button.muted {
  background: #ff3e00;
  color: white;
}

button.muted:hover {
  background: #ff5722;
}

@media (prefers-color-scheme: dark) {
  .audio-session-card {
    background: rgba(0, 0, 0, 0.3);
    border-color: rgba(36, 200, 219, 0.3);
  }

  .audio-session-card:hover {
    background: rgba(0, 0, 0, 0.4);
    border-color: rgba(36, 200, 219, 0.5);
  }
}
</style>
