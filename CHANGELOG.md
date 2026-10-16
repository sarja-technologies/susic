# Changelog

## 0.3.0 — unreleased

### Added
- **YouTube search without an API key**: yt-dlp `ytsearch` results (title, artist/channel, thumbnail,
  duration) inline in Search, debounced as you type, with a *Creative Commons only* filter.
- **Import workflow**: import a result (or any URL) to this device or to your Susic server. Format
  (M4A/MP3/FLAC/WAV/Opus/original) and bitrate, live progress (size, speed, ETA, stages), cancel
  (kills yt-dlp, removes partial files), serial queue, duplicate reuse, declared licence recorded,
  tags and square cover art embedded, thumbnail fallback, Play when done.
- **Server**: opt-in `ytdlp_enabled`/`imports_dir`; `GET /youtube`, `/youtube/search`,
  `POST /youtube/update`, `/imports` jobs; `ytdlp-install`/`ytdlp-update` commands; `check` reports
  yt-dlp/FFmpeg. Android searches and imports through the server.
- **yt-dlp management**: one-click install of the official release (SHA-256 verified), update,
  FFmpeg and JS-runtime detection (Deno/Node passed to yt-dlp), extra PATH locations for GUI apps.
- Clear messages for rate limiting/bot checks, HTTP 403 (retried once), DRM, age/members/payment/
  region restrictions, unavailable videos, outdated yt-dlp and network errors.
- Live end-to-end tests against real YouTube (`SUSIC_LIVE_YTDLP=1`).

### Changed
- The YouTube Data API is now an optional search backend; yt-dlp is the default.
- Library schema migration 4: import jobs record title, target, licence and track id.
- yt-dlp runs with `--ignore-config`; URLs are passed after `--`; playlist URLs are refused.

### Security
- Server imports: host allowlist (`import_hosts`, YouTube by default); credentials-in-URL and
  over-long URLs refused; queue limit (`imports_max_queue`, 429); free-space reserve
  (`imports_min_free_mb`, 507); per-import time limit (`imports_max_minutes`); output files must be
  regular files inside the imports folder.
- Searches are capped at 2 concurrent (429) so they can no longer exhaust the blocking thread pool
  used by streaming; yt-dlp updates are serialised.
- Cancel and time-outs kill yt-dlp's whole process group (FFmpeg included).
- systemd unit hardened further (`systemd-analyze security` 5.9 → 1.4); documented imports drop-in
  corrected (`ReadWritePaths="-/srv/music/Susic Imports"` — the previous `\x20` form broke startup).
- The server checks the imports folder is writable from inside its sandbox: startup warning,
  `imports_writable` in `GET /youtube`, clear refusal.

### Fixed
- The "Importing… 1/1" banner stayed visible after a URL import finished.
- A YouTube result could offer "Play" for a track that no longer existed (e.g. after disconnecting
  the server it was imported to).
- yt-dlp filesystem errors (`[Errno 30]` etc.) were reported as a bare file path.
- A server disk-full answer (507) was shown as "server unreachable".

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
