<script>
  import { getSelection, updateKeyFields, testKey, resolveScope, emptyKeyAt } from "./store.svelte.js";
  import IconGlyph from "./IconGlyph.svelte";
  import IconPicker from "./IconPicker.svelte";
  import { t } from "./i18n.js";

  let { deckId } = $props();
  let selection = $derived(getSelection());
  let scope = $derived(selection.pos != null ? resolveScope(deckId, selection.pageId) : null);
  let key = $derived(scope?.keys.find((candidate) => candidate.pos === selection.pos) ?? null);
  let picking = $state(false);
  let imageInput = $state();

  const palette = [null, "#303743", "#394B63", "#315C4A", "#654C33", "#5A3F68", "#7A2733"];

  function set(field, value) {
    updateKeyFields(deckId, selection.pageId, selection.pos, { [field]: value });
  }

  function setFace(field, value) {
    set("toggle", { ...(key.toggle ?? {}), [field]: value });
  }

  function description(value) {
    if (key?.kind === "folder") return t("insp.action.page");
    if (value?.startsWith("launch:")) return t("insp.action.launch", key.label);
    const known = {
      "ctrl+c": "copy", "ctrl+v": "paste", "ctrl+x": "cut", "ctrl+z": "undo",
      "ctrl+y": "redo", "ctrl+a": "selectall", screenshot: "screenshot",
      trackpad: "trackpad", dictate: "dictate", "obs:record": "record",
      "obs:stream": "stream", "obs:mic": "mic",
    };
    if (value?.startsWith("obs:scene:")) return t("insp.action.scene", value.slice(10));
    return known[value] ? t(`insp.action.${known[value]}`) : t("insp.action.advanced");
  }

  let managedToggle = $derived(key?.action?.startsWith("obs:") === true);
  let hasAdvanced = $derived(
    key?.kind === "action" &&
      (!!key.hold || !!key.double || key.action?.includes(">>") || (!!key.toggle && !managedToggle)),
  );

  async function pickImage(file) {
    if (!file) return;
    const size = 96;
    try {
      const bitmap = await createImageBitmap(file);
      const canvas = document.createElement("canvas");
      canvas.width = canvas.height = size;
      const ctx = canvas.getContext("2d");
      const scale = Math.min(size / bitmap.width, size / bitmap.height);
      const width = bitmap.width * scale;
      const height = bitmap.height * scale;
      ctx.drawImage(bitmap, (size - width) / 2, (size - height) / 2, width, height);
      set("image", canvas.toDataURL("image/png"));
    } catch {
      set("image", null);
    } finally {
      if (imageInput) imageInput.value = "";
    }
  }
</script>

