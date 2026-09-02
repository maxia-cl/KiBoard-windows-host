<script>
  import { onMount } from "svelte";
  import layoutPhotoshop from "../../KiBoard-protocol/protocol/fixtures/layout-auto-photoshop.json";
  import { isTauri, loadAutoPreview } from "./bridge.js";
  import IconGlyph from "./IconGlyph.svelte";
  import Key from "./Key.svelte";
  import { t } from "./i18n.js";

  const FALLBACK = {
    ...layoutPhotoshop,
    keys: layoutPhotoshop.keys.slice(0, 12),
    grid: { rows: 3, cols: 5 },
  };

  let layout = $state(isTauri() ? null : FALLBACK);
  let page = $state(0);
  let loading = $state(true);
  let fingerprint = "";

  async function refresh(requestedPage = page) {
    try {
      const next = await loadAutoPreview(requestedPage);
      if (!next) return;
      const nextFingerprint = JSON.stringify(next);
      if (nextFingerprint !== fingerprint) {
        fingerprint = nextFingerprint;
        layout = next;
        page = next.page ?? 0;
      }
    } catch {
      // A transient invoke failure should not erase the last good surface.
    } finally {
      loading = false;
    }
  }

  function selectPage(next) {
    if (next === page) return;
    page = next;
    refresh(next);
  }

  onMount(() => {
    refresh();
    const timer = setInterval(refresh, 500);
    return () => clearInterval(timer);
  });
</script>

