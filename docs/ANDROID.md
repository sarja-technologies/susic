# Android

## Build and install

Requirements: JDK 21, Android SDK platform 36 + build-tools 36, NDK 26.3.11579264, Rust Android targets.

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android
export ANDROID_HOME=$HOME/Android/Sdk NDK_HOME=$ANDROID_HOME/ndk/26.3.11579264
cd apps/susic-app && npm ci
npx tauri android build --debug --apk               # all ABIs, debug-signed
npx tauri android build --debug --apk --target x86_64   # emulator only
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

The APK is debug-signed; a release keystore is deliberately not in the repository
(see `docs/RELEASING.md`). Allow "install unknown apps" if you sideload.

## Background playback, lock screen and notification

Playback runs in a Media3 `MediaSessionService` (`PlaybackService`) with a foreground-service type of
`mediaPlayback`. The notification, lock-screen controls and Bluetooth buttons are provided by Media3's
`MediaSession`. Next/previous from those controls are routed to the Rust play queue.

### Verification status (Android 14 x86_64 emulator, 320×640)

| Check | Result |
|---|---|
| App installs and starts; local library, unified server library, artwork | ✅ verified |
| Pair with the server (`adb reverse`), sync 6 server tracks | ✅ verified |
| Stream a server-only song through Media3 | ✅ `PLAYING`, 40 s buffered |
| Continues playing when the app is sent to the home screen | ✅ verified |
| Media notification (`MediaStyle`, `category=transport`, 3 actions), foreground service | ✅ verified via `dumpsys` (after fixing a missing `addSession`) |
| Lock-screen artwork/controls, Bluetooth headset buttons, audio focus on a call | ⚠️ **not verified** (no physical device; emulator has no audio hardware) |
| Real phone, Tailscale remote streaming, battery behaviour | ⚠️ **not verified** |
| Cold-start race: launching the app twice in quick succession once showed "Plugin not found" | ⚠️ seen once, not reproduced after a clean launch; needs investigation |

Android 13+ asks for the notification permission at first launch; without it, playback works but the
notification is hidden.
