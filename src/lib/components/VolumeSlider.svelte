<!--
  VolumeSlider Component
  Vertical volume slider with smooth drag, wheel, and click-to-animate support
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  
  interface Props {
    volume: number;
    sessionId: string;
    displayName: string;
    disabled?: boolean;
  }
  
  let { volume, sessionId, displayName, disabled = false }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    dragstart: { sessionId: string };
    dragmove: { sessionId: string; volume: number };
    dragend: { sessionId: string; volume: number };
    trackclick: { sessionId: string; volume: number };
    wheel: { sessionId: string; volume: number };
  }>();
  
  let isDragging = $state(false);
  let wasTrackClick = $state(false);
  let startVolume = $state(0);
  
  function handlePointerDown(e: PointerEvent) {
    const slider = e.currentTarget as HTMLInputElement;
    isDragging = false;
    wasTrackClick = false;
    startVolume = volume;
    
    dispatch('dragstart', { sessionId });
    
    try {
      slider.setPointerCapture(e.pointerId);
    } catch {
      // Ignore if pointer capture not available
    }
  }
  
  function handlePointerMove(e: PointerEvent) {
    if (e.buttons !== 1) return;
    
    if (!isDragging) {
      isDragging = true;
    }
  }
  
  function handleInput(e: Event) {
    const slider = e.currentTarget as HTMLInputElement;
    const newValue = parseFloat(slider.value);
    
    // If this was a track click (not a drag), animate to position
    if (wasTrackClick && !isDragging) {
      return;
    }
    
    if (!isDragging) {
      // Track click - animate to position
      wasTrackClick = true;
      slider.value = startVolume.toString();
      dispatch('trackclick', { sessionId, volume: newValue });
      return;
    }
    
    // Normal drag - update immediately
    dispatch('dragmove', { sessionId, volume: newValue });
  }
  
  function handlePointerUp(e: PointerEvent) {
    const slider = e.currentTarget as HTMLInputElement;
    
    if (isDragging) {
      const finalValue = parseFloat(slider.value);
      dispatch('dragend', { sessionId, volume: finalValue });
    }
    
    isDragging = false;
    wasTrackClick = false;
    
    if (slider.hasPointerCapture?.(e.pointerId)) {
      try {
        slider.releasePointerCapture(e.pointerId);
      } catch {
        // Ignore if pointer capture not available
      }
    }
  }
  
  function handleWheel(e: WheelEvent) {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -0.05 : 0.05;
    const newVolume = Math.max(0, Math.min(1, volume + delta));
    dispatch('wheel', { sessionId, volume: newVolume });
  }
</script>

<div class="volume-bar-container">
  <input
    type="range"
    class="volume-slider"
    min="0"
    max="1"
    step="0.01"
    value={volume}
    aria-label="Volume for {displayName}"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(volume * 100)}
    aria-valuetext="{Math.round(volume * 100)} percent"
    style="--volume-percent: {volume * 100}%"
    {disabled}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    oninput={handleInput}
    onpointerup={handlePointerUp}
    onwheel={handleWheel}
  />
</div>

<style>
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

  .volume-slider:hover:not(:disabled) {
    filter: drop-shadow(0 0 40px rgba(255, 255, 255, 0.25));
  }

  .volume-slider:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 4px;
    border-radius: 2rem;
  }

  .volume-slider:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .volume-slider:disabled::-webkit-slider-runnable-track {
    cursor: not-allowed;
  }
</style>
