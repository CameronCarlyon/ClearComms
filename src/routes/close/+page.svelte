<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";

  async function confirmClose() {
    await invoke("quit_application");
  }

  async function minimiseToTray() {
    const window = (await import("@tauri-apps/api/window")).Window.getCurrent();
    await window.hide();
  }

  function goBack() {
    goto("/");
  }
</script>

<div class="close-container">
  <main class="close-content">
    <h1 class="close-title">Close ClearComms</h1>
    <p class="close-message">Are you sure you would like to close ClearComms?</p>
    
    <div class="button-group">
      <button class="btn btn-close-confirm" onclick={confirmClose}>
        Close
      </button>
      <button class="btn btn-minimise" onclick={minimiseToTray}>
        Minimise to System Tray
      </button>
      <button class="btn btn-cancel" onclick={goBack}>
        Nevermind
      </button>
    </div>
  </main>
</div>

<style>
  :root {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
    font-size: 14px;
    line-height: 1.4;
    
    /* Monochrome color palette */
    --bg-dark: #1a1a1a;
    --bg-medium: #2a2a2a;
    --bg-light: #3a3a3a;
    --text-primary: #ffffff;
    --text-secondary: #cccccc;
    --text-muted: #888888;
    --border-color: rgba(255, 255, 255, 0.1);
    --shadow-soft: rgba(0, 0, 0, 0.3);
  }

  * {
    box-sizing: border-box;
  }

  .close-container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    width: 100vw;
    background: var(--bg-dark);
    padding: 2rem;
  }

  .close-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2rem;
    max-width: 400px;
    width: 100%;
  }

  .close-title {
    font-size: 2rem;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
    text-align: center;
  }

  .close-message {
    font-size: 1.1rem;
    color: var(--text-secondary);
    margin: 0;
    text-align: center;
    line-height: 1.6;
  }

  .button-group {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    width: 100%;
  }

  .btn {
    padding: 16px 32px;
    font-size: 1.1rem;
    border-radius: 8px;
    font-weight: 500;
    transition: all 0.15s ease;
    cursor: pointer;
    border: 2px solid transparent;
    width: 100%;
    text-align: center;
  }

  .btn:active {
    transform: scale(0.98);
  }

  .btn-close-confirm {
    background: #ff4444;
    color: white;
    border-color: #ff4444;
  }

  .btn-close-confirm:hover {
    background: #cc0000;
    border-color: #cc0000;
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(255, 68, 68, 0.3);
  }

  .btn-minimise {
    background: var(--text-primary);
    color: var(--bg-dark);
    border-color: var(--text-primary);
  }

  .btn-minimise:hover {
    background: var(--text-secondary);
    border-color: var(--text-secondary);
    transform: translateY(-2px);
    box-shadow: 0 4px 12px rgba(255, 255, 255, 0.2);
  }

  .btn-cancel {
    background: transparent;
    color: var(--text-secondary);
    border-color: var(--border-color);
  }

  .btn-cancel:hover {
    background: var(--bg-light);
    border-color: var(--text-secondary);
    color: var(--text-primary);
  }
</style>
