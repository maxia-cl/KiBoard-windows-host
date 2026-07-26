<script>
  import { getToast } from "./store.svelte.js";
  let toast = $derived(getToast());
</script>

{#if toast}
  <div class="toast">{toast}</div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    background: #1a1a1c;
    color: var(--deck-color-text-primary);
    padding: 8px 16px;
    border-radius: 8px;
    font-size: 13px;
    z-index: 1100;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
  }
</style>
