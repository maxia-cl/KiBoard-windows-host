<script>
  import { getCatalogue, dropCatalogueItem } from "./store.svelte.js";
  import IconGlyph from "./IconGlyph.svelte";
  import { t } from "./i18n.js";

  let { deckId, pageId, pos, onclose } = $props();

  let query = $state("");
  // Reactive, not a snapshot: the catalogue arrives from the host after mount.
  let catalogue = $derived(getCatalogue());
  let filteredGroups = $derived(
    catalogue.groups
      .map((g) => ({ ...g, items: g.items.filter((i) => i.label.toLowerCase().includes(query.toLowerCase())) }))
      .filter((g) => g.items.length)
  );

  function assign(item) {
    dropCatalogueItem(deckId, pageId, pos, item);
    onclose();
  }

  function focusOnMount(node) {
    node.focus();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onpointerdown={onclose} role="presentation">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="popover" onpointerdown={(e) => e.stopPropagation()} role="presentation">
    <input use:focusOnMount placeholder={t("search.plain")} bind:value={query} />
    <div class="list">
      {#each filteredGroups as group (group.id)}
        <div class="group-label">{group.label}</div>
        {#each group.items as item (item.id)}
          <button class="item" onclick={() => assign(item)}>
            <span class="glyph">
              {#if item.image}
                <img src={item.image} alt="" />
              {:else}
                <IconGlyph name={item.icon} />
              {/if}
            </span>
            {item.label}
          </button>
        {/each}
      {/each}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 10vh;
    z-index: 1200;
  }
  .popover {
    background: #1e1e20;
    border-radius: 10px;
    padding: 12px;
    width: 320px;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  input {
    background: #2c2c2e;
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 6px 10px;
    color: var(--deck-color-text-primary);
  }
  .list {
    overflow-y: auto;
  }
  .group-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--deck-color-text-secondary);
    margin: 8px 0 4px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    background: none;
    border: none;
    color: var(--deck-color-text-primary);
    padding: 6px 4px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
  }
  .item:hover {
    background: #2c2c2e;
  }
  .glyph img {
    width: 18px;
    height: 18px;
  }
</style>
