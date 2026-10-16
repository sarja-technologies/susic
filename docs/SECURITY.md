# Security model

Report vulnerabilities privately via GitHub Security Advisories on the repository.

## Threat model

Susic targets a personal server reached over a LAN or a private tailnet. It is **not** designed to be
exposed to the public internet.

## Server

* **Pairing.** `susic-server pair` creates an 8-character one-time code (alphabet without look-alike
  characters, 10-minute validity, single use). Stored only as a BLAKE3 hash. Failed attempts are
  rate-limited (10/min, then HTTP 429).
* **Device tokens.** 256-bit random, issued per device on pairing, stored only as a BLAKE3 hash.
  Revocable per device (`susic-server revoke`, or Settings → Paired devices); effective immediately.
* **Transport.** Plain HTTP by design; rely on Tailscale/WireGuard or put it behind `tailscale serve`
  for TLS. The server has no built-in TLS.
* **Query-string tokens.** Accepted for stream URLs because Media3/`<audio>` cannot set headers. A
  token in a URL can land in logs or proxies; the server itself does not log URLs. Keep it off
  shared reverse proxies.
* **Filesystem.** Only paths produced by the server's own scan are served; `..` segments are rejected.
  The systemd unit runs as an unprivileged user with `ProtectSystem=strict`, `NoNewPrivileges`,
  read-only `$HOME`, private `/tmp`, and write access only to `/var/lib/susic`.
* **Scope of a token.** A paired device can read the library, change favourites/playlists, list/revoke
  devices and trigger rescans. There is a single logical user; there are no per-user permissions.

## Client

* Credentials live in `server.json` inside the app data directory with mode `0600`, **separate from
  `library.db`**, so exported/copied libraries never carry the token. On Android the file is in the
  app-private directory.
* **Limitation:** the token is not encrypted at rest (no OS keyring / Android Keystore yet). Anyone with
  access to your user account or a rooted device can read it. Revoke from the server if a device is lost.
* YouTube API keys are stored in the local settings table in plain text (your own key, local only).
* yt-dlp is an external executable. "Install"/"Update" download only the official release from
  github.com/yt-dlp over HTTPS and refuse it unless its SHA-256 matches the release's `SHA2-256SUMS`.
  yt-dlp always runs with `--ignore-config`, URL arguments follow `--`, only `http(s)` URLs are accepted,
  and no cookies or credentials are ever passed.
* With `ytdlp_enabled = true`, any **paired** device (all YouTube/import endpoints require a device
  token) can make the server search YouTube and download into `imports_dir`:
  * Sources are limited to `import_hosts` (YouTube by default). URLs with credentials, non-http(s)
    schemes, playlists and URLs over 2 KB are refused. Setting `import_hosts = ["*"]` lets paired devices
    make the server fetch any URL it can reach, including LAN hosts.
  * Files are only accepted if they are regular files inside `imports_dir`; anything else is deleted
    and the import fails. Under systemd, only `imports_dir` (via the documented drop-in),
    `/var/lib/susic` and a private `/tmp` are writable, which was checked from inside the running service.
  * Resource limits: ≤ 20 queued imports, ≤ 1 GiB per file, ≥ 2 GB kept free, ≤ 2 h per import, one
    import at a time, ≤ 2 concurrent searches (others get 429 instead of tying up the thread pool that
    streaming uses), one yt-dlp update at a time.
  * Cancel and time-outs kill yt-dlp's whole process group, so FFmpeg can't keep writing afterwards.
  * The job list (URLs and titles) is visible to every paired device. The hardened unit scores 1.4
    ("OK") in `systemd-analyze security`.
  * Disconnecting a device in the app forgets its token locally but does not revoke it on the server;
    revoke old devices with `susic-server revoke <ID>` or from the device list in Settings → Susic server.
* The webview CSP limits network access to the app itself; server and YouTube requests are made from
  Rust, not the webview.
* Android sets `usesCleartextTraffic="true"` so the user can enter an `http://` tailnet/LAN address.

## Existing 0.1 features

Desktop→phone transfer uses a Noise-protocol channel with a one-time code (see `susic-sync`).

## Dependency audit

`cargo audit` (RustSec) and `npm audit` run in CI. At the time of writing: 0 vulnerabilities; 3
"unmaintained" warnings from Tauri/GTK transitive crates (`paste`, `proc-macro-error`, `glib` 0.18
unsoundness in an iterator impl Susic does not use). No secrets, keystores or private keys are in the
repository or its history (scanned).
