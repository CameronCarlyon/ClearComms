<!--
  BootScreen Component
  Displays loading/error state during application initialisation
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ButtonPill } from "$lib/components";
  
  interface Props {
    status: string;
    errorMessage?: string;
  }
  
  let { status, errorMessage = '' }: Props = $props();
  
  const isFailed = $derived(status === 'Failed');
  let isRestarting = $state(false);
  
  async function handleRestart() {
    isRestarting = true;
    try {
      await invoke("restart_application");
    } catch (error) {
      console.error("Failed to restart application:", error);
      isRestarting = false;
    }
  }
</script>

<div class="boot-screen" role="status" aria-live="polite">
  <svg class="logo" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 869.83 749.18" aria-label="ClearComms">
    <defs>
      <style>
        .logo-fill {
          fill: var(--text-primary);
        }
        .logo-stroke {
          fill: none;
          stroke: var(--text-primary);
          stroke-linecap: round;
          stroke-linejoin: round;
          stroke-width: 60px;
        }
      </style>
    </defs>
    <g id="Soundwaves">
      <path class="logo-stroke" d="M576.28,475.23c15.17-30.28,23.72-64.46,23.72-100.64s-8.55-70.35-23.72-100.64"/>
      <path class="logo-stroke" d="M683.62,528.9c23.26-46.44,36.38-98.84,36.38-154.31s-13.11-107.88-36.38-154.31"/>
      <path class="logo-stroke" d="M790.84,582.51c31.34-62.56,48.99-133.18,48.99-207.92s-17.65-145.36-48.99-207.92"/>
    </g>
    <path class="logo-fill" d="M494.9,546.34c-8.9-4.45-19.43-4.2-28.1.68-123.33,69.39-293.3-29.31-286.77-175.54-2.02-143.53,165.45-237.47,286.75-169.34,8.68,4.88,19.21,5.15,28.12.69l121.48-60.74c18.97-9.48,22.34-35.08,6.55-49.23C389.92-115.86-1.05,48.49,0,374.59c-1.05,326.1,389.98,490.45,622.93,281.72,15.79-14.15,12.41-39.74-6.55-49.23l-121.48-60.74Z"/>
  </svg>
  <p class="boot-status" class:error={isFailed} role={isFailed ? 'alert' : 'status'}>
    {isFailed ? errorMessage : status}
  </p>
  {#if isFailed}
    <ButtonPill
      label="Restart Application"
      loadingLabel="Restarting..."
      isLoading={isRestarting}
      onclick={handleRestart}
      ariaLabel="Restart application"
    />
  {/if}
</div>

<style>
  .boot-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100vw;
    background: transparent;
    gap: 1.5rem;
  }

  .logo {
    width: 200px;
    height: auto;
    max-width: 90vw;
  }

  .boot-status {
    font-size: 1rem;
    color: var(--text-secondary);
    text-align: center;
    max-width: 90%;
  }

  .boot-status.error {
    color: #ff4444;
  }

  :global(.restart-button) {
    margin-top: 1rem;
  }
</style>
