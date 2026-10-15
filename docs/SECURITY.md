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
