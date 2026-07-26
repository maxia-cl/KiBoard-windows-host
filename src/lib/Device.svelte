<script>
  import Key from "./Key.svelte";
  import ModelSelector from "./ModelSelector.svelte";
  import AssignPopover from "./AssignPopover.svelte";
  import {
    getDecks,
    getSelection,
    selectPage,
    enterFolder,
    exitFolder,
    select,
    addPage,
    duplicatePage,
    setModel,
    testKey,
    canUndo,
    canRedo,
    undo,
    redo,
    resolveScope,
    moveKey,
    swapKeys,
  } from "./store.svelte.js";
  import { getDrag, startKeyDrag, onDragMove, endDrag } from "./dnd.svelte.js";
  import { gridFor } from "./model.js";

  let { deckId } = $props();

  let decks = $derived(getDecks());
  let deck = $derived(decks.find((d) => d.id === deckId));
  let selection = $derived(getSelection());
  let grid = $derived(gridFor(deck?.model));
  let scope = $derived(deck ? resolveScope(deckId, selection.pageIndex, selection.folderId) : null);
  let inFolder = $derived(!!selection.folderId);
  let drag = $derived(getDrag());
  let assignTarget = $state(null); // pos awaiting catalogue assignment via double-click

  function scopeFolderId() {
    return inFolder ? selection.folderId : null;
  }
  function scopePageIndex() {
    return inFolder ? null : selection.pageIndex;
  }

  function dropTargetFor(pos) {
    if (!drag?.target || drag.target.type !== "key") return false;
    return (
      drag.target.pos === pos &&
      drag.target.pageIndex === scopePageIndex() &&
      drag.target.folderId === scopeFolderId()
    );
  }
  function replaceBlinkFor(pos) {
    if (!dropTargetFor(pos)) return false;
    const key = scope.keys[pos];
    return key.kind !== "empty" && key.kind !== "folder";
  }

  function handlePointerDown(e, pos) {
    const key = scope.keys[pos];
    if (key.kind === "empty") {
      select(pos);
      return;
    }
    e.preventDefault();
    startKeyDrag(deckId, scopePageIndex(), scopeFolderId(), pos, key, e.clientX, e.clientY);
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

  function handleDblClick(pos) {
    const key = scope.keys[pos];
    if (key.kind === "folder") {
      enterFolder(key.folderId);
    } else {
      assignTarget = pos;
    }
  }

  function handleKeydown(e) {
    if (selection.pos == null || !scope) return;
    const { rows, cols } = grid;
    let delta = null;
    if (e.key === "ArrowLeft") delta = -1;
    else if (e.key === "ArrowRight") delta = 1;
    else if (e.key === "ArrowUp") delta = -cols;
    else if (e.key === "ArrowDown") delta = cols;
    else return;

    const pos = selection.pos;
    const target = pos + delta;
    if (target < 0 || target >= rows * cols) return;
    if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && Math.floor(target / cols) !== Math.floor(pos / cols)) return;
    e.preventDefault();

    const loc = { pageIndex: scopePageIndex(), folderId: scopeFolderId(), pos };
    const to = { ...loc, pos: target };
    const targetKey = scope.keys[target];
    if (targetKey.kind === "empty") moveKey(deckId, loc, to);
    else swapKeys(deckId, loc, to);
    select(target);
  }
</script>

{#if deck}
  <div class="stage">
    <div class="toolbar">
      <span class="deck-name">Deck: {deck.name}</span>
      <ModelSelector value={deck.model} onchange={(preset) => setModel(deckId, preset)} />
      {#if !inFolder}
        <span class="page-count">{selection.pageIndex + 1}/{deck.pages.length}</span>
      {/if}
    </div>

    {#if inFolder}
      <div class="folder-header">
        <button onclick={() => exitFolder()}>← Back</button>
        <span>{deck.folders[selection.folderId]?.name}</span>
      </div>
    {/if}

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="bezel" data-drop-bezel onkeydown={handleKeydown} role="group" tabindex="-1">
      <div class="grid" style={`grid-template-columns: repeat(${grid.cols}, var(--deck-key-size))`}>
        {#each scope.keys as key, pos (pos)}
          <Key
            keyData={key}
            {pos}
            pageIndex={scopePageIndex()}
            folderId={scopeFolderId()}
            selected={selection.pos === pos}
            isDropTarget={dropTargetFor(pos)}
            replaceBlink={replaceBlinkFor(pos)}
            onpointerdown={(e) => handlePointerDown(e, pos)}
            onselect={() => select(pos)}
            ondblclick={() => handleDblClick(pos)}
          />
        {/each}
      </div>

      {#if !inFolder && deck.pages.length > 1}
        <div class="dots">
          {#each deck.pages as _, i}
            <button
              class="dot"
              class:active={i === selection.pageIndex}
              data-drop-page-dot
              data-page-index={i}
              onclick={() => selectPage(i)}
              aria-label={`Page ${i + 1}`}
            ></button>
          {/each}
        </div>
      {/if}

      <div class="logo">KiBoard</div>
    </div>

    {#if !inFolder}
      <div class="page-controls">
        <button onclick={() => addPage(deckId)}>+ page</button>
        <button onclick={() => duplicatePage(deckId, selection.pageIndex)}>duplicate</button>
        <button disabled={!canUndo()} onclick={() => undo()} title="Ctrl+Z">↺</button>
        <button disabled={!canRedo()} onclick={() => redo()} title="Ctrl+Y">↻</button>
      </div>
    {/if}
  </div>

  {#if assignTarget !== null}
    <AssignPopover
      {deckId}
      pageIndex={scopePageIndex()}
      folderId={scopeFolderId()}
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
