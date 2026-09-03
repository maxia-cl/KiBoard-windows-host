// The wire shape, unchanged: KiBoard-protocol/protocol/README.md §3-4.1.
//
// F5 removed two editor-only inventions from phase FP. `folderId` is gone — a folder key is a
// key of kind "folder" whose `target` names another `Page` of the same deck, which is what the
// host has resolved since F2. And `targetPage` is gone for the same reason: `target` already
// names a page id.

/**
 * The grid the editor draws — deliberately THE PHONE'S, not a Stream Deck preset.
 *
 * The editor used to author on 5x3 while the phone drew 5x2, so the same deck was fifteen keys
 * and five screens on the desk but ten keys and seven screens in the hand. Both were "right" and
 * the pair was confusing, which defeats the point of a WYSIWYG editor.
 *
 * Kept in step with `KiBoard-app/lib/ui/deck/adaptive_grid.dart`, where the phone fixes 5 keys
 * along the long edge and 3 along the short one. ponytail: two constants in two repositories
 * rather than a protocol round trip for two integers — `deck-tokens.json` never carried this
 * number, and a `gridPresets` entry would mean three pull requests and a re-tag to change it.
 * Nothing breaks if they drift, the counts simply stop matching again.
 */
export const AUTHORING_GRID = { rows: 3, cols: 5 };
// Android owns the last three cells for the foreground-app panel. Drawing twelve authored keys
// here makes Manual's preview paginate exactly as the phone instead of showing three phantom keys.
export const RESERVED = 3;
export const SCREEN = AUTHORING_GRID.rows * AUTHORING_GRID.cols - RESERVED;

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
