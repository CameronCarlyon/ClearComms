<!--
  ButtonAddApplication Component
  A button that expands into a menu for adding pinned applications to the mixer.
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { AudioSession } from '$lib/types';
  import ButtonExpandable from './ButtonExpandable.svelte';
  import ListOption from './ListOption.svelte';
  import { formatProcessName, applyDisplayNameOverride } from '$lib/stores/audioStore';
  import { registerMixerMenu } from '$lib/stores/mixerMenuStore.svelte';

  interface Props {
    expanded: boolean;
    availableSessions: AudioSession[];
    onboarding?: boolean;
  }

  let {
    expanded = $bindable(),
    availableSessions,
    onboarding = false
  }: Props = $props();

  // Only one mixer menu may be open at a time: opening this one collapses any
  // channel's simulator function menu.
  registerMixerMenu(() => expanded, () => { expanded = false; });

  const dispatch = createEventDispatcher<{
    select: { processName: string };
  }>();

  function handleSelect(event: CustomEvent<{ processName: string | undefined }>) {
    if (event.detail.processName) {
      dispatch('select', { processName: event.detail.processName });
    }
  }

  /** Track previous expanded state to reset animation on each open. */
  let prevExpanded = $state(false);
  /** Key value to force re-mount of the {#each} block when the menu opens. */
  let animationKey = $state(0);

  $effect(() => {
    if (expanded && !prevExpanded) {
      // Menu just opened - increment key to force re-mount and restart animations
      animationKey++;
    }
    prevExpanded = expanded;
  });
</script>

<ButtonExpandable
  bind:expanded
  {onboarding}
  disabled={availableSessions.length === 0}
  ariaLabel={availableSessions.length > 0 ? (expanded ? "Close application list" : "Add application") : "No applications available"}
  title={availableSessions.length > 0 ? (expanded ? "Close" : "Add Application") : "No applications available"}
>
  {#snippet children()}
    {#key animationKey}
      {#each availableSessions as session, i}
        {@const delay = (0.2 + i * 0.05).toFixed(2)}
        <span class="expandable__item" style="animation-delay: {delay}s">
          <ListOption
            processName={session.process_name}
            displayName={applyDisplayNameOverride(session.display_name || formatProcessName(session.process_name), session.process_name)}
            on:select={handleSelect}
          />
        </span>
      {/each}
    {/key}
  {/snippet}
</ButtonExpandable>

