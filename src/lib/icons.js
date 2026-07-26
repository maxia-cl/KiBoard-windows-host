// Placeholder glyph vocabulary. Production replaces this with a real icon set (F5); FP only
// needs something legible enough to validate layout and drag interactions.
const GLYPHS = {
  app: "\u{1F4E6}",
  brush: "\u{1F58C}️",
  crop: "✂️",
  undo: "↩️",
  redo: "↪️",
  save: "\u{1F4BE}",
  layers: "\u{1F5C2}️",
  opacity: "\u{1F3A8}",
  zoom: "\u{1F50D}",
  screenshot: "\u{1F4F8}",
  windows: "\u{1FA9F}",
  mic: "\u{1F3A4}",
  volume: "\u{1F50A}",
  mode: "\u{1F504}",
  folder: "\u{1F4C1}",
  back: "←",
  page: "⏭️",
  obs: "\u{1F3A5}",
  macro: "⚙️",
  close: "✕",
};

export function iconGlyph(name) {
  return GLYPHS[name] ?? "⬛";
}
