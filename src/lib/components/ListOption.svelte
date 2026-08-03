<!--
  ListOption Component
  A selectable item in a dropdown or list
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  
  interface Props {
    processName?: string;
    displayName?: string;
    danger?: boolean;
    warning?: boolean;
    ariaLabel?: string;
    fullWidth?: boolean;
    class?: string;
    /** CSS animation delay in seconds for sequential entrance animation. */
    animationDelay?: string;
  }
  
  let {
    processName,
    displayName,
    danger = false,
    warning = false,
    ariaLabel,
    fullWidth = false,
    class: className = '',
    animationDelay = '0s'
  }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    select: { processName: string | undefined };
  }>();
  
  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    dispatch('select', { processName });
  }

  /** Action to trigger a fade-in transition after mount.
   *  Uses CSS transition by setting opacity after a reflow. */
  function fadeIn(node: HTMLElement, delaySec: string) {
    // Start invisible
    node.style.opacity = '0';
    // Force reflow
    void node.offsetHeight;
    // Set the transition with the specified delay
    node.style.transition = `opacity 0.25s ease ${delaySec}`;
    // Trigger the transition
    node.style.opacity = '1';
    return {
      destroy() {
        node.style.transition = '';
        node.style.opacity = '';
      }
    };
  }
</script>

<button
  class="list-option {className}"
  class:danger
  class:warning
  class:full-width={fullWidth}
  role="option"
  aria-selected="false"
  onclick={handleClick}
  aria-label={ariaLabel || (displayName ? `Select ${displayName}` : '')}
  use:fadeIn={animationDelay}
>
  {displayName || ''}
</button>

<style>
  .list-option {
    display: flex;
    flex-direction: row;
    align-items: center;
    width: 100%;
    padding: 0.75rem 1rem;
    padding: 1rem;
    min-width: 100%;
    background: transparent;
    border: none;
    border-radius: 23px;
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 500;
    text-align: center;
    justify-content: center;
    cursor: pointer;
    transform-origin: center;
    transition: background 0.3s ease, box-shadow 0.2s ease, transform 0.12s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    height: 46px;
    min-height: 46px;
  }

  .close-option {
    text-align: center;
  }

  .list-option.full-width {
    width: calc(100% - 12px);
    white-space: normal;
  }

  .list-option:hover {
    background: var(--bg-card-hover);
  }

  .list-option:active {
    background: var(--bg-card-hover);
    color: var(--text-primary);
    transform: scale(0.97);
  }

  .list-option.danger:hover {
    background: #ff4444 !important;
    color: white !important;
    box-shadow: 0 0 80px rgba(255, 68, 68, 0.5);
  }

  .list-option.warning:hover {
    background: #ff8c00 !important;
    color: white !important;
    box-shadow: 0 0 80px rgba(255, 140, 0, 0.5);
  }

</style>
