<script>
  import { getSelection, updateKeyFields, testKey, resolveScope } from "./store.svelte.js";
  import IconGlyph from "./IconGlyph.svelte";
  import IconPicker from "./IconPicker.svelte";
  import { t } from "./i18n.js";

  let { deckId } = $props();

  let selection = $derived(getSelection());
  let scope = $derived(selection.pos != null ? resolveScope(deckId, selection.pageId) : null);
  // `pos` is the key's ABSOLUTE address in the page, not an index into a dense screen array.
  let key = $derived(scope?.keys.find((k) => k.pos === selection.pos) ?? null);
  // Which icon field the picker is filling: the key's own, or the ON face's.
  let picking = $state(null);

  function set(field, value) {
    updateKeyFields(deckId, selection.pageId, selection.pos, { [field]: value });
  }

  /** Edits one field of the ON face without disturbing the rest of it (protocol §3). */
  function setFace(field, value) {
    set("toggle", { ...(key.toggle ?? {}), [field]: value });
  }

  // --- multi-action (F6) ---------------------------------------------------
  // The v1 DSL chains steps with ">>" and has since v1; what F6 adds is a way to write one
  // without typing the separator. Purely a view over the same string — the stored action stays
  // exactly what `run_action` has always parsed, so nothing downstream learns a new shape.
  const steps = (action) => (action ?? "").split(">>").map((s) => s.trim());
  const joined = (list) => list.filter((s) => s !== "").join(" >> ");

  function setStep(action, i, value) {
    const list = steps(action);
    list[i] = value;
    return joined(list);
  }

  /**
   * A custom image, downscaled to the key's own size before it is stored.
   *
   * The whole `layout` frame is capped at 64 KB (protocol §4), and this data: URI is stored in the
   * deck and re-sent on every push — a 2 MB photo dropped on three keys would make the page
   * unsendable. 96 px is twice the drawn key on a phone at 3x, and lands around 4 KB.
   */
  async function pickImage(file, apply) {
    if (!file) return;
    const SIZE = 96;
    try {
      const bitmap = await createImageBitmap(file);
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = SIZE;
      const ctx = canvas.getContext("2d");
      const scale = Math.min(SIZE / bitmap.width, SIZE / bitmap.height);
      const w = bitmap.width * scale;
      const h = bitmap.height * scale;
      ctx.drawImage(bitmap, (SIZE - w) / 2, (SIZE - h) / 2, w, h);
      apply(canvas.toDataURL("image/png"));
    } catch {
      // A file the browser cannot decode (a renamed .txt, a broken PNG) is not worth a dialog.
      apply(null);
    }
  }
</script>

