# Code signing policy

[Español](CODE_SIGNING.es.md) · **English**

KiBoard's public-release policy is to use **free code signing provided by SignPath.io,
certificate by SignPath Foundation**. No Windows release is considered production-ready until
its executable and every installer pass the repository's trusted Authenticode verification.

## Scope and provenance

- Source code is published under the MIT license in
  [`maxia-cl/KiBoard-windows-host`](https://github.com/maxia-cl/KiBoard-windows-host).
- Release candidates are built from public `v*` tags by GitHub Actions. The workflow checks out
  the pinned public protocol repository, builds the Tauri application and retains the release as a
  draft until verification is complete.
- `tool/verify-authenticode.ps1` rejects an executable, NSIS installer, or MSI whose trusted
  Authenticode status is not `Valid`.
- Tauri updater signatures are applied after Authenticode signing. They protect the update feed
  but do not replace the Windows publisher signature.

## Team roles

- Committers and reviewers: members of the
  [`maxia-cl` organization](https://github.com/orgs/maxia-cl/people).
- Signing approvers: owners of the
  [`maxia-cl` organization](https://github.com/orgs/maxia-cl/people?query=role%3Aowner).

Maintainers use multi-factor authentication. A maintainer must review the source, CI result,
release notes, artifact hashes, and privacy disclosures before approving a signing request.

## Privacy and security

KiBoard's [privacy policy](PRIVACY.md) describes the optional anonymous interaction analytics and
the local-network data exchanged with a paired Android device. Analytics can be disabled in the
application settings. KiBoard does not sign third-party binaries as its own, and it does not use
its signing access for unrelated projects.

If a signed artifact cannot be reproduced from the public tagged source, fails malware scanning,
or does not match these disclosures, maintainers must reject or revoke the release.

