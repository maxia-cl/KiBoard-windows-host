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
  return await invoke("get_decks");
}

/** Resolves to `{ ok }` or `{ ok: false, error }` — the host validates before it writes. */
export async function saveDecks(decks) {
  return await invoke("save_decks", { decks });
}

/** Shows UNSAVED decks on every phone in manual mode. Validated host-side, same as a save. */
export async function previewDecks(decks) {
  return await invoke("preview_decks", { decks });
}

/** Drops the preview: the phones go back to what is on disk. */
export async function clearPreview() {
  return await invoke("clear_preview");
}

/** The machine's installed apps (F4), each with its real icon already as a `data:` URI. */
export async function loadAppCatalogue() {
  return await invoke("app_catalogue");
}

/** Runs an action for real, on this desktop. The ▶ Test button; nothing is simulated. */
export async function testAction(action) {
  return await invoke("test_action", { action });
}

/** Live OBS scenes, so the catalogue offers the user's own instead of two hardcoded samples. */
export async function loadObsScenes() {
  return await invoke("obs_scenes");
}
