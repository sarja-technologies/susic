# Platform support and validation status

"Validated" means exercised for real on the stated target. "Built" means it compiles/packages only.

| Capability | Linux desktop | Android |
|---|---|---|
| App launch | Validated (X11 session) | Validated (API 34 x86_64 emulator) |
| Library import + metadata + artwork + LRC sidecar | Validated (mp3, flac, wav, ogg, m4a, opus fixtures) | Validated (files placed in app storage; 5 tracks) |
| Playback / pause / seek / end | Validated by an audio-device test (`desktop_engine…`) | Validated (emulator MediaSession: PLAYING/PAUSED, position) |
| Auto-advance, next from media key | Unit-tested (controller) | Validated, app backgrounded |
| Background playback | n/a | Validated (HOME pressed, still PLAYING) |
| Notification / lock-screen / Bluetooth controls | MPRIS: built, not exercised | Media session + media buttons validated via `adb` key events; physical Bluetooth not tested |
| File import | Dialog + drag-drop: built | Document picker → private copy: built, picker UI not driven in the emulator |
| Paired transfer | Protocol unit/loopback-tested | Built; UI not driven end-to-end |
| Real physical Android device | — | **Not tested** |
| Windows / macOS / iOS | Not built | Not built |

Packaged artifacts are only described as tested to the extent of this table.
