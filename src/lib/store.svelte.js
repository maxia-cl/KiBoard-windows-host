// Editor state over REAL decks (F5). Phase FP's in-memory mock built these from fixtures and
// invented two concepts the protocol does not have: a `folders` map and a per-deck `model` that
// capped how many keys a page could hold. Both are gone.
//
// A scope is now one thing — a page id. A "folder" IS a page; the only difference is how you get
// there, which is a key of kind "folder" whose `target` names it. That is what the host has
// resolved since F2, so the editor and the phone finally agree on what a deck is.

import { emptyKey, screenKeys, screensOf, SCREEN } from "./model.js";
import {
  loadDecks,
  saveDecks,
  loadAppCatalogue,
  loadObsScenes,
  loadObsInfo,
  testAction,
  previewDecks,
  clearPreview,
  exportDeck,
} from "./bridge.js";
import { t } from "./i18n.js";

let decks = $state([]);
let catalogue = $state({ groups: [] });
let loaded = $state(false);
let dirty = $state(false);
let saveError = $state(null);

let selection = $state({ deckId: null, pageId: null, screen: 0, pos: null });
let toast = $state(null);
let historyStack = $state([]);
let redoStack = $state([]);

// --- loading -------------------------------------------------------------

export async function init() {
  const [realDecks, apps, scenes, obs] = await Promise.all([
    loadDecks(),
    loadAppCatalogue(),
    loadObsScenes().catch(() => []), // OBS not running is normal, not an error
    loadObsInfo().catch(() => ({ running: false, connected: false })),
  ]);
  decks = realDecks;
  catalogue = buildCatalogue(apps, scenes, obs);
  const first = decks[0];
  selection = { deckId: first?.id ?? null, pageId: first?.pages[0]?.id ?? null, screen: 0, pos: null };
  loaded = true;
}

function buildCatalogue(apps, scenes, obs) {
  const groups = [
    {
      id: "frequent",
      label: t("cat.frequent"),
      items: [
        { type: "common", id: "copy", label: "Copiar", icon: "copy", action: "ctrl+c" },
        { type: "common", id: "paste", label: "Pegar", icon: "paste", action: "ctrl+v" },
        { type: "common", id: "cut", label: "Cortar", icon: "cut", action: "ctrl+x" },
        { type: "common", id: "undo", label: "Deshacer", icon: "undo", action: "ctrl+z" },
        { type: "common", id: "redo", label: "Rehacer", icon: "redo", action: "ctrl+y" },
        { type: "common", id: "select-all", label: "Sel. todo", icon: "selectall", action: "ctrl+a" },
      ],
    },
    {
      id: "tools",
      label: t("cat.tools"),
      items: [
        { type: "system", id: "screenshot", label: "Captura", icon: "screenshot", action: "screenshot" },
        { type: "system", id: "trackpad", label: "Trackpad", icon: "mouse", action: "trackpad" },
        { type: "system", id: "dictate", label: "Dictar", icon: "mic", action: "dictate" },
      ],
    },
  ];

  if (apps.length) {
    groups.push({
      id: "apps",
      label: t("cat.recentapps"),
      items: apps.map((app) => ({
        type: "app",
        id: app.id,
        label: app.name,
        icon: "app",
        image: app.image ?? undefined,
        // `launch:` already focuses a running instance, so common users need no hidden hold rule.
        action: `launch:${app.id}`,
      })),
    });
  }

  if (obs?.running || obs?.connected) {
    groups.push({
      id: "obs",
      label: t("cat.obs"),
      items: [
        {
          type: "obs", id: "record", label: "Grabar", icon: "record", action: "obs:record",
          toggle: { label: "Detener grab.", icon: "record", color: "#7A2733" },
        },
        {
          type: "obs", id: "stream", label: "Directo", icon: "stream", action: "obs:stream", danger: true,
          toggle: { label: "Cortar directo", icon: "stream", color: "#7A2733" },
        },
        {
          type: "obs", id: "mic", label: "Mic", icon: "mic", action: "obs:mic",
          toggle: { label: "Activar mic", icon: "mic", color: "#315C4A" },
        },
        ...scenes.map((name) => ({
          type: "obs",
          id: `scene-${name}`,
          label: name,
          icon: "obs",
          action: `obs:scene:${name}`,
          toggle: { label: name, icon: "obs", color: "#315C4A" },
        })),
      ],
    });
  }

  return {
    groups,
  };
}

