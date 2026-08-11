<!--
  ButtonSimChannel Component
  Fixed-width expandable button matching the dock's pattern. The trigger stays
  pinned at 46px wide and expands downward (height 100%) to reveal the category
  list, just like the Dock settings/close menus — without the width growth.
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { SimChannelCategory } from '$lib/types';
  import ButtonExpandable from './ButtonExpandable.svelte';
  import ListOption from './ListOption.svelte';

  interface Props {
    processName: string;
    /** Currently assigned category, if any */
    assigned: SimChannelCategory | null;
    /** Categories the active aircraft profile supports */
    categories: SimChannelCategory[];
  }

  let { processName, assigned, categories }: Props = $props();

  let expanded = $state(false);

  const dispatch = createEventDispatcher<{
    setsimcategory: { processName: string; category: SimChannelCategory | null };
  }>();

  function handleSelect(event: CustomEvent<{ processName: string | undefined }>) {
    const raw = event.detail.processName;
    if (raw === '__NONE__') {
      expanded = false;
      dispatch('setsimcategory', { processName, category: null });
    } else if (raw) {
      expanded = false;
      dispatch('setsimcategory', { processName, category: raw as SimChannelCategory });
    }
  }

  /** Track previous expanded state to re-mount the list for animation restart */
  let prevExpanded = $state(false);
  let listKey = $state(0);

  $effect(() => {
    if (expanded && !prevExpanded) listKey++;
    prevExpanded = expanded;
  });
</script>

<div class="sim-channel-btn">
  <ButtonExpandable
    bind:expanded
    ariaLabel={assigned ? `Change sim channel: ${assigned}` : 'Assign sim channel'}
    title={assigned ? `Sim Channel: ${assigned}` : 'Assign Sim Channel'}
    variant="controls"
  >
    {#snippet icon()}
      <!-- Antenna icon when unassigned; category text when assigned -->
      {#if assigned}
        <span class="sim-channel-trigger__label">{assigned}</span>
      {:else}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <path d="M5 12a10 10 0 0 1 14 0"/>
          <path d="M8.5 15.5a5 5 0 0 1 7 0"/>
          <circle cx="12" cy="19" r="1" fill="currentColor" stroke="none"/>
        </svg>
      {/if}
    {/snippet}
    {#snippet expandedIcon()}
      <!-- Close icon -->
      <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
        <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
      </svg>
    {/snippet}
    {#snippet children()}
      {#if assigned}
        <span class="expandable__item" style="animation-delay: 0.25s">
          <ListOption processName="__NONE__" displayName="None" animationDelay="0.25s" on:select={handleSelect} />
        </span>
      {/if}
      {#key listKey}
        {#each categories as category, i}
          {@const delay = (0.27 + i * 0.04).toFixed(2)}
          <span class="expandable__item">
            <ListOption processName={category} displayName={category} animationDelay={delay + 's'} on:select={handleSelect} />
          </span>
        {/each}
      {/key}
    {/snippet}
  </ButtonExpandable>
</div>

<style>
  .sim-channel-btn {
    flex-shrink: 0;
  }

  /* Match the "Add Application" button height when expanded — include transition */
  .sim-channel-btn :global(.btn-expandable) {
    transition:
      width 0.3s ease,
      height 0.3s ease,
      background 0.3s ease,
      border-color 0.3s ease,
      padding 0.3s ease;
  }

  .sim-channel-btn :global(.btn-expandable.expanded) {
    height: 572.67px !important;
    max-height: 572.67px !important;
  }

  /* Add gap between list items to match ButtonAddApplication styling */
  .sim-channel-btn :global(.btn-expandable__list) {
    gap: 6px;
  }

  .sim-channel-trigger__label {
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
</style>