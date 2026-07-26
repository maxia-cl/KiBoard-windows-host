// Shapes mirror the wire `layout` message in KiBoard-protocol/protocol/README.md §3-4.1,
// plus two editor-only fields that never cross the wire: `folderId` (kind "folder") and
// `targetPage` (kind "page"), which tell the mock host which local page to open on press.

import tokens from "../mock/deck-tokens.json";

export const GRID_PRESETS = tokens.gridPresets;

export function gridFor(presetName) {
  const preset = GRID_PRESETS[presetName] ?? GRID_PRESETS.mk2;
  return { rows: preset.rows, cols: preset.cols };
}

export function capacityOf(presetName) {
  const { rows, cols } = gridFor(presetName);
  return rows * cols;
}

export function emptyKey(pos) {
  return { pos, kind: "empty" };
}

/** Pads/truncates a sparse key list into a dense array of the given capacity, indexed by `pos`. */
export function denseKeys(keys, capacity) {
  const dense = Array.from({ length: capacity }, (_, pos) => emptyKey(pos));
  for (const key of keys) {
    if (key.pos < capacity) dense[key.pos] = key;
  }
  return dense;
}

export function isOccupied(key) {
  return key && key.kind !== "empty";
}
