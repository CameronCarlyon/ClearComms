<!--
  OptionToggle Component
  A pill-shaped toggle button for the settings menu
-->
<script lang="ts">
  interface Props {
    label: string;
    checked?: boolean;
    disabled?: boolean;
    ariaLabel?: string;
    title?: string;
    ontoggle?: (detail: { checked: boolean }) => void;
  }

  let {
    label,
    checked = $bindable(false),
    disabled = false,
    ariaLabel,
    title,
    ontoggle
  }: Props = $props();

  function handleClick() {
    if (disabled) {
      return;
    }

    checked = !checked;
    ontoggle?.({ checked });
  }
</script>

<button
  class="option-toggle"
  class:checked
  class:disabled
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel || `Toggle ${label}`}
  {title}
  onclick={handleClick}
  disabled={disabled || undefined}
>
  <span class="option-toggle__dot" aria-hidden="true"></span>
  <span class="option-toggle__label">{label}</span>
</button>

<style>
  .option-toggle {
    --dot-inset: 10.5px;
    --dot-size: 25px;
    --dot-size-stretched: 40px;
    --dot-background: var(--text-primary);
    --dot-motion-duration: 480ms;
    --dot-motion-ease: cubic-bezier(0.22, 1, 0.36, 1);
    position: relative;
    width: 100%;
    height: 46px;
    box-sizing: border-box;
    background: transparent;
    border: none;
    border-radius: 999px;
    color: var(--text-primary);
    cursor: pointer;
    transform-origin: center;
    transition:
      background 300ms ease,
      transform 120ms ease;
  }

  .option-toggle__label {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 0.8rem;
    font-weight: 500;
    letter-spacing: 0.01em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
    transition: opacity 240ms ease;
  }

  .option-toggle__dot {
    position: absolute;
    left: calc(var(--dot-inset) + (var(--dot-size) / 2));
    top: 50%;
    width: var(--dot-size);
    height: var(--dot-size);
    border-radius: 50%;
    background: var(--dot-background);
    opacity: 0;
    transform: translate(-50%, -50%);
    transition:
      background 300ms ease,
      opacity 180ms ease,
      transform var(--dot-motion-duration) var(--dot-motion-ease),
      left var(--dot-motion-duration) var(--dot-motion-ease),
      width 1000ms ease;
    /* Different animation name per state forces re-trigger on each toggle */
    animation: dot-stretch-off var(--dot-motion-duration) var(--dot-motion-ease);
  }

  .option-toggle.checked .option-toggle__dot {
    left: calc(100% - var(--dot-inset) - (var(--dot-size) / 2));
    transform: translate(-50%, -50%);
    animation: dot-stretch-on var(--dot-motion-duration) var(--dot-motion-ease);
  }

  .option-toggle:hover:not(.disabled):not(.checked) .option-toggle__dot,
  .option-toggle:focus-visible:not(.disabled):not(.checked) .option-toggle__dot {
    opacity: 0.2;
  }

  .option-toggle:hover:not(.disabled).checked .option-toggle__dot,
  .option-toggle:focus-visible:not(.disabled).checked .option-toggle__dot {
    opacity: 1;
  }

  .option-toggle:hover:not(.disabled) {
    background: var(--bg-card-hover);
  }

  .option-toggle:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }

  .option-toggle:active:not(.disabled) {
    transform: scale(0.985);
    background: var(--bg-card-hover);
  }

  .option-toggle:active:not(.disabled) .option-toggle__dot {
    transform: translate(-50%, -50%) scale(0.97);
  }

  .option-toggle.disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  @media (prefers-reduced-motion: reduce) {
    .option-toggle,
    .option-toggle__label,
    .option-toggle__dot {
      transition-duration: 0ms !important;
      animation-duration: 0ms !important;
    }
  }

  /* Shared stretch profile (via CSS vars) keeps the shape in one place.
     Two animation names are still required so toggling checked reliably
     restarts the keyframed stretch in pure CSS. */
  @keyframes dot-stretch-on {
    0%   { width: var(--dot-size); }
    30%  { width: var(--dot-size-stretched); }
    40%  { width: var(--dot-size-stretched); }
    70%  { width: var(--dot-size); }
    100% { width: var(--dot-size); }
  }

  @keyframes dot-stretch-off {
    0%   { width: var(--dot-size); }
    30%  { width: var(--dot-size-stretched); }
    40%  { width: var(--dot-size-stretched); }
    70%  { width: var(--dot-size); }
    100% { width: var(--dot-size); }
  }

</style>