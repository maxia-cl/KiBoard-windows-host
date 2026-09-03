<script>
  import IconGlyph from "./IconGlyph.svelte";
  import { t } from "./i18n.js";
  import { trackInteraction } from "./telemetry.js";

  let { current = null, onpick, onclose } = $props();

  const ICONS = [
    "app", "brush", "crop", "undo", "redo", "save", "layers", "opacity",
    "zoom", "screenshot", "windows", "mic", "volume", "mode", "folder",
    "obs", "macro", "close", "back", "page",
  ];

  function dismiss() {
    trackInteraction("editor_icon_picker_dismissed");
    onclose();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay" onpointerdown={dismiss} role="presentation">
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="picker" onpointerdown={(e) => e.stopPropagation()} role="presentation">
    <div class="title">{t("icons.pick")}</div>
    <div class="grid">
      {#each ICONS as icon (icon)}
        <button data-telemetry="editor_key_icon_changed" class:selected={icon === current} onclick={() => onpick(icon)}>
          <IconGlyph name={icon} />
        </button>
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
    align-items: center;
    justify-content: center;
    z-index: 1200;
  }
  .picker {
    background: var(--deck-color-surface);
    border-radius: 10px;
    padding: 16px;
    width: 260px;
  }
  .title {
    color: var(--deck-color-text-primary);
    font-size: 13px;
    margin-bottom: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 6px;
  }
  .grid button {
    aspect-ratio: 1;
    border-radius: 6px;
    border: none;
    background: var(--deck-color-surface-raised);
    font-size: 18px;
    cursor: pointer;
  }
  .grid button.selected {
    outline: 2px solid var(--deck-color-accent);
  }
</style>
