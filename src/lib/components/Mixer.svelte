<!--
  Mixer Component
  Displays audio application channels with volume controls, or onboarding view when empty
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { AudioSession, AxisMapping, ButtonMapping } from '$lib/types';
  import ApplicationChannel from './ApplicationChannel.svelte';
  import ButtonAddApplication from './ButtonAddApplication.svelte';
  import { formatProcessName } from '$lib/stores/audioStore';

  interface Props {
    boundSessions: AudioSession[];
    availableSessions: AudioSession[];
    axisMappings: AxisMapping[];
    buttonMappings: ButtonMapping[];
    isEditMode: boolean;
    isBindingMode: boolean;
    isButtonBindingMode: boolean;
    pendingBinding: { sessionId: string; sessionName: string; processId: number; processName: string } | null;
    pendingButtonBinding: { sessionId: string; sessionName: string; processId: number; processName: string } | null;
    addAppListExpanded: boolean;
    addAppComponentKey: number;
  }

  let {
    boundSessions,
    availableSessions,
    axisMappings,
    buttonMappings,
    isEditMode,
    isBindingMode,
    isButtonBindingMode,
    pendingBinding,
    pendingButtonBinding,
    addAppListExpanded = $bindable(),
    addAppComponentKey
  }: Props = $props();

  let isOnboarding = $derived(boundSessions.length === 0);

  const dispatch = createEventDispatcher<{
    volumedragstart: { sessionId: string };
    volumedragmove: { sessionId: string; volume: number };
    volumedragend: { sessionId: string; volume: number };
    volumetrackclick: { sessionId: string; volume: number };
    volumewheel: { sessionId: string; volume: number };
    mutetoggle: { sessionId: string; muted: boolean };
    startaxisbinding: { session: AudioSession };
    startbuttonbinding: { session: AudioSession };
    cancelaxisbinding: void;
    cancelbuttonbinding: void;
    removeaxismapping: { processName: string };
    removebuttonmapping: { processName: string };
    toggleinversion: { processName: string };
    removeapplication: { processName: string };
    select: { processName: string };
  }>();

  const isBindingNewApp = $derived(
    isBindingMode && pendingBinding !== null && !boundSessions.some(s => s.process_name === pendingBinding?.processName)
  );

  let mixerContainer: HTMLDivElement;

  function handleWheel(event: WheelEvent) {
    const container = mixerContainer;
    if (!container) return;

    // Don't scroll if hovering over a volume slider
    const target = event.target as HTMLElement;
    if (target.classList.contains('volume-slider') || target.closest('.volume-bar-container')) {
      return;
    }

    // Only handle wheel event if container has horizontal scroll
    if (container.scrollWidth > container.clientWidth) {
      event.preventDefault();
      container.scrollLeft += event.deltaY > 0 ? 50 : -50;
    }
  }
</script>

