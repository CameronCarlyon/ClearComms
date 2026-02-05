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
  }

  let {
    expanded = $bindable(),
    availableSessions
  }: Props = $props();

  const dispatch = createEventDispatcher<{
    select: { processName: string };
  }>();

  function handleSelect(event: CustomEvent<{ processName: string }>) {
    dispatch('select', event.detail);
  }
</script>

<ButtonExpandable
  bind:expanded
  disabled={availableSessions.length === 0}
  ariaLabel={availableSessions.length > 0 ? (expanded ? "Close application list" : "Add application") : "No applications available"}
  title={availableSessions.length > 0 ? (expanded ? "Close" : "Add Application") : "No applications available"}
>
  {#snippet children()}
    {#each availableSessions as session}
      <ListOption
        processName={session.process_name}
        displayName={formatProcessName(session.process_name)}
        on:select={handleSelect}
      />
    {/each}
  {/snippet}
</ButtonExpandable>
