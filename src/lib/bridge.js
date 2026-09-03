// TauriBridge (F5): the editor's only door to the host. Replaces phase FP's in-memory mock
// (docs/implementation-plan.md §5) — the component tree above the store does not know it changed.
//
// Everything here speaks the WIRE shape (KiBoard-protocol §3): `Deck { id, name, icon, pages }`,
// `Page { id, name, keys }`, `Key { pos, kind, action, target, ... }`. No translation layer, on
// purpose: a shape only the editor understood is a shape that drifts from the protocol.

// Straight to the compiled host via `window.__TAURI__` (withGlobalTauri in tauri.conf.json), not
// the npm package — which is not a dependency here. Only meaningful inside the real Tauri window;
// `vite dev` in a plain browser has no host to talk to.
export function invoke(cmd, args) {
  const api = typeof window !== "undefined" ? window.__TAURI__?.core : undefined;
  if (!api) return Promise.reject(new Error("Not running inside the Tauri window"));
  return api.invoke(cmd, args);
}

export function isTauri() {
  return typeof window !== "undefined" && !!window.__TAURI__;
}

export async function loadDecks() {
  if (!isTauri()) {
    const rows = [
      ["Copiar", "copy", "ctrl+c"], ["Pegar", "paste", "ctrl+v"],
      ["Cortar", "cut", "ctrl+x"], ["Deshacer", "undo", "ctrl+z"],
      ["Rehacer", "redo", "ctrl+y"], ["Sel. todo", "selectall", "ctrl+a"],
      ["Captura", "screenshot", "screenshot"], ["Trackpad", "mouse", "trackpad"],
      ["Dictar", "mic", "dictate"],
    ];
    return [{
      id: "starter", name: "KiBoard", icon: "deck",
      pages: [{
        id: "p0", name: "",
        keys: rows.map(([label, icon, action], pos) => ({ pos, label, icon, action, kind: "action" })),
      }],
    }];
  }
  return await invoke("get_decks");
}

/** The current automatic layout, rendered for the same 3x5 landscape surface as Android. */
export async function loadAutoPreview(page = 0) {
  return await invoke("auto_preview", { page });
}

/** Resolves to `{ ok }` or `{ ok: false, error }` — the host validates before it writes. */
export async function saveDecks(decks) {
  if (!isTauri()) return { ok: true };
  return await invoke("save_decks", { decks });
}

/** Shows UNSAVED decks on every phone in manual mode. Validated host-side, same as a save. */
export async function previewDecks(decks) {
  if (!isTauri()) return { ok: true };
  return await invoke("preview_decks", { decks });
}

/** Drops the preview: the phones go back to what is on disk. */
export async function clearPreview() {
  if (!isTauri()) return;
  return await invoke("clear_preview");
}

/** The machine's installed apps (F4), each with its real icon already as a `data:` URI. */
export async function loadAppCatalogue() {
  if (!isTauri()) {
    return [
      { id: "Google.Chrome", name: "Google Chrome" },
      { id: "OpenAI.Codex", name: "Codex" },
      { id: "Microsoft.WindowsNotepad", name: "Bloc de notas" },
    ];
  }
  return await invoke("app_catalogue");
}

/** Runs an action for real, on this desktop. The ▶ Test button; nothing is simulated. */
export async function testAction(action) {
  return await invoke("test_action", { action });
}

/** Live OBS scenes, so the catalogue offers the user's own instead of two hardcoded samples. */
export async function loadObsScenes() {
  if (!isTauri()) return [];
  return await invoke("obs_scenes");
}

/** Whether OBS is available right now; keeps an irrelevant integration out of Manual. */
export async function loadObsInfo() {
  if (!isTauri()) return { running: false, connected: false };
  return await invoke("obs_info");
}

/**
 * Writes a deck to `<config>/decks/<id>.kbdeck.json` and opens Explorer on it (F6).
 *
 * The SAVED deck, not the one being edited: exporting an unsaved draft would hand someone a file
 * that does not match what this machine runs. Resolves to `{ ok, path }`.
 */
export async function exportDeck(deckId) {
  return await invoke("export_deck", { deckId });
}
