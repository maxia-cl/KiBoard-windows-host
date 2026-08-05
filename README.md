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

**FP through F6 done, F7 part-way.** `KiBoard-protocol` is pinned as a git submodule at
`KiBoard-protocol/`, tag `v0.3.0-f7`; `npm run dev`/`build` regenerate `src/tokens.g.css` from it
automatically.

- **F0/F1** — the v1 host was ported from `ricardomendezv/Kiboard` with a `git subtree split` (full
  history) and modularized into `config.rs`, `net/`, `engine/`, `platform/`, `integrations/obs.rs`
  with no behaviour change. Then real mDNS (`net/discovery.rs`) and v2 pairing by six-digit code,
  with per-device revocable tokens. A v1-shaped `hello` is rejected with `protocol_too_old`.
- **F2/F3** — the Deck/Page/Key model, repagination onto whatever grid the client declares, and
  §4.2's rule that the phone sends a POSITION and never an action. Sessions persist; the host
  translates es/en/zh.
- **F4** — the app catalogue from `Get-StartApps`, a generated **Launcher** deck, `launch:`,
  `focus:` and `kill:`. Packaged apps need a second identity (the window's AppUserModel ID), since
  every one of their windows is owned by `ApplicationFrameHost.exe`.
- **F5** — the editor on real data: `get_decks`/`save_decks`/`app_catalogue` hand back the same
  `Deck` struct that travels on the wire, and unsaved decks reach the phone live behind one
  accessor used for rendering *and* press resolution.
- **F6** — Elgato parity, and past it: two-state keys resolved host-side, action chains, custom
  images, share by file. For `obs:` keys **OBS owns the face** — a scene key lights up only while
  it is on air, and the deck repaints when OBS changes with nobody pressing anything.
- **F7 so far** — the pairing panel and `pairing_status` show `ip:port`, so a phone on a network
  that drops multicast can be told what to type. And the transport is now **`wss://`** (§2.2): one
  self-signed certificate per installation in `cert.der`/`key.der` next to `config.json`, which the
  client pins. **There is no plaintext fallback and this is a breaking change** — host and phone
  must be rebuilt and installed together.

Still open in F7: first-run onboarding, the updater/signing/store listings, telemetry on the
pairing funnel, and the QR half of the manual-address fallback (the phone has no scanner yet).

41 tests, clippy clean.

## Releases and updates

Builds are published to
[`KiBoard-windows-host-releases`](https://github.com/maxia-cl/KiBoard-windows-host-releases), which
is public because the updater needs a feed it can read without a token. The updater endpoint in
`src-tauri/tauri.conf.json` points there.

**Two things are still v1's and must be replaced before the first v2 release:**

1. **The signing key.** `plugins.updater.pubkey` is still the key KiBoard v1 signs with. A v2
   release signed with a v2 key would be rejected by it. Generate one, keep the private half out of
   this repo, and paste the public half into `tauri.conf.json`:

   ```bash
   npx tauri signer generate -w kiboard2-updater.key
   ```

   The private key and its password become the `TAURI_SIGNING_PRIVATE_KEY` and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets of whatever builds the release. Losing them means
   no installed host can ever update again — there is no recovery path, only a manual reinstall.

2. **A token for the releases repo.** `.github/workflows/release.yml` publishes into
   `KiBoard-windows-host-releases`, a different repository, which the default `GITHUB_TOKEN` cannot
   write to. It needs a `RELEASES_TOKEN` secret with contents write on that repo.

The workflow builds on a `v*` tag, writes `latest.json` — the feed the updater reads — and opens the
release as a **draft**, so nothing goes public until someone looks at it. It refuses to run at all
without the signing key, rather than publishing a bundle no installed host would accept.

**The version is `2.0.0`** in both `Cargo.toml` and `tauri.conf.json`, and the phone app matches.
`0.1.29` was where v1's numbering stopped and came across in the subtree port; host and phone share
a number because `wss://` means they have to be installed together anyway.

Until the key exists the updater simply finds nothing, which is the safe failure. The endpoint used
to point at v1's feed — that one was **not** safe: the signature matched, so a v2 host could have
installed v1 over itself.

## Stack

Tauri 2 + Rust, editor UI in Svelte 5 + Vite. Windows first; platform-specific code is isolated
under `platform/` from day one, so macOS and Linux can follow later, each in its own repo, without
unpicking it.

## Conventions

All code is English — identifiers, comments, commits, log output. User-facing strings go through
`i18n`, never inline. Documents ship in English and Spanish (`NAME.md` / `NAME.es.md`).
See [`CONTRIBUTING.md`](https://github.com/maxia-cl/KiBoard-protocol/blob/main/CONTRIBUTING.md).
