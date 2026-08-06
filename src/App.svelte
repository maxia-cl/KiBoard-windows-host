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
  import { listDevices } from "./lib/pairing.js";
  import { invoke } from "./lib/bridge.js";
  import { t } from "./lib/i18n.js";
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
    duplicateDeck,
    renameDeck,
    importDeck,
    exportSelectedDeck,
    showToast,
  } from "./lib/store.svelte.js";

  let mode = $state("manual"); // "auto" | "manual" — docs/implementation-plan.md §3.2

  // Leaving manual mode or closing the window puts every phone back on the SAVED decks. Without
  // this, a phone keeps showing an unsaved preview of a deck that exists nowhere.
  function leaveManual() {
    mode = "auto";
    stopPreview();
  }
  let showPairing = $state(false);
  let firstRun = $state(false);
  let clients = $state(0);
  let loadError = $state(null);

  let decks = $derived(getDecks());
  let selection = $derived(getSelection());
  let ready = $derived(isLoaded());
  let deck = $derived(decks.find((d) => d.id === selection.deckId) ?? null);

  // Import is the browser's own file input: a deck is a JSON file, and reading one needs no
  // dialog plugin on the Rust side. Export does need the host — it writes where the user can
  // find it (see `export_deck`).
  let fileInput = $state();
  async function onFileChosen(e) {
    const file = e.currentTarget.files?.[0];
    e.currentTarget.value = ""; // so choosing the same file twice fires again
    if (!file) return;
    try {
      importDeck(JSON.parse(await file.text()));
    } catch {
      showToast(t("import.notjson"));
    }
  }

  onMount(() => {
    init().catch((e) => (loadError = String(e)));

    // B1. A PC with no phone paired to it does nothing at all, and the only way to pair is a ⚙ the
    // reader has no reason to press. So the panel opens itself — once, on a host that has never
    // paired anything. Revoking every device brings it back, which is right: that PC is useless
    // again until something pairs.
    listDevices()
      .then((d) => {
        if (d.length === 0) {
          firstRun = true;
          showPairing = true;
        }
      })
      .catch(() => {
        /* no backend (vite dev in a browser) — the panel says so itself */
      });

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
    <!--
      The same lockup the phone shows (its `Wordmark`): the brush mark tinted with the brand red,
      then the rest of the word in a light sans with wide tracking. The two halves of KiBoard are
      one product and have to be named the same way; this used to be "KiBoard" in bold system sans.
      The mark is a white PNG, so CSS masks it and paints the accent through — no second, recoloured
      copy of the file to keep in step.
    -->
    <span class="brand"><i class="mark" aria-hidden="true"></i><span class="word">board</span></span>
    <div class="tabs">
      <button class:active={mode === "auto"} onclick={leaveManual}>{t("tab.auto")}</button>
      <button class:active={mode === "manual"} onclick={() => (mode = "manual")}>{t("tab.manual")}</button>
    </div>
    {#if mode === "manual" && ready && deck}
      {#if decks.length > 1}
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
      <!-- onchange, not oninput: a rename is one undo step, not one per letter. -->
      <input
        class="deck-name"
        value={deck.name}
        aria-label={t("deck.name")}
        onchange={(e) => renameDeck(deck.id, e.currentTarget.value)}
      />
      <button class="tool" onclick={() => duplicateDeck(deck.id)} title={t("deck.duplicate")}>⧉</button>
      <button class="tool" onclick={() => exportSelectedDeck(deck.id)} title={t("deck.export")}>↑</button>
      <button class="tool" onclick={() => fileInput.click()} title={t("deck.import")}>↓</button>
      <input
        class="hidden-file"
        type="file"
        accept=".json,application/json"
        bind:this={fileInput}
        onchange={onFileChosen}
      />
    {/if}
    <span class="spacer"></span>
    {#if mode === "manual"}
      <button class="save" class:dirty={isDirty()} disabled={!isDirty()} onclick={() => save()}>
        {isDirty() ? t("save.changes") : t("save.done")}
      </button>
    {/if}
    <span class="connected">📱 {t("connected", clients)}</span>
    <button class="settings" onclick={() => (showPairing = true)} aria-label={t("pairing.title")}>⚙</button>
  </header>

  {#if mode === "manual"}
    {#if loadError}
      <p class="load-state">{t("load.error", loadError)}</p>
    {:else if !ready}
      <p class="load-state">{t("load.reading")}</p>
    {:else if !selection.deckId}
      <p class="load-state">{t("load.empty")}</p>
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
    <PairingPanel {firstRun} onclose={() => ((showPairing = false), (firstRun = false))} />
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
  .deck-name,
  .tool,
  .save {
    background: #232326;
    border: 1px solid #34343a;
    color: var(--deck-color-text-primary);
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
  }
  .deck-name {
    width: 140px;
    cursor: text;
  }
  .tool {
    padding: 4px 8px;
    font-size: 13px;
  }
  .hidden-file {
    display: none;
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
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .mark {
    width: 26px;
    height: 28px;
    background: var(--deck-color-accent, #b22420);
    mask: url(/mark.png) center / contain no-repeat;
    -webkit-mask: url(/mark.png) center / contain no-repeat;
  }
  .word {
    /* 22sp over a 34dp mark on the phone; 0.28 em is KiMouse's tracking. */
    font-size: 17px;
    font-weight: 300;
    letter-spacing: 0.28em;
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
