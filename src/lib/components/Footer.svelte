<!--
  Footer Component
  Simple footer with attribution link and simulator connection status
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import type { SimStatus } from '$lib/types';

  interface Props {
    simStatus: SimStatus;
  }

  let { simStatus }: Props = $props();

  async function handleLinkClick(e: MouseEvent) {
    e.preventDefault();
    await invoke('open_url', { url: 'https://cameroncarlyon.com' });
  }

  function statusColour(status: SimStatus): string {
    if (!status.connected) return 'var(--text-muted)';
    if (status.wasmPresent) return '#4ade80'; // green
    return '#facc15'; // yellow
  }

  function statusTooltip(status: SimStatus): string {
    if (!status.connected) return 'Simulator disconnected';
    if (status.wasmPresent) {
      const parts = [`Connected (${status.simVersion})`];
      if (status.aircraftTitle) parts.push(status.aircraftTitle);
      return parts.join(' — ');
    }
    return `Connected (${status.simVersion}) — MobiFlight WASM absent`;
  }
</script>

<footer>
  <div class="sim-status" title={statusTooltip(simStatus)}>
    <span class="status-dot" style:background-color={statusColour(simStatus)}></span>
  </div>
  <p>
    Crafted by <a
      href="https://cameroncarlyon.com"
      onclick={handleLinkClick}
      class="hyperlink"
      aria-label="Visit Cameron Carlyon's website (opens in external browser)"
    >Cameron Carlyon</a>
  </p>
</footer>

<style>
  footer {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    color: var(--text-muted);
    z-index: 2;
    gap: 0.25rem;
  }

  .sim-status {
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: help;
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    display: inline-block;
    transition: background-color 0.3s ease;
  }

  footer p {
    font-size: 0.8rem;
    color: var(--text-muted);
    text-align: center;
    margin: 0;
  }

  .hyperlink {
    color: var(--text-muted);
    text-decoration: none;
    cursor: pointer;
    transition: color 0.2s ease, filter 0.2s ease;
    display: inline-block;
  }

  .hyperlink:hover {
    color: var(--text-primary);
    filter: drop-shadow(0 0 30px rgba(255, 255, 255, 1)) drop-shadow(0 0 60px rgba(255, 255, 255, 0.8)) drop-shadow(0 0 100px rgba(255, 255, 255, 0.6)) drop-shadow(0 0 140px rgba(255, 255, 255, 0.4));
  }
</style>
