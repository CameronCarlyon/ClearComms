<!--
  ButtonRound Component
  Circular button with multiple variants for channel controls.
  
  Variants:
  - toggle: For on/off states (e.g., mute button). Solid white when active, hollow when inactive.
  - bind: For starting a binding action. Shows icon with hover state that swaps to hoverIcon.
  - mapping: Shows an existing mapping badge. Swaps to removeIcon on hover.
  - action: For destructive actions (e.g., remove). Shows danger styling when danger=true.
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    variant: 'toggle' | 'bind' | 'mapping' | 'action';
    active?: boolean;
    disabled?: boolean;
    danger?: boolean;
    ariaLabel?: string;
    title?: string;
    icon?: Snippet;
    hoverIcon?: Snippet;
    removeIcon?: Snippet;
  }

  let {
    variant,
    active = false,
    disabled = false,
    danger = false,
    ariaLabel = '',
    title = '',
    icon,
    hoverIcon,
    removeIcon
  }: Props = $props();

  const dispatch = createEventDispatcher<{
    toggle: void;
    startbind: void;
    cancel: void;
    remove: void;
    click: void;
  }>();

  function handleClick() {
    if (disabled) return;

    switch (variant) {
      case 'toggle':
        dispatch('toggle');
        break;
      case 'bind':
        if (active) {
          dispatch('cancel');
        } else {
          dispatch('startbind');
        }
        break;
      case 'mapping':
        dispatch('remove');
        break;
      case 'action':
        dispatch('click');
        break;
    }
  }

  // Determine button state class based on variant and props
  const stateClass = $derived.by(() => {
    if (disabled && variant !== 'mapping') return 'btn-unavail';
    
    switch (variant) {
      case 'toggle':
        return active ? 'btn-enabled' : 'btn-disabled';
      case 'bind':
        return 'btn-disabled';
      case 'mapping':
        return 'mapping-badge mapping-removable';
      case 'action':
        return danger ? 'btn-close' : 'btn-disabled';
      default:
        return 'btn-disabled';
    }
  });
</script>

<button
  class="btn btn-channel {stateClass}"
  class:btn-bind={variant === 'bind' && !active}
  class:active={active && variant === 'toggle'}
  class:button={variant === 'mapping'}
  onclick={handleClick}
  {disabled}
  aria-label={ariaLabel}
  aria-pressed={variant === 'toggle' ? active : undefined}
  {title}
  type="button"
>
  {#if variant === 'bind' && !active}
    <!-- Bind variant: swap between default and hover icons -->
    <span class="bind-icon default" aria-hidden="true">
      {#if icon}
        {@render icon()}
      {/if}
    </span>
    <span class="bind-icon hover" aria-hidden="true">
      {#if hoverIcon}
        {@render hoverIcon()}
      {/if}
    </span>
  {:else if variant === 'mapping'}
    <!-- Mapping variant: swap between icon and remove icon on hover -->
    <span class="mapping-icon default" aria-hidden="true">
      {#if icon}
        {@render icon()}
      {/if}
    </span>
    <span class="mapping-icon hover" aria-hidden="true">
      {#if removeIcon}
        {@render removeIcon()}
      {/if}
    </span>
  {:else}
    <!-- Toggle, active bind, and action variants: single icon -->
    {#if icon}
      {@render icon()}
    {/if}
  {/if}
</button>

<style>
  /* Base button styles - shared by all variants */
  .btn-channel {
    /* Layout */
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    
    /* Size */
    width: 46px;
    height: 46px;
    min-width: 46px;
    min-height: 46px;
    
    /* Shape */
    border-radius: 50%;
    
    /* Default appearance */
    background: var(--bg-card);
    color: var(--text-primary);
    border: 0.5px solid var(--text-muted);
    
    /* Interaction */
    cursor: pointer;
    transition: box-shadow 0.2s ease, border-color 0.2s ease;
  }

  .btn-channel:active:not(:disabled) {
    transform: scale(0.98);
  }

  /* Variant: Enabled (solid white) */
  .btn-enabled {
    background: var(--text-primary);
    color: var(--bg-primary);
    border: 2px solid var(--text-primary);
  }

  .btn-enabled :global(svg) {
    fill: #181818;
  }

  .btn-enabled:hover:not(:disabled) {
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }

  /* Variant: Disabled (hollow outline with hover) */
  .btn-disabled:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.45);
  }

  /* Variant: Unavailable (no interaction) */
  .btn-unavail {
    cursor: not-allowed;
    pointer-events: none;
    opacity: 0.5;
  }

  /* Variant: Close/Danger (red background) */
  .btn-close {
    background: #ff4444;
    color: white;
    border: none;
  }

  .btn-close:hover:not(:disabled) {
    box-shadow: 0 0 80px rgba(255, 68, 68, 0.5);
  }

  /* Variant: Mapping badge */
  .mapping-badge {
    position: relative;
  }

  .mapping-badge.mapping-removable:hover {
    border-color: var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.45);
  }

  /* Bind button icon animation */
  .btn-channel.btn-bind {
    position: relative;
  }

  .bind-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    inset: 0;
    transition: opacity 0.2s ease;
  }

  .bind-icon.default {
    opacity: 1;
  }

  .bind-icon.hover {
    opacity: 0;
  }

  .btn-bind:hover .bind-icon.default {
    opacity: 0;
  }

  .btn-bind:hover .bind-icon.hover {
    opacity: 1;
  }

  /* Mapping badge icon animation (swap to X on hover) */
  .mapping-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    position: absolute;
    inset: 0;
    transition: opacity 0.2s ease;
  }

  .mapping-icon.default {
    opacity: 1;
  }

  .mapping-icon.hover {
    opacity: 0;
  }

  .mapping-badge.mapping-removable:hover .mapping-icon.default {
    opacity: 0;
  }

  .mapping-badge.mapping-removable:hover .mapping-icon.hover {
    opacity: 1;
    color: var(--text-primary);
  }
</style>
