<!--
  Dock Component
  Bottom hover dock with settings, edit, and close controls
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Window } from '@tauri-apps/api/window';
  import { createEventDispatcher } from 'svelte';
  import { ButtonExpandable, ButtonRound, ListOption, SettingsOption } from '$lib/components';
  
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
  
  async function handleOpenGuide(e: MouseEvent) {
    e.preventDefault();
    await invoke('open_url', { url: 'https://github.com/CameronCarlyon/ClearComms/blob/main/GUIDE.md' });
  }
  
  async function handleOpenGithub(e: MouseEvent) {
    e.preventDefault();
    await invoke('open_url', { url: 'https://github.com/CameronCarlyon/ClearComms' });
  }
  
  function handleTogglePin(e: MouseEvent) {
    e.preventDefault();
    dispatch('togglewindowpinned');
  }
  
  async function handleQuit() {
    await invoke('quit_application');
  }
  
  async function handleMinimise() {
    // Reset all menu and dock states before hiding
    closeMenuExpanded = false;
    settingsMenuExpanded = false;
    dockOpen = false;
    
    const window = Window.getCurrent();
    await window.hide();
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
        <div class="settings-container">
          <SettingsOption
            ariaLabel="View ClearComms guide (opens in external browser)"
            title="User Guide"
            animationIndex={3}
            onclick={handleOpenGuide}
          >
            {#snippet icon()}
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="25" height="25" fill="currentColor" aria-hidden="true">
                <path d="M224 224C224 171 267 128 320 128C373 128 416 171 416 224C416 266.7 388.1 302.9 349.5 315.4C321.1 324.6 288 350.7 288 392L288 416C288 433.7 302.3 448 320 448C337.7 448 352 433.7 352 416L352 392C352 390.3 352.6 387.9 355.5 384.7C358.5 381.4 363.4 378.2 369.2 376.3C433.5 355.6 480 295.3 480 224C480 135.6 408.4 64 320 64C231.6 64 160 135.6 160 224C160 241.7 174.3 256 192 256C209.7 256 224 241.7 224 224zM320 576C342.1 576 360 558.1 360 536C360 513.9 342.1 496 320 496C297.9 496 280 513.9 280 536C280 558.1 297.9 576 320 576z"/>
              </svg>
            {/snippet}
          </SettingsOption>
          <SettingsOption
            ariaLabel="Visit ClearComms repository on GitHub (opens in external browser)"
            title="GitHub Repository"
            animationIndex={4}
            onclick={handleOpenGithub}
          >
            {#snippet icon()}
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="35" height="35" fill="currentColor" aria-hidden="true">
                <path d="M237.9 461.4C237.9 463.4 235.6 465 232.7 465C229.4 465.3 227.1 463.7 227.1 461.4C227.1 459.4 229.4 457.8 232.3 457.8C235.3 457.5 237.9 459.1 237.9 461.4zM206.8 456.9C206.1 458.9 208.1 461.2 211.1 461.8C213.7 462.8 216.7 461.8 217.3 459.8C217.9 457.8 216 455.5 213 454.6C210.4 453.9 207.5 454.9 206.8 456.9zM251 455.2C248.1 455.9 246.1 457.8 246.4 460.1C246.7 462.1 249.3 463.4 252.3 462.7C255.2 462 257.2 460.1 256.9 458.1C256.6 456.2 253.9 454.9 251 455.2zM316.8 72C178.1 72 72 177.3 72 316C72 426.9 141.8 521.8 241.5 555.2C254.3 557.5 258.8 549.6 258.8 543.1C258.8 536.9 258.5 502.7 258.5 481.7C258.5 481.7 188.5 496.7 173.8 451.9C173.8 451.9 162.4 422.8 146 415.3C146 415.3 123.1 399.6 147.6 399.9C147.6 399.9 172.5 401.9 186.2 425.7C208.1 464.3 244.8 453.2 259.1 446.6C261.4 430.6 267.9 419.5 275.1 412.9C219.2 406.7 162.8 398.6 162.8 302.4C162.8 274.9 170.4 261.1 186.4 243.5C183.8 237 175.3 210.2 189 175.6C209.9 169.1 258 202.6 258 202.6C278 197 299.5 194.1 320.8 194.1C342.1 194.1 363.6 197 383.6 202.6C383.6 202.6 431.7 169 452.6 175.6C466.3 210.3 457.8 237 455.2 243.5C471.2 261.2 481 275 481 302.4C481 398.9 422.1 406.6 366.2 412.9C375.4 420.8 383.2 435.8 383.2 459.3C383.2 493 382.9 534.7 382.9 542.9C382.9 549.4 387.5 557.3 400.2 555C500.2 521.8 568 426.9 568 316C568 177.3 455.5 72 316.8 72zM169.2 416.9C167.9 417.9 168.2 420.2 169.9 422.1C171.5 423.7 173.8 424.4 175.1 423.1C176.4 422.1 176.1 419.8 174.4 417.9C172.8 416.3 170.5 415.6 169.2 416.9zM158.4 408.8C157.7 410.1 158.7 411.7 160.7 412.7C162.3 413.7 164.3 413.4 165 412C165.7 410.7 164.7 409.1 162.7 408.1C160.7 407.5 159.1 407.8 158.4 408.8zM190.8 444.4C189.2 445.7 189.8 448.7 192.1 450.6C194.4 452.9 197.3 453.2 198.6 451.6C199.9 450.3 199.3 447.3 197.3 445.4C195.1 443.1 192.1 442.8 190.8 444.4zM179.4 429.7C177.8 430.7 177.8 433.3 179.4 435.6C181 437.9 183.7 438.9 185 437.9C186.6 436.6 186.6 434 185 431.7C183.6 429.4 181 428.4 179.4 429.7z"/>
              </svg>
            {/snippet}
          </SettingsOption>
          <SettingsOption
            ariaLabel={windowPinned ? "Disable pin on top" : "Enable pin on top"}
            ariaPressed={windowPinned}
            title={windowPinned ? "Pinned on Top" : "Pin on Top"}
            animationIndex={5}
            onclick={handleTogglePin}
          >
            {#snippet icon()}
              <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="25" height="25" fill={windowPinned ? "currentColor" : "none"} stroke={windowPinned ? "none" : "currentColor"} stroke-width="65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M160 96C160 78.3 174.3 64 192 64L448 64C465.7 64 480 78.3 480 96C480 113.7 465.7 128 448 128L418.5 128L428.8 262.1C465.9 283.3 494.6 318.5 507 361.8L510.8 375.2C513.6 384.9 511.6 395.2 505.6 403.3C499.6 411.4 490 416 480 416L160 416C150 416 140.5 411.3 134.5 403.3C128.5 395.3 126.5 384.9 129.3 375.2L133 361.8C145.4 318.5 174 283.3 211.2 262.1L221.5 128L192 128C174.3 128 160 113.7 160 96zM288 464L352 464L352 576C352 593.7 337.7 608 320 608C302.3 608 288 593.7 288 576L288 464z"/>
              </svg>
            {/snippet}
          </SettingsOption>
        </div>
      {/snippet}
    </ButtonExpandable>
    </div>
    
    <!-- Edit Button -->
    <div class="edit-button-wrapper" class:hidden={settingsMenuExpanded || closeMenuExpanded} class:visible={dockOpen && !settingsMenuExpanded && !closeMenuExpanded}>
      <ButtonRound
        variant="toggle"
        active={isEditMode}
        disabled={!audioInitialised}
        ariaLabel={isEditMode ? 'Exit edit mode' : 'Enter edit mode to configure bindings'}
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
        anchor="right"
        ariaLabel={closeMenuExpanded ? "Cancel" : "Close application"}
        title={closeMenuExpanded ? "Cancel" : "Quit"}
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
        <ListOption
          displayName="Quit"
          danger={true}
          fullWidth={true}
          ariaLabel="Quit the application"
          class="close-option"
          animationType="fadeIn"
          animationIndex={6}
          on:select={handleQuit}
        />
        <ListOption
          displayName="Minimise"
          fullWidth={true}
          ariaLabel="Minimise the application"
          animationType="fadeIn"
          class="close-option"
          animationIndex={5}
          on:select={handleMinimise}
        />
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
    transition: opacity 0.3s ease;
    margin: 4px 0;
  }

  .dock-hover-zone.expanded::before {
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
    display: flex;
    flex-direction: row;
    justify-content: space-between;
    align-items: flex-end;
    height: 0;
    max-height: 0;
    transition: height 0.3s ease, max-height 0.3s ease, padding 0.3s ease;
    position: relative;
    width: 180px;
  }

  .dock.open {
    height: 60px;
    max-height: 60px;
  }

  .dock.expanded-settings {
    height: 110px;
    max-height: 110px;
  }

  .dock.expanded-close {
    height: 162px;
    max-height: 162px;
  }

  /* Menu button wrappers */
  .settings-wrapper,
  .close-wrapper {
    width: 46px;
    height: 46px;
    transform: scale(0);
    transition: transform 0.3s ease, opacity 0.3s ease, width 0.3s ease, flex 0.3s ease, height 0.3s ease;
  }

  .settings-wrapper.expanded,
  .close-wrapper.expanded {
    height: 100%;
    flex: 1 1 1;
    width: 100%;
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

  /* Settings menu content */
  .settings-container {
    display: flex;
    flex-direction: row;
    justify-content: space-between;
  }
</style>
