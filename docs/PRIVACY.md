# Privacy and data handling

- All data (library database, artwork thumbnails, settings, queue) is stored locally in the app data directory.
- Susic has no accounts, analytics, or telemetry, and makes no network requests unless you use an optional feature:
  - **LRCLIB lyrics** (off by default): sends title, artist, album and duration of the chosen song to lrclib.net.
  - **YouTube search** (off by default): with the default yt-dlp backend, your search words go to
    youtube.com from this device, or from your Susic server when this device has no yt-dlp (Android).
    No account, cookies or API key are used. Results show thumbnails loaded from i.ytimg.com. With
    the optional API backend, your query and your own API key go to googleapis.com.
  - **YouTube / URL import** (off by default): yt-dlp fetches the item you chose from its source, on
    this device or on your server. The import queue keeps the URL, title and declared licence locally.
  - **yt-dlp install/update** (only when you press the button): downloads the official release and its
    checksum list from github.com (latest-version check via api.github.com).
  - **Device transfer**: local network only, encrypted with a key derived from a one-time pairing code.
- Susic never modifies or deletes your audio files unless you explicitly confirm deleting a file.
  Metadata edits are stored in Susic's database, not in the audio file.
- On Android, imported files are copied into app-private storage.
