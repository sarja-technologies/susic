# Susic server

The optional Susic server turns a spare machine (for example an old Dell OptiPlex with an SSD full of
music) into a private streaming library. Susic apps keep working fully without it.

It indexes a directory, stores metadata in SQLite, and serves audio over a small token-authenticated
HTTP API with Range support (seek, resume, buffering). It is a single ~9 MB binary; idle it uses
≈8 MB RAM and no measurable CPU (measured with 1,500 tracks indexed).

## What it will and will not do

* **Read-only on your music.** It never writes, renames or deletes files in `music_dir`.
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

Environment overrides: `SUSIC_BIND`, `SUSIC_MUSIC_DIR`, `SUSIC_DATA_DIR`, `SUSIC_SCAN_INTERVAL`.

### Commands

```
susic-server [--config FILE] serve | scan | pair | devices | revoke <ID> | purge-missing | check | version
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

## Verified vs. not verified

Verified by automated tests against a real socket and real audio files: pairing, token auth and
revocation, ranged streaming, move/delete detection, unmounted-directory protection, playlist
conflict resolution, client reconnect after a server restart, resumable downloads.

**Not verified here:** an actual reboot of a systemd host (the unit passes `systemd-analyze verify`),
and a real OptiPlex/SSD. Test `sudo reboot` once after installing.
