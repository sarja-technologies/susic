# Releasing and publication checklist

## Licence

The repository is **GPL-3.0-or-later** (see `LICENSE`). Recommendation: keep it.

* Strong copyleft keeps improvements to a player that handles people's personal libraries open.
* Every dependency is GPL-3.0-compatible (audit of 610 crates: MIT/Apache-2.0/BSD/Zlib/Unicode/MPL-2.0
  /CDLA-Permissive-2.0; `cargo metadata` license scan, no GPL-incompatible or unknown licences).
* Alternative worth considering for the **server only**: AGPL-3.0-or-later, which would also require
  operators who modify and host it for others to share changes. Not needed for a personal server.
* Apache-2.0/MIT would permit closed-source forks and app-store redistribution without source; choose
  those only if that is the goal.

Third-party assets: the logo and icons are original (see `apps/susic-app/assets/`), fonts are the system
stack (none bundled), fixtures are synthetic sine tones. Optional tools (`yt-dlp`, `ffmpeg`) are
invoked, not bundled.

## Before making the repository public (owner's decision)

- [ ] Decide the licence (above) and confirm the copyright holder line in `LICENSE`.
- [ ] Re-run `cargo audit`, `npm audit`, and a secrets scan; confirm none of your personal music,
      fixtures, API keys or keystores are in git history (`git log --all --stat`).
- [ ] Review `docs/PRIVACY.md` and `docs/SECURITY.md` for accuracy against the released build.
- [ ] Set a release signing key for Android (keep it **out** of git; use GitHub Actions secrets) and
      replace the debug-signed APK.
- [ ] Run the manual verification below on real hardware.
- [ ] Enable GitHub Security Advisories, branch protection and required CI.
- [ ] Only then change the repository visibility. **This was not done automatically.**

## CI status (read first)

GitHub Actions has **never run** on this private repository. Every job is rejected before it starts
with *"The job was not started because recent account payments have failed or your spending limit
needs to be increased"* (Settings → Billing & plans). This is an account/billing issue, not a code
issue. Branch protection and required checks are also unavailable on this plan for private repos
(HTTP 403 "Upgrade to GitHub Pro or make this repository public").

Until billing is fixed, run the CI jobs' commands locally from a clean clone (see below) and record
the results in the PR. Don't treat a red CI run as passing, and don't remove jobs to make it green.
Options: fix the payment method / raise the Actions spending limit; or register a self-hosted runner
(free for private repos) on a machine you control.

## Release gates (every release)

- [ ] All CI jobs green on GitHub (Rust, audit, server, frontend, Linux bundles, Android APK).
- [ ] Local equivalent from a **clean clone** if CI can't run:
      `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`, `cargo audit`, `npm ci && npm audit && npm run check && npm test && npm run build`,
      `cargo build --release -p susic-server`, `npx tauri build --bundles deb,appimage`,
      `npx tauri android build --debug --apk`.
- [ ] Live YouTube tests with Creative Commons media only:
      `SUSIC_LIVE_YTDLP=1 cargo test -p susic-import --test live_ytdlp -- --test-threads=1`,
      `SUSIC_LIVE_YTDLP=1 cargo test -p susic-client --test youtube_live`,
      `SUSIC_LIVE_YTDLP=1 cargo test -p susic-app youtube`.
      (Without the variable these suites pass trivially — they are skipped, not run.)
- [ ] Versions bumped together: `Cargo.toml`, `tauri.conf.json`, `package.json`, `CHANGELOG.md`.
- [ ] Release binaries' glibc floor noted in the release notes (server built on Ubuntu 24.04 → glibc 2.39).

## Manual verification matrix (not covered by CI)

Status for **0.3.0** (✅ done, ◐ partly / substitute, ✗ not done):

| Area | Check | 0.3.0 |
|---|---|---|
| systemd install | `install-server.sh`, hardened unit, YouTube drop-in, `systemd-analyze security` | ✅ Ubuntu 24.04 container (systemd 255) |
| Server reboot | `sudo reboot`; `systemctl status susic-server`; `curl /health` | ◐ container reboot only |
| Unmounted SSD | Stop mount; restart service; library must stay intact (logged warning) | ◐ folder removed in container; real mount not tested |
| Server YouTube import | Search, CC import, cancel, update under the sandbox | ✅ container |
| Tailscale remote | From a phone on mobile data: connect via the tailnet IP, stream, seek, background | ◐ same machine over tailnet IP/MagicDNS; no second device online |
| Interrupt | Toggle airplane mode mid-song; playback resumes within ~2 min of reconnect | ✗ |
| Android real device | Lock-screen controls, Bluetooth buttons, call interruption, battery use | ✗ emulator only |
| Desktop audio | Real output device | ◐ PipeWire capture of real playback; speakers not listened to |
| Offline | Download a track, disable network, play it | ✗ (automated test only) |

## Packaging strategy

* **Desktop:** `.deb` and AppImage built by `release.yml` on `v*` tags (draft GitHub release, with
  `SHA256SUMS.txt`). Flatpak/Snap are future work.
* **Server:** `susic-server-linux-x86_64.tar.gz` (binary + unit + example config + installer) attached to
  the same release. A `.deb` for the server is future work.
* **Android:** CI uploads a debug APK. Release builds need `signingConfigs` backed by secrets; F-Droid
  inclusion is possible since all dependencies are FOSS.
* Versioning: SemVer; bump `Cargo.toml` (workspace), `tauri.conf.json`, `package.json`, `CHANGELOG.md`.
