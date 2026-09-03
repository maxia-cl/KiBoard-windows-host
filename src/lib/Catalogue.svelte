<script>
  import { getCatalogue, assignToSelection } from "./store.svelte.js";
  import IconGlyph from "./IconGlyph.svelte";
  import { startCatalogueDrag, onDragMove, endDrag, cancelDrag } from "./dnd.svelte.js";
  import { t } from "./i18n.js";

  let { deckId } = $props();

  let query = $state("");
  let gesture = null;
  // Reactive, not a snapshot: the catalogue arrives from the host after mount.
  let catalogue = $derived(getCatalogue());
  let filteredGroups = $derived(
    catalogue.groups
      .map((g) => ({ ...g, items: g.items.filter((i) => i.label.toLowerCase().includes(query.toLowerCase())) }))
      .filter((g) => g.items.length)
  );

  function handlePointerDown(e, item) {
    e.preventDefault();
    gesture = { x: e.clientX, y: e.clientY, moved: false, item };
    startCatalogueDrag(item, e.clientX, e.clientY);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
    window.addEventListener("pointercancel", onCancel, { once: true });
  }
  function onMove(e) {
    if (gesture && Math.hypot(e.clientX - gesture.x, e.clientY - gesture.y) > 5) {
      gesture.moved = true;
    }
    if (!gesture?.moved) return;
    onDragMove(deckId, e.clientX, e.clientY);
  }
  function onUp(e) {
    if (gesture?.moved) endDrag(deckId, e.ctrlKey);
    else {
      cancelDrag();
      if (gesture?.item) assignToSelection(gesture.item);
    }
    gesture = null;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointercancel", onCancel);
  }
  function onCancel() {
    gesture = null;
    cancelDrag();
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  }
</script>

<div class="catalogue">
  <h2>{t("catalogue.add")}</h2>
  <p class="intro">{t("catalogue.hint")}</p>
  <input placeholder={t("search")} bind:value={query} />
  {#each filteredGroups as group (group.id)}
    <details open={group.id === "frequent" || query.trim() !== ""}>
      <summary>{group.label}</summary>
      {#each group.items as item (item.id)}
        <div
          class="item"
          role="button"
          tabindex="0"
          onpointerdown={(e) => handlePointerDown(e, item)}
          onkeydown={(e) => {
            // The keyboard path: Enter assigns to the selected key, no dragging involved.
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              assignToSelection(item);
            }
          }}
          title={t("catalogue.click")}
        >
          <span class="glyph">
            {#if item.image}
              <img src={item.image} alt="" draggable="false" />
            {:else}
              <IconGlyph name={item.icon} />
            {/if}
          </span>
          <span class="label">{item.label}</span>
        </div>
      {/each}
    </details>
  {/each}
</div>

<style>
  .catalogue {
    width: 232px;
    flex-shrink: 0;
    padding: 12px;
    overflow-y: auto;
    color: var(--deck-color-text-primary);
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .intro {
    margin: 4px 0 12px;
    color: var(--deck-color-text-secondary);
    font-size: 12px;
    line-height: 1.35;
  }
  input {
    width: 100%;
    box-sizing: border-box;
    background: var(--deck-color-surface-raised);
    border: 1px solid var(--deck-color-surface-border);
    border-radius: 6px;
    padding: 6px 10px;
    color: inherit;
    margin-bottom: 10px;
  }
  summary {
    cursor: pointer;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--deck-color-text-secondary);
    margin: 10px 0 4px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 6px;
    border-radius: 6px;
    cursor: grab;
  }
  .item:hover {
    background: var(--deck-color-surface-raised);
  }
  .item:focus-visible {
    outline: 2px solid var(--deck-color-accent);
    outline-offset: 1px;
  }
  .glyph {
    width: 20px;
    text-align: center;
  }
  .glyph img {
    width: 18px;
    height: 18px;
    object-fit: contain;
  }
  .label {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
