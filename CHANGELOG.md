# Changelog

## 0.2.0 — unreleased

### Added
- Optional **Susic server** (`susic-server`): directory indexer, move/delete detection, Range streaming,
  token pairing and revocation, favourites/playlist sync, systemd unit, installer, health endpoints.
- Server client in the app: pairing, unified local+server library with availability indicators,
  streaming via a seekable reconnecting reader, offline downloads with size limit and optional
  auto-cache, outage auto-resume, online/offline indicator, paired-device management.
- Unified search (device + server) with clearly labelled YouTube **discovery-only** results.
- Home quick-launch speed dial (pin albums/artists/playlists), favourites and downloads views.
- Optional dark theme; Appearance setting.
- Docs: server, Tailscale, security, Android, YouTube, releasing; CONTRIBUTING.
- CI: dependency audit, server build and tarball.

### Changed
- New visual identity: white/warm off-white, orange accent, charcoal type, new logo and icon set.
- Library schema migration 3 (append-only; existing libraries upgrade in place).

### Fixed
- Android: media notification and foreground state were never posted (session not registered).
- Svelte effect loop in server settings; Android status-bar overlap.
- Updated `rustls` to 0.23.46 (four RustSec advisories).


## 0.1.0 — first release

- Local SQLite library with migrations, favourites, history, automatic and user playlists
- Desktop playback (rodio/symphonia), MPRIS/media keys, persistent queue, shuffle/repeat
- Android playback via Media3 `MediaSessionService` (background, notification, lock screen, media buttons)
- Tag/artwork/lyrics extraction, LRC sync, LRCLIB (opt-in), manual lyrics
- Optional yt-dlp import (desktop), optional YouTube Data API search
- Encrypted desktop → Android LAN transfer (Noise `NNpsk0`)
- Svelte 5 UI with desktop and mobile layouts
