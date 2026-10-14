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
| AndroidX Media3 (ExoPlayer, MediaSession) | Apache-2.0 |

Optional external tools that Susic can invoke but does not bundle: **yt-dlp** (Unlicense) and **FFmpeg** (LGPL/GPL).
Lyrics may be fetched from **LRCLIB**; YouTube search uses the **YouTube Data API** under Google's terms.
