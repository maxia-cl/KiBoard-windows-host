// The glyph vocabulary, mirrored in KiBoard-app/lib/ui/icons.dart so a key looks the same in the
// editor as it does on the phone.
//
// It used to hold 22 names while the profiles referenced 93, so most keys drew the ⬛ fallback on
// both sides. `icons_cover_every_profile` in config.rs now fails the build if a profile names one
// that is missing here.
const GLYPHS = {
  // --- surfaces and navigation ---
  app: "\u{1F4E6}",
  apps: "\u{1F5C3}️",
  deck: "\u{1F5C4}️",
  work: "\u{1F4BC}",
  windows: "\u{1FA9F}",
  folder: "\u{1F4C1}",
  newfolder: "\u{1F4C2}",
  home: "\u{1F3E0}",
  back: "←",
  fwdnav: "→",
  prev: "‹",
  next: "›",
  page: "⏭️",
  scrollup: "▲",
  scrolldown: "▼",
  tab: "⇥",
  find: "\u{1F50E}",
  pin: "\u{1F4CC}",
  history: "\u{1F551}",
  mode: "\u{1F504}",
  macro: "\u{1F3AC}",
  settings: "⚙️",
  terminal: "\u{1F5A5}️",

  // --- editing ---
  new: "＋",
  copy: "\u{1F4CB}",
  paste: "\u{1F4CE}",
  cut: "✂️",
  duplicate: "\u{1F5D0}️",
  delete: "\u{1F5D1}️",
  undo: "↩️",
  redo: "↪️",
  save: "\u{1F4BE}",
  rename: "\u{1F58A}️",
  selectall: "▦",
  cursor: "\u{1F5B1}️",
  move: "✥",
  group: "\u{1F5C3}️",
  replace: "⇄",
  refresh: "\u{1F503}",
  repeat: "\u{1F501}",
  shuffle: "\u{1F500}",
  filter: "\u{1F53D}",
  sum: "∑",
  print: "\u{1F5A8}️",

  // --- text ---
  text: "\u{1F524}",
  bold: "\u{1D401}",
  italic: "\u{1D466}",
  underline: "U̲",
  format: "≡",
  highlight: "\u{1F58D}️",
  note: "\u{1F5D2}️",
  comment: "\u{1F4AC}",
  link: "\u{1F517}",

  // --- drawing ---
  brush: "\u{1F58C}️",
  pencil: "✏️",
  eraser: "\u{1F9F9}",
  fill: "\u{1FAA3}",
  palette: "\u{1F3A8}",
  colorpick: "\u{1F489}",
  crop: "⛶",
  frame: "\u{1F5BC}️",
  rect: "▭",
  ellipse: "◯",
  line: "─",
  rotate: "\u{1F5D8}️",
  layers: "\u{1F5C2}️",
  opacity: "◐",
  dark: "⬤",
  light: "◯",

  // --- view ---
  zoom: "\u{1F50D}",
  zoomin: "\u{1F50D}",
  zoomout: "\u{1F50E}",
  fullscreen: "⛶",
  screenshot: "\u{1F4F8}",
  hand: "✋",
  mouse: "\u{1F5B1}️",

  // --- media and capture ---
  obs: "\u{1F3A5}",
  video: "\u{1F4FA}",
  record: "⏺️",
  stream: "\u{1F4E1}",
  clip: "\u{1F3AC}",
  play: "▶️",
  subtitles: "\u{1F4AC}",
  mic: "\u{1F3A4}",
  mute: "\u{1F507}",
  vol: "\u{1F50A}",
  volume: "\u{1F50A}",

  // --- messages and people ---
  send: "➤",
  reply: "↩️",
  replyall: "↩️",
  forward: "➡️",
  share: "\u{1F517}",
  archive: "\u{1F4E6}",
  people: "\u{1F465}",
  assign: "\u{1F464}",
  login: "\u{1F511}",
  logout: "\u{1F6AA}",
  calendar: "\u{1F4C5}",
  star: "★",

  // --- transfer ---
  upload: "⬆️",
  download: "⬇️",

  // --- outcomes ---
  accept: "✓",
  close: "✕",
  // Claude Code's own two: which model is answering, and how hard it is being asked to think.
  model: "\u{1F9E0}",
  effort: "\u{1F39A}️",
};

export function iconGlyph(name) {
  return GLYPHS[name] ?? "⬛";
}

/// Every name that has a glyph — the icon picker offers these.
export const iconNames = Object.keys(GLYPHS);
