// Native Pointer Events drag engine (docs/implementation-plan.md §3.4). Deliberately not the
// HTML5 drag-and-drop API: that gives no control over the ghost and fights `transform`, which
// the bezel/stand tilt (§3.0) relies on.
//
// F5: a drop location is `{ pageId, pos }`. It used to be `{ pageIndex, folderId, pos }` — two
// identifiers for one thing, because phase FP treated folders as a separate world from pages.
import {
  moveKey,
  copyKey,
  swapKeys,
  moveKeyToScreen,
  emptyKeyAt,
  dropCatalogueItem,
  select,
  selectScreen,
  resolveScope,
  enterPage,
} from "./store.svelte.js";
import { emptyKey } from "./model.js";

const AUTO_FLIP_MS = 600;

let drag = $state(null); // { kind: 'catalogue' | 'key', item?, from?, key?, x, y, target }

export function getDrag() {
  return drag;
}

let hoverDot = null;
let hoverTimer = null;

function clearHoverTimer() {
  if (hoverTimer) clearTimeout(hoverTimer);
  hoverTimer = null;
  hoverDot = null;
}

function findDropTarget(x, y) {
  const el = document.elementFromPoint(x, y);
  if (!el) return { type: "outside" };

  const keyEl = el.closest("[data-drop-key]");
  if (keyEl) {
    return { type: "key", pageId: keyEl.dataset.pageId, pos: Number(keyEl.dataset.pos) };
  }
  const dotEl = el.closest("[data-drop-screen-dot]");
  if (dotEl) return { type: "screen-dot", screen: Number(dotEl.dataset.screen) };

  return el.closest("[data-drop-bezel]") ? { type: "bezel-empty" } : { type: "outside" };
}

/** Hovering a dot for a moment flips to that screen, so a drag can cross screens. */
function updateAutoFlip(target) {
  if (target?.type !== "screen-dot") {
    clearHoverTimer();
    return;
  }
  if (hoverDot === target.screen) return;
  clearHoverTimer();
  hoverDot = target.screen;
  hoverTimer = setTimeout(() => selectScreen(target.screen), AUTO_FLIP_MS);
}

export function startCatalogueDrag(item, x, y) {
  drag = { kind: "catalogue", item, x, y, target: null };
}

export function startKeyDrag(deckId, pageId, pos, keySnapshot, x, y) {
  select(pos);
  drag = { kind: "key", deckId, from: { pageId, pos }, key: keySnapshot, x, y, target: null };
}

export function onDragMove(deckId, x, y) {
  if (!drag) return;
  drag.x = x;
  drag.y = y;
  drag.target = findDropTarget(x, y);
  updateAutoFlip(drag.target);
}

function keyAt(deckId, pageId, pos) {
  const scope = resolveScope(deckId, pageId);
  return scope?.keys.find((k) => k.pos === pos) ?? emptyKey(pos);
}

export function endDrag(deckId, ctrlKey) {
  if (!drag) return;
  clearHoverTimer();
  const target = drag.target;

  if (drag.kind === "catalogue") {
    if (target?.type === "key") {
      dropCatalogueItem(deckId, target.pageId, target.pos, drag.item);
    }
  } else if (drag.kind === "key") {
    const from = drag.from;
    if (target?.type === "key") {
      const to = { pageId: target.pageId, pos: target.pos };
      if (to.pos !== from.pos || to.pageId !== from.pageId) {
        const targetKey = keyAt(deckId, target.pageId, target.pos);
        if (targetKey.kind === "folder") {
          // Dropping ONTO a folder key means "into the page it opens", not "replace it".
          moveKey(deckId, from, { pageId: targetKey.target, pos: firstFree(deckId, targetKey.target) });
          enterPage(targetKey.target);
        } else if (targetKey.kind === "empty") {
          if (ctrlKey) copyKey(deckId, from, to);
          else moveKey(deckId, from, to);
        } else {
          swapKeys(deckId, from, to);
        }
      }
    } else if (target?.type === "screen-dot") {
      moveKeyToScreen(deckId, from, target.screen);
    } else if (target?.type === "outside") {
      emptyKeyAt(deckId, from.pageId, from.pos);
    }
    // 'bezel-empty': ambiguous drop, treated as a cancel.
  }

  drag = null;
}

/** Lowest unoccupied position of a page — where a key dropped into it lands. */
function firstFree(deckId, pageId) {
  const taken = new Set(resolveScope(deckId, pageId)?.keys.map((k) => k.pos) ?? []);
  let pos = 0;
  while (taken.has(pos)) pos += 1;
  return pos;
}

export function cancelDrag() {
  clearHoverTimer();
  drag = null;
}
