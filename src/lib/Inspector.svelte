<script>
  import { getSelection, updateKeyFields, testKey, resolveScope } from "./store.svelte.js";
  import { iconGlyph } from "./icons.js";
  import IconPicker from "./IconPicker.svelte";

  let { deckId } = $props();

  let selection = $derived(getSelection());
  let scope = $derived(
    selection.pos != null ? resolveScope(deckId, selection.folderId ? null : selection.pageIndex, selection.folderId) : null
  );
  let key = $derived(scope ? scope.keys[selection.pos] : null);
  let showIconPicker = $state(false);

  function set(field, value) {
    updateKeyFields(deckId, selection.folderId ? null : selection.pageIndex, selection.folderId, selection.pos, {
      [field]: value,
    });
  }
</script>

<div class="inspector">
  {#if key && key.kind !== "empty"}
    <div class="preview-key" style:background-color={key.color ?? null}>
      {#if key.image}
        <img src={key.image} alt="" />
      {:else}
        <span class="glyph">{iconGlyph(key.icon)}</span>
      {/if}
    </div>

    <label>
      Label
      <input value={key.label ?? ""} oninput={(e) => set("label", e.currentTarget.value)} />
    </label>

    <label>
      Icon
      <button class="icon-btn" onclick={() => (showIconPicker = true)}>
        {iconGlyph(key.icon)} change…
      </button>
    </label>

    <label>
      Colour
      <input type="color" value={key.color ?? "#2C2C2E"} oninput={(e) => set("color", e.currentTarget.value)} />
    </label>

    {#if key.kind === "action"}
      <label>
        Short press
        <input value={key.action ?? ""} oninput={(e) => set("action", e.currentTarget.value)} />
      </label>
      <label>
        Long
        <input value={key.hold ?? ""} oninput={(e) => set("hold", e.currentTarget.value)} />
      </label>
      <label>
        Double
        <input value={key.double ?? ""} oninput={(e) => set("double", e.currentTarget.value)} />
      </label>
      <label class="checkbox">
        <input type="checkbox" checked={!!key.danger} onchange={(e) => set("danger", e.currentTarget.checked)} />
        Ask to confirm
      </label>
      <button class="test" onclick={() => testKey(key)}>▶ Test</button>
    {:else if key.kind === "folder"}
      <p class="hint">Folder — double-click it on the device to open and configure its contents.</p>
    {/if}
  {:else}
    <p class="hint">Select a key to edit it.</p>
  {/if}
</div>

{#if showIconPicker}
  <IconPicker
    current={key?.icon}
    onpick={(icon) => {
      set("icon", icon);
      showIconPicker = false;
    }}
    onclose={() => (showIconPicker = false)}
  />
{/if}

<style>
  .inspector {
    width: 240px;
    flex-shrink: 0;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    color: var(--deck-color-text-primary);
    overflow-y: auto;
  }
  .preview-key {
    width: 64px;
    height: 64px;
    border-radius: 8px;
    background: var(--deck-color-key-default-background);
    display: flex;
    align-items: center;
    justify-content: center;
    align-self: center;
  }
  .preview-key img {
    width: 40px;
    height: 40px;
    object-fit: contain;
  }
  .glyph {
    font-size: 24px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--deck-color-text-secondary);
  }
  label.checkbox {
    flex-direction: row;
    align-items: center;
  }
  input,
  .icon-btn {
    background: #2c2c2e;
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 6px 8px;
    color: var(--deck-color-text-primary);
    font-size: 13px;
  }
  .icon-btn {
    cursor: pointer;
    text-align: left;
  }
  .test {
    margin-top: 8px;
    background: var(--deck-color-accent);
    color: white;
    border: none;
    border-radius: 6px;
    padding: 8px;
    cursor: pointer;
    font-weight: 600;
  }
  .hint {
    font-size: 12px;
    color: var(--deck-color-text-secondary);
  }
</style>
