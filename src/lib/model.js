// The wire shape, unchanged: KiBoard-protocol/protocol/README.md §3-4.1.
//
// F5 removed two editor-only inventions from phase FP. `folderId` is gone — a folder key is a
// key of kind "folder" whose `target` names another `Page` of the same deck, which is what the
// host has resolved since F2. And `targetPage` is gone for the same reason: `target` already
// names a page id.

import tokens from "../../KiBoard-protocol/protocol/deck-tokens.json";

/**
 * The authoring grid. Decks are written against the reference device and the host repaginates
 * them onto whatever grid each client declares in `hello`, so this is a convention for drawing,
 * never a limit on how many keys a page may hold — the same 5x3 the host means by REFERENCE_PAGE.
 */
export const AUTHORING_GRID = tokens.gridPresets.mk2;
export const SCREEN = AUTHORING_GRID.rows * AUTHORING_GRID.cols;

export function emptyKey(pos) {
  return { pos, kind: "empty" };
}

/** How many screens of `SCREEN` keys a page occupies. At least one, so an empty page still draws. */
export function screensOf(keys) {
  const last = keys.reduce((max, k) => Math.max(max, k.pos), -1);
  return Math.max(1, Math.floor(last / SCREEN) + 1);
}

/**
 * One screen of a page as a dense array of `SCREEN` keys, indexed from 0.
 *
 * This is the editor drawing exactly what the phone draws: a page is ONE long list of keys and
 * both sides cut it into screens. `pos` stays absolute — it is the address the phone sends back,
 * so it can never be rewritten to a per-screen index.
 */
export function screenKeys(keys, screen) {
  const base = screen * SCREEN;
  const dense = Array.from({ length: SCREEN }, (_, i) => emptyKey(base + i));
  for (const key of keys) {
    if (key.pos >= base && key.pos < base + SCREEN) dense[key.pos - base] = key;
  }
  return dense;
}

export function isOccupied(key) {
  return key && key.kind !== "empty";
}
