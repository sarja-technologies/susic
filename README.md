<p align="center"><img src="apps/susic-app/assets/logo.svg" width="96" alt="Susic logo"></p>

# Susic

A beautiful, offline-first, cross-platform music library and player for Linux desktop and Android.
No account, no cloud, no telemetry. Your music stays on your device.

> **Status: 0.1.0 (first release).** See [Known limitations](docs/KNOWN_LIMITATIONS.md) for exactly what is and isn't implemented.

## Features

- Local library (SQLite): songs, albums, artists, favourites, play history, automatic playlists
  (Liked Songs, Recently Added, Recently Played, Most Played) and your own playlists
- MP3, M4A/AAC, FLAC, WAV, Ogg Vorbis (Opus: Android; see limitations) with embedded tags, artwork and lyrics
- Persistent queue, shuffle without early repeats, repeat track/queue, play next, drag-and-drop reordering
- Synced (`.lrc`/embedded) and plain lyrics, manual timing offset, optional LRCLIB lookup
- Desktop: native playback, Linux media keys / MPRIS, drag-and-drop import, optional yt-dlp URL import
- Android: Media3/ExoPlayer background playback, notification + lock-screen + Bluetooth controls, audio focus
- Encrypted LAN transfer from Susic Desktop to Susic Android with a one-time pairing code
- Optional YouTube Data API search (your own key; results open in YouTube)

## Install

**Ubuntu / Debian**

```bash
sudo apt install ./Susic_0.1.0_amd64.deb      # or run the .AppImage
```

**Android**

1. Download `app-universal-debug.apk` (CI artifact or your own build).
2. Allow "Install unknown apps" for your browser/file manager and open the APK.
   Or with a computer: `adb install -r app-universal-debug.apk`.

The APK is a **debug-signed** build. No release signing key is included.

## Build from source

Requirements: Rust (stable), Node 20+, and on Ubuntu
`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev libssl-dev libdbus-1-dev patchelf`.

```bash
cd apps/susic-app
npm ci
npm run tauri dev                       # run desktop in dev mode
npx tauri build --bundles deb,appimage  # → target/release/bundle/
```

Android (JDK 21, Android SDK platform 36 + build-tools 36, NDK 26.3, Rust Android targets):

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
export ANDROID_HOME=$HOME/Android/Sdk NDK_HOME=$ANDROID_HOME/ndk/26.3.11579264
npx tauri android build --debug --apk                # all ABIs
npx tauri android build --debug --apk --target x86_64  # emulator only
# → apps/susic-app/src-tauri/gen/android/app/build/outputs/apk/universal/debug/
```

Tests and checks:

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p susic-app -- --ignored desktop_engine   # needs an audio device
cd apps/susic-app && npm run check && npm test
scripts/make-fixtures.sh ./fixtures                   # synthetic test audio (ffmpeg)
```

## Using Susic

1. Launch Susic. On first run choose **Add music files** or **Add a folder** (or drag files onto the window).
   Watched folders are re-scanned on launch; unchanged files are skipped.
2. Double-click a song (tap on Android) to play. Use the bottom bar for transport, queue and lyrics.
3. **Transfer to phone:** on desktop open *Transfer music → Start sharing*; on Android open
   *More → Transfer → Connect to Desktop* and enter the address and pairing code.
4. Optional tools: install `yt-dlp` and `ffmpeg` to enable URL import on desktop.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Platform support & testing status](docs/PLATFORMS.md)
- [Known limitations](docs/KNOWN_LIMITATIONS.md)
- [Privacy & data handling](docs/PRIVACY.md)
- [Third-party licences](THIRD_PARTY.md) · [Changelog](CHANGELOG.md)

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
