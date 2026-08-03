<!--
  SettingsOption Component
  An icon button for settings menu items
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  
  interface Props {
    ariaLabel?: string;
    ariaPressed?: boolean;
    title?: string;
    icon?: Snippet;
    onclick?: (e: MouseEvent) => void;
  }
  
  let {
    ariaLabel,
    ariaPressed,
    title,
    icon,
    onclick
  }: Props = $props();
  
  function handleClick(e: MouseEvent) {
    onclick?.(e);
  }
</script>

<button
  class="settings-option"
  onclick={handleClick}
  aria-label={ariaLabel}
  aria-pressed={ariaPressed}
  {title}
  type="button"
>
  {#if icon}
    {@render icon()}
  {/if}
</button>

<style>
  .settings-option {
    width: 46px;
    height: 46px;
    min-width: 46px;
    min-height: 46px;
    background: transparent;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s ease;
  }

  .settings-option:hover {
    background: var(--bg-card-hover);
  }

  .settings-option:active {
    background: var(--text-primary);
    color: var(--bg-primary);
  }
</style>
