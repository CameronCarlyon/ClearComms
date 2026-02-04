<!--
  ButtonPill Component
  Reusable pill-shaped button with loading state support
-->
<script lang="ts">
  interface Props {
    label: string;
    loadingLabel?: string;
    disabled?: boolean;
    isLoading?: boolean;
    onclick?: () => void | Promise<void>;
    ariaLabel?: string;
    class?: string;
  }

  let {
    label = 'Button',
    loadingLabel = 'Loading...',
    disabled = false,
    isLoading = false,
    onclick,
    ariaLabel,
    class: className = ''
  }: Props = $props();

  async function handleClick() {
    if (disabled || isLoading) return;
    await onclick?.();
  }
</script>

<button
  class="btn-pill {className}"
  onclick={handleClick}
  disabled={disabled || isLoading}
  aria-label={ariaLabel}
  aria-busy={isLoading}
>
  {isLoading ? loadingLabel : label}
</button>

<style>
  .btn-pill {
    padding: 12px 24px;
    background: var(--text-primary);
    color: var(--bg-primary);
    border: none;
    border-radius: 8px;
    font-size: 1rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s ease;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .btn-pill:hover:not(:disabled) {
    box-shadow: 0 0 100px rgba(255, 255, 255, 0.75);
  }

  .btn-pill:disabled {
    opacity: 0.7;
    cursor: not-allowed;
  }
</style>
