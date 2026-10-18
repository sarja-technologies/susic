# Releasing and publication checklist

## Licence

The repository is dedicated to the public domain under **CC0 1.0 Universal** (see `LICENSE`, SPDX
`CC0-1.0`). It was previously GPL-3.0-or-later.

* Relicensing basis: all commits in the history are by the project owner (the commits attributed to
  `WilloDude` use the same address as the owner's other commits) and no external contribution has been
  merged. Anyone who contributes later does so under CC0 (see `CONTRIBUTING.md`).
* Copies and forks obtained before the change remain available under GPL-3.0-or-later for that revision;
  the change applies to the repository going forward.
* CC0 covers only material the project's authors own. Dependencies are **not** relicensed and keep their
  licences (MIT / Apache-2.0 / BSD / Zlib / Unicode / MPL-2.0 / CDLA-Permissive-2.0 per the
  `cargo metadata` audit of 610 crates, plus npm and Gradle dependencies); redistributing a built binary
  must comply with them (for example MPL-2.0 for Symphonia, attribution for MIT/Apache-2.0). See
  `THIRD_PARTY.md`.

Third-party assets: the logo and icons are original (see `apps/susic-app/assets/`), fonts are the system
stack (none bundled), fixtures are synthetic sine tones. Optional tools (`yt-dlp`, `ffmpeg`) are
invoked, not bundled.

## Before making the repository public (owner's decision)

- [x] Licence decided: CC0 1.0 Universal (above).
- [ ] Re-run `cargo audit`, `npm audit`, and a secrets scan; confirm none of your personal music,
      fixtures, API keys or keystores are in git history (`git log --all --stat`).
- [ ] Review `docs/PRIVACY.md` and `docs/SECURITY.md` for accuracy against the released build.
- [ ] Set a release signing key for Android (keep it **out** of git; use GitHub Actions secrets) and
      replace the debug-signed APK.
- [ ] Run the manual verification below on real hardware.
- [ ] Enable GitHub Security Advisories, branch protection and required CI.
- [ ] Only then change the repository visibility. **This was not done automatically.**

## Manual verification matrix (not covered by CI)

| Area | Check |
|---|---|
| Server reboot | `sudo reboot`; `systemctl status susic-server`; `curl /health` |
| Unmounted SSD | Stop mount; restart service; library must stay intact (logged warning) |
| Tailscale remote | From a phone on mobile data: connect via the tailnet IP, stream, seek, background |
| Interrupt | Toggle airplane mode mid-song; playback resumes within ~2 min of reconnect |
| Android real device | Lock-screen controls, Bluetooth buttons, call interruption, battery use |
| Offline | Download a track, disable network, play it |

## Packaging strategy

* **Desktop:** `.deb` and AppImage built by `release.yml` on `v*` tags (draft GitHub release, with
  `SHA256SUMS.txt`). Flatpak/Snap are future work.
* **Server:** `susic-server-linux-x86_64.tar.gz` (binary + unit + example config + installer) attached to
  the same release. A `.deb` for the server is future work.
* **Android:** CI uploads a debug APK. Release builds need `signingConfigs` backed by secrets; F-Droid
  inclusion is possible since all dependencies are FOSS.
* Versioning: SemVer; bump `Cargo.toml` (workspace), `tauri.conf.json`, `package.json`, `CHANGELOG.md`.
