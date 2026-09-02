<script>
  import { iconAsset, iconGlyph } from "./icons.js";

  let { name = null, color = null } = $props();
  let asset = $derived(iconAsset(name));
  let filledCircle = $derived(!asset && (name === "accept" || name === "close" || name === "new"));
</script>

<span
  class="icon-glyph"
  class:filled-circle={filledCircle}
  class:record={name === "record"}
  style:color={color ?? null}
  aria-hidden="true"
>
  {#if asset?.monochrome}
    <span class="asset-mask" style={`--asset: url("${asset.url}")`}></span>
  {:else if asset}
    <img class="asset-image" src={asset.url} alt="" />
  {:else}
    <span class="mark">{iconGlyph(name)}</span>
  {/if}
</span>

<style>
  .icon-glyph {
    display: inline-grid;
    place-items: center;
    line-height: 1;
    vertical-align: middle;
  }
  .asset-image,
  .asset-mask {
    width: 0.95em;
    height: 0.95em;
  }
  .asset-image {
    display: block;
    object-fit: contain;
  }
  .asset-mask {
    display: block;
    background: currentColor;
    -webkit-mask: var(--asset) center / contain no-repeat;
    mask: var(--asset) center / contain no-repeat;
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
