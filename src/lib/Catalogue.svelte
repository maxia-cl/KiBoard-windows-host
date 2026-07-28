<script>
  import { getCatalogue } from "./store.svelte.js";
  import { iconGlyph } from "./icons.js";
  import { startCatalogueDrag, onDragMove, endDrag } from "./dnd.svelte.js";

  let { deckId } = $props();

  let query = $state("");
  // Reactive, not a snapshot: the catalogue arrives from the host after mount.
  let catalogue = $derived(getCatalogue());
  let filteredGroups = $derived(
    catalogue.groups
      .map((g) => ({ ...g, items: g.items.filter((i) => i.label.toLowerCase().includes(query.toLowerCase())) }))
      .filter((g) => g.items.length)
  );

  function handlePointerDown(e, item) {
    e.preventDefault();
    startCatalogueDrag(item, e.clientX, e.clientY);
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
</script>

<div class="catalogue">
  <input placeholder="🔍 search…" bind:value={query} />
  {#each filteredGroups as group (group.id)}
    <details open>
      <summary>{group.label}</summary>
      {#each group.items as item (item.id)}
        <div
          class="item"
          role="button"
          tabindex="0"
          onpointerdown={(e) => handlePointerDown(e, item)}
          onkeydown={(e) => {
            if (e.key === "Enter" || e.key === " ") e.preventDefault();
          }}
        >
          <span class="glyph">
            {#if item.image}
              <img src={item.image} alt="" draggable="false" />
            {:else}
              {iconGlyph(item.icon)}
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
    width: 220px;
    flex-shrink: 0;
    padding: 12px;
    overflow-y: auto;
    color: var(--deck-color-text-primary);
  }
  input {
    width: 100%;
    box-sizing: border-box;
    background: #2c2c2e;
    border: 1px solid #3a3a3c;
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
    padding: 5px 4px;
    border-radius: 6px;
    cursor: grab;
  }
  .item:hover {
    background: #2c2c2e;
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
