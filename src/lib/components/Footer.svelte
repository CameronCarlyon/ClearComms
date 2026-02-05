<!--
  Footer Component
  Simple footer with attribution link
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  
  async function handleLinkClick(e: MouseEvent) {
    e.preventDefault();
    await invoke('open_url', { url: 'https://cameroncarlyon.com' });
  }
</script>

<footer>
  <p>
    Crafted by <a 
      href="https://cameroncarlyon.com" 
      onclick={handleLinkClick} 
      class="hyperlink" 
      aria-label="Visit Cameron Carlyon's website (opens in external browser)"
    >Cameron Carlyon</a>
  </p>
</footer>

<style>
  footer {
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    color: var(--text-muted);
    z-index: 2;
  }

  footer p {
    font-size: 0.8rem;
    color: var(--text-muted);
    text-align: center;
    margin: 0;
  }

  .hyperlink {
    color: var(--text-muted);
    text-decoration: none;
    cursor: pointer;
    transition: color 0.2s ease, filter 0.2s ease;
    display: inline-block;
  }

  .hyperlink:hover {
    color: var(--text-primary);
    filter: drop-shadow(0 0 30px rgba(255, 255, 255, 1)) drop-shadow(0 0 60px rgba(255, 255, 255, 0.8)) drop-shadow(0 0 100px rgba(255, 255, 255, 0.6)) drop-shadow(0 0 140px rgba(255, 255, 255, 0.4));
  }
</style>
