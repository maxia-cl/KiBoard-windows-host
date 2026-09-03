// Product analytics intentionally carries identifiers only. Never pass labels, filenames,
// window titles, typed text, pairing data or action strings through this bridge.
export function trackInteraction(interaction) {
  const api = typeof window !== "undefined" ? window.__TAURI__?.core : undefined;
  if (!api) return;
  api.invoke("track_ui_interaction", { interaction }).catch(() => {
    // Analytics must never interrupt the interaction it is measuring.
  });
}

/**
 * One delegated listener covers controls rendered later by Svelte as well as today's controls.
 * Every tracked element owns a fixed `data-telemetry` identifier, keeping user-visible text out
 * of Aptabase. Change events cover selects, toggles and completed field edits without recording
 * every character typed.
 */
export function installInteractionTracking(root = document) {
  function send(event) {
    const target = event.target?.closest?.("[data-telemetry]");
    if (!target || target.disabled) return;
    // Native form controls emit click and then change for one user action. The completed change
    // is the useful event and avoids double-counting toggles, selects and field edits.
    if (event.type === "click" && target.matches("input, select, textarea")) return;
    trackInteraction(target.dataset.telemetry);
  }

  root.addEventListener("click", send);
  root.addEventListener("change", send);
  return () => {
    root.removeEventListener("click", send);
    root.removeEventListener("change", send);
  };
}
