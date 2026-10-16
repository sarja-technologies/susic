# Susic server

The optional Susic server turns a spare machine (for example an old Dell OptiPlex with an SSD full of
music) into a private streaming library. Susic apps keep working fully without it.

It indexes a directory, stores metadata in SQLite, and serves audio over a small token-authenticated
HTTP API with Range support (seek, resume, buffering). It is a single ~9 MB binary; idle it uses
≈8 MB RAM and no measurable CPU (measured with 1,500 tracks indexed).

## What it will and will not do

* **Read-only on your music.** It never writes, renames or deletes files in `music_dir`. The one
  exception is opt-in: with `ytdlp_enabled = true` it *adds* new imported files to
  `music_dir/imports_dir` (see [YOUTUBE.md](YOUTUBE.md)). It still never changes or deletes existing files.
* **Never touches disks or mounts.** You choose `music_dir`; the installer does not partition,
  format, mount or move anything.
* **Unmounted-SSD safe.** If the music directory is missing, or is present but yields no audio while
  the database is non-empty, the scan refuses to remove tracks (logged as a warning). Remount and the
  library is intact.
* Tracks moved or renamed on disk are recognised by content fingerprint and keep their identity, so
  favourites and playlists survive reorganising the SSD.

## Install (Ubuntu / Debian, systemd)

```bash
# On a build machine (or the server itself)
cargo build --release -p susic-server
sudo deploy/install-server.sh                 # creates user 'susic', /etc/susic, /var/lib/susic, unit

sudoedit /etc/susic/server.toml               # set music_dir and bind
sudo -u susic susic-server --config /etc/susic/server.toml check
sudo systemctl enable --now susic-server
sudo -u susic susic-server --config /etc/susic/server.toml pair   # prints a one-time code
curl -s http://127.0.0.1:8484/health
```

Or use the release tarball (`susic-server-linux-x86_64.tar.gz`), which contains the binary, unit,
example config and installer.

If the music SSD is its own mount, make systemd wait for it:

```bash
sudo systemctl edit susic-server
#   [Unit]
#   RequiresMountsFor=/mnt/music
```

### Configuration

`/etc/susic/server.toml` (see `deploy/server.example.toml`):

| Key | Default | Meaning |
|---|---|---|
| `bind` | `127.0.0.1:8484` | Listen address. Use your Tailscale/LAN IP, or keep loopback and use `tailscale serve`. |
| `music_dir` | `/srv/music` | Library root. Change it any time and restart (see below). |
| `data_dir` | `/var/lib/susic` | Database + artwork thumbnails. |
| `scan_interval_secs` | `300` | Rescan interval. `0` = only at startup. |
| `server_name` | `Susic Server` | Shown in clients. |
| `ytdlp_enabled` | `false` | Let paired devices search YouTube and import audio via the server's yt-dlp + FFmpeg ([YOUTUBE.md](YOUTUBE.md)). |
| `imports_dir` | `Susic Imports` | Sub-folder of `music_dir` that receives imports (relative, no `..`). |
| `import_hosts` | `["youtube.com", "youtu.be", "youtube-nocookie.com"]` | Hosts imports may come from (subdomains included). `["*"]` allows any URL yt-dlp supports — **including hosts on your LAN**, so only do that if you trust every paired device. |
| `imports_min_free_mb` | `2048` | Imports are refused when the imports filesystem has less free space (each file is also capped at 1 GiB). |
| `imports_max_queue` | `20` | Maximum queued + running imports; more get HTTP 429. |
| `imports_max_minutes` | `120` | A single import is stopped (and cleaned up) after this long. |

Environment overrides: `SUSIC_BIND`, `SUSIC_MUSIC_DIR`, `SUSIC_DATA_DIR`, `SUSIC_SCAN_INTERVAL`,
`SUSIC_YTDLP_ENABLED`, `SUSIC_IMPORTS_DIR`.

#### Enabling YouTube import under systemd

The shipped unit is sandboxed (`ProtectSystem=strict`; only `/var/lib/susic` and a private `/tmp`
are writable), so imports also need write access to **only** the imports folder:

```sh
sudo install -d -o susic -g susic "/srv/music/Susic Imports"
sudo apt install ffmpeg nodejs     # Node.js (or Deno) is recommended for yt-dlp
sudo -u susic susic-server --config /etc/susic/server.toml ytdlp-install
sudo systemctl edit susic-server   # add the drop-in below
sudo systemctl restart susic-server
journalctl -u susic-server -n 5    # no "YouTube import will fail" warning = OK
```

```ini
[Service]
ReadWritePaths="-/srv/music/Susic Imports"
```

* **Quote** paths containing spaces. (`\x20` does *not* work here: systemd turns it into
  `Susicx20Imports` and the service fails to start.)
* Keep the leading **`-`**. Without it, systemd refuses to start the service at all
  (`status=226/NAMESPACE`) whenever the folder is missing — for example while the music disk is
  unmounted — which would defeat the unmounted-disk protection above.
* `susic-server check` runs outside the sandbox, so it can say "writable" even when the service
  can't write. The service checks for itself: it logs `YouTube import will fail: …` at startup,
  reports `imports_writable` in `GET /api/v1/youtube` (the apps show it in Settings), and refuses
  imports with that reason.

The managed yt-dlp goes in `data_dir/tools/` (an `ffmpeg` placed there is preferred over the system
one) and its cache in `data_dir/cache/yt-dlp/`.

