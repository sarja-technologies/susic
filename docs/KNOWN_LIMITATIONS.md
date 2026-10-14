# Known limitations (0.1.0)

Not implemented — nothing in the UI pretends otherwise:

- Gapless playback, crossfade, equaliser, audio output device selection
- QR-code pairing (pairing uses address + one-time code; the code is *not* encoded for scanning)
- URL import on Android (a desktop yt-dlp binary cannot be bundled; use desktop → phone transfer)
- Windows, macOS and iOS builds (architecture is portable; no packaging or testing yet)
- Release-signed APK / AAB (only debug-signed APKs are produced; no signing key is included)
- Playlist artwork editing (artwork is derived from the first track with art)

Partial / caveats:

- **Opus on Linux desktop**: the symphonia decoder used for desktop playback does not decode Opus; Opus files
  import (tags/duration) but report a playback error on desktop. Android plays Opus through Media3.
- Desktop playback not exercised through physical speakers by the author; verified via device-open test.
- Android: queue/auto-advance is driven from Rust by polling the Media3 player, so tracks are not gapless.
- Transfer works only on the same LAN; the sender keeps one session per pairing code.
- YouTube search needs your own Data API key and is subject to your quota and YouTube's developer policies.
- Equal-title tracks by the same artist with identical audio content are treated as duplicates.
