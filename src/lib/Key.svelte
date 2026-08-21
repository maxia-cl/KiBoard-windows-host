<script>
  import IconGlyph from "./IconGlyph.svelte";
  import { isDirectionalIcon } from "./icons.js";

  let {
    keyData,
    pos,
    pageId = null,
    selected = false,
    isDropTarget = false,
    replaceBlink = false,
    onselect = () => {},
    ondblclick = () => {},
    onpointerdown = () => {},
  } = $props();
</script>

<div
  class="key"
  class:empty={keyData.kind === "empty"}
  class:folder={keyData.kind === "folder"}
  class:danger={keyData.danger}
  class:selected
  class:drop-target={isDropTarget}
  class:replace-blink={replaceBlink}
  style:background-color={keyData.color ?? null}
  data-drop-key
  data-page-id={pageId ?? ""}
  data-pos={pos}
  onpointerdown={(e) => onpointerdown(e)}
  onclick={() => onselect()}
  ondblclick={() => ondblclick()}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onselect();
    }
  }}
  role="button"
  tabindex="0"
>
  {#if keyData.kind !== "empty"}
    {#if keyData.state?.on}<span class="state-dot"></span>{/if}
    <span class="glyph" class:directional={isDirectionalIcon(keyData.icon)}>
      {#if keyData.image}
        <img src={keyData.image} alt="" draggable="false" />
      {:else}
        <IconGlyph name={keyData.icon} color={keyData.iconColor} />
      {/if}
    </span>
    <span class="label">{keyData.label}</span>
  {/if}
</div>

<style>
  .key {
    position: relative;
    width: var(--deck-key-size);
    height: var(--deck-key-size);
    border-radius: var(--deck-key-corner-radius);
    background: var(--deck-color-key-default-background);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    cursor: grab;
    user-select: none;
    overflow: hidden;
    outline: none;
    box-shadow: none;
    border: none;
    transition:
      transform var(--deck-press-duration) ease,
      filter var(--deck-press-duration) ease,
      outline-color 0.12s ease;
  }
  .key.empty {
    background: var(--deck-color-key-empty-background);
    cursor: default;
  }
  .key.folder {
    background: var(--deck-color-key-default-background);
  }
  .key.danger {
    background: var(--deck-color-key-danger-background);
  }
  .key.selected {
    outline: 2px solid var(--deck-color-accent);
    outline-offset: 2px;
  }
  .key.drop-target {
    outline: 2px solid var(--deck-color-accent);
    outline-offset: 2px;
    filter: brightness(1.15);
  }
  .key.replace-blink {
    animation: blink 0.5s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      filter: brightness(1.4);
    }
  }
  .key:active:not(.empty) {
    transform: scale(var(--deck-press-scale));
    filter: brightness(calc(1 - var(--deck-press-darken-percent) / 100));
  }
  .glyph {
    font-size: calc(var(--deck-key-size) * 0.46);
    line-height: 1;
  }
  .glyph.directional {
    font-size: calc(var(--deck-key-size) * 0.68);
  }
  .glyph img {
    width: calc(var(--deck-key-size) * 0.5);
    height: calc(var(--deck-key-size) * 0.5);
    object-fit: contain;
    image-rendering: pixelated;
  }
  .label {
    font-size: clamp(13px, calc(var(--deck-key-size) * 0.15), 18px);
    font-weight: 400;
    line-height: 1.05;
    color: var(--deck-color-text-primary);
    text-align: center;
    max-width: 96%;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .state-dot {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--deck-color-state-on);
  }
</style>
