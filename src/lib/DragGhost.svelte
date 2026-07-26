<script>
  import { getDrag } from "./dnd.svelte.js";
  import { iconGlyph } from "./icons.js";

  let drag = $derived(getDrag());
  let face = $derived(drag ? (drag.kind === "key" ? drag.key : drag.item) : null);
</script>

{#if drag && face}
  <div class="ghost" style:left={`${drag.x}px`} style:top={`${drag.y}px`}>
    {#if face.image}
      <img src={face.image} alt="" />
    {:else}
      <span class="glyph">{iconGlyph(face.icon)}</span>
    {/if}
  </div>
{/if}

<style>
  .ghost {
    position: fixed;
    width: var(--deck-key-size);
    height: var(--deck-key-size);
    margin-left: calc(var(--deck-key-size) * -0.5);
    margin-top: calc(var(--deck-key-size) * -0.5);
    border-radius: var(--deck-key-corner-radius);
    background: var(--deck-color-key-default-background);
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    opacity: 0.85;
    z-index: 1000;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.5);
  }
  .ghost img {
    width: 60%;
    height: 60%;
    object-fit: contain;
  }
  .glyph {
    font-size: calc(var(--deck-key-size) * 0.32);
  }
</style>
