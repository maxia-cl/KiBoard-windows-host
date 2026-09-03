# KiBoard for Windows privacy policy

Effective date: 3 September 2026

KiBoard Host connects an Android phone or tablet to a Windows PC and uses anonymous analytics to
understand feature usage and find workflows that need improvement. KiBoard does not show ads,
create accounts, or sell data.

## Usage analytics

The host sends events to Aptabase when the application starts and when the user interacts with
KiBoard. Events may describe:

- the interaction type, such as pressing a deck key, changing page or mode, opening Launcher,
  using the trackpad, dictating, pairing a device, or editing a deck;
- non-identifying functional context, such as automatic/manual mode, key position, press type,
  action category, a built-in surface identifier, orientation, grid size, and accepted or rejected
  result; custom profiles are grouped into a single category;
- KiBoard version, Windows language, operating system, debug/release build, and a random identifier
  that lasts only for the current host process.

KiBoard **does not send** Aptabase application or window names, device names, deck or profile names,
custom labels or actions, typed or dictated text, audio, local addresses, pairing codes or tokens,
or certificates.

The configured endpoint uses Aptabase's United States region. Aptabase states that it does not use
persistent device identifiers and generates a daily server-side identifier from the IP address,
user agent, and a rotating salt. It also states that events are retained for up to five years. See
<https://aptabase.com/legal/privacy>.

## Local data and connection

Paired devices, decks, profiles, settings, and tokens are stored locally on the PC. Phone commands
travel over the encrypted local-network connection to the host to perform the requested action.
This information is not included in the analytics events described above.

## Contact and changes

Privacy questions can be opened at
<https://github.com/maxia-cl/KiBoard-windows-host/issues>. Material changes will be published in
this repository with a new effective date.
