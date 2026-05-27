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
    animationIndex?: number;
    animationType?: 'fadeInSlide' | 'fadeIn';
    ontoggle?: (detail: { checked: boolean }) => void;
  }

  let {
    label,
    checked = $bindable(false),
    disabled = false,
    ariaLabel,
    title,
    animationIndex = 0,
    animationType = 'fadeInSlide',
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
  class:fade-in={animationType === 'fadeIn'}
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={ariaLabel || `Toggle ${label}`}
  {title}
  onclick={handleClick}
  style="--animation-delay: {animationIndex * 0.05}s"
  disabled={disabled || undefined}
>
  <span class="option-toggle__indicator" aria-hidden="true"></span>
  <span class="option-toggle__label">{label}</span>
</button>

<style>
  .option-toggle {
    --thumb-inset: 10.5px;
    --thumb-size: 25px;
    --thumb-background: var(--text-primary);
    position: relative;
    width: 100%;
    height: 46px;
    box-sizing: border-box;
    background: transparent;
    animation-delay: var(--animation-delay, 0s);
    border: none;
    border-radius: 999px;
    color: var(--text-primary);
    cursor: pointer;
    transform-origin: center;
    transition:
      background 300ms ease,
      transform 120ms ease;
    animation-name: fadeInSlide;
    animation-duration: 0.25s;
    animation-timing-function: ease-out;
    animation-fill-mode: forwards;
    opacity: 0;
  }

  .option-toggle.fade-in {
    animation-name: fadeIn;
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

  .option-toggle__indicator {
    position: absolute;
    left: var(--thumb-inset);
    top: 50%;
    width: var(--thumb-size);
    height: var(--thumb-size);
    border-radius: 50%;
    background: var(--thumb-background);
    opacity: 0;
    transform: translateY(-50%);
    transition:
      background 300ms ease,
      opacity 180ms ease,
      transform 360ms cubic-bezier(0.22, 1, 0.36, 1),
      left 360ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .option-toggle.checked .option-toggle__indicator {
    left: calc(100% - var(--thumb-inset) - var(--thumb-size));
    transform: translateY(-50%) scale(1.04);
  }

  .option-toggle:hover:not(.disabled):not(.checked) .option-toggle__indicator,
  .option-toggle:focus-visible:not(.disabled):not(.checked) .option-toggle__indicator {
    opacity: 0.2;
  }

  .option-toggle:hover:not(.disabled).checked .option-toggle__indicator,
  .option-toggle:focus-visible:not(.disabled).checked .option-toggle__indicator {
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

  .option-toggle:active:not(.disabled) .option-toggle__indicator {
    transform: translateY(-50%) scale(0.97);
  }

  .option-toggle.disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  @media (prefers-reduced-motion: reduce) {
    .option-toggle,
    .option-toggle__label,
    .option-toggle__indicator {
      transition-duration: 0ms !important;
      animation-duration: 0ms !important;
    }
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

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>