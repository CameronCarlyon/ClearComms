<!--
  ButtonExpandable Component
  A button that expands to show a list of options (used for Add App, Settings, Close menus)
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  
  interface Props {
    expanded: boolean;
    disabled?: boolean;
    ariaLabel: string;
    title: string;
    variant?: 'default' | 'controls';
    class?: string;
    icon?: Snippet;
    expandedIcon?: Snippet;
    children?: Snippet;
  }
  
  let { 
    expanded = $bindable(), 
    disabled = false, 
    ariaLabel, 
    title, 
    variant = 'default',
    class: className = '',
    icon,
    expandedIcon,
    children
  }: Props = $props();
  
  function handleClick() {
    expanded = !expanded;
  }
</script>

<div 
  class="btn-add-app-container {variant} {className}"
  class:expanded
>
  {#if expanded && children}
    <div class="add-app-list" role="listbox">
      {@render children()}
    </div>
  {/if}
  
  <button 
    class="btn-add-app"
    onclick={handleClick}
    {disabled}
    aria-label={ariaLabel}
    {title}
    aria-expanded={expanded}
  >
    {#if expanded && expandedIcon}
      {@render expandedIcon()}
    {:else if icon}
      {@render icon()}
    {:else}
      <!-- Default plus/x icon -->
      <svg class="add-app-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor" aria-hidden="true">
        <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
      </svg>
    {/if}
  </button>
</div>

<style>
  .btn-add-app-container {
    position: relative;
    width: 46px;
    height: 100%;
    border-radius: 29px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    transition: width 0.3s ease, background 0.3s ease, border-color 0.3s ease;
    background: transparent;
    border: 1px solid transparent;
  }

  .btn-add-app-container.expanded {
    width: 180px;
    background: var(--bg-card);
    border-color: var(--text-muted);
    justify-content: space-between;
  }

  /* Controls variant */
  .btn-add-app-container.controls {
    height: 46px;
    width: 46px;
    justify-content: flex-end;
    transition: width 0.3s ease, height 0.3s ease, background 0.3s ease, border-color 0.3s ease, transform 0.3s ease, opacity 0.3s ease;
    transform: scale(0);
    align-self: flex-end;
  }

  .btn-add-app-container.controls.expanded {
    height: 100%;
    transform: scale(1) !important;
    background: var(--bg-card);
    border-color: var(--text-muted);
    justify-content: flex-start;
  }

  .btn-add-app-container.controls.expanded .add-app-list {
    flex: 1;
    overflow: hidden;
  }

  /* Hidden state */
  .btn-add-app-container.hidden {
    transform: scale(0);
    opacity: 0;
    pointer-events: none;
  }

  /* Button styles */
  .btn-add-app {
    width: 46px;
    height: 100%;
    min-width: 46px;
    min-height: 46px;
    border-radius: 23px;
    background: var(--bg-card);
    border: 1px solid var(--text-muted);
    color: white;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.2s ease, box-shadow 0.2s ease, height 0.3s ease, background 0.3s ease;
    flex-shrink: 0;
  }

  .btn-add-app-container.expanded .btn-add-app {
    width: calc(100% - 12px);
    height: 46px;
    min-height: 46px;
    margin: 6px;
    margin-top: 0px;
    background: transparent;
    border-color: transparent;
    border-radius: 23px;
  }

  .btn-add-app:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    color: var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.1);
  }

  .btn-add-app-container.expanded .btn-add-app:hover:not(:disabled) {
    border: 1px solid transparent;
    box-shadow: none;
    background: var(--bg-card-hover);
  }

  .btn-add-app:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .btn-add-app .add-app-icon {
    width: 24px;
    height: 24px;
    opacity: 1;
    transition: opacity 0.2s ease, transform 0.3s ease;
  }

  .btn-add-app-container.expanded .add-app-icon {
    opacity: 1;
    transform: rotate(45deg);
  }

  .btn-add-app-container.controls.expanded .add-app-icon {
    transform: none;
  }

  /* List styles */
  .add-app-list {
    display: flex;
    flex-direction: column;
    width: 100%;
    overflow-y: auto;
    min-height: 0;
  }

  .add-app-list::-webkit-scrollbar {
    display: none;
  }
</style>
