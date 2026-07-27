# KiBoard — Windows host

**English** · [Español](README.es.md)

The PC side. A Tauri 2 + Rust application that runs in the tray, **advertises itself on the local
network** over mDNS, detects the foreground app, executes actions, and serves layouts to paired
phones.

It also contains the **visual deck editor**: a drawn Stream Deck the user configures by dragging
apps and actions onto keys.

## What lives here

- **Service** — mDNS advertisement, WebSocket server, pairing by six-digit code with one
  revocable token per device.
- **Detection** — foreground app, shell detection from the process tree, UI Automation.
- **Execution** — the macro DSL (chained steps, hotkeys, typed text, UIA presses, OBS, volume,
  mouse), screenshots, launching and focusing installed apps.
- **App catalogue** — enumerates Win32 and UWP apps through `shell:AppsFolder`, with real icons.
- **Editor** — Svelte 5 UI with drag and drop, key inspector and live preview to the phone.

## Related repositories

| Repository | What it is | Visibility |
|---|---|---|
| [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol) | Message contract, visual tokens, fixtures, project docs | Private |
| [`KiBoard-app`](https://github.com/maxia-cl/KiBoard-app) | Mobile app | Private |
| [`KiBoard-windows-host-releases`](https://github.com/maxia-cl/KiBoard-windows-host-releases) | Installers and update feed | Public |

`KiBoard-protocol` is the **source of truth**. Change it first, then this repo.
Builds are published to `KiBoard-windows-host-releases`, never here — this repo stays private and
the updater needs a public feed.

## Status

**F0 done.** The v1 host (`src-tauri/`) was ported from `ricardomendezv/Kiboard` via a
`git subtree split` (full history preserved) and modularized into `config.rs`, `net/`, `engine/`,
`platform/`, `integrations/obs.rs` with no behavior change — verified against a real WebSocket
handshake using the v1 protocol. `KiBoard-protocol` is pinned as a git submodule at
`KiBoard-protocol/` (tag `v0.1.0-fp`); `npm run dev`/`build` regenerate `src/tokens.g.css` from it
automatically.

The **phase FP editor mock-up** — the drawn device, the searchable catalogue, the key inspector
and all eight drag-and-drop operations, including undo/redo and the double-click/arrow-key
accessible path — still runs against an in-memory `MockBridge`, now reading fixtures straight from
the submodule instead of a local copy. Verified interactively in a running `vite dev` session. Real
Tauri wiring (`TauriBridge` replacing `MockBridge`) is F5. See the implementation plan in
`KiBoard-protocol`.

## Stack

Tauri 2 + Rust, editor UI in Svelte 5 + Vite. Windows first; platform-specific code is isolated
under `platform/` from day one, so macOS and Linux can follow later, each in its own repo, without
unpicking it.

## Conventions

All code is English — identifiers, comments, commits, log output. User-facing strings go through
`i18n`, never inline. Documents ship in English and Spanish (`NAME.md` / `NAME.es.md`).
See [`CONTRIBUTING.md`](https://github.com/maxia-cl/KiBoard-protocol/blob/main/CONTRIBUTING.md).
