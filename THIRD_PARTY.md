# Third-party software

Susic is built on open-source components. Principal dependencies (see `Cargo.lock` and
`apps/susic-app/package-lock.json` for the full list and exact versions):

| Component | Licence |
|---|---|
| Tauri 2, wry, tao | MIT / Apache-2.0 |
| Svelte 5, Vite | MIT |
| rusqlite / SQLite | MIT / public domain |
| rodio, cpal | MIT / Apache-2.0 |
| Symphonia | MPL-2.0 |
| lofty | MIT / Apache-2.0 |
| snow (Noise) | Apache-2.0 / MIT |
| image, walkdir, blake3, ureq, qrcode, souvlaki | MIT / Apache-2.0 (blake3: CC0/Apache-2.0) |
| axum, tokio, tower, tower-http (server) | MIT |
| toml, rand | MIT / Apache-2.0 |
| AndroidX Media3 (ExoPlayer, MediaSession) | Apache-2.0 |

Optional external tools that Susic can invoke but does not bundle: **yt-dlp** (Unlicense) and **FFmpeg** (LGPL/GPL).
Lyrics may be fetched from **LRCLIB**; YouTube search uses the **YouTube Data API** under Google's terms.

A scripted `cargo metadata` licence scan of all 610 crates found only MIT/Apache-2.0/BSD/Zlib/Unicode/MPL-2.0/CDLA-Permissive-2.0 (webpki-roots) licences. None are GPL/AGPL, so none conflict with Susic's CC0 dedication; they remain under their own terms.

## Licence of Susic itself

Susic's original source, documentation and assets are dedicated under CC0 1.0 Universal (see `LICENSE`). CC0 does **not** apply to anything listed in this file or to the files below, which keep their original licences and copyright notices:

* `crates/tauri-plugin-susic-player/android/.tauri/tauri-api/` — generated Tauri Android API sources, "Copyright 2019-2024 Tauri Programme within The Commons Conservancy", Apache-2.0 OR MIT (SPDX headers retained in each file).
* `apps/susic-app/xdotool-0.4.0-py3-none-any.whl` — third-party Python package (xdotool, MIT per its package metadata) committed as a build/test helper; not authored by Susic and not dedicated to the public domain.
* Dependencies fetched at build time (Cargo, npm, Gradle) under the licences in the table above. MPL-2.0 (Symphonia) is file-level copyleft and still applies to Symphonia's files.