// --- reads ---------------------------------------------------------------

export const getDecks = () => decks;
export const getCatalogue = () => catalogue;
export const getSelection = () => selection;
export const getToast = () => toast;
export const isLoaded = () => loaded;
export const isDirty = () => dirty;
export const getSaveError = () => saveError;

export function showToast(text) {
  toast = text;
  setTimeout(() => {
    if (toast === text) toast = null;
  }, 2200);
}

const findDeck = (deckId) => decks.find((d) => d.id === deckId);
const findPage = (deckId, pageId) => findDeck(deckId)?.pages.find((p) => p.id === pageId);

/**
 * The keys of a page, plus a setter.
 *
 * `keys` is a getter, not a snapshot: a move resolves both its `from` and `to` scopes up front,
 * and when they land on the same page the second write must see the first one's result. A
 * captured array would make the second `set` overwrite the first — a lost update.
 */
export function resolveScope(deckId, pageId) {
  const page = findPage(deckId, pageId);
  if (!page) return null;
  return {
    get keys() {
      return page.keys;
    },
    set: (keys) => {
      page.keys = keys;
    },
  };
}

/** The dense screen the editor is drawing right now. */
export function currentScreen(deckId, pageId, screen) {
  const scope = resolveScope(deckId, pageId);
  return scope ? screenKeys(scope.keys, screen) : [];
}

/** How many dots the device shows — the same count the phone would show for its own grid. */
export function screenCount(deckId, pageId) {
  const scope = resolveScope(deckId, pageId);
  return scope ? screensOf(scope.keys) : 1;
}

// --- history and persistence ---------------------------------------------

const snapshot = () => $state.snapshot(decks);

// Live preview (§5): every edit is shown on the connected phones before anything is saved, which
// is the whole point of a WYSIWYG editor for a device you hold in your other hand.
// ponytail: coalesced by a timer rather than diffed — a drag fires a dozen edits a second and the
// phone only needs the last one. 200 ms reads as immediate and costs one push per gesture.
const PREVIEW_DEBOUNCE_MS = 200;
let previewTimer = null;

function schedulePreview() {
  clearTimeout(previewTimer);
  previewTimer = setTimeout(() => {
    // A preview the host rejects is not worth reporting: the Save button is where a real problem
    // belongs, and interrupting a drag with an error would be worse than the stale phone.
    previewDecks(snapshot()).catch(() => {});
  }, PREVIEW_DEBOUNCE_MS);
}

/** Puts the phones back on the saved decks — leaving manual mode, or closing the window. */
export function stopPreview() {
  clearTimeout(previewTimer);
  return clearPreview().catch(() => {});
}

function withHistory(fn) {
  historyStack.push(snapshot());
  if (historyStack.length > 50) historyStack.shift();
  redoStack = [];
  fn();
  dirty = true;
  schedulePreview();
}

export const canUndo = () => historyStack.length > 0;
export const canRedo = () => redoStack.length > 0;

/**
 * Puts the selection back on something that exists.
 *
 * A history jump can take the selected deck or page away — undoing an import, a duplicate or a
 * new page does exactly that. Left alone, the editor draws a deck that is not there: no keys, no
 * name, and no deck picker to escape with, because the picker lives on the same deck. The host
 * snaps the same way when a phone is left pointing at a deleted deck (`manual_layout`).
 */
function snapSelection() {
  const deck = decks.find((d) => d.id === selection.deckId) ?? decks[0];
  if (!deck) {
    selection = { deckId: null, pageId: null, screen: 0, pos: null };
    return;
  }
  const page = deck.pages.find((p) => p.id === selection.pageId) ?? deck.pages[0];
  if (deck.id !== selection.deckId || page?.id !== selection.pageId) {
    selection = { deckId: deck.id, pageId: page?.id ?? null, screen: 0, pos: null };
  }
}