{#if boundSessions.length > 0 || isEditMode}
  <!-- Mixer View -->
  <div class="mixer-container" bind:this={mixerContainer} onwheel={handleWheel}>
    {#each boundSessions as session (session.session_id)}
      {@const mapping = axisMappings.find(m => m.processName === session.process_name)}
      {@const buttonMapping = buttonMappings.find(m => m.processName === session.process_name)}
      
      <ApplicationChannel
        {session}
        axisMapping={mapping}
        {buttonMapping}
        {isEditMode}
        isBindingAxis={isBindingMode && pendingBinding?.sessionId === session.session_id}
        isBindingButton={isButtonBindingMode && pendingButtonBinding?.sessionId === session.session_id}
        on:volumedragstart
        on:volumedragmove
        on:volumedragend
        on:volumetrackclick
        on:volumewheel
        on:mutetoggle
        on:startaxisbinding
        on:startbuttonbinding
        on:cancelaxisbinding
        on:cancelbuttonbinding
        on:removeaxismapping
        on:removebuttonmapping
        on:toggleinversion
        on:removeapplication
      />
    {/each}

    <!-- Add Application Column - Only in Edit Mode -->
    {#if isEditMode}
      {#if isBindingNewApp && pendingBinding}
        <!-- Binding in Progress for NEW App -->
        <div class="application-channel add-app-column" role="group" aria-label="Binding in progress for {pendingBinding.sessionName}">
          <span class="app-name inactive">{formatProcessName(pendingBinding.processName)}</span>
          <div class="volume-bar-container">
            <input type="range" class="volume-slider" min="0" max="1" step="0.01" value={0.5} style="--volume-percent: 50%" disabled />
          </div>
          <button class="btn btn-channel btn-disabled" onclick={() => dispatch('cancelbuttonbinding')} aria-label="Cancel mute binding" title="Cancel Mute Binding" type="button">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor"><path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/></svg>
          </button>
          <button class="btn btn-channel btn-disabled" onclick={() => dispatch('cancelaxisbinding')} aria-label="Cancel axis binding" title="Cancel Axis Binding" type="button">
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor"><path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/></svg>
          </button>
        </div>
      {:else}
        <!-- Add Application Button -->
        {#key addAppComponentKey}
          <ButtonAddApplication
            bind:expanded={addAppListExpanded}
            {availableSessions}
            onboarding={isOnboarding}
            on:select
          />
        {/key}
      {/if}
    {/if}
  </div>
{/if}

<style>
  .mixer-container {
    display: flex;
    flex-direction: row;
    justify-content: center;
    gap: 46px;
    overflow-x: scroll;
    flex: 1;
    min-height: 0;
    align-items: center;
    transition: opacity 0.3s ease, transform 0.3s ease;
    scroll-behavior: smooth;
    scrollbar-width: none;
    padding: 0rem 2.5rem;
  }

  .application-channel {
    display: flex;
    height: 100%;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    transition: all 0.2s ease;
  }

  .application-channel.add-app-column {
    opacity: 0.6;
    justify-content: center;
  }

  .application-channel.add-app-column .volume-slider {
    pointer-events: none;
  }

  .app-name {
    text-align: center;
    font-size: 0.8rem;
    font-weight: 700;
    color: var(--text-primary);
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 3rem;
  }

  .app-name.inactive {
    color: var(--text-muted);
    font-weight: 500;
  }

  .volume-bar-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    width: 100%;
    flex: 1;
    min-height: 0;
    position: relative;
  }

  .volume-slider {
    appearance: none;
    width: 46px;
    flex: 1;
    background: transparent;
    cursor: pointer;
    writing-mode: vertical-lr;
    direction: rtl;
    border-radius: 2rem;
    overflow: hidden;
    border: 0.5px solid var(--text-muted);
    box-sizing: border-box;
    transition: filter 0.2s ease;
  }

  .volume-slider::-webkit-slider-runnable-track {
    width: 100%;
    height: 100%;
    background: transparent;
    border-radius: 2rem;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 45px;
    border-radius: 2rem;
    background: var(--text-primary);
    border: none;
    box-shadow: 0 1000px 0 1000px var(--text-primary);
    clip-path: inset(0 0 -1000px 0 round 2rem);
  }

  .volume-slider:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .btn {
    padding: 0;
    background: var(--text-primary);
    border: none;
    color: var(--bg-primary);
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

  .btn:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }

  .btn:active:not(:disabled) {
    transform: scale(0.98);
  }

  .btn-channel {
    box-sizing: border-box;
    width: 46px;
    height: 46px;
    min-width: 46px;
    min-height: 46px;
    max-width: 46px;
    max-height: 46px;
    border-radius: 50%;
    font-size: 1.3rem;
    transition: background 0.2s ease, color 0.2s ease, border 0.2s ease, box-shadow 0.2s ease;
    flex-shrink: 0;
    flex-grow: 0;
  }

  .btn-disabled {
    border-radius: 50%;
    background: var(--bg-card);
    color: var(--text-primary);
    border: 0.5px solid var(--text-muted);
    transition: border 0.2s ease, box-shadow 0.2s ease;
  }

  .btn-disabled:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.45);
  }
</style>
