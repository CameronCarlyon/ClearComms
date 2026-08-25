<!--
  Dock Component
  Bottom hover dock with settings, edit, and close controls
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { createEventDispatcher } from 'svelte';
  import { ButtonExpandable, ButtonRound, ListOption, OptionToggle, Dropdown } from '$lib/components';
  
  interface Props {
    dockOpen: boolean;
    settingsMenuExpanded: boolean;
    closeMenuExpanded: boolean;
    isEditMode: boolean;
    audioInitialised: boolean;
    windowPinned: boolean;
  }
  
  let { 
    dockOpen = $bindable(),
    settingsMenuExpanded = $bindable(),
    closeMenuExpanded = $bindable(),
    isEditMode,
    audioInitialised,
    windowPinned
  }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    toggleeditmode: void;
    togglewindowpinned: void;
  }>();
  
  // Close the other menu when one opens (mutually exclusive)
  $effect(() => {
    if (settingsMenuExpanded && closeMenuExpanded) {
      closeMenuExpanded = false;
    }
  });
  
  $effect(() => {
    if (closeMenuExpanded && settingsMenuExpanded) {
      settingsMenuExpanded = false;
    }
  });
  
  // Close all menus when dock closes
  $effect(() => {
    if (!dockOpen) {
      settingsMenuExpanded = false;
      closeMenuExpanded = false;
    }
  });

  let dockContainer: HTMLElement | null = $state(null);
  
  function handleDockFocusIn(e: FocusEvent) {
    const relatedTarget = e.relatedTarget as Node | null;
    // Only open dock if focus came from within the page (user tabbing in)
    // If relatedTarget is null, focus came from window restoration, don't auto-open
    if (relatedTarget === null) {
      return;
    }
    dockOpen = true;
  }
  
  function handleDockFocusOut(e: FocusEvent) {
    const nextFocus = e.relatedTarget as Node | null;
    if (dockContainer && nextFocus && dockContainer.contains(nextFocus)) {
      return;
    }
    // Don't close dock if a menu is expanded or about to expand
    if (settingsMenuExpanded || closeMenuExpanded) {
      return;
    }
    dockOpen = false;
  }
  
  async function handleOpenGuide() {
    await invoke('open_url', { url: 'https://github.com/CameronCarlyon/ClearComms?tab=readme-ov-file#usage' });
  }
  
  function handleTogglePin() {
    dispatch('togglewindowpinned');
  }
  
  async function handleQuit() {
    await invoke('quit_application');
  }

  async function handleReboot() {
    await invoke('restart_application');
  }
  
  async function handleMinimise() {
    // Reset all menu and dock states before hiding
    closeMenuExpanded = false;
    settingsMenuExpanded = false;
    dockOpen = false;

    await invoke('hide_main_window');
  }
</script>

<div 
  class="dock-hover-zone" 
  class:expanded={settingsMenuExpanded || closeMenuExpanded}
  onmouseenter={() => { dockOpen = true; }}
  onmouseleave={() => { 
    if (!settingsMenuExpanded && !closeMenuExpanded) {
      dockOpen = false;
    }
  }}
  bind:this={dockContainer}
  onfocusin={handleDockFocusIn}
  onfocusout={handleDockFocusOut}
  role="region"
  aria-label="Application controls"
