<script>
  import { onMount } from "svelte";
  import "./tokens.g.css";
  import Catalogue from "./lib/Catalogue.svelte";
  import Device from "./lib/Device.svelte";
  import Inspector from "./lib/Inspector.svelte";
  import AutoTab from "./lib/AutoTab.svelte";
  import DragGhost from "./lib/DragGhost.svelte";
  import Toast from "./lib/Toast.svelte";
  import PairingPanel from "./lib/PairingPanel.svelte";
  import { invoke } from "./lib/bridge.js";
  import {
    undo,
    redo,
    init,
    isLoaded,
    isDirty,
    save,
    getDecks,
    getSelection,
    selectDeck,
    stopPreview,
  } from "./lib/store.svelte.js";

  let mode = $state("manual"); // "auto" | "manual" — docs/implementation-plan.md §3.2

  // Leaving manual mode or closing the window puts every phone back on the SAVED decks. Without
  // this, a phone keeps showing an unsaved preview of a deck that exists nowhere.
  function leaveManual() {
    mode = "auto";
    stopPreview();
  }
  let showPairing = $state(false);
  let clients = $state(0);
  let loadError = $state(null);

  let decks = $derived(getDecks());
  let selection = $derived(getSelection());
  let ready = $derived(isLoaded());

  onMount(() => {
    init().catch((e) => (loadError = String(e)));

    // The badge is the only honest way to know a save will be felt somewhere.
    const poll = setInterval(async () => {
      try {
        clients = (await invoke("host_status"))?.clients ?? 0;
      } catch {
        /* the host UI outliving the backend is not worth a message */
      }
    }, 2000);

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
      } else if (key === "s") {
        e.preventDefault();
        save();
      }
    }
    window.addEventListener("keydown", onKeydown);
    window.addEventListener("beforeunload", stopPreview);
    return () => {
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("beforeunload", stopPreview);
      clearInterval(poll);
      stopPreview();
    };
  });
</script>

<div class="app">
  <header>
    <span class="brand">KiBoard</span>
    <div class="tabs">
      <button class:active={mode === "auto"} onclick={leaveManual}>Auto</button>
      <button class:active={mode === "manual"} onclick={() => (mode = "manual")}>Manual</button>
    </div>
    {#if mode === "manual" && decks.length > 1}
      <select
        class="deck-picker"
        value={selection.deckId}
        onchange={(e) => selectDeck(e.currentTarget.value)}
      >
        {#each decks as d}
          <option value={d.id}>{d.name}</option>
        {/each}
      </select>
    {/if}
    <span class="spacer"></span>
    {#if mode === "manual"}
      <button class="save" class:dirty={isDirty()} disabled={!isDirty()} onclick={() => save()}>
        {isDirty() ? "Save changes" : "Saved"}
      </button>
    {/if}
    <span class="connected">📱 {clients} connected</span>
    <button class="settings" onclick={() => (showPairing = true)} aria-label="Pairing &amp; devices">⚙</button>
  </header>

  {#if mode === "manual"}
    {#if loadError}
      <p class="load-state">Could not read the decks: {loadError}</p>
    {:else if !ready}
      <p class="load-state">Reading your decks and apps…</p>
    {:else if !selection.deckId}
      <p class="load-state">No decks yet.</p>
    {:else}
      <div class="columns">
        <Catalogue deckId={selection.deckId} />
        <Device deckId={selection.deckId} />
        <Inspector deckId={selection.deckId} />
      </div>
    {/if}
  {:else}
    <AutoTab />
  {/if}

  <DragGhost />
  <Toast />
  {#if showPairing}
    <PairingPanel onclose={() => (showPairing = false)} />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .spacer {
    flex: 1;
  }
  .deck-picker,
  .save {
    background: #232326;
    border: 1px solid #34343a;
    color: var(--deck-color-text-primary);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .save.dirty {
    background: var(--deck-color-accent, #b22420);
    border-color: transparent;
    color: #fff;
  }
  .load-state {
    color: var(--deck-color-text-secondary);
    padding: 32px;
    text-align: center;
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
    background: none;
    border: none;
    color: var(--deck-color-text-secondary);
    font-size: 16px;
    cursor: pointer;
    padding: 2px 4px;
  }
  .columns {
    display: flex;
    flex: 1;
    min-height: 0;
  }
</style>
