// Native Pointer Events drag engine (docs/implementation-plan.md §3.4). Deliberately not the
// HTML5 drag-and-drop API: that gives no control over the ghost and fights `transform`, which
// the bezel/stand tilt (§3.0) relies on.
import {
  moveKey,
  copyKey,
  swapKeys,
  moveKeyToPage,
  moveKeyToFolder,
  emptyKeyAt,
  dropCatalogueItem,
  dropCatalogueItemOnPageDot,
  select,
  selectPage,
  resolveScope,
} from "./store.svelte.js";

const AUTO_PAGE_FLIP_MS = 600;

let drag = $state(null); // { kind: 'catalogue' | 'key', item?, from?, key?, x, y, target }

export function getDrag() {
  return drag;
}

let hoverDotIndex = null;
let hoverTimer = null;

function clearHoverTimer() {
  if (hoverTimer) clearTimeout(hoverTimer);
  hoverTimer = null;
  hoverDotIndex = null;
}

function findDropTarget(x, y) {
  const el = document.elementFromPoint(x, y);
  if (!el) return { type: "outside" };

  const keyEl = el.closest("[data-drop-key]");
  if (keyEl) {
    return {
      type: "key",
      pageIndex: keyEl.dataset.pageIndex !== "" ? Number(keyEl.dataset.pageIndex) : null,
      folderId: keyEl.dataset.folderId || null,
      pos: Number(keyEl.dataset.pos),
    };
  }
  const dotEl = el.closest("[data-drop-page-dot]");
  if (dotEl) return { type: "page-dot", pageIndex: Number(dotEl.dataset.pageIndex) };

  const bezelEl = el.closest("[data-drop-bezel]");
  if (!bezelEl) return { type: "outside" };
  return { type: "bezel-empty" };
}

function updateAutoFlip(target, deckId) {
  if (target?.type !== "page-dot") {
    clearHoverTimer();
    return;
  }
  if (hoverDotIndex === target.pageIndex) return;
  clearHoverTimer();
  hoverDotIndex = target.pageIndex;
  hoverTimer = setTimeout(() => {
    selectPage(target.pageIndex);
  }, AUTO_PAGE_FLIP_MS);
}

export function startCatalogueDrag(item, x, y) {
  drag = { kind: "catalogue", item, x, y, target: null };
}

export function startKeyDrag(deckId, pageIndex, folderId, pos, keySnapshot, x, y) {
  select(pos);
  drag = { kind: "key", deckId, from: { pageIndex, folderId, pos }, key: keySnapshot, x, y, target: null };
}

export function onDragMove(deckId, x, y) {
  if (!drag) return;
  drag.x = x;
  drag.y = y;
  drag.target = findDropTarget(x, y);
  updateAutoFlip(drag.target, deckId);
}

export function endDrag(deckId, ctrlKey) {
  if (!drag) return;
  clearHoverTimer();
  const target = drag.target;

  if (drag.kind === "catalogue") {
    if (target?.type === "key") {
      dropCatalogueItem(deckId, target.pageIndex, target.folderId, target.pos, drag.item);
    } else if (target?.type === "page-dot") {
      dropCatalogueItemOnPageDot(deckId, target.pageIndex, drag.item);
    }
  } else if (drag.kind === "key") {
    const from = drag.from;
    if (target?.type === "key") {
      const to = { pageIndex: target.pageIndex, folderId: target.folderId, pos: target.pos };
      const sameSlot = to.pos === from.pos && to.folderId === from.folderId && to.pageIndex === from.pageIndex;
      if (!sameSlot) {
        const scope = resolveScope(deckId, target.pageIndex, target.folderId);
        const targetKey = scope.keys[target.pos];
        if (targetKey.kind === "folder") {
          moveKeyToFolder(deckId, from, targetKey.folderId);
        } else if (targetKey.kind === "empty") {
          if (ctrlKey) copyKey(deckId, from, to);
          else moveKey(deckId, from, to);
        } else {
          swapKeys(deckId, from, to);
        }
      }
    } else if (target?.type === "page-dot") {
      moveKeyToPage(deckId, from, target.pageIndex);
    } else if (target?.type === "outside") {
      emptyKeyAt(deckId, from.pageIndex, from.folderId, from.pos);
    }
    // 'bezel-empty': ambiguous drop, treated as a cancel.
  }

  drag = null;
}

export function cancelDrag() {
  clearHoverTimer();
  drag = null;
}