export function undo() {
  if (!historyStack.length) return;
  redoStack.push(snapshot());
  decks = historyStack.pop();
  snapSelection();
  dirty = true;
  schedulePreview();
}
export function redo() {
  if (!redoStack.length) return;
  historyStack.push(snapshot());
  decks = redoStack.pop();
  snapSelection();
  dirty = true;
  schedulePreview();
}

/** Writes to `config.json` and re-sends the layout to every phone in manual mode. */
export async function save() {
  // A preview still in flight would land AFTER the save and put the phones back on unsaved decks.
  clearTimeout(previewTimer);
  const result = await saveDecks(snapshot());
  if (!result.ok) {
    saveError = result.error;
    showToast(t("toast.notsaved", result.error));
    return false;
  }
  saveError = null;
  dirty = false;
  showToast(t("toast.saved"));
  return true;
}

// --- selection -----------------------------------------------------------

export function select(pos) {
  selection.pos = pos;
}

export function selectDeck(deckId) {
  const deck = findDeck(deckId);
  selection = { deckId, pageId: deck?.pages[0]?.id ?? null, screen: 0, pos: null };
}

export function selectScreen(screen) {
  selection.screen = screen;
  selection.pos = null;
}

/** Follows a `folder`/`page` key into the page it targets. */
export function enterPage(pageId) {
  selection.pageId = pageId;
  selection.screen = 0;
  selection.pos = null;
}

/** Back to the deck's entry page. */
export function exitPage() {
  enterPage(findDeck(selection.deckId)?.pages[0]?.id ?? null);
}

export const isEntryPage = () =>
  findDeck(selection.deckId)?.pages[0]?.id === selection.pageId;

// --- edits ---------------------------------------------------------------

function keyAt(scope, pos) {
  return scope.keys.find((k) => k.pos === pos) ?? emptyKey(pos);
}

function upsertKey(scope, pos, key) {
  const others = scope.keys.filter((k) => k.pos !== pos);
  scope.set(key.kind === "empty" ? others : [...others, { ...key, pos }]);
}

function keyFromCatalogueItem(item, pos) {
  if (item.type === "page-template") {
    // A new page, and the key that reaches it — the two are meaningless apart. `target` is what
    // the host resolves on press (§3), which is why the page is created with this exact id.
    return { pos, label: "New page", icon: "folder", kind: "folder", target: `p${crypto.randomUUID().slice(0, 8)}` };
  }
  return {
    pos,
    label: item.label,
    icon: item.icon,
    image: item.image,
    action: item.action,
    hold: item.hold,
    double: item.double,
    toggle: item.toggle,
    danger: item.danger,
    kind: "action",
  };
}

const simpleKeys = () => [
  ["Copiar", "copy", "ctrl+c"],
  ["Pegar", "paste", "ctrl+v"],
  ["Cortar", "cut", "ctrl+x"],
  ["Deshacer", "undo", "ctrl+z"],
  ["Rehacer", "redo", "ctrl+y"],
  ["Sel. todo", "selectall", "ctrl+a"],
  ["Captura", "screenshot", "screenshot"],
  ["Trackpad", "mouse", "trackpad"],
  ["Dictar", "mic", "dictate"],
].map(([label, icon, action], pos) => ({ pos, label, icon, action, kind: "action" }));

/** Replaces only the selected Manual deck; Launcher and every other deck remain untouched. */
export function resetDeckToSimple(deckId) {
  const deck = findDeck(deckId);
  if (!deck) return;
  const pageId = deck.pages[0]?.id ?? `p${crypto.randomUUID().slice(0, 8)}`;
  withHistory(() => {
    deck.pages = [{ id: pageId, name: "", keys: simpleKeys() }];
  });
  selection = { deckId, pageId, screen: 0, pos: null };
  showToast(t("toast.simplified"));
}

