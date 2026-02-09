<!--
  ButtonExpandable Component
  A button that expands to display a list of options (Utilised for Add Application, Settings Menu, Close Menu)
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  
  interface Props {
    expanded: boolean;
    disabled?: boolean;
    ariaLabel: string;
    title: string;
    variant?: 'default' | 'controls';
    anchor?: 'left' | 'right';
    onboarding?: boolean;
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
    anchor = 'left',
    onboarding = false,
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
  class="btn-expandable {variant} anchor-{anchor} {className}"
  class:expanded
  class:onboarding
>
  {#if expanded && children}
    <div class="btn-expandable__list" role="listbox">
      {@render children()}
    </div>
  {/if}
  
  <button 
    class="btn-expandable__trigger"
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
      <!-- Default +/X icon -->
      <svg class="btn-expandable__icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor" aria-hidden="true">
        <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
      </svg>
    {/if}
  </button>
</div>

<style>
  .btn-expandable {
    position: relative;
    width: 46px;
    height: 100%;
    border-radius: 29px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    transition: width 0.3s ease, height 0.3s ease, background 0.3s ease, border-color 0.3s ease, padding 0.3s ease;
    background: transparent;
    border: 1px solid transparent;
    box-sizing: border-box;
  }

  /* Anchor positioning - button sticks to anchor side when collapsed */
  .btn-expandable.anchor-left {
    align-items: flex-start;
  }

  .btn-expandable.anchor-right {
    align-items: flex-end;
  }

  .btn-expandable.onboarding {
    height: 46px;
  }

  .btn-expandable.expanded {
    width: 180px;
    height: 100%;
    max-height: 100%;
    align-items: stretch;
    background: var(--bg-card);
    border-color: var(--text-muted);
    justify-content: space-between;
    overflow: visible;
    padding: 6px;
  }

  /* Button styles */
  .btn-expandable__trigger {
    width: 100%;
    height: 100%;
    min-width: 46px;
    min-height: 46px;
    border-radius: 23px;
    background: var(--bg-card);
    border: 1px solid var(--text-muted);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: border-color 0.2s ease, box-shadow 0.2s ease, height 0.3s ease, background 0.3s ease, width 0.3s ease, margin 0.3s ease;
    flex-shrink: 0;
    font-size: 0.8rem;
    z-index: 2;
  }

  .btn-expandable.expanded .btn-expandable__trigger {
    width: calc(100% - 12px);
    height: 46px;
    width: 100%;
    min-height: 46px;
    background: transparent;
    border-color: transparent;
  }

  .btn-expandable__trigger:hover:not(:disabled) {
    border: 1.5px solid var(--text-primary);
    color: var(--text-primary);
    box-shadow: 0 0 80px rgba(255, 255, 255, 0.1);
  }

  .btn-expandable.expanded .btn-expandable__trigger:hover:not(:disabled) {
    border: 1px solid transparent;
    box-shadow: none;
    background: var(--bg-card-hover);
  }

  .btn-expandable__trigger:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .btn-expandable__trigger .btn-expandable__icon {
    width: 24px;
    height: 24px;
    opacity: 1;
    transition: opacity 0.2s ease, transform 0.3s ease;
  }

  .btn-expandable.expanded .btn-expandable__icon {
    opacity: 1;
    transform: rotate(45deg);
  }

  .btn-expandable.controls.expanded .btn-expandable__icon {
    transform: none;
  }

  /* List styles */
  .btn-expandable__list {
    display: flex;
    flex-direction: column;
    overflow-y: visible;
    min-height: 0;
    gap: 6px;
    z-index: 1;
  }

  .btn-expandable__list::-webkit-scrollbar {
    display: none;
  }
</style>