>
  <div 
    class="dock" 
    class:open={dockOpen}
    class:expanded-close={closeMenuExpanded}
    class:expanded-settings={settingsMenuExpanded}
  >
    <!-- Settings Menu -->
    <div class="settings-wrapper" class:expanded={settingsMenuExpanded} class:hidden={closeMenuExpanded}>
      <ButtonExpandable
        bind:expanded={settingsMenuExpanded}
        class={settingsMenuExpanded ? 'dock-expandable-open' : ''}
        anchor="left"
        ariaLabel={settingsMenuExpanded ? "Close settings menu" : "Open settings menu"}
        title={settingsMenuExpanded ? "Close" : "Menu"}
      >
      {#snippet icon()}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
          <path d="M259.1 73.5C262.1 58.7 275.2 48 290.4 48L350.2 48C365.4 48 378.5 58.7 381.5 73.5L396 143.5C410.1 149.5 423.3 157.2 435.3 166.3L503.1 143.8C517.5 139 533.3 145 540.9 158.2L570.8 210C578.4 223.2 575.7 239.8 564.3 249.9L511 297.3C511.9 304.7 512.3 312.3 512.3 320C512.3 327.7 511.8 335.3 511 342.7L564.4 390.2C575.8 400.3 578.4 417 570.9 430.1L541 481.9C533.4 495 517.6 501.1 503.2 496.3L435.4 473.8C423.3 482.9 410.1 490.5 396.1 496.6L381.7 566.5C378.6 581.4 365.5 592 350.4 592L290.6 592C275.4 592 262.3 581.3 259.3 566.5L244.9 496.6C230.8 490.6 217.7 482.9 205.6 473.8L137.5 496.3C123.1 501.1 107.3 495.1 99.7 481.9L69.8 430.1C62.2 416.9 64.9 400.3 76.3 390.2L129.7 342.7C128.8 335.3 128.4 327.7 128.4 320C128.4 312.3 128.9 304.7 129.7 297.3L76.3 249.8C64.9 239.7 62.3 223 69.8 209.9L99.7 158.1C107.3 144.9 123.1 138.9 137.5 143.7L205.3 166.2C217.4 157.1 230.6 149.5 244.6 143.4L259.1 73.5zM320.3 400C364.5 399.8 400.2 363.9 400 319.7C399.8 275.5 363.9 239.8 319.7 240C275.5 240.2 239.8 276.1 240 320.3C240.2 364.5 276.1 400.2 320.3 400z"/>
        </svg>
      {/snippet}
      {#snippet expandedIcon()}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
          <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
        </svg>
      {/snippet}
      {#snippet children()}
        <span class="expandable__item" style="animation-delay: 0.20s">
          <ListOption
            displayName="User Guide"
            fullWidth={true}
            ariaLabel="View ClearComms user guide (opens in external browser)."
            on:select={handleOpenGuide}
          />
        </span>
        <span class="expandable__item" style="animation-delay: 0.25s">
          <OptionToggle
            label="Pin Window"
            checked={windowPinned}
            ariaLabel={windowPinned ? "Disable always on top" : "Enable always on top"}
            title="Keep ClearComms' window on top of others, even when interacting with other windows."
            ontoggle={handleTogglePin}
          />
        </span>
        <span class="expandable__item" style="animation-delay: 0.30s">
          <Dropdown
            label="Nerd Zone"
            options={['Disabled', 'Enabled']}
            ariaLabel="View or hide debugging features."
            title="View debugging information for development and troubleshooting."
          />
        </span>
        <span class="expandable__item" style="animation-delay: 0.35s">
          <ListOption
            displayName="Reboot"
            warning={true}
            fullWidth={true}
            ariaLabel="Restart the application."
            on:select={handleReboot}
          />
        </span>
      {/snippet}
    </ButtonExpandable>
    </div>
    
    <!-- Edit Button -->
    <div class="edit-button-wrapper" class:hidden={settingsMenuExpanded || closeMenuExpanded} class:visible={dockOpen && !settingsMenuExpanded && !closeMenuExpanded}>
      <ButtonRound
        variant="toggle"
        active={isEditMode}
        disabled={!audioInitialised}
        ariaLabel={isEditMode ? 'Exit edit mode.' : 'Enter edit mode to configure bindings.'}
        title={isEditMode ? 'Exit Edit Mode' : 'Edit Bindings'}
        on:toggle={() => dispatch('toggleeditmode')}
      >
        {#snippet icon()}
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
            <path d="M416.9 85.2L372 130.1L509.9 268L554.8 223.1C568.4 209.6 576 191.2 576 172C576 152.8 568.4 134.4 554.8 120.9L519.1 85.2C505.6 71.6 487.2 64 468 64C448.8 64 430.4 71.6 416.9 85.2zM338.1 164L122.9 379.1C112.2 389.8 104.4 403.2 100.3 417.8L64.9 545.6C62.6 553.9 64.9 562.9 71.1 569C77.3 575.1 86.2 577.5 94.5 575.2L222.3 539.7C236.9 535.6 250.2 527.9 261 517.1L476 301.9L338.1 164z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    </div>
    
    <!-- Close Menu -->
    <div class="close-wrapper" class:expanded={closeMenuExpanded} class:hidden={settingsMenuExpanded}>
      <ButtonExpandable
        bind:expanded={closeMenuExpanded}
        class={closeMenuExpanded ? 'dock-expandable-open' : ''}
        anchor="right"
        ariaLabel={closeMenuExpanded ? "Close quit menu" : "Close application"}
        title={closeMenuExpanded ? "Close" : "Quit"}
      >
      {#snippet icon()}
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="24" height="24" fill="currentColor">
          <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
        </svg>
      {/snippet}
      {#snippet expandedIcon()}
        <p>Return</p>
      {/snippet}
      {#snippet children()}
        <span class="expandable__item" style="animation-delay: 0.20s">
          <ListOption
            displayName="Quit"
            danger={true}
            fullWidth={true}
            ariaLabel="Quit the application."
            class="close-option"
            on:select={handleQuit}
          />
        </span>
        <span class="expandable__item" style="animation-delay: 0.25s">
          <ListOption
            displayName="Minimise"
            fullWidth={true}
            ariaLabel="Minimise the application."
            class="close-option"
            on:select={handleMinimise}
          />
        </span>
      {/snippet}
    </ButtonExpandable>
    </div>
  </div>
</div>

<style>
  /* Dock hover zone */
  .dock-hover-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    width: 100%;
    flex-shrink: 0;
    min-height: 12px;
    cursor: pointer;
    transition: min-height 0.3s ease;
  }

  .dock-hover-zone.expanded {
    min-height: auto;
    cursor: default;
  }

  .dock-hover-zone::before {
    content: '';
    width: 40px;
    height: 4px;
    border-radius: 2px;
    background: var(--text-muted);
    opacity: 0.3;
    transition: opacity 0.3s ease, height 0.3s ease, margin 0.3s ease;
    margin: 4px 0;
  }

  .dock-hover-zone.expanded::before {
    height: 0;
    margin: 0;
    opacity: 0;
    pointer-events: none;
  }

  .dock-hover-zone.expanded:hover::before {
    opacity: 0;
  }

  .dock-hover-zone:hover::before {
    opacity: 0.6;
  }

  /* Dock container */
  .dock {
    --dock-height-transition-duration: 0.3s;
    --dock-height-transition-easing: cubic-bezier(0.32, 0.72, 0, 1);
    display: flex;
    flex-direction: row;
    justify-content: center;
    gap: 15px;
    align-items: flex-end;
    height: 0;
    max-height: 0;
    transition:
      height var(--dock-height-transition-duration) var(--dock-height-transition-easing),
      max-height var(--dock-height-transition-duration) var(--dock-height-transition-easing),
      padding 0.3s ease,
      gap 0.3s ease;
    position: relative;
    width: 162px;
  }

  .dock.open {
    height: 60px;
    max-height: 60px;
  }

  .dock.expanded-settings {
    --dock-height-transition-duration: 0.5s;
    height: 600.67px;
    max-height: 600.67px;
    gap: 0rem;
  }

  .dock.expanded-close {
    height: 162px;
    max-height: 162px;
    gap: 0rem;
  }

  /* Menu button wrappers */
  .settings-wrapper,
  .close-wrapper {
    width: 46px;
    height: 46px;
    transform: scale(0);
    transition:
      transform 0.3s ease,
      opacity 0.3s ease,
      width 0.3s ease,
      flex 0.3s ease,
      height var(--dock-height-transition-duration) var(--dock-height-transition-easing);
  }

  .settings-wrapper.expanded,
  .close-wrapper.expanded {
    display: flex;
    flex-direction: row;
    justify-content: center;;
    height: 100%;
    width: 100%;
    flex: 1 1 1;
    transform: scale(1);
  }

  .settings-wrapper.hidden,
  .close-wrapper.hidden {
    width: 0 !important;
    flex: 0 0 0 !important;
    transform: scale(0) !important;
    pointer-events: none;
  }

  .dock.open .settings-wrapper,
  .dock.open .close-wrapper {
    transform: scale(1);
  }

  /* Edit button wrapper */
  .edit-button-wrapper {
    height: 46px;
    width: 46px;
    transition: transform 0.3s ease, opacity 0.3s ease, width 0.3s ease, flex 0.3s ease;
    transform: scale(0);
    opacity: 0;
    align-self: flex-end;
    flex: 0 0 auto;
  }

  .edit-button-wrapper.visible {
    transform: scale(1);
    opacity: 1;
  }

  .edit-button-wrapper.hidden {
    width: 0 !important;
    flex: 0 0 0 !important;
    transform: scale(0) !important;
    pointer-events: none;
  }

  .dock.open .edit-button-wrapper {
    transform: scale(1);
    opacity: 1;
  }
</style>
