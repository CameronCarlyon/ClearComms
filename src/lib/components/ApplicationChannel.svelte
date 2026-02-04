<!--
  ApplicationChannel Component
  A vertical application channel containing volume slider, mute button, and edit mode controls
-->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { AudioSession, AxisMapping, ButtonMapping } from '$lib/types';
  import VolumeSlider from './VolumeSlider.svelte';
  import ButtonRound from './ButtonRound.svelte';
  import { formatProcessName } from '$lib/stores/audioStore';
  
  interface Props {
    session: AudioSession;
    axisMapping: AxisMapping | undefined;
    buttonMapping: ButtonMapping | undefined;
    isEditMode: boolean;
    isBindingAxis: boolean;
    isBindingButton: boolean;
  }
  
  let { 
    session, 
    axisMapping, 
    buttonMapping, 
    isEditMode, 
    isBindingAxis, 
    isBindingButton 
  }: Props = $props();
  
  const dispatch = createEventDispatcher<{
    volumedragstart: { sessionId: string };
    volumedragmove: { sessionId: string; volume: number };
    volumedragend: { sessionId: string; volume: number };
    volumetrackclick: { sessionId: string; volume: number };
    volumewheel: { sessionId: string; volume: number };
    mutetoggle: { sessionId: string; muted: boolean };
    startaxisbinding: { session: AudioSession };
    startbuttonbinding: { session: AudioSession };
    cancelaxisbinding: void;
    cancelbuttonbinding: void;
    removeaxismapping: { processName: string };
    removebuttonmapping: { processName: string };
    toggleinversion: { processName: string };
    removeapplication: { processName: string };
  }>();
  
  const isInactive = $derived(session.session_id.startsWith('inactive_'));
  const displayName = $derived(formatProcessName(session.process_name));
</script>

<div 
  class="application-channel" 
  class:has-mapping={!!axisMapping || !!buttonMapping} 
  class:inactive={isInactive} 
  class:inactive-edit-mode={isInactive && isEditMode} 
  role="group" 
  aria-label="Audio controls for {session.display_name}"
