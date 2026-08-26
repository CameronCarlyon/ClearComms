<!--
  ButtonSimFunction Component
  Fixed-width expandable button matching the dock's pattern. The trigger stays
  pinned at 46px wide and expands downward (height 100%) to reveal the category
  list, just like the Dock settings/close menus: without the width growth.
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { SimFunctionCategory } from '$lib/types';
  import ButtonExpandable from './ButtonExpandable.svelte';
  import ListOption from './ListOption.svelte';
  import { registerMixerMenu } from '$lib/stores/mixerMenuStore.svelte';

  interface Props {
    processName: string;
    /** Currently assigned category, if any */
    assigned: SimFunctionCategory | null;
    /** Categories the active aircraft profile supports */
    categories: SimFunctionCategory[];
    /** Bindable, so the parent channel can collapse its other controls while open */
    expanded?: boolean;
  }

  let { processName, assigned, categories, expanded = $bindable(false) }: Props = $props();

  // Only one mixer menu may be open at a time: opening this one collapses the
  // add-application menu or any other channel's function menu.
  registerMixerMenu(() => expanded, () => { expanded = false; });

  const dispatch = createEventDispatcher<{
    setsimcategory: { processName: string; category: SimFunctionCategory | null };
  }>();

  function handleSelect(event: CustomEvent<{ processName: string | undefined }>) {
    const raw = event.detail.processName;
    if (!raw) return;

    expanded = false;
    // Choosing the category that is already bound clears it: the row
    // advertises this by swapping its label for a close icon on hover.
    dispatch('setsimcategory', {
      processName,
      category: raw === assigned ? null : (raw as SimFunctionCategory),
    });
  }

  /** Track previous expanded state to re-mount the list for animation restart */
  let prevExpanded = $state(false);
  let listKey = $state(0);

  $effect(() => {
    if (expanded && !prevExpanded) listKey++;
    prevExpanded = expanded;
  });
</script>

<div class="sim-function-btn" class:assigned={assigned !== null}>
  <ButtonExpandable
    bind:expanded
    ariaLabel={expanded
      ? 'Close simulator function list'
      : assigned
        ? `Change simulator function: ${assigned}`
        : 'Bind simulator function'}
    title={expanded
      ? 'Close'
      : assigned
        ? `Simulator Function: ${assigned}`
        : 'Bind Simulator Function'}
    variant="controls"
  >
    {#snippet icon()}
      <!-- Dial by default; while unbound, hovering swaps in the same plus the
           Bind Axis and Bind Mute buttons use. The assigned category is conveyed
           by the tooltip and aria-label, not by the icon. -->
      <span class="sim-function-layer default" aria-hidden="true">
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="32" height="32" fill="currentColor">
          <path fill-rule="evenodd" d="M 12 5 A 7 7 0 1 0 12 19 A 7 7 0 1 0 12 5 Z M 12.71 12.71 L 16.71 8.71 A 1 1 0 0 0 15.29 7.29 L 11.29 11.29 A 1 1 0 0 0 12.71 12.71 Z"/>
        </svg>
      </span>
      {#if assigned === null}
        <span class="sim-function-layer hover" aria-hidden="true">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
          </svg>
        </span>
      {/if}
    {/snippet}
    {#snippet expandedIcon()}
      <!-- Close icon -->
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
      </svg>
    {/snippet}
    {#snippet children()}
      {#key listKey}
        {#each categories as category, i}
          {@const delay = (0.27 + i * 0.04).toFixed(2)}
          <span class="expandable__item" style="animation-delay: {delay}s">
            <ListOption
              processName={category}
              displayName={category}
              selected={category === assigned}
              on:select={handleSelect}
            />
          </span>
        {/each}
      {/key}
    {/snippet}
  </ButtonExpandable>
</div>

<style>
  .sim-function-btn {
    flex-shrink: 0;

    /* The category list is exactly the 8 SimFunctionCategory values; a bound
       one is cleared by selecting it again rather than by a separate row.
       Deriving the expanded height from these constants keeps it exact if
       the row count or spacing ever changes, instead of a magic number that
       has to be re-measured by hand. */
    --sim-function-item-count: 8;
    --sim-function-item-height: 46px;
    --sim-function-item-gap: 6px;
    --sim-function-padding: 6px;
    /* The trigger is simply one more row, so the menu is (count + 1) rows with
       a gap between each, plus top and bottom padding. */
    --sim-function-expanded-height: calc(
      ((var(--sim-function-item-count) + 1) * var(--sim-function-item-height))
      + (var(--sim-function-item-count) * var(--sim-function-item-gap))
      + (var(--sim-function-padding) * 2)
    );
  }

  /* Match the "Add Application" button height when expanded: include transition.
     The collapsed height must also be a fixed px value, not the inherited
     `height: 100%`: this wrapper has no explicit height of its own, so a
     percentage here resolves to `auto`, and CSS cannot transition to/from
     `auto`. That, not the expanded value, is why this previously opened with
     no animation. */
  .sim-function-btn :global(.btn-expandable) {
    height: var(--sim-function-item-height);
    transition:
      width 0.3s ease,
      height 0.3s ease,
      background 0.3s ease,
      border-color 0.3s ease,
      padding 0.3s ease;
  }

  .sim-function-btn :global(.btn-expandable.expanded) {
    height: var(--sim-function-expanded-height) !important;
    max-height: var(--sim-function-expanded-height) !important;
  }

  /* Add gap between list items to match ButtonAddApplication styling */
  .sim-function-btn :global(.btn-expandable__list) {
    gap: var(--sim-function-item-gap);
  }

  /* Bound state: solid fill with an inverted icon, matching the toggle buttons
     in ButtonRound (.btn-enabled). Restricted to the collapsed trigger: while
     expanded the trigger deliberately goes transparent so the menu reads as a
     single surface. The icon inherits this via its fill="currentColor". */
  .sim-function-btn.assigned :global(.btn-expandable:not(.expanded) .btn-expandable__trigger) {
    background: var(--text-primary);
    color: var(--bg-primary);
    border: 2px solid var(--text-primary);
  }

  /* The base trigger's hover sets colour to --text-primary, which on the filled
     background would make the icon invisible. Hold the inverted colour and use
     the glow alone, as .btn-enabled:hover does. */
  .sim-function-btn.assigned :global(.btn-expandable:not(.expanded) .btn-expandable__trigger:hover:not(:disabled)) {
    color: var(--bg-primary);
    border: 2px solid var(--text-primary);
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }

  /* The icon layers are absolutely positioned, so the trigger has to be the
     containing block: ButtonExpandable does not position it itself. */
  .sim-function-btn :global(.btn-expandable__trigger) {
    position: relative;
  }

  /* Cross-fade the dial out and the plus in, matching the bind buttons' icon
     swap in ButtonRound (.bind-icon). Only while unbound: a bound trigger has
     no plus layer, so fading its dial out would leave it empty. */
  .sim-function-layer {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    inset: 0;
    transition: opacity 0.2s ease;
  }

  .sim-function-layer.default {
    opacity: 1;
  }

  .sim-function-layer.hover {
    opacity: 0;
  }

  .sim-function-btn:not(.assigned) :global(.btn-expandable__trigger:hover .sim-function-layer.default) {
    opacity: 0;
  }

  .sim-function-btn:not(.assigned) :global(.btn-expandable__trigger:hover .sim-function-layer.hover) {
    opacity: 1;
  }
</style>