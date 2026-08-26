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
    /** Marks this option as the current selection */
    selected?: boolean;
    ariaLabel?: string;
    fullWidth?: boolean;
    class?: string;
  }

  let {
    processName,
    displayName,
    danger = false,
    warning = false,
    selected = false,
    ariaLabel,
    fullWidth = false,
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
  class:warning
  class:selected
  class:full-width={fullWidth}
  role="option"
  aria-selected={selected}
  onclick={handleClick}
  aria-label={ariaLabel || (displayName ? `${selected ? 'Clear' : 'Select'} ${displayName}` : '')}
>
  {displayName || ''}
  {#if selected}
    <!-- Revealed on hover in place of the label: choosing the current
         selection clears it. -->
    <svg class="list-option__clear" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" aria-hidden="true">
      <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
    </svg>
  {/if}
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
    transition: background 0.3s ease, color 0.2s ease, box-shadow 0.2s ease, transform 0.12s ease;
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

  /* Selected state: solid fill with inverted text, matching the collapsed
     ButtonSimFunction trigger and ButtonRound's .btn-enabled. The :hover and
     :active pairs are spelled out because those base rules set their own
     background and colour at equal or higher specificity, which would
     otherwise wash out the fill. */
  .list-option.selected {
    position: relative;
    background: var(--text-primary);
    color: var(--bg-primary);
  }

  .list-option.selected:active {
    background: var(--text-primary);
    color: var(--bg-primary);
    transform: scale(0.97);
  }

  /* Hovering the current selection offers to clear it, so the label gives way
     to the close icon. Only the label's ink is hidden: it keeps its box, so
     the row neither reflows nor changes width. */
  .list-option.selected:hover,
  .list-option.selected:hover:active {
    background: var(--text-primary);
    color: transparent;
  }

  /* Sits over the hidden label rather than beside it, so the icon lands in the
     centre of the row. Its fill is set explicitly because currentColor is
     transparent while hovering. */
  .list-option__clear {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    opacity: 0;
    transition: opacity 0.2s ease;
    fill: var(--bg-primary);
  }

  .list-option.selected:hover .list-option__clear {
    opacity: 1;
  }

</style>
