<!--
  SettingsOption Component
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  
  interface Props {
    processName: string;
    displayName: string;
  }
  
  let { processName, displayName }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    select: { processName: string };
  }>();
  
  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    dispatch('select', { processName });
  }
</script>

<button 
  class="settings-option"
  role="option"
  aria-selected="false"
  onclick={handleClick}
  aria-label="Select {displayName}"
>
  {displayName}
    <div class="btn-radio-container">
        <div class="btn-radio">

        </div>
    </div>
    <textarea>
    </textarea>
</button>

<style>
  .settings-option {
    padding: 1rem;
    margin: 6px;
    background: transparent;
    border: none;
    border-radius: 23px;
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
  }

  .settings-option:hover {
    background: var(--bg-card-hover);
  }

  .settings-option:active {
    background: var(--text-primary);
    color: var(--bg-primary);
  }
</style>