**Validated** on Ubuntu 24.04 (systemd 255) in a `systemd-nspawn` container with the release binary
built there, using `deploy/install-server.sh`, this unit and the drop-in above. Real CC-licensed
imports, search, cancel and in-API update all worked under the sandbox. From inside the service, only
the imports folder, `/var/lib/susic` and its private `/tmp` were writable (other music folders gave
`Read-only file system` even when Unix permissions allowed writing). The service survived a container
reboot and an unmounted music folder. `systemd-analyze security susic-server`: **1.4 OK**.

**Binary compatibility:** the release tarball is built on Ubuntu 24.04 and needs glibc ≥ 2.39
(Ubuntu 24.04+, Debian 13+). On older systems, build it there with
`cargo build --release -p susic-server`.

### Commands

```
susic-server [--config FILE] serve | scan | pair | devices | revoke <ID> | purge-missing | check |
                             ytdlp-install | ytdlp-update | version
```

Logs go to stderr → `journalctl -u susic-server -f`. Each scan logs added/updated/moved/removed counts.

## Health checks

* `GET /health` (no auth): `{"status":"ok","name":…,"version":…,"api":1,"uptime_secs":…}` — suitable for
  uptime monitors. Exposes no library data.
* `GET /api/v1/status` (auth): track count, size, revision, last scan result/error.
* systemd restarts the service on failure (`Restart=on-failure`).

## Changing the music location

1. Stop: `sudo systemctl stop susic-server`.
2. Edit `music_dir` in `/etc/susic/server.toml`.
3. Start. If the new tree is the same files at a new path, they are matched by fingerprint and keep
   their identity (favourites/playlists intact). Files that truly vanished are marked missing and
   disappear from clients; `susic-server purge-missing` forgets them permanently.

## Backup and migration

Everything the server owns is in `data_dir`:

```bash
sudo systemctl stop susic-server
sudo tar -C /var/lib -czf susic-backup-$(date +%F).tgz susic
sudo systemctl start susic-server
```

Your music is not in that archive — back it up the way you already back up the SSD. To migrate to a new
machine: install the server, restore the archive to `/var/lib/susic`, copy `/etc/susic/server.toml`,
point `music_dir` at the music and start. Paired devices keep working (tokens are stored hashed in the
database). Because the database is regenerable from the music, losing it costs only favourites,
playlists and pairings.

## API (v1)

All under `/api/v1`, `Authorization: Bearer <token>` (or `?access_token=` for media players).

| Method & path | Purpose |
|---|---|
| `POST /pair` | `{code, device_name}` → `{token, device_id, server_name}` (public, rate-limited) |
| `GET /library?since=<rev>` | Tracks changed since `rev`, plus removed ids, plus current `revision` |
| `GET /tracks/{id}/stream` | Audio with HTTP Range / If-Range support |
| `GET /tracks/{id}/artwork`, `/lyrics` | Thumbnail / embedded or `.lrc` lyrics |
| `PUT /tracks/{id}/favourite` | `{favourite: bool}` |
| `GET/PUT/DELETE /playlists[/{uid}]` | Last-writer-wins playlist sync |
| `GET /devices`, `DELETE /devices/{id}` | List / revoke paired devices |
| `POST /scan`, `GET /status` | Trigger rescan / status |
| `GET /youtube` | YouTube capabilities: `{enabled, ytdlp, version, ffmpeg, js_runtime, imports_dir}` |
| `GET /youtube/search?q=&cc=0\|1&limit=` | yt-dlp search → `{results:[{id,url,title,artist,channel,duration_ms,thumb,live}]}`. 403 when disabled; 429 when 2 searches are already running (searches never queue, so they can't starve streaming) |
| `POST /youtube/update` | Install or update the server's yt-dlp (verified official release) |
| `GET /imports`, `POST /imports`, `DELETE /imports/{id}` | Import jobs: `{url, format, quality, title}` → 202 job; poll for `status`, `progress`, `message`, `license`, `track_id`; DELETE cancels (killing yt-dlp *and* FFmpeg) or forgets a finished job. Refusals: 400 bad URL / host not in `import_hosts`, 403 disabled / tools missing / folder not writable, 429 queue full, 507 low disk space |

## Verified vs. not verified

Verified by automated tests against a real socket and real audio files: pairing, token auth and
revocation, ranged streaming, move/delete detection, unmounted-directory protection, playlist
conflict resolution, client reconnect after a server restart, resumable downloads. Offline tests
with a fake yt-dlp (run in CI) cover import authentication, the host allowlist, queue limit, disk
reserve, search concurrency, cancel killing yt-dlp's whole process tree, time limits, refusing files
outside the imports folder, and the read-only-folder check (`cargo test -p susic-server --test
imports`, `-p susic-import --test fake_ytdlp`). With `SUSIC_LIVE_YTDLP=1`, live tests also cover
yt-dlp install via the API, real YouTube search, a real import of Creative Commons media, indexing and
streaming it back (`cargo test -p susic-client --test youtube_live`).

The systemd install (installer, hardened unit, YouTube drop-in, reboot, unmounted music folder) was
validated in an Ubuntu 24.04 `systemd-nspawn` container — see "Enabling YouTube import under systemd".

**Not verified here:** a physical host reboot and a real OptiPlex/SSD mount. A container reboot
exercises systemd's unit handling but not your bootloader, disks or `RequiresMountsFor` ordering.
Test `sudo reboot` once after installing.
