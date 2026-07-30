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
  testAction,
  previewDecks,
  clearPreview,
} from "./bridge.js";

let decks = $state([]);
let catalogue = $state({ groups: [] });
let loaded = $state(false);
let dirty = $state(false);
let saveError = $state(null);

let selection = $state({ deckId: null, pageId: null, screen: 0, pos: null });
let toast = $state(null);
let historyStack = [];
let redoStack = [];

// --- loading -------------------------------------------------------------

export async function init() {
  const [realDecks, apps, scenes] = await Promise.all([
    loadDecks(),
    loadAppCatalogue(),
    loadObsScenes().catch(() => []), // OBS not running is normal, not an error
  ]);
  decks = realDecks;
  catalogue = buildCatalogue(apps, scenes);
  const first = decks[0];
  selection = { deckId: first?.id ?? null, pageId: first?.pages[0]?.id ?? null, screen: 0, pos: null };
  loaded = true;
}

function buildCatalogue(apps, scenes) {
  return {
    groups: [
      {
        id: "apps",
        label: "Apps",
        items: apps.map((app) => ({
          type: "app",
          id: app.id,
          label: app.name,
          icon: "app",
          image: app.image ?? undefined,
          action: `launch:${app.id}`,
          // Long press focuses without launching — Elgato's "Key Logic" shape (protocol §3).
          hold: `focus:${app.id}`,
        })),
      },
      {
        id: "system",
        label: "System",
        items: [
          { type: "system", id: "windows", label: "Windows", icon: "windows", action: "windows" },
          { type: "system", id: "trackpad", label: "Trackpad", icon: "mouse", action: "trackpad" },
          { type: "system", id: "dictate", label: "Dictate", icon: "mic", action: "dictate" },
          { type: "system", id: "screenshot", label: "Screenshot", icon: "screenshot", action: "screenshot" },
          { type: "system", id: "mode-auto", label: "Auto", icon: "mode", action: "mode:auto" },
        ],
      },
      {
        id: "obs",
        label: "OBS",
        items: [
          { type: "obs", id: "record", label: "Record", icon: "obs", action: "obs:record" },
          { type: "obs", id: "stream", label: "Stream", icon: "obs", action: "obs:stream" },
          { type: "obs", id: "mic", label: "Mic", icon: "mic", action: "obs:mic" },
          // The user's OWN scenes, live from the running instance — no hardcoded samples.
          ...scenes.map((name) => ({
            type: "obs",
            id: `scene-${name}`,
            label: name,
            icon: "obs",
            action: `obs:scene:${name}`,
          })),
        ],
      },
      {
        id: "pages",
        label: "Pages",
        items: [{ type: "page-template", id: "new-page", label: "New page", icon: "folder" }],
      },
    ],
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

export function undo() {
  if (!historyStack.length) return;
  redoStack.push(snapshot());
  decks = historyStack.pop();
  dirty = true;
  schedulePreview();
}
export function redo() {
  if (!redoStack.length) return;
  historyStack.push(snapshot());
  decks = redoStack.pop();
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
    showToast(`Not saved: ${result.error}`);
    return false;
  }
  saveError = null;
  dirty = false;
  showToast("Saved — phones updated");
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
    kind: "action",
  };
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
  showToast(existing.kind === "empty" ? `Assigned to key ${pos + 1}` : `Replaced key ${pos + 1}`);
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
    showToast("This screen is full — select a key to replace");
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
  showToast(`Moved to key ${to.pos + 1}`);
}

export function copyKey(deckId, from, to) {
  const moving = keyAt(resolveScope(deckId, from.pageId), from.pos);
  if (moving.kind === "empty") return;
  const toScope = resolveScope(deckId, to.pageId);
  withHistory(() => upsertKey(toScope, to.pos, { ...moving, pos: to.pos }));
  showToast(`Copied to key ${to.pos + 1}`);
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
  showToast(`Swapped keys ${from.pos + 1} and ${to.pos + 1}`);
}

export function emptyKeyAt(deckId, pageId, pos) {
  const scope = resolveScope(deckId, pageId);
  if (keyAt(scope, pos).kind === "empty") return;
  withHistory(() => upsertKey(scope, pos, emptyKey(pos)));
  showToast(`Cleared key ${pos + 1}`);
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
    showToast("That screen is full");
    return;
  }
  withHistory(() => {
    upsertKey(scope, from.pos, emptyKey(from.pos));
    upsertKey(scope, free, { ...moving, pos: free });
  });
  showToast(`Moved to screen ${targetScreen + 1}`);
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
    showToast(`▶ failed: ${e}`);
  }
}
