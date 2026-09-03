# Changelog

**English** · [Español](CHANGELOG.es.md)

Notable changes to the KiBoard host. The phone app has its own; the message contract is versioned
separately in [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol).

## 2.0.1 (2026-09-03) — protocol `v0.5.0`

### Added

- Anonymous, optional interaction analytics with a fixed safe vocabulary and no user content.
- A Windows automatic-deck preview that mirrors the live Android layout.
- Shared expressive deck icons and high-resolution application artwork.

### Changed

- Manual mode edits only custom KiBoard decks; the generated Launcher remains automatic.
- Launcher keeps visual apps used in the last 30 days and orders the most recent first.
- Choosing or opening an app from Launcher returns directly to that app's automatic deck.
- Manual configuration is reduced to the actions common users actually need.

### Fixed

- Release installs can no longer start a development-server binary.
- The updater manifest uses Tauri's supported NSIS preference.

## 2.0.0 (2026-09-02) — protocol `v0.3.0-f7`

The first version number of KiBoard 2. `0.1.29` was where v1's numbering stopped and it came
across in the subtree port; the host and the phone app share `2.0.0` because `wss://` means they
have to be installed together anyway.

### Breaking

- **The transport is `wss://`.** Everything used to cross the LAN in clear text, including the token
  in `hello`, which is the whole of a device's authority. The host now mints one self-signed
  certificate per installation into `cert.der`/`key.der` next to `config.json` and serves TLS with
  it; mDNS advertises `tls = 1`. **There is no plaintext fallback**, and an old phone cannot talk to
  a new host or the reverse — both have to be rebuilt and installed together.
- The certificate is per-installation state, like `host_id`. Regenerating it locks out every paired
  device and looks like an attack to each of them.

### Added

- **First run explains itself** (B1): with nothing paired, the pairing panel opens by itself and
  states the three steps, including the Windows network prompt that costs you mDNS if dismissed.
- **The PC says where it is** (R1): the pairing panel and `pairing_status` show `ip:port`, so a
  phone on a network that drops multicast can be told what to type.
- **OBS owns the face of an OBS key**: a scene key lights up only while it is on air, and the deck
  repaints when OBS changes with nobody pressing anything.
- Two-state keys, action chains, custom images and share-by-file (Elgato parity).
- The app catalogue from `Get-StartApps`, a generated **Launcher** deck, and `launch:` / `focus:` /
  `kill:`.
- The editor on real data, with live preview to the phone of decks that are not saved yet.

### Fixed

- KiBoard no longer rewrites its Windows startup registry entry on every launch, and registration
  errors are now reported instead of silently ignored.
- The Launcher deck never reached anyone who already had a config: the seeding was guarded by
  `decks.is_empty()`, which silently skips every existing user. Backfilled, and recorded even when
  nothing is added so a deleted Launcher stays deleted.
- `danger` keys were painted red but never actually asked, so one mis-tap closed the foreground app.
- 81 of the 93 icon names the default profiles referenced were never drawn, and fell through to a
  blank square. Both vocabularies are now 105 names and a test fails if they ever drift again.
- `cargo test` used to rewrite the developer's live config.

### Changed

- The updater endpoint pointed at **v1's** release feed, whose signature would have verified — a v2
  host could have installed v1 over itself. It now points at
  `maxia-cl/KiBoard-windows-host-releases`.
