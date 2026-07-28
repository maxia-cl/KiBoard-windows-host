<script>
  import Key from "./Key.svelte";
  import AssignPopover from "./AssignPopover.svelte";
  import {
    getDecks,
    getSelection,
    selectScreen,
    enterPage,
    exitPage,
    isEntryPage,
    select,
    testKey,
    canUndo,
    canRedo,
    undo,
    redo,
    currentScreen,
    screenCount,
    moveKey,
    swapKeys,
  } from "./store.svelte.js";
  import { getDrag, startKeyDrag, onDragMove, endDrag } from "./dnd.svelte.js";
  import { AUTHORING_GRID, SCREEN } from "./model.js";

  let { deckId } = $props();

  let decks = $derived(getDecks());
  let deck = $derived(decks.find((d) => d.id === deckId));
  let selection = $derived(getSelection());
  let grid = AUTHORING_GRID;
  // The dense screen being drawn. `pos` on these keys is ABSOLUTE within the page, which is what
  // every edit and every drop location uses — the phone addresses keys the same way.
  let keys = $derived(deck ? currentScreen(deckId, selection.pageId, selection.screen) : []);
  let screens = $derived(deck ? screenCount(deckId, selection.pageId) : 1);
  let page = $derived(deck?.pages.find((p) => p.id === selection.pageId));
  let drag = $derived(getDrag());
  let assignTarget = $state(null); // pos awaiting catalogue assignment via double-click

  function dropTargetFor(pos) {
    if (!drag?.target || drag.target.type !== "key") return false;
    return drag.target.pos === pos && drag.target.pageId === selection.pageId;
  }
  function replaceBlinkFor(pos, key) {
    return dropTargetFor(pos) && key.kind !== "empty" && key.kind !== "folder";
  }

  function handlePointerDown(e, key) {
    if (key.kind === "empty") {
      select(key.pos);
      return;
    }
    e.preventDefault();
    startKeyDrag(deckId, selection.pageId, key.pos, key, e.clientX, e.clientY);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
  }
  function onMove(e) {
    onDragMove(deckId, e.clientX, e.clientY);
  }
  function onUp(e) {
    endDrag(deckId, e.ctrlKey);
    window.removeEventListener("pointermove", onMove);
  }

  function handleDblClick(key) {
    if (key.kind === "folder" && key.target) enterPage(key.target);
    else assignTarget = key.pos;
  }

  function handleKeydown(e) {
    if (selection.pos == null || !deck) return;
    const { cols } = grid;
    let delta = null;
    if (e.key === "ArrowLeft") delta = -1;
    else if (e.key === "ArrowRight") delta = 1;
    else if (e.key === "ArrowUp") delta = -cols;
    else if (e.key === "ArrowDown") delta = cols;
    else return;

    const pos = selection.pos;
    const target = pos + delta;
    // Arrows move WITHIN the screen on show: crossing into the next one silently would look like
    // the key vanished.
    const base = selection.screen * SCREEN;
    if (target < base || target >= base + SCREEN) return;
    if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && Math.floor(target / cols) !== Math.floor(pos / cols)) return;
    e.preventDefault();

    const from = { pageId: selection.pageId, pos };
    const to = { pageId: selection.pageId, pos: target };
    const targetKey = keys[target - base];
    if (targetKey.kind === "empty") moveKey(deckId, from, to);
    else swapKeys(deckId, from, to);
    select(target);
  }
</script>

{#if deck}
  <div class="stage">
    <div class="toolbar">
      <span class="deck-name">Deck: {deck.name}</span>
      <span class="page-count">
        {#if screens > 1}screen {selection.screen + 1}/{screens}{/if}
      </span>
    </div>

    {#if !isEntryPage()}
      <div class="folder-header">
        <button onclick={() => exitPage()}>← Back</button>
        <span>{page?.name || "Page"}</span>
      </div>
    {/if}

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="bezel" data-drop-bezel onkeydown={handleKeydown} role="group" tabindex="-1">
      <div class="grid" style={`grid-template-columns: repeat(${grid.cols}, var(--deck-key-size))`}>
        {#each keys as key (key.pos)}
          <Key
            keyData={key}
            pos={key.pos}
            pageId={selection.pageId}
            selected={selection.pos === key.pos}
            isDropTarget={dropTargetFor(key.pos)}
            replaceBlink={replaceBlinkFor(key.pos, key)}
            onpointerdown={(e) => handlePointerDown(e, key)}
            onselect={() => select(key.pos)}
            ondblclick={() => handleDblClick(key)}
          />
        {/each}
      </div>

      <!-- The same dots the phone draws, meaning the same thing: this page cut into screens.
           A page grows a screen by having a key placed past the current last one. -->
      {#if screens > 1}
        <div class="dots">
          {#each Array(screens) as _, i}
            <button
              class="dot"
              class:active={i === selection.screen}
              data-drop-screen-dot
              data-screen={i}
              onclick={() => selectScreen(i)}
              aria-label={`Screen ${i + 1}`}
            ></button>
          {/each}
        </div>
      {/if}

      <div class="logo">KiBoard</div>
    </div>

    <div class="page-controls">
      <button disabled={!canUndo()} onclick={() => undo()} title="Ctrl+Z">↺</button>
      <button disabled={!canRedo()} onclick={() => redo()} title="Ctrl+Y">↻</button>
    </div>
  </div>

  {#if assignTarget !== null}
    <AssignPopover
      {deckId}
      pageId={selection.pageId}
      pos={assignTarget}
      onclose={() => (assignTarget = null)}
    />
  {/if}
{/if}

<style>
  .stage {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 16px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--deck-color-text-primary);
  }
  .deck-name {
    font-weight: 600;
  }
  .page-count {
    color: var(--deck-color-text-secondary);
    font-size: 13px;
  }
  .folder-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--deck-color-text-primary);
    align-self: flex-start;
  }
  .bezel {
    position: relative;
    background: var(--deck-bezel-gradient);
    border-radius: var(--deck-bezel-corner-radius);
    padding: var(--deck-bezel-padding-top) var(--deck-bezel-padding-side) var(--deck-bezel-padding-bottom);
    transform: perspective(900px) rotateX(var(--deck-stand-tilt));
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4);
  }
  .grid {
    display: grid;
    gap: calc(var(--deck-key-size) * var(--deck-key-gap-ratio));
  }
  .dots {
    display: flex;
    justify-content: center;
    gap: 6px;
    margin-top: 10px;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: none;
    background: var(--deck-color-page-dot-inactive);
    padding: 0;
    cursor: pointer;
  }
  .dot.active {
    background: var(--deck-color-page-dot-active);
  }
  .logo {
    text-align: center;
    margin-top: 8px;
    font-size: 11px;
    letter-spacing: 0.14em;
    color: var(--deck-color-text-secondary);
    text-transform: uppercase;
  }
  .page-controls {
    display: flex;
    gap: 8px;
  }
</style>
