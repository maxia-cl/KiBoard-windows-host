// In-memory HostBridge mock for phase FP (docs/implementation-plan.md §5, FP deliverable 4).
// F5 replaces this module's role with TauriBridge invoking real Rust commands; the component
// tree above it (Device/Key/Catalogue/Inspector) does not change.

import catalogueApps from "../../KiBoard-protocol/protocol/fixtures/catalogue-apps.json";
import layoutLauncher from "../../KiBoard-protocol/protocol/fixtures/layout-manual-launcher.json";
import layoutFolder from "../../KiBoard-protocol/protocol/fixtures/layout-folder.json";
import { denseKeys, emptyKey, capacityOf } from "./model.js";

const PLACEHOLDER_ICON = catalogueApps.apps[0].icon;

function keysFromLayout(layout) {
  return layout.keys.filter((k) => k.kind !== "empty");
}

function buildInitialDecks() {
  const launcherKeys = keysFromLayout(layoutLauncher).map((k) =>
    k.pos === 4 ? { ...k, folderId: "obs" } : k.pos === 14 ? { ...k, targetPage: 1, action: undefined } : k
  );

  return [
    {
      id: "launcher",
      name: "Launcher",
      model: "mk2",
      pages: [launcherKeys, []],
      folders: {
        obs: { name: "OBS", keys: keysFromLayout(layoutFolder).filter((k) => k.pos !== 0) },
      },
    },
  ];
}

function buildCatalogue() {
  return {
    groups: [
      {
        id: "apps",
        label: "Apps",
        items: catalogueApps.apps.map((app) => ({
          type: "app",
          id: app.aumid,
          label: app.name,
          icon: "app",
          image: app.icon,
          running: app.running,
          action: `launch:${app.aumid}`,
          hold: `focus:${app.aumid}`,
        })),
      },
      {
        id: "system",
        label: "System",
        items: [
          { type: "system", id: "volume", label: "Volume", icon: "volume", action: "vol:toggle_mute" },
          { type: "system", id: "screenshot", label: "Screenshot", icon: "screenshot", action: "screenshot" },
          { type: "system", id: "windows", label: "Windows", icon: "windows", action: "windows" },
          { type: "system", id: "mode-auto", label: "Auto", icon: "mode", action: "mode:auto" },
        ],
      },
      {
        id: "obs",
        label: "OBS",
        items: [
          { type: "obs", id: "stream", label: "Start streaming", icon: "obs", action: "obs:stream_toggle" },
          { type: "obs", id: "record", label: "Start recording", icon: "obs", action: "obs:record_toggle" },
          { type: "obs", id: "scene-game", label: "Scene: Game", icon: "obs", action: "obs:scene:Game" },
          { type: "obs", id: "scene-brb", label: "Scene: BRB", icon: "obs", action: "obs:scene:BRB" },
        ],
      },
      {
        id: "macros",
        label: "Macros",
        items: [
          { type: "macro", id: "sample", label: "Sample macro", icon: "macro", action: "hotkey:Ctrl+Alt+M" },
        ],
      },
      {
        id: "folders",
        label: "Folders",
        items: [{ type: "folder-template", id: "new-folder", label: "New folder", icon: "folder" }],
      },
    ],
  };
}