<aside class="inspector">
  <h2>{t("insp.title")}</h2>
  {#if key && key.kind !== "empty"}
    <div class="preview-key" style:background-color={key.color ?? null}>
      {#if key.image}<img src={key.image} alt="" />
      {:else}<span class="glyph"><IconGlyph name={key.icon} color={key.iconColor} /></span>{/if}
    </div>

    <label>
      {t("insp.label")}
      <input data-telemetry="editor_key_label_changed" value={key.label ?? ""} oninput={(e) => set("label", e.currentTarget.value)} />
    </label>

    <div class="field">
      <span>{t("insp.appearance")}</span>
      <div class="row">
        <button data-telemetry="editor_icon_picker_opened" class="secondary grow" onclick={() => (picking = true)}><IconGlyph name={key.icon} /> {t("insp.icon")}</button>
        <button data-telemetry="editor_image_picker_opened" class="secondary grow" onclick={() => imageInput.click()}>{t("insp.image")}</button>
        <input data-telemetry="editor_key_image_changed" class="hidden-file" bind:this={imageInput} type="file" accept="image/png,image/jpeg,image/webp" onchange={(e) => pickImage(e.currentTarget.files?.[0])} />
      </div>
      {#if key.image}<button data-telemetry="editor_key_image_removed" class="text-button" onclick={() => set("image", null)}>{t("insp.backtoicon")}</button>{/if}
    </div>

    <div class="field">
      <span>{t("insp.colour")}</span>
      <div class="palette">
        {#each palette as color}
          <button
            data-telemetry="editor_key_color_changed"
            class:default-color={color === null}
            class:selected-color={(key.color ?? null) === color}
            style:background-color={color}
            onclick={() => set("color", color)}
            aria-label={color ?? t("insp.defaultcolour")}
          ></button>
        {/each}
        <label class="custom-color" title={t("insp.customcolour")}>+
          <input data-telemetry="editor_key_custom_color_changed" type="color" value={key.color ?? "#303743"} oninput={(e) => set("color", e.currentTarget.value)} />
        </label>
      </div>
    </div>

    <div class="field">
      <span>{t("insp.action")}</span>
      <div class="action-summary"><IconGlyph name="bolt" /> {description(key.action)}</div>
    </div>

    {#if key.kind === "action"}<button data-telemetry="editor_key_tested" class="test" onclick={() => testKey(key)}>{t("insp.test")}</button>{/if}

    {#if hasAdvanced}
      <details class="advanced">
        <summary data-telemetry="editor_advanced_toggled">{t("insp.more")}</summary>
        <label>{t("insp.short")}<input data-telemetry="editor_short_action_changed" value={key.action ?? ""} oninput={(e) => set("action", e.currentTarget.value)} /></label>
        <label>{t("insp.long")}<input data-telemetry="editor_long_action_changed" value={key.hold ?? ""} oninput={(e) => set("hold", e.currentTarget.value || null)} /></label>
        <label>{t("insp.double")}<input data-telemetry="editor_double_action_changed" value={key.double ?? ""} oninput={(e) => set("double", e.currentTarget.value || null)} /></label>
        <label class="checkbox">
          <input data-telemetry="editor_toggle_changed" type="checkbox" checked={!!key.toggle} onchange={(e) => set("toggle", e.currentTarget.checked ? { label: key.label ?? "" } : null)} />
          {t("insp.second")}
        </label>
        {#if key.toggle}
          <label>{t("insp.whenon")}<input data-telemetry="editor_toggle_label_changed" value={key.toggle.label ?? ""} oninput={(e) => setFace("label", e.currentTarget.value)} /></label>
        {/if}
      </details>
    {/if}

    <button data-telemetry="editor_key_deleted" class="delete" onclick={() => emptyKeyAt(deckId, selection.pageId, selection.pos)}>{t("insp.delete")}</button>
  {:else}
    <p class="hint">{t("insp.empty")}</p>
  {/if}
</aside>

{#if picking}
  <IconPicker
    current={key?.icon}
    onpick={(icon) => { set("icon", icon); set("image", null); picking = false; }}
    onclose={() => (picking = false)}
  />
{/if}

<style>
  .inspector { width: 260px; flex-shrink: 0; padding: 14px; display: flex; flex-direction: column; gap: 12px; color: var(--deck-color-text-primary); overflow-y: auto; border-left: 1px solid var(--deck-color-surface-border); }
  h2 { margin: 0; font-size: 15px; }
  .preview-key { width: 72px; height: 72px; border-radius: 12px; background: var(--deck-color-key-default-background); display: flex; align-items: center; justify-content: center; align-self: center; box-shadow: 0 5px 10px -3px rgb(0 0 0 / 65%); }
  .preview-key img { width: 48px; height: 48px; object-fit: contain; }
  .glyph { font-size: 28px; }
  label, .field { display: flex; flex-direction: column; gap: 5px; font-size: 12px; color: var(--deck-color-text-secondary); }
  label.checkbox { flex-direction: row; align-items: center; }
  input, .secondary { background: var(--deck-color-surface-raised); border: 1px solid var(--deck-color-surface-border); border-radius: 7px; padding: 7px 9px; color: var(--deck-color-text-primary); min-width: 0; }
  .row { display: flex; gap: 6px; min-width: 0; }
  .grow { flex: 1; }
  .secondary { cursor: pointer; }
  .hidden-file { display: none; }
  .text-button { align-self: flex-start; padding: 0; border: 0; background: none; color: var(--deck-color-accent); cursor: pointer; font-size: 12px; }
  .palette { display: flex; gap: 7px; align-items: center; }
  .palette > button, .custom-color { width: 25px; height: 25px; border: 1px solid rgb(255 255 255 / 18%); border-radius: 7px; cursor: pointer; }
  .palette > button.default-color { background: linear-gradient(135deg, #303743 45%, transparent 46%, transparent 54%, #303743 55%); }
  .palette > button.selected-color { outline: 2px solid var(--deck-color-accent); outline-offset: 2px; }
  .custom-color { position: relative; display: grid; place-items: center; background: var(--deck-color-surface-raised); color: var(--deck-color-text-primary); overflow: hidden; }
  .custom-color input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .action-summary { display: flex; align-items: center; gap: 8px; min-height: 38px; padding: 8px 10px; border: 1px solid var(--deck-color-surface-border); border-radius: 8px; background: var(--deck-color-surface-raised); color: var(--deck-color-text-primary); font-size: 13px; }
  .test, .delete { border-radius: 8px; padding: 9px; cursor: pointer; font-weight: 600; }
  .test { background: var(--deck-color-accent); color: white; border: 0; }
  .delete { margin-top: auto; background: transparent; color: #ff7b86; border: 1px solid rgb(255 82 82 / 30%); }
  .advanced { border-top: 1px solid var(--deck-color-surface-border); padding-top: 8px; }
  .advanced summary { cursor: pointer; color: var(--deck-color-text-secondary); font-size: 12px; }
  .advanced label { margin-top: 8px; }
  .hint { font-size: 12px; color: var(--deck-color-text-secondary); line-height: 1.45; }
</style>
