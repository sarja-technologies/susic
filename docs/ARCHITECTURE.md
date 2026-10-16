# Susic architecture

```
apps/susic-app/            Tauri 2 shell + Svelte 5 UI
  src/                     Svelte components, views, stores (TypeScript)
  src-tauri/               Rust app: commands, controller, playback engines
crates/susic-core/         Pure logic: models, play queue/shuffle, LRC parser
crates/susic-library/      SQLite persistence, migrations, queries
crates/susic-import/       Tag/artwork extraction, scanning, yt-dlp, LRCLIB
crates/susic-sync/         Noise-encrypted paired-device transfer
crates/tauri-plugin-susic-player/   Kotlin Media3 + MediaSession bridge (Android)
```

## Data flow

The Svelte UI calls Rust commands (`invoke`) and listens to events
(`player://state`, `queue://changed`, `library://changed`, `import://*`).
All state lives in Rust:

* **`Controller`** (`src-tauri/src/controller.rs`) owns the SQLite library, the
  `PlayQueue` and the active playback `Engine`. Auto-advance, play-count
  tracking and queue persistence happen here, *not* in the webview, so playback
  keeps going when the UI is suspended (important on Android).
* **`Engine`** is a trait with two implementations:
  * Desktop: `rodio` + `symphonia` on a dedicated audio thread.
  * Android: a thread that drives the Kotlin plugin (`ExoPlayer` inside a
    `MediaSessionService`) through `run_mobile_plugin` and polls its state.
* The Rust backend never loads whole files into memory; decoders stream from disk.

## Library

SQLite (bundled, WAL). Migrations are an append-only list applied by
`PRAGMA user_version`; tests build a v1 database, insert data and verify it
survives upgrade. User edits live in `track_edits` and never modify the
original tag data. Removing a track only deletes the library row; audio
deletion is a separate, explicitly confirmed command.

Duplicate detection uses a BLAKE3 fingerprint of size + first/last 256 KiB.
Rescans skip files whose path and size are unchanged.

## Providers

1. Local files / folders (`susic-import::read_track`, `scan_folder`).
2. YouTube search & import (`susic-import::ytdlp`, app `youtube.rs`, server `imports.rs`): yt-dlp
   `ytsearch` (no API key), or optionally the YouTube Data API; imports via yt-dlp + FFmpeg into this
   device's library, or into the server's `imports_dir` (indexed, then synced to all clients — the
   Android path). yt-dlp is found on PATH or installed on request as a checksum-verified managed copy;
   it is never bundled. See [YOUTUBE.md](YOUTUBE.md). Optional LRCLIB lyric lookup.
3. Arbitrary-URL import through the same yt-dlp pipeline.
4. Paired-device transfer (`susic-sync`).

## Transfer protocol

TCP carrying a `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s` session. The PSK derives
from a 12-character one-time code (≈60 bits) shown on the sender. The code is
valid for one sharing session, expires after 15 minutes of inactivity and is
revoked after 5 failed handshakes. Requests: `list`, `get(id, offset)` (resumable),
`artwork`, `lyrics`, `bye`. The receiver verifies each file's fingerprint before
accepting it, de-duplicates by hash, and checks free space first.

## Android

The Kotlin plugin owns `ExoPlayer` in a process-wide `PlayerHub`, exposes it
through a `MediaSessionService` (notification, lock screen, Bluetooth, audio
focus, becoming-noisy) and copies picked files into app-private storage. Rust
learns about `ended`/`next`/`prev`/`error` events by polling `state`.

## Server integration (0.2)

```
crates/susic-server   axum + SQLite: scan, pair, library delta, Range streaming, playlists
crates/susic-client   ureq client, resumable download, HttpRangeReader (Read+Seek, read-ahead, retry)
apps/susic-app/src-tauri/src/server.rs   pairing, sync, downloads, cache limit, online monitor
```

* **One row per song.** `tracks.remote_id` links a library row to a server track. Matching a server
  track to a local row is by content fingerprint, so owning a song locally never duplicates it.
  `availability` (`local`, `local+server`, `server`, `downloaded`) is derived, never stored.
* **Playback resolution** (`Controller::resolve_playable`): local file → offline copy → stream URL.
  The desktop engine decodes `HttpRangeReader` directly (seeking via Range requests); Android passes
  the URL to Media3.
* **Sync**: the server keeps a monotonically increasing revision; clients ask for `since=<rev>` and
  receive changed tracks and removed ids. A 15 s monitor pings `/health`, syncs on reconnect and about
  once a minute.
* **Safety**: the scanner never removes tracks when the music directory looks unmounted/empty.
