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
    animationIndex?: number;
    icon?: Snippet;
    onclick?: (e: MouseEvent) => void;
  }
  
  let { 
    ariaLabel,
    ariaPressed,
    title,
    animationIndex = 0,
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
  style="--animation-delay: {animationIndex * 0.05}s"
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
    animation: fadeIn 0.25s ease-out forwards;
    animation-delay: var(--animation-delay, 0s);
    opacity: 0;
  }
  
  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .settings-option:hover {
    background: var(--bg-card-hover);
  }

  .settings-option:active {
    background: var(--text-primary);
    color: var(--bg-primary);
  }
</style>
