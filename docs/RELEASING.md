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
