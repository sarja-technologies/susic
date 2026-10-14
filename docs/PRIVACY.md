# Privacy and data handling

- All data (library database, artwork thumbnails, settings, queue) is stored locally in the app data directory.
- Susic has no accounts, analytics, or telemetry, and makes no network requests unless you use an optional feature:
  - **LRCLIB lyrics** (off by default): sends title, artist, album and duration of the chosen song to lrclib.net.
  - **YouTube search** (off by default): sends your query and your own API key to googleapis.com.
  - **yt-dlp import**: runs the external tool you installed, against the URL you enter.
  - **Device transfer**: local network only, encrypted with a key derived from a one-time pairing code.
- Susic never modifies or deletes your audio files unless you explicitly confirm deleting a file.
  Metadata edits are stored in Susic's database, not in the audio file.
- On Android, imported files are copied into app-private storage.
