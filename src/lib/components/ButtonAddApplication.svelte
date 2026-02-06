<!--
  ButtonAddApplication Component
  A specialized button for adding applications to the mixer
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { AudioSession } from '$lib/types';
  import ButtonExpandable from './ButtonExpandable.svelte';
  import ListOption from './ListOption.svelte';
  import { formatProcessName } from '$lib/stores/audioStore';

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

  const dispatch = createEventDispatcher<{
    select: { processName: string };
  }>();

  function handleSelect(event: CustomEvent<{ processName: string | undefined }>) {
    if (event.detail.processName) {
      dispatch('select', { processName: event.detail.processName });
    }
  }
</script>

<ButtonExpandable
  bind:expanded
  {onboarding}
  disabled={availableSessions.length === 0}
  ariaLabel={availableSessions.length > 0 ? (expanded ? "Close application list" : "Add application") : "No applications available"}
  title={availableSessions.length > 0 ? (expanded ? "Close" : "Add Application") : "No applications available"}
>
  {#snippet children()}
    {#each availableSessions as session, index}
      <ListOption
        processName={session.process_name}
        displayName={formatProcessName(session.process_name)}
        animationIndex={index}
        on:select={handleSelect}
      />
    {/each}
  {/snippet}
</ButtonExpandable>