/** Adds a linked page from the page currently on show, then opens it for editing. */
export function addPage(deckId) {
  const deck = findDeck(deckId);
  const scope = resolveScope(deckId, selection.pageId);
  if (!deck || !scope) return;
  const number = deck.pages.length + 1;
  const pageId = `p${crypto.randomUUID().slice(0, 8)}`;
  const taken = new Set(scope.keys.map((key) => key.pos));
  let pos = 0;
  while (taken.has(pos)) pos += 1;
  withHistory(() => {
    upsertKey(scope, pos, {
      pos,
      label: `Página ${number}`,
      icon: "folder",
      kind: "folder",
      target: pageId,
    });
    deck.pages.push({ id: pageId, name: `Página ${number}`, keys: [] });
  });
  selection = { deckId, pageId, screen: 0, pos: null };
  showToast(t("toast.pageadded", number));
}

export function dropCatalogueItem(deckId, pageId, pos, item) {
  const scope = resolveScope(deckId, pageId);
  const existing = keyAt(scope, pos);
  withHistory(() => {
    const key = keyFromCatalogueItem(item, pos);
    upsertKey(scope, pos, key);
    if (item.type === "page-template") {
      findDeck(deckId).pages.push({ id: key.target, name: "New page", keys: [] });
    }
  });
  showToast(t(existing.kind === "empty" ? "toast.assigned" : "toast.replaced", pos + 1));
}

/**
 * The keyboard path (§5): assigns a catalogue item without a mouse. Enter on an item drops it on
 * the selected key, or on the first free slot of the screen on show when nothing is selected —
 * dragging is the fast way, not the only way.
 */
export function assignToSelection(item) {
  const { deckId, pageId, screen, pos } = selection;
  if (!deckId || !pageId) return;
  let target = pos;
  if (target == null) {
    const taken = new Set(resolveScope(deckId, pageId)?.keys.map((k) => k.pos) ?? []);
    const base = screen * SCREEN;
    for (let i = base; i < base + SCREEN; i++) {
      if (!taken.has(i)) {
        target = i;
        break;
      }
    }
  }
  if (target == null) {
    showToast(t("toast.fullselect"));
    return;
  }
  dropCatalogueItem(deckId, pageId, target, item);
  select(target);
}

export function moveKey(deckId, from, to) {
  const fromScope = resolveScope(deckId, from.pageId);
  const toScope = resolveScope(deckId, to.pageId);
  const moving = keyAt(fromScope, from.pos);
  if (moving.kind === "empty") return;
  withHistory(() => {
    upsertKey(fromScope, from.pos, emptyKey(from.pos));
    upsertKey(toScope, to.pos, { ...moving, pos: to.pos });
  });
  showToast(t("toast.moved", to.pos + 1));
}

export function copyKey(deckId, from, to) {
  const moving = keyAt(resolveScope(deckId, from.pageId), from.pos);
  if (moving.kind === "empty") return;
  const toScope = resolveScope(deckId, to.pageId);
  withHistory(() => upsertKey(toScope, to.pos, { ...moving, pos: to.pos }));
  showToast(t("toast.copied", to.pos + 1));
}

export function swapKeys(deckId, from, to) {
  const fromScope = resolveScope(deckId, from.pageId);
  const toScope = resolveScope(deckId, to.pageId);
  const a = keyAt(fromScope, from.pos);
  const b = keyAt(toScope, to.pos);
  withHistory(() => {
    upsertKey(fromScope, from.pos, { ...b, pos: from.pos });
    upsertKey(toScope, to.pos, { ...a, pos: to.pos });
  });
  showToast(t("toast.swapped", from.pos + 1, to.pos + 1));
}

export function emptyKeyAt(deckId, pageId, pos) {
  const scope = resolveScope(deckId, pageId);
  if (keyAt(scope, pos).kind === "empty") return;
  withHistory(() => upsertKey(scope, pos, emptyKey(pos)));
  showToast(t("toast.cleared", pos + 1));
}

/** Drops a key onto another screen of the same page, into its first free slot. */
export function moveKeyToScreen(deckId, from, targetScreen) {
  const scope = resolveScope(deckId, from.pageId);
  const moving = keyAt(scope, from.pos);
  if (moving.kind === "empty") return;
  const base = targetScreen * SCREEN;
  const taken = new Set(scope.keys.map((k) => k.pos));
  let free = -1;
  for (let i = base; i < base + SCREEN; i++) {
    if (!taken.has(i) || i === from.pos) {
      free = i;
      break;
    }
  }
  if (free === -1) {
    showToast(t("toast.screenfull"));
    return;
  }
  withHistory(() => {
    upsertKey(scope, from.pos, emptyKey(from.pos));
    upsertKey(scope, free, { ...moving, pos: free });
  });
  showToast(t("toast.movedscreen", targetScreen + 1));
}

