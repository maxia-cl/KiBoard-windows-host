<script>
  import { GRID_PRESETS } from "./model.js";

  let { value, onchange } = $props();
  const presets = Object.entries(GRID_PRESETS).filter(([key]) => key !== "auto");
</script>

<select {value} onchange={(e) => onchange(e.currentTarget.value)}>
  {#each presets as [key, grid] (key)}
    <option value={key}>{key.toUpperCase()} {grid.rows}×{grid.cols}</option>
  {/each}
</select>

<style>
  select {
    background: #2c2c2e;
    color: var(--deck-color-text-primary);
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 13px;
  }
</style>