function keyFromCatalogueItem(item, pos) {
  if (item.type === "folder-template") {
    return { pos, label: "New folder", icon: "folder", kind: "folder", folderId: crypto.randomUUID() };
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

let decks = $state(buildInitialDecks());
const catalogue = buildCatalogue();

let selection = $state({ deckId: "launcher", pageIndex: 0, folderId: null, pos: null });
let toast = $state(null);
let historyStack = [];
let redoStack = [];

function snapshot() {
  return $state.snapshot(decks);
}

function withHistory(fn) {
  historyStack.push(snapshot());
  if (historyStack.length > 50) historyStack.shift();
  redoStack = [];
  fn();
}

export function canUndo() {
  return historyStack.length > 0;
}
export function canRedo() {
  return redoStack.length > 0;
}
export function undo() {
  if (!historyStack.length) return;
  redoStack.push(snapshot());
  decks = historyStack.pop();
}
export function redo() {
  if (!redoStack.length) return;
  historyStack.push(snapshot());
  decks = redoStack.pop();
}

export function getDecks() {
  return decks;
}
export function getCatalogue() {
  return catalogue;
}
export function getSelection() {
  return selection;
}
export function getToast() {
  return toast;
}
export function showToast(text) {
  toast = text;
  setTimeout(() => {
    if (toast === text) toast = null;
  }, 2200);
}

function findDeck(deckId) {
  return decks.find((d) => d.id === deckId);
}

/**
 * Resolves the current scope (a page or a folder) into its dense key array + a setter.
 *
 * `raw`/`keys` are getters, not snapshots: a move or a swap resolves both its `from` and `to`
 * scopes up front, and when they land on the same underlying array (moving within one page),
 * the second write must see the first write's result. A captured-at-resolve-time array would
 * make the second `setRaw` overwrite the first — a lost update.
 */
export function resolveScope(deckId, pageIndex, folderId) {
  const deck = findDeck(deckId);
  if (!deck) return null;
  const capacity = capacityOf(deck.model);
  if (folderId) {
    const folder = deck.folders[folderId];
    return {
      capacity,
      get raw() {
        return folder.keys;
      },
      get keys() {
        return denseKeys(folder.keys, capacity);
      },
      setRaw: (keys) => {
        folder.keys = keys;
      },
    };
  }
  return {
    capacity,
    get raw() {
      return deck.pages[pageIndex] ?? [];
    },
    get keys() {
      return denseKeys(deck.pages[pageIndex] ?? [], capacity);
    },
    setRaw: (keys) => {
      deck.pages[pageIndex] = keys;
    },
  };
}

export function select(pos) {
  selection.pos = pos;
}

export function selectPage(pageIndex) {
  selection.pageIndex = pageIndex;
  selection.folderId = null;
  selection.pos = null;
}

export function enterFolder(folderId) {
  selection.folderId = folderId;
  selection.pos = null;
}

export function exitFolder() {
  selection.folderId = null;
  selection.pos = null;
}

function upsertKey(scope, pos, key) {
  const others = scope.raw.filter((k) => k.pos !== pos);
  scope.setRaw(key.kind === "empty" ? others : [...others, { ...key, pos }]);
}

function keyAt(scope, pos) {
  return scope.raw.find((k) => k.pos === pos) ?? emptyKey(pos);
}

export function dropCatalogueItem(deckId, pageIndex, folderId, pos, item) {
  const scope = resolveScope(deckId, pageIndex, folderId);
  const existing = keyAt(scope, pos);
  withHistory(() => {
    upsertKey(scope, pos, keyFromCatalogueItem(item, pos));
    if (item.type === "folder-template") {
      const deck = findDeck(deckId);
      const key = keyAt(scope, pos);
      deck.folders[key.folderId] = { name: "New folder", keys: [] };
    }
  });
  showToast(existing.kind === "empty" ? `Assigned to key ${pos + 1}` : `Replaced key ${pos + 1}`);
}

export function moveKey(deckId, from, to) {
  const fromScope = resolveScope(deckId, from.pageIndex, from.folderId);
  const toScope = resolveScope(deckId, to.pageIndex, to.folderId);
  const movingKey = keyAt(fromScope, from.pos);
  if (movingKey.kind === "empty") return;
  withHistory(() => {
    upsertKey(fromScope, from.pos, emptyKey(from.pos));
    upsertKey(toScope, to.pos, { ...movingKey, pos: to.pos });
  });
  showToast(`Moved to key ${to.pos + 1}`);
}

export function copyKey(deckId, from, to) {
  const fromScope = resolveScope(deckId, from.pageIndex, from.folderId);
  const toScope = resolveScope(deckId, to.pageIndex, to.folderId);
  const movingKey = keyAt(fromScope, from.pos);
  if (movingKey.kind === "empty") return;
  withHistory(() => {
    upsertKey(toScope, to.pos, { ...movingKey, pos: to.pos });
  });
  showToast(`Copied to key ${to.pos + 1}`);
}

export function swapKeys(deckId, from, to) {
  const fromScope = resolveScope(deckId, from.pageIndex, from.folderId);
  const toScope = resolveScope(deckId, to.pageIndex, to.folderId);
  const a = keyAt(fromScope, from.pos);
  const b = keyAt(toScope, to.pos);
  withHistory(() => {
    upsertKey(fromScope, from.pos, { ...b, pos: from.pos });
    upsertKey(toScope, to.pos, { ...a, pos: to.pos });
  });
  showToast(`Swapped keys ${from.pos + 1} and ${to.pos + 1}`);
}

export function emptyKeyAt(deckId, pageIndex, folderId, pos) {
  const scope = resolveScope(deckId, pageIndex, folderId);
  if (keyAt(scope, pos).kind === "empty") return;
  withHistory(() => upsertKey(scope, pos, emptyKey(pos)));
  showToast(`Cleared key ${pos + 1}`);
}

export function moveKeyToPage(deckId, from, targetPageIndex) {
  const deck = findDeck(deckId);
  const fromScope = resolveScope(deckId, from.pageIndex, from.folderId);
  const movingKey = keyAt(fromScope, from.pos);
  if (movingKey.kind === "empty") return;
  const capacity = capacityOf(deck.model);
  const targetKeys = denseKeys(deck.pages[targetPageIndex] ?? [], capacity);
  const firstFree = targetKeys.findIndex((k) => k.kind === "empty");
  if (firstFree === -1) {
    showToast("Target page is full");
    return;
  }
  withHistory(() => {
    upsertKey(fromScope, from.pos, emptyKey(from.pos));
    deck.pages[targetPageIndex] = [
      ...(deck.pages[targetPageIndex] ?? []).filter((k) => k.pos !== firstFree),
      { ...movingKey, pos: firstFree },
    ];
  });
  showToast(`Moved to page ${targetPageIndex + 1}`);
}

export function dropCatalogueItemOnPageDot(deckId, targetPageIndex, item) {
  const deck = findDeck(deckId);
  const capacity = capacityOf(deck.model);
  const targetKeys = denseKeys(deck.pages[targetPageIndex] ?? [], capacity);
  const firstFree = targetKeys.findIndex((k) => k.kind === "empty");
  if (firstFree === -1) {
    showToast("Target page is full");
    return;
  }
  withHistory(() => {
    deck.pages[targetPageIndex] = [
      ...(deck.pages[targetPageIndex] ?? []).filter((k) => k.pos !== firstFree),
      { ...keyFromCatalogueItem(item, firstFree) },
    ];
  });
  showToast(`Added to page ${targetPageIndex + 1}`);
}

export function moveKeyToFolder(deckId, from, folderId) {
  const deck = findDeck(deckId);
  const fromScope = resolveScope(deckId, from.pageIndex, from.folderId);
  const movingKey = keyAt(fromScope, from.pos);
  if (movingKey.kind === "empty" || folderId === from.folderId) return;
  const folder = deck.folders[folderId];
  const capacity = capacityOf(deck.model);
  const folderKeys = denseKeys(folder.keys, capacity);
  const firstFree = folderKeys.findIndex((k) => k.kind === "empty");
  if (firstFree === -1) {
    showToast("Folder is full");
    return;
  }
  withHistory(() => {
    upsertKey(fromScope, from.pos, emptyKey(from.pos));
    folder.keys = [...folder.keys.filter((k) => k.pos !== firstFree), { ...movingKey, pos: firstFree }];
  });
  showToast(`Moved into folder "${folder.name}"`);
}

export function addPage(deckId) {
  withHistory(() => findDeck(deckId).pages.push([]));
  showToast("Page added");
}

export function duplicatePage(deckId, pageIndex) {
  const deck = findDeck(deckId);
  withHistory(() => deck.pages.splice(pageIndex + 1, 0, snapshotPage(deck.pages[pageIndex])));
  showToast("Page duplicated");
}

function snapshotPage(page) {
  return page.map((k) => ({ ...k }));
}

export function setModel(deckId, presetName) {
  const deck = findDeck(deckId);
  const oldCapacity = capacityOf(deck.model);
  const newCapacity = capacityOf(presetName);
  withHistory(() => {
    if (newCapacity < oldCapacity) {
      const flat = deck.pages.flat().sort((a, b) => a.pos - b.pos);
      const rechunked = [[]];
      let page = 0;
      let slot = 0;
      for (const key of flat) {
        if (slot >= newCapacity) {
          page += 1;
          slot = 0;
          rechunked.push([]);
        }
        rechunked[page].push({ ...key, pos: slot });
        slot += 1;
      }
      deck.pages = rechunked;
    }
    deck.model = presetName;
  });
  showToast(`Model set to ${presetName.toUpperCase()}`);
}

export function testKey(key) {
  if (!key || key.kind === "empty") return;
  showToast(`▶ Test: ${key.action ?? key.label ?? "key"} (simulated, nothing runs in FP)`);
}

export function updateKeyFields(deckId, pageIndex, folderId, pos, fields) {
  const scope = resolveScope(deckId, pageIndex, folderId);
  const current = keyAt(scope, pos);
  if (current.kind === "empty") return;
  withHistory(() => upsertKey(scope, pos, { ...current, ...fields }));
}

export const PLACEHOLDER_APP_ICON = PLACEHOLDER_ICON;
