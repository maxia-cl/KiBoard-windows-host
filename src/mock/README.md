# Mock data (phase FP only)

These files are **copied**, not pinned, from `KiBoard-protocol/{deck-tokens.json,fixtures/}`. That
is a deliberate FP shortcut (see `docs/implementation-plan.md` in `KiBoard-protocol`): the real
submodule pin and the generated-at-build-time tokens land in **F0**. When F0 lands:

- Delete this folder.
- Add `KiBoard-protocol` as a git submodule.
- Generate `src/tokens.g.css` from the pinned `deck-tokens.json` via
  `node <submodule>/generate-tokens.mjs <dart-out> src/tokens.g.css` as a Vite prebuild step.
- Replace `MockBridge` with `TauriBridge` (F5) and the copied fixtures with real Tauri commands.

Until then, `mock/deck-tokens.json` and `mock/fixtures/*.json` are the single source these
components read from — copying them again from `KiBoard-protocol` is the only maintenance this
folder needs.