// --- decks (F6) ----------------------------------------------------------

const shortId = () => crypto.randomUUID().slice(0, 8);
const freeDeckId = (base) => {
  let id = base;
  for (let n = 2; decks.some((d) => d.id === id); n++) id = `${base}-${n}`;
  return id;
};

/// The NAME needs the same treatment as the id. Duplicating twice used to leave two decks both
/// called "Work copy", and the picker on the phone lists names — so the one thing a user has to
/// tell them apart by was the one thing that collided.
const freeDeckName = (base) => {
  let name = base;
  for (let n = 2; decks.some((d) => d.name === name); n++) name = `${base} ${n}`;
  return name;
};

/**
 * A deck with fresh ids, everywhere.
 *
 * Page ids cannot be reused: a `folder`/`page` key names its destination in `target`, so a copy
 * that kept them would have its navigation land on the ORIGINAL deck's pages the moment the two
 * drift apart. The old→new map is what keeps the copy pointing at itself.
 */
function reidentify(deck, id, name) {
  const pageIds = new Map(deck.pages.map((p) => [p.id, `p${shortId()}`]));
  return {
    ...deck,
    id,
    name,
    pages: deck.pages.map((page) => ({
      ...page,
      id: pageIds.get(page.id),
      keys: page.keys.map((key) =>
        key.target ? { ...key, target: pageIds.get(key.target) ?? key.target } : { ...key },
      ),
    })),
  };
}

export function duplicateDeck(deckId) {
  const deck = findDeck(deckId);
  if (!deck) return;
  const copy = reidentify(
    $state.snapshot(deck),
    freeDeckId(`${deck.id}-copy`),
    freeDeckName(`${deck.name} copy`),
  );
  withHistory(() => decks.push(copy));
  selectDeck(copy.id);
  showToast(t("toast.duplicated", copy.name));
}

export function renameDeck(deckId, name) {
  const deck = findDeck(deckId);
  if (!deck || deck.name === name) return;
  withHistory(() => (deck.name = name));
}

/** Writes the SAVED deck to a file and reveals it. Unsaved edits are not in it — see `exportDeck`. */
export async function exportSelectedDeck(deckId) {
  if (dirty) {
    showToast(t("toast.savefirst"));
    return;
  }
  const result = await exportDeck(deckId).catch((e) => ({ ok: false, error: String(e) }));
  showToast(result.ok ? t("toast.exported", result.path) : t("toast.exportfailed", result.error));
}

/**
 * Adds a deck read from a `.kbdeck.json` file. Always re-identified, never merged into the deck it
 * came from: an import that silently replaced a deck of the same id would be a way to lose work by
 * opening a file. Saving is still the user's move, so the host validates it like any other edit.
 */
export function importDeck(parsed) {
  if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.pages) || !parsed.pages.length) {
    showToast(t("toast.notadeck"));
    return;
  }
  const base = typeof parsed.id === "string" && parsed.id ? parsed.id : "imported";
  const deck = reidentify(parsed, freeDeckId(base), freeDeckName(parsed.name || "Imported deck"));
  withHistory(() => decks.push(deck));
  selectDeck(deck.id);
  showToast(t("toast.imported", deck.name));
}

export function updateKeyFields(deckId, pageId, pos, fields) {
  const scope = resolveScope(deckId, pageId);
  const current = keyAt(scope, pos);
  if (current.kind === "empty") return;
  withHistory(() => upsertKey(scope, pos, { ...current, ...fields }));
}

/** ▶ Test. Runs the action for real on this desktop — nothing is simulated any more. */
export async function testKey(key) {
  if (!key || key.kind === "empty" || !key.action) return;
  try {
    await testAction(key.action);
    showToast(`▶ ${key.action}`);
  } catch (e) {
    showToast(t("toast.testfailed", e));
  }
}
