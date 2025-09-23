<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

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
  }
  
  let axisData = $state<AxisData[]>([]);
  let pollingInterval: number | null = null;
  let isPolling = $state(false);

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
      errorMsg = "";
      const data = await invoke<AxisData[]>("get_all_axis_values");
      axisData = data;
    } catch (error) {
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
        console.error("Polling error:", error);
        // Don't stop polling on individual errors
      }
    }, 50);
  }
  
  function stopPolling() {
    if (pollingInterval) {
      clearInterval(pollingInterval);
      pollingInterval = null;
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
</script>

<main class="container">
  <h1>ClearComms - Raw Input Test</h1>

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
          </div>
        {/each}
      </div>
    {:else if isPolling}
      <p class="status">Waiting for axis data...</p>
    {:else}
      <p class="status">No axis data available. Enumerate devices and start polling.</p>
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
</style>