<section class="auto-tab" aria-label={t("auto.live")}>
  <div class="preview-heading">
    <div>
      <span class="eyebrow"><i></i>{t("auto.live")}</span>
      <p>{t("auto.following")}</p>
    </div>
    {#if layout?.source?.appName}
      <span class="current-app">{layout.source.appName}</span>
    {/if}
  </div>

  {#if layout}
    <div class="phone-preview">
      <aside class="phone-rail" aria-hidden="true">
        <div class="rail-item">
          <IconGlyph name="apps" />
          <span>{t("auto.launcher")}</span>
        </div>
        <strong>{layout.source?.appName || "KiBoard"}</strong>
        <div class="rail-actions">
          <div class="rail-item active">
            <IconGlyph name="bolt" />
            <span>Auto</span>
          </div>
          <div class="rail-item">
            <IconGlyph name="settings" />
            <span>{t("auto.settings")}</span>
          </div>
        </div>
      </aside>

      <div class="bezel">
        <div class="grid">
          {#each layout.keys as key (key.pos)}
            <Key keyData={key} pos={key.pos} interactive={false} />
          {/each}
          <div class="foreground-panel">
            <div class="foreground-identity">
              {#if layout.source?.appIcon}
                <img src={layout.source.appIcon} alt="" />
              {:else}
                <IconGlyph name="windows" />
              {/if}
              <span>{layout.source?.appName || "KiBoard"}</span>
            </div>
            <IconGlyph name="close" color="var(--deck-color-key-danger-background)" />
          </div>
        </div>

        {#if (layout.pages ?? 1) > 1}
          <div class="page-dots" aria-label={t("auto.page", page + 1, layout.pages)}>
            {#each Array(layout.pages) as _, i}
              <button
                class:active={i === page}
                onclick={() => selectPage(i)}
                aria-label={t("auto.page", i + 1, layout.pages)}
                aria-current={i === page ? "page" : undefined}
              ></button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {:else}
    <div class="waiting" aria-live="polite">
      <span class:spin={loading}>◌</span>
      <p>{t("auto.waiting")}</p>
    </div>
  {/if}
</section>

<style>
  .auto-tab {
    --deck-key-size: clamp(88px, min(21vh, 14vw), 134px);
    display: flex;
    flex: 1;
    min-height: 0;
    flex-direction: column;
    padding: 18px 22px 22px;
    gap: 14px;
    background:
      radial-gradient(circle at 70% 8%, rgb(65 185 255 / 7%), transparent 34%),
      var(--deck-color-app-background);
  }
  .preview-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 38px;
  }
  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--deck-color-text-primary);
    font-size: 14px;
    font-weight: 650;
  }
  .eyebrow i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--deck-color-state-on);
    box-shadow: 0 0 0 4px rgb(54 214 126 / 10%);
  }
  .preview-heading p {
    margin: 4px 0 0 16px;
    color: var(--deck-color-text-secondary);
    font-size: 12px;
  }
  .current-app {
    max-width: 280px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 6px 10px;
    border: 1px solid var(--deck-color-surface-border);
    border-radius: 999px;
    color: var(--deck-color-text-secondary);
    background: var(--deck-color-surface);
    font-size: 12px;
  }
  .phone-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: stretch;
    justify-content: center;
    overflow: hidden;
    border: 1px solid var(--deck-color-surface-border);
    border-radius: 28px;
    background: #080a0d;
    box-shadow: 0 24px 70px rgb(0 0 0 / 45%);
  }
  .phone-rail {
    width: 112px;
    flex: 0 0 112px;
    padding: 16px 10px;
    display: flex;
    flex-direction: column;
    align-items: center;
    color: var(--deck-color-text-secondary);
    background: #090c10;
  }
  .phone-rail strong {
    flex: 1;
    display: grid;
    place-items: center;
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    color: var(--deck-color-text-primary);
    font-size: 13px;
    letter-spacing: 0.02em;
    overflow: hidden;
    max-height: 170px;
  }
  .rail-actions {
    display: grid;
    gap: 10px;
  }
  .rail-item {
    width: 84px;
    min-height: 58px;
    display: grid;
    place-items: center;
    align-content: center;
    gap: 4px;
    border-radius: 14px;
    background: var(--deck-color-key-default-background);
    color: var(--deck-color-text-primary);
    box-shadow: 0 4px 10px rgb(0 0 0 / 28%);
    font-size: 11px;
  }
  .rail-item :global(.icon-glyph) {
    font-size: 20px;
  }
  .rail-item.active {
    box-shadow: inset 3px 0 0 var(--deck-color-accent), 0 4px 10px rgb(0 0 0 / 28%);
  }
  .bezel {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 8px 12px;
    background: var(--deck-bezel-gradient);
    border-radius: 24px 0 0 24px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(5, var(--deck-key-size));
    grid-template-rows: repeat(3, var(--deck-key-size));
    gap: calc(var(--deck-key-size) * var(--deck-key-gap-ratio));
  }
  .foreground-panel {
    grid-column: 3 / span 3;
    grid-row: 3;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 0 calc(var(--deck-key-size) * 0.14);
    border-radius: var(--deck-key-corner-radius);
    border: 1px solid rgb(255 255 255 / 8%);
    background: rgb(0 0 0 / 20%);
    color: var(--deck-color-text-primary);
  }
  .foreground-panel > :global(.icon-glyph) {
    flex: 0 0 auto;
    font-size: calc(var(--deck-key-size) * 0.27);
  }
  .foreground-identity {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: calc(var(--deck-key-size) * 0.13);
    font-size: calc(var(--deck-key-size) * 0.15);
  }
  .foreground-identity img,
  .foreground-identity :global(.icon-glyph) {
    width: calc(var(--deck-key-size) * 0.5);
    height: calc(var(--deck-key-size) * 0.5);
    flex: 0 0 auto;
    object-fit: contain;
    font-size: calc(var(--deck-key-size) * 0.4);
  }
  .foreground-identity span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .page-dots {
    height: 22px;
    display: flex;
    align-items: end;
    justify-content: center;
    gap: 7px;
  }
  .page-dots button {
    width: 8px;
    height: 8px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--deck-color-page-dot-inactive);
    cursor: pointer;
    transition: width 120ms ease, background 120ms ease;
  }
  .page-dots button.active {
    width: 20px;
    background: var(--deck-color-page-dot-active);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 35%);
  }
  .waiting {
    flex: 1;
    display: grid;
    place-content: center;
    place-items: center;
    color: var(--deck-color-text-secondary);
  }
  .waiting span {
    color: var(--deck-color-accent);
    font-size: 30px;
  }
  .waiting p {
    margin-top: 8px;
  }
  .spin {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
  @media (max-height: 690px) {
    .auto-tab { padding-top: 12px; gap: 8px; }
    .preview-heading p { display: none; }
    .phone-rail { padding-block: 10px; }
    .rail-item { min-height: 52px; }
  }
</style>
