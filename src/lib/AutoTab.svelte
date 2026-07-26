<script>
  // Simplified for FP: one real fixture (Photoshop) stands in for the ~100-profile catalogue.
  // Real per-app profiles are ported from v1 in F2 (docs/implementation-plan.md §3.2).
  import layoutPhotoshop from "../mock/fixtures/layout-auto-photoshop.json";
  import Key from "./Key.svelte";
  import { gridFor } from "./model.js";

  const PROFILES = [
    "Adobe Photoshop", "Adobe Illustrator", "Microsoft Excel", "Google Chrome",
    "Visual Studio Code", "Slack", "OBS Studio", "Figma", "Blender", "Discord",
  ].map((name, i) => ({ id: String(i), name }));

  let query = $state("");
  let selectedId = $state("0");
  let filtered = $derived(PROFILES.filter((p) => p.name.toLowerCase().includes(query.toLowerCase())));
  let grid = $derived(gridFor("mk2"));
</script>

<div class="auto-tab">
  <div class="profile-list">
    <input placeholder="🔍 search ~100 profiles…" bind:value={query} />
    {#each filtered as p (p.id)}
      <button class:active={p.id === selectedId} onclick={() => (selectedId = p.id)}>{p.name}</button>
    {/each}
  </div>
  <div class="stage">
    <div class="bezel">
      <div class="grid" style={`grid-template-columns: repeat(${grid.cols}, var(--deck-key-size))`}>
        {#each layoutPhotoshop.keys as key (key.pos)}
          <Key keyData={key} pos={key.pos} />
        {/each}
      </div>
      <div class="logo">KiBoard</div>
    </div>
    <p class="hint">
      Read-only preview. Auto-mode profiles switch by themselves when the foreground app changes —
      they aren't configured by dragging here, only browsed.
    </p>
  </div>
</div>

<style>
  .auto-tab {
    display: flex;
    flex: 1;
  }
  .profile-list {
    width: 240px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    color: var(--deck-color-text-primary);
  }
  .profile-list input {
    background: #2c2c2e;
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 6px 10px;
    color: inherit;
    margin-bottom: 8px;
  }
  .profile-list button {
    text-align: left;
    background: none;
    border: none;
    color: inherit;
    padding: 6px 8px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .profile-list button:hover {
    background: #2c2c2e;
  }
  .profile-list button.active {
    background: var(--deck-color-accent);
  }
  .stage {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 24px;
    gap: 16px;
  }
  .bezel {
    background: var(--deck-bezel-gradient);
    border-radius: var(--deck-bezel-corner-radius);
    padding: var(--deck-bezel-padding-top) var(--deck-bezel-padding-side) var(--deck-bezel-padding-bottom);
    transform: perspective(900px) rotateX(var(--deck-stand-tilt));
    box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4);
  }
  .grid {
    display: grid;
    gap: calc(var(--deck-key-size) * var(--deck-key-gap-ratio));
  }
  .logo {
    text-align: center;
    margin-top: 8px;
    font-size: 11px;
    letter-spacing: 0.14em;
    color: var(--deck-color-text-secondary);
    text-transform: uppercase;
  }
  .hint {
    max-width: 420px;
    text-align: center;
    font-size: 12px;
    color: var(--deck-color-text-secondary);
  }
</style>
