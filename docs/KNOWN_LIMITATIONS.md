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

## Added in 0.2.0

* One server connection at a time. Credentials are not encrypted at rest (file mode `0600`).
* Server transport is plain HTTP; use Tailscale or `tailscale serve`. No built-in TLS.
* Playlist sync is last-writer-wins per playlist; songs that exist only on a device are kept locally
  but are not uploaded. Deleting a server track removes it from clients unless it was downloaded.
* Server tag edits and album-art replacement are not supported from clients; edit tags with your
  usual tagger and rescan.
* The server indexes at most what `lofty` can read; unsupported or corrupt files are skipped and
  listed in the scan log.
* YouTube results are discovery-only by design ([docs/YOUTUBE.md](YOUTUBE.md)); there is no in-app
  YouTube playback.
* Outage auto-resume is implemented and the reconnecting reader is integration-tested, but the full
  "network drops mid-song → UI resumes" path has not been exercised end to end on a device.
* Android: lock-screen/Bluetooth/call-interruption and real-device behaviour are unverified
  ([docs/ANDROID.md](ANDROID.md)). Android runs on x86_64 emulator only so far.
* Gapless playback and ReplayGain are not implemented.
