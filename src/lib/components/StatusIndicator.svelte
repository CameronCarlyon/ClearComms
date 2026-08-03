<!--
  StatusIndicator Component
  Displays a connection status indicator with label on the left and
  status light on the right. Dimensions: 100% width x 40px height.
  Accepts a `statusSource` prop to choose which status to display:
  - "simconnect" for SimConnect connection status
  - "wasm" for MobiFlight Event Module WASM status
-->
<script lang="ts">
  import type { SimStatus } from '$lib/types';
  import { simStatus } from '$lib/stores/simStore.svelte';

  interface Props {
    statusSource: 'simconnect' | 'wasm';
  }

  let { statusSource }: Props = $props();

  function indicatorColour(status: SimStatus): string {
    if (!status.connected) return 'var(--text-muted)';
    if (statusSource === 'simconnect') return '#4ade80'; // green
    // wasm
    return status.wasmPresent ? '#4ade80' : '#f97316'; // green | orange
  }

  // Static label mapping — "SimConnect" and "WASM" respectively.
  const LABELS: Record<string, string> = { simconnect: 'SimConnect', wasm: 'WASM' };

  function tooltipText(status: SimStatus): string {
    if (statusSource === 'simconnect') {
      if (!status.connected) return 'SimConnect is not connected to the simulator';
      let parts = [`Connected (${status.simVersion})`];
      if (status.aircraftTitle) parts.push(status.aircraftTitle);
      return parts.join(' — ');
    }
    // wasm
    if (!status.connected) return 'The MobiFlight Event Module is not available';
    if (status.wasmPresent) return 'The MobiFlight Event Module is active';
    return 'The MobiFlight Event Module absent. Aircraft data unavailable';
  }
</script>

<div class="status-indicator" title={tooltipText(simStatus)}>
  <span class="status-indicator__label">{LABELS[statusSource]}</span>
  <span class="status-indicator__dot" style:background-color={indicatorColour(simStatus)}></span>
</div>

<style>
  .status-indicator {
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    cursor: help;
    border-radius: 20px;
    transition: background 0.2s ease;
  }

  .status-indicator:hover {
    background: var(--bg-card-hover);
  }

  .status-indicator__label {
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--text-primary);
    letter-spacing: 0.02em;
  }

  .status-indicator__dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    transition: background-color 0.3s ease;
  }
</style>
