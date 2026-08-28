# Changelog

**English** · [Español](CHANGELOG.es.md)

Notable changes to the KiBoard host. The phone app has its own; the message contract is versioned
separately in [`KiBoard-protocol`](https://github.com/maxia-cl/KiBoard-protocol).

Nothing here has been released yet — v2 has never been published, so the versions below are the
implementation phases, not release tags. The first release also needs its own signing key and its
own version number (see "Releases and updates" in the README).

## 2.0.0 (unreleased) — protocol `v0.3.0-f7`

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
- The generated Launcher now keeps only apps used in the rolling last 30 days and orders them by
  most recent use. KiBoard persists foreground activity locally so the order keeps improving after
  the first run, while custom or deleted Launcher decks remain untouched.
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
