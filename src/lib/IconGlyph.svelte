<script>
  import { iconGlyph } from "./icons.js";

  let { name = null, color = null } = $props();
  let filledCircle = $derived(name === "accept" || name === "close" || name === "new");
</script>

<span
  class="icon-glyph"
  class:filled-circle={filledCircle}
  class:record={name === "record"}
  style:color={color ?? null}
  aria-hidden="true"
>
  <span class="mark">{iconGlyph(name)}</span>
</span>

<style>
  .icon-glyph {
    display: inline-grid;
    place-items: center;
    line-height: 1;
    vertical-align: middle;
  }
  .filled-circle {
    width: 0.95em;
    height: 0.95em;
    border-radius: 50%;
    background: currentColor;
  }
  .filled-circle .mark {
    color: var(--deck-color-key-default-background);
    font-size: 0.68em;
    font-weight: 800;
  }
  .record {
    color: var(--deck-color-key-danger-background);
  }
</style>
