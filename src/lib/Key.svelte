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
    interactive = true,
    onselect = () => {},
    ondblclick = () => {},
    onpointerdown = () => {},
  } = $props();

  function initials(label) {
    const words = (label ?? "").trim().split(/\s+/).filter(Boolean);
    if (!words.length) return "?";
    return (words.length === 1 ? words[0].slice(0, 2) : words[0][0] + words[1][0]).toUpperCase();
  }
</script>

<!-- role and tabindex are paired at runtime by the interactive prop. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="key"
  class:empty={keyData.kind === "empty"}
  class:folder={keyData.kind === "folder"}
  class:danger={keyData.danger}
  class:selected
  class:drop-target={isDropTarget}
  class:replace-blink={replaceBlink}
  class:preview={!interactive}
  style:background-color={keyData.color ?? null}
  data-drop-key
  data-page-id={pageId ?? ""}
  data-pos={pos}
  onpointerdown={(e) => interactive && onpointerdown(e)}
  onclick={() => interactive && onselect()}
  ondblclick={() => interactive && ondblclick()}
  onkeydown={(e) => {
    if (interactive && (e.key === "Enter" || e.key === " ")) {
      e.preventDefault();
      onselect();
    }
  }}
  role={interactive ? "button" : undefined}
  tabindex={interactive ? "0" : undefined}
>
  {#if keyData.kind !== "empty"}
    {#if keyData.state?.on}<span class="state-dot"></span>{/if}
    <span class="glyph" class:directional={isDirectionalIcon(keyData.icon)}>
      {#if keyData.image}
        <img src={keyData.image} alt="" draggable="false" />
      {:else if keyData.icon === "app"}
        <span class="app-monogram">{initials(keyData.label)}</span>
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
    background-color: var(--deck-color-key-default-background);
    background-image: linear-gradient(
      180deg,
      rgb(255 255 255 / 10%) 0,
      transparent 48%,
      rgb(0 0 0 / 9%) 100%
    );
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    cursor: grab;
    user-select: none;
    overflow: hidden;
    outline: none;
    box-shadow: 0 5px 10px -3px rgb(0 0 0 / 65%);
    border: 1px solid rgb(0 0 0 / 28%);
    transition:
      transform var(--deck-press-duration) ease,
      filter var(--deck-press-duration) ease,
      outline-color 0.12s ease;
  }
  .key.empty {
    background-color: var(--deck-color-key-empty-background);
    cursor: default;
  }
  .key.folder {
    background-color: var(--deck-color-key-default-background);
  }
  .key.danger {
    background-color: var(--deck-color-key-danger-background);
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
  .key.preview {
    cursor: default;
    pointer-events: none;
  }
  .glyph {
    font-size: calc(var(--deck-key-size) * 0.46);
    line-height: 1;
  }
  .app-monogram {
    width: 0.94em;
    height: 0.94em;
    display: grid;
    place-items: center;
    border-radius: 26%;
    background: linear-gradient(145deg, #55c8f4, #7659c8);
    color: #fff;
    font-size: 0.38em;
    font-weight: 700;
    letter-spacing: -0.04em;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 20%);
  }
  .glyph.directional {
    font-size: calc(var(--deck-key-size) * 0.68);
  }
  .glyph img {
    width: calc(var(--deck-key-size) * 0.5);
    height: calc(var(--deck-key-size) * 0.5);
    object-fit: contain;
    image-rendering: auto;
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