>
  <!-- Volume Slider -->
  <VolumeSlider
    volume={session.volume}
    sessionId={session.session_id}
    displayName={session.display_name}
    disabled={isInactive}
    on:dragstart={(e) => dispatch('volumedragstart', e.detail)}
    on:dragmove={(e) => dispatch('volumedragmove', e.detail)}
    on:dragend={(e) => dispatch('volumedragend', e.detail)}
    on:trackclick={(e) => dispatch('volumetrackclick', e.detail)}
    on:wheel={(e) => dispatch('volumewheel', e.detail)}
  />

  <!-- Mute Button / Button Binding Control -->
  {#if isEditMode}
    {#if buttonMapping}
      <!-- Button mapping badge - shows bound button with remove on hover -->
      <ButtonRound
        variant="mapping"
        ariaLabel="Remove mute button binding for {session.display_name}: {buttonMapping.buttonName}"
        title="Mute Button: {buttonMapping.buttonName}"
        on:remove={() => dispatch('removebuttonmapping', { processName: session.process_name })}
      >
        {#snippet icon()}
          <!-- Mute button icon (X) -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
          </svg>
        {/snippet}
        {#snippet removeIcon()}
          <!-- Remove icon (X) -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {:else if isBindingButton}
      <!-- Cancel button binding -->
      <ButtonRound
        variant="bind"
        active={true}
        ariaLabel="Cancel mute button binding for {session.display_name}"
        title="Cancel Mute Binding"
        on:cancel={() => dispatch('cancelbuttonbinding')}
      >
        {#snippet icon()}
          <!-- Cancel icon (X) -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {:else}
      <!-- Start button binding -->
      <ButtonRound
        variant="bind"
        ariaLabel="Bind hardware button to mute {session.display_name}"
        title="Bind Mute Button"
        on:startbind={() => dispatch('startbuttonbinding', { session })}
      >
        {#snippet icon()}
          <!-- Muted speaker icon -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416zM399 239C389.6 248.4 389.6 263.6 399 272.9L446 319.9L399 366.9C389.6 376.3 389.6 391.5 399 400.8C408.4 410.1 423.6 410.2 432.9 400.8L479.9 353.8L526.9 400.8C536.3 410.2 551.5 410.2 560.8 400.8C570.1 391.4 570.2 376.2 560.8 366.9L513.8 319.9L560.8 272.9C570.2 263.5 570.2 248.3 560.8 239C551.4 229.7 536.2 229.6 526.9 239L479.9 286L432.9 239C423.5 229.6 408.3 229.6 399 239z"/>
          </svg>
        {/snippet}
        {#snippet hoverIcon()}
          <!-- Plus icon -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {/if}
  {:else}
    <!-- Mute Button -->
    <ButtonRound
      variant="toggle"
      active={!session.is_muted}
      disabled={isInactive}
      ariaLabel="{session.is_muted ? 'Unmute' : 'Mute'} {session.display_name}"
      title={session.is_muted ? 'Unmute' : 'Mute'}
      on:toggle={() => dispatch('mutetoggle', { sessionId: session.session_id, muted: !session.is_muted })}
    >
      {#snippet icon()}
        {#if session.is_muted || session.volume === 0}
          <!-- Muted or 0% volume -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
            <path d="M80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416zM399 239C389.6 248.4 389.6 263.6 399 272.9L446 319.9L399 366.9C389.6 376.3 389.6 391.5 399 400.8C408.4 410.1 423.6 410.2 432.9 400.8L479.9 353.8L526.9 400.8C536.3 410.2 551.5 410.2 560.8 400.8C570.1 391.4 570.2 376.2 560.8 366.9L513.8 319.9L560.8 272.9C570.2 263.5 570.2 248.3 560.8 239C551.4 229.7 536.2 229.6 526.9 239L479.9 286L432.9 239C423.5 229.6 408.3 229.6 399 239z"/>
          </svg>
        {:else if session.volume === 1}
          <!-- 100% volume -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
            <path d="M533.6 96.5C523.3 88.1 508.2 89.7 499.8 100C491.4 110.3 493 125.4 503.3 133.8C557.5 177.8 592 244.8 592 320C592 395.2 557.5 462.2 503.3 506.3C493 514.7 491.5 529.8 499.8 540.1C508.1 550.4 523.3 551.9 533.6 543.6C598.5 490.7 640 410.2 640 320C640 229.8 598.5 149.2 533.6 96.5zM473.1 171C462.8 162.6 447.7 164.2 439.3 174.5C430.9 184.8 432.5 199.9 442.8 208.3C475.3 234.7 496 274.9 496 320C496 365.1 475.3 405.3 442.8 431.8C432.5 440.2 431 455.3 439.3 465.6C447.6 475.9 462.8 477.4 473.1 469.1C516.3 433.9 544 380.2 544 320.1C544 260 516.3 206.3 473.1 171.1zM412.6 245.5C402.3 237.1 387.2 238.7 378.8 249C370.4 259.3 372 274.4 382.3 282.8C393.1 291.6 400 305 400 320C400 335 393.1 348.4 382.3 357.3C372 365.7 370.5 380.8 378.8 391.1C387.1 401.4 402.3 402.9 412.6 394.6C434.1 376.9 448 350.1 448 320C448 289.9 434.1 263.1 412.6 245.5zM80 416L128 416L262.1 535.2C268.5 540.9 276.7 544 285.2 544C304.4 544 320 528.4 320 509.2L320 130.8C320 111.6 304.4 96 285.2 96C276.7 96 268.5 99.1 262.1 104.8L128 224L80 224C53.5 224 32 245.5 32 272L32 368C32 394.5 53.5 416 80 416z"/>
          </svg>
        {:else}
          <!-- Between 1% and 99% volume -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor" aria-hidden="true">
            <path d="M144 416L192 416L326.1 535.2C332.5 540.9 340.7 544 349.2 544C368.4 544 384 528.4 384 509.2L384 130.8C384 111.6 368.4 96 349.2 96C340.7 96 332.5 99.1 326.1 104.8L192 224L144 224C117.5 224 96 245.5 96 272L96 368C96 394.5 117.5 416 144 416zM476.6 245.5C466.3 237.1 451.2 238.7 442.8 249C434.4 259.3 436 274.4 446.3 282.8C457.1 291.6 464 305 464 320C464 335 457.1 348.4 446.3 357.3C436 365.7 434.5 380.8 442.8 391.1C451.1 401.4 466.3 402.9 476.6 394.6C498.1 376.9 512 350.1 512 320C512 289.9 498.1 263.1 476.5 245.5z"/>
          </svg>
        {/if}
      {/snippet}
    </ButtonRound>
  {/if}

  <!-- Axis Binding Control (Edit Mode Only) -->
  {#if isEditMode}
    {#if axisMapping}
      <!-- Axis mapping badge - shows bound axis with remove on hover -->
      <ButtonRound
        variant="mapping"
        ariaLabel="Remove volume axis binding for {session.display_name}: {axisMapping.axisName}"
        title="Volume Axis: {axisMapping.axisName}"
        on:remove={() => dispatch('removeaxismapping', { processName: session.process_name })}
      >
        {#snippet icon()}
          <!-- Axis/gamepad icon -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M448 128C554 128 640 214 640 320C640 426 554 512 448 512L192 512C86 512 0 426 0 320C0 214 86 128 192 128L448 128zM192 240C178.7 240 168 250.7 168 264L168 296L136 296C122.7 296 112 306.7 112 320C112 333.3 122.7 344 136 344L168 344L168 376C168 389.3 178.7 400 192 400C205.3 400 216 389.3 216 376L216 344L248 344C261.3 344 272 333.3 272 320C272 306.7 261.3 296 248 296L216 296L216 264C216 250.7 205.3 240 192 240zM432 336C414.3 336 400 350.3 400 368C400 385.7 414.3 400 432 400C449.7 400 464 385.7 464 368C464 350.3 449.7 336 432 336zM496 240C478.3 240 464 254.3 464 272C464 289.7 478.3 304 496 304C513.7 304 528 289.7 528 272C528 254.3 513.7 240 496 240z"/>
          </svg>
        {/snippet}
        {#snippet removeIcon()}
          <!-- Remove icon (X) -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {:else if isBindingAxis}
      <!-- Cancel axis binding -->
      <ButtonRound
        variant="bind"
        active={true}
        ariaLabel="Cancel axis binding for {session.display_name}"
        title="Cancel Axis Binding"
        on:cancel={() => dispatch('cancelaxisbinding')}
      >
        {#snippet icon()}
          <!-- Cancel icon (X) -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M183.1 137.4C170.6 124.9 150.3 124.9 137.8 137.4C125.3 149.9 125.3 170.2 137.8 182.7L275.2 320L137.9 457.4C125.4 469.9 125.4 490.2 137.9 502.7C150.4 515.2 170.7 515.2 183.2 502.7L320.5 365.3L457.9 502.6C470.4 515.1 490.7 515.1 503.2 502.6C515.7 490.1 515.7 469.8 503.2 457.3L365.8 320L503.1 182.6C515.6 170.1 515.6 149.8 503.1 137.3C490.6 124.8 470.3 124.8 457.8 137.3L320.5 274.7L183.1 137.4z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {:else}
      <!-- Start axis binding -->
      <ButtonRound
        variant="bind"
        ariaLabel="Bind hardware axis to control volume for {session.display_name}"
        title="Bind Volume Axis"
        on:startbind={() => dispatch('startaxisbinding', { session })}
      >
        {#snippet icon()}
          <!-- Gamepad icon -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M448 128C554 128 640 214 640 320C640 426 554 512 448 512L192 512C86 512 0 426 0 320C0 214 86 128 192 128L448 128zM192 240C178.7 240 168 250.7 168 264L168 296L136 296C122.7 296 112 306.7 112 320C112 333.3 122.7 344 136 344L168 344L168 376C168 389.3 178.7 400 192 400C205.3 400 216 389.3 216 376L216 344L248 344C261.3 344 272 333.3 272 320C272 306.7 261.3 296 248 296L216 296L216 264C216 250.7 205.3 240 192 240zM432 336C414.3 336 400 350.3 400 368C400 385.7 414.3 400 432 400C449.7 400 464 385.7 464 368C464 350.3 449.7 336 432 336zM496 240C478.3 240 464 254.3 464 272C464 289.7 478.3 304 496 304C513.7 304 528 289.7 528 272C528 254.3 513.7 240 496 240z"/>
          </svg>
        {/snippet}
        {#snippet hoverIcon()}
          <!-- Plus icon -->
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
            <path d="M352 128C352 110.3 337.7 96 320 96C302.3 96 288 110.3 288 128L288 288L128 288C110.3 288 96 302.3 96 320C96 337.7 110.3 352 128 352L288 352L288 512C288 529.7 302.3 544 320 544C337.7 544 352 529.7 352 512L352 352L512 352C529.7 352 544 337.7 544 320C544 302.3 529.7 288 512 288L352 288L352 128z"/>
          </svg>
        {/snippet}
      </ButtonRound>
    {/if}
    
    <!-- Axis Inversion Toggle -->
    <ButtonRound
      variant="toggle"
      active={axisMapping?.inverted ?? false}
      disabled={!axisMapping}
      ariaLabel={axisMapping 
        ? `${axisMapping.inverted ? 'Disable' : 'Enable'} axis inversion for ${session.display_name}`
        : `No axis binding for ${session.display_name}`}
      title={axisMapping ? 'Reverse Axis Direction' : 'Bind an axis to enable inversion'}
      on:toggle={() => dispatch('toggleinversion', { processName: session.process_name })}
    >
      {#snippet icon()}
        <!-- Vertical arrows icon -->
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
          <path d="M342.6 41.4C330.1 28.9 309.8 28.9 297.3 41.4L201.3 137.4C188.8 149.9 188.8 170.2 201.3 182.7C213.8 195.2 234.1 195.2 246.6 182.7L288 141.3L288 498.7L246.6 457.4C234.1 444.9 213.8 444.9 201.3 457.4C188.8 469.9 188.8 490.2 201.3 502.7L297.3 598.7C303.3 604.7 311.4 608.1 319.9 608.1C328.4 608.1 336.5 604.7 342.5 598.7L438.5 502.7C451 490.2 451 469.9 438.5 457.4C426 444.9 405.7 444.9 393.2 457.4L351.8 498.8L351.8 141.3L393.2 182.7C405.7 195.2 426 195.2 438.5 182.7C451 170.2 451 149.9 438.5 137.4L342.5 41.4z"/>
        </svg>
      {/snippet}
    </ButtonRound>

    <!-- Remove Application Button -->
    <ButtonRound
      variant="action"
      danger={true}
      ariaLabel="Remove {session.display_name} from mixer"
      title="Remove Application"
      on:click={() => dispatch('removeapplication', { processName: session.process_name })}
    >
      {#snippet icon()}
        <!-- Trash icon -->
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="20" height="20" fill="currentColor">
          <path d="M232.7 69.9L224 96L128 96C110.3 96 96 110.3 96 128C96 145.7 110.3 160 128 160L512 160C529.7 160 544 145.7 544 128C544 110.3 529.7 96 512 96L416 96L407.3 69.9C402.9 56.8 390.7 48 376.9 48L263.1 48C249.3 48 237.1 56.8 232.7 69.9zM512 208L128 208L149.1 531.1C150.7 556.4 171.7 576 197 576L443 576C468.3 576 489.3 556.4 490.9 531.1L512 208z"/>
        </svg>
      {/snippet}
    </ButtonRound>
  {/if}

  <!-- Application Name -->
  <span class="app-name" title={session.display_name}>{displayName}</span>
</div>

<style>
  .application-channel {
    display: flex;
    height: 100%;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    transition: all 0.2s ease;
  }

  .application-channel.inactive {
    opacity: 0.5;
  }

  .application-channel.inactive-edit-mode {
    opacity: 1;
  }

  .app-name {
    text-align: center;
    font-size: 0.8rem;
    font-weight: 700;
    color: var(--text-primary);
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 3rem;
  }
</style>
