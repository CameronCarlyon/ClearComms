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
    ariaLabel?: string;
    fullWidth?: boolean;
    animationIndex?: number;
    class?: string;
  }
  
  let { 
    processName, 
    displayName,
    danger = false,
    ariaLabel,
    fullWidth = false,
    animationIndex = 0,
    class: className = ''
  }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    select: { processName: string | undefined };
  }>();
  
  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    dispatch('select', { processName });
  }
</script>

<button 
  class="list-option {className}"
  class:danger
  class:full-width={fullWidth}
  role="option"
  aria-selected="false"
  onclick={handleClick}
  aria-label={ariaLabel || (displayName ? `Select ${displayName}` : '')}
  style="--animation-delay: {animationIndex * 0.05}s"
>
  {displayName || ''}
</button>

<style>
  .list-option {
    padding: 1rem;
    min-width: 100%;
    background: transparent;
    border: none;
    border-radius: 23px;
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition: background 0.3s ease, box-shadow 0.2s ease;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    animation: fadeInSlide 0.25s ease-out forwards;
    animation-delay: var(--animation-delay, 0s);
    opacity: 0;
    height: 46px;
    min-height: 46px;
  }

  .close-option {
    text-align: center;
  }

  @keyframes fadeInSlide {
    from {
      opacity: 0;
      transform: translateY(-8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .list-option.full-width {
    width: calc(100% - 12px);
    white-space: normal;
  }

  .list-option:hover {
    background: var(--bg-card-hover);
  }

  .list-option:active {
    background: var(--text-primary);
    color: var(--bg-primary);
  }

  .list-option.danger:hover {
    background: #ff4444 !important;
    color: white !important;
    box-shadow: 0 0 80px rgba(255, 68, 68, 0.5);
  }
</style>
