<script>
  import { onMount } from "svelte";
  import "./tokens.g.css";
  import Catalogue from "./lib/Catalogue.svelte";
  import Device from "./lib/Device.svelte";
  import Inspector from "./lib/Inspector.svelte";
  import AutoTab from "./lib/AutoTab.svelte";
  import DragGhost from "./lib/DragGhost.svelte";
  import Toast from "./lib/Toast.svelte";
  import { undo, redo } from "./lib/store.svelte.js";

  let mode = $state("manual"); // "auto" | "manual" — docs/implementation-plan.md §3.2

  onMount(() => {
    function onKeydown(e) {
      if (!(e.ctrlKey || e.metaKey)) return;
      const key = e.key.toLowerCase();
      if (key === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
      } else if (key === "y") {
        e.preventDefault();
        redo();
      }
    }
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<div class="app">
  <div class="watermark">MOCK-UP · phase FP · no real host, nothing executes</div>

  <header>
    <span class="brand">KiBoard</span>
    <div class="tabs">
      <button class:active={mode === "auto"} onclick={() => (mode = "auto")}>Auto</button>
      <button class:active={mode === "manual"} onclick={() => (mode = "manual")}>Manual</button>
    </div>
    <span class="connected">📱 0 connected (mock)</span>
    <span class="settings" aria-hidden="true">⚙</span>
  </header>

  {#if mode === "manual"}
    <div class="columns">
      <Catalogue deckId="launcher" />
      <Device deckId="launcher" />
      <Inspector deckId="launcher" />
    </div>
  {:else}
    <AutoTab />
  {/if}

  <DragGhost />
  <Toast />
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .watermark {
    background: var(--deck-color-accent, #b22420);
    color: white;
    text-align: center;
    font-size: 11px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 4px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 16px;
    border-bottom: 1px solid #2a2a2c;
    color: var(--deck-color-text-primary);
  }
  .brand {
    font-weight: 700;
  }
  .tabs {
    display: flex;
    gap: 4px;
    background: #232326;
    border-radius: 8px;
    padding: 2px;
  }
  .tabs button {
    background: none;
    border: none;
    color: var(--deck-color-text-secondary);
    padding: 4px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .tabs button.active {
    background: var(--deck-color-accent);
    color: white;
  }
  .connected {
    margin-left: auto;
    font-size: 13px;
    color: var(--deck-color-text-secondary);
  }
  .settings {
    color: var(--deck-color-text-secondary);
  }
  .columns {
    display: flex;
    flex: 1;
    min-height: 0;
  }
</style>