<div class="inspector">
  {#if key && key.kind !== "empty"}
    <div class="preview-key" style:background-color={key.color ?? null}>
      {#if key.image}
        <img src={key.image} alt="" />
      {:else}
        <span class="glyph"><IconGlyph name={key.icon} color={key.iconColor} /></span>
      {/if}
    </div>

    <label>
      {t("insp.label")}
      <input value={key.label ?? ""} oninput={(e) => set("label", e.currentTarget.value)} />
    </label>

    <label>
      {t("insp.icon")}
      <button class="icon-btn" onclick={() => (picking = "icon")}>
        <IconGlyph name={key.icon} /> {t("insp.change")}
      </button>
    </label>

    <label>
      {t("insp.image")}
      <div class="row">
        <input
          type="file"
          accept="image/png,image/jpeg,image/webp"
          onchange={(e) => pickImage(e.currentTarget.files?.[0], (img) => set("image", img))}
        />
        {#if key.image}
          <button class="icon-btn" onclick={() => set("image", null)} title={t("insp.backtoicon")}>×</button>
        {/if}
      </div>
    </label>

    <label>
      {t("insp.colour")}
      <input type="color" value={key.color ?? "#2C2C2E"} oninput={(e) => set("color", e.currentTarget.value)} />
    </label>

    {#if key.kind === "action"}
      <fieldset>
        <legend>{t("insp.short")}</legend>
        {#each steps(key.action) as step, i}
          <div class="row">
            <input
              value={step}
              placeholder="ctrl+c"
              oninput={(e) => set("action", setStep(key.action, i, e.currentTarget.value))}
            />
            {#if steps(key.action).length > 1}
              <button class="icon-btn" onclick={() => set("action", setStep(key.action, i, ""))} title={t("insp.removestep")}>×</button>
            {/if}
          </div>
        {/each}
        <button class="add" onclick={() => set("action", `${key.action ?? ""} >> `)}>{t("insp.addstep")}</button>
      </fieldset>

      <label>
        {t("insp.long")}
        <input value={key.hold ?? ""} oninput={(e) => set("hold", e.currentTarget.value)} />
      </label>
      <label>
        {t("insp.double")}
        <input value={key.double ?? ""} oninput={(e) => set("double", e.currentTarget.value)} />
      </label>

      <label class="checkbox">
        <input
          type="checkbox"
          checked={!!key.toggle}
          onchange={(e) => set("toggle", e.currentTarget.checked ? { label: key.label ?? "" } : null)}
        />
        {t("insp.second")}
      </label>

      {#if key.toggle}
        <fieldset>
          <legend>{t("insp.whenon")}</legend>
          <p class="hint">{t("insp.facehint")}</p>
          <input
            value={key.toggle.label ?? ""}
            placeholder={t("insp.label")}
            oninput={(e) => setFace("label", e.currentTarget.value)}
          />
          <div class="row">
            <button class="icon-btn grow" onclick={() => (picking = "toggle-icon")}>
              <IconGlyph name={key.toggle.icon} /> {t("insp.faceicon")}
            </button>
            <input
              type="color"
              value={key.toggle.color ?? key.color ?? "#2C2C2E"}
              oninput={(e) => setFace("color", e.currentTarget.value)}
            />
          </div>
          <input
            value={key.toggle.action ?? ""}
            placeholder={t("insp.faceaction")}
            oninput={(e) => setFace("action", e.currentTarget.value)}
          />
        </fieldset>
      {/if}

      <label class="checkbox">
        <input type="checkbox" checked={!!key.danger} onchange={(e) => set("danger", e.currentTarget.checked)} />
        {t("insp.confirm")}
      </label>
      <button class="test" onclick={() => testKey(key)}>{t("insp.test")}</button>
    {:else if key.kind === "folder"}
      <p class="hint">{t("insp.folder")}</p>
    {/if}
  {:else}
    <p class="hint">{t("insp.empty")}</p>
  {/if}
</div>

{#if picking}
  <IconPicker
    current={picking === "icon" ? key?.icon : key?.toggle?.icon}
    onpick={(icon) => {
      if (picking === "icon") set("icon", icon);
      else setFace("icon", icon);
      picking = null;
    }}
    onclose={() => (picking = null)}
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
  fieldset {
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  legend {
    font-size: 12px;
    color: var(--deck-color-text-secondary);
    padding: 0 4px;
  }
  .row {
    display: flex;
    gap: 4px;
    align-items: center;
    min-width: 0;
  }
  .row input:not([type]) {
    flex: 1;
    min-width: 0;
  }
  .grow {
    flex: 1;
  }
  input,
  .icon-btn,
  .add {
    background: #2c2c2e;
    border: 1px solid #3a3a3c;
    border-radius: 6px;
    padding: 6px 8px;
    color: var(--deck-color-text-primary);
    font-size: 13px;
    min-width: 0;
  }
  input[type="file"] {
    font-size: 11px;
    padding: 4px;
  }
  .icon-btn,
  .add {
    cursor: pointer;
    text-align: left;
  }
  .add {
    font-size: 12px;
    color: var(--deck-color-text-secondary);
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
