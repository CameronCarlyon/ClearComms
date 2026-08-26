<!--
  Dropdown Component
  A dropdown selector for the settings menu.
  Expands downward to reveal options inside the button container.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import StatusIndicator from './StatusIndicator.svelte';

  interface Props {
    label: string;
    options?: string[];
    selected?: string;
    ariaLabel?: string;
    title?: string;
    onselect?: (detail: { value: string }) => void;
    children?: Snippet;
  }

  let {
    label,
    options,
    selected = $bindable(),
    ariaLabel,
    title,
    onselect,
    children
  }: Props = $props();

  let expanded = $state(false);
  let dropdownRef: HTMLElement | null = $state(null);

  // Base height includes: 2x StatusIndicator (40px each + 6px gap) + dropdown trigger (46px)
  const STATUS_INDICATOR_HEIGHT = 86; // 40 + 40 + 6
  let expandedHeight = $derived(
    STATUS_INDICATOR_HEIGHT + 46 + (options ?? []).length * 46 + Math.max(0, (options ?? []).length - 1) * 6
  );

  function handleSelect(value: string) {
    selected = value;
    onselect?.({ value });
  }

  function toggleExpanded() {
    expanded = !expanded;
  }

</script>

<div
  class="dropdown"
  class:expanded
  style="--expanded-height: {expandedHeight}px;"
>
  <button
    class="dropdown__trigger-wrapper"
    class:open={expanded}
    class:closed={!expanded}
    type="button"
    aria-label={ariaLabel || `Select ${label}`}
    {title}
    onclick={toggleExpanded}
    onkeydown={(e: KeyboardEvent) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        toggleExpanded();
      }
    }}
  >
    <span class="dropdown__label">{label}</span>
    <svg class="dropdown__chevron" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
      <path class="dropdown__chevron-path" d="M6 9 L12 15 L18 9"/>
    </svg>
  </button>
  <div class="dropdown__list-wrapper" role="listbox" aria-label={label} aria-hidden={!expanded}>
    <div class="dropdown__list">
      {#key expanded}
        <!-- SimConnect and WASM status indicators -->
        <span class="dropdown__item"><StatusIndicator statusSource="simconnect" /></span>
        <span class="dropdown__item"><StatusIndicator statusSource="wasm" /></span>
      {/key}
    </div>
  </div>
</div>

<style>
  .dropdown {
    width: 100%;
    height: 46px;
    transition: height 0.3s ease;
    border-radius: 24px;
    overflow: hidden;
  }

  .dropdown:hover {
    background: var(--bg-card-hover);
  }

  .dropdown.expanded {
    height: var(--expanded-height, 200px);
    background: var(--bg-card-hover);
  }

  .dropdown:not(.expanded):active {
    transform: scale(0.985);
    transition: transform 120ms ease;
    transform-origin: center;
  }


  .dropdown__trigger-wrapper {
    width: 100%;
    height: 46px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: 24px;
    color: var(--text-primary);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: background 0.3s ease, height 0.3s ease, width 0.3s ease, transform 0.12s ease, margin 0.3s ease;
    transform-origin: center;
    position: relative;
    outline: none;
  }

  .dropdown__trigger-wrapper:active {
    transform: scale(0.985);
  }

  .dropdown__trigger-wrapper:hover {
    background: var(--bg-card-hover);
  }

  .dropdown__trigger-wrapper.open {
    width: 136.67px;
    height: 40px;
    margin: 6px 6px 0px 6px;
  }

  .dropdown__trigger-wrapper.closed {
    height: 46px;
  }

  .dropdown__trigger-wrapper:focus-visible {
    outline: 2px solid var(--text-primary);
    outline-offset: 2px;
  }

  .dropdown__label {
    font-size: inherit;
    font-weight: inherit;
    transition: transform 0.2s ease;
    transform: translateX(0);
  }

  .dropdown__trigger-wrapper.open .dropdown__label,
  .dropdown__trigger-wrapper:hover .dropdown__label {
    transform: translateX(-15px);
  }

  .dropdown__chevron {
    position: absolute;
    right: 15px;
    top: 50%;
    transform: translateY(-50%);
    width: 24px;
    height: 24px;
    opacity: 0;
    transition: opacity 0.2s ease;
  }

  .dropdown__chevron-path {
    transition: d 0.4s ease;
    d: path("M6 12 L12 12 L18 12");
  }
  
  .dropdown__trigger-wrapper.open .dropdown__chevron,
  .dropdown__trigger-wrapper:hover .dropdown__chevron {
    opacity: 1;
  }

  .dropdown__trigger-wrapper:hover .dropdown__chevron-path {
    d: path("M6 9 L12 15 L18 9");
  }

  .dropdown__trigger-wrapper.open .dropdown__chevron-path {
    d: path("M6 15 L12 9 L18 15");
  }

  .dropdown__list-wrapper {
    width: 100%;
    height: calc(100% - 46px);
    overflow: hidden;
    transition: max-height 0.3s ease;
  }

  .dropdown__list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    height: 100%;
    overflow-y: auto;
    padding: 6px;
    box-sizing: border-box;
  }

  .dropdown__list::-webkit-scrollbar {
    display: none;
  }

  @keyframes dropdownFadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .dropdown__item {
    opacity: 0;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(1) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.20s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(2) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.25s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(3) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.30s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(4) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.35s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(5) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.40s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(6) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.45s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(7) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.50s;
  }

  .dropdown.expanded .dropdown__list > *:nth-child(8) {
    opacity: 0;
    animation: dropdownFadeIn 0.25s ease forwards;
    animation-delay: 0.55s;
  }

</style>
