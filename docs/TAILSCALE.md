# Listening from anywhere with Tailscale

Susic never needs the server on the public internet. [Tailscale](https://tailscale.com) gives your
server and phone/laptop a private WireGuard network ("tailnet"); Susic simply talks to the server's
tailnet address.

## Setup

1. Install Tailscale on the server and on each client (`curl -fsSL https://tailscale.com/install.sh | sh`,
   then `sudo tailscale up`; Android: Play Store / F-Droid app).
2. On the server get its address: `tailscale ip -4` (e.g. `100.101.102.103`) or use MagicDNS
   (`optiplex.your-tailnet.ts.net`). Prefer the **full** MagicDNS name: the bare short name
   (`optiplex`) only resolves on devices whose DNS search domain includes your tailnet.
3. Set `bind = "100.101.102.103:8484"` in `/etc/susic/server.toml` (or keep `127.0.0.1:8484` and use
   `sudo tailscale serve --bg 8484` for HTTPS on the tailnet) and restart the service.
4. `susic-server pair` on the server; in Susic open **Settings → Susic server**, enter
   `http://100.101.102.103:8484` and the code.

At home you can enter the LAN address instead (`http://192.168.x.y:8484`) — same server, same
library. Susic stores one address; switch it in Settings if you prefer the LAN path at home. Using the
tailnet address everywhere works from anywhere (Tailscale connects directly on the LAN).

## Security notes

* Tailscale encrypts and authenticates traffic end to end, so plain `http://` over a tailnet is not
  readable by third parties. Over an untrusted network without Tailscale it **is** — don't do that.
* Do not port-forward the server or bind it to a public address. If you want HTTPS, use
  `tailscale serve` (Let's Encrypt certificate for the MagicDNS name) and enter the `https://` URL.
* Restrict who on your tailnet can reach port 8484 with Tailscale ACLs if the tailnet is shared.
* Android allows cleartext HTTP to the user-entered server address; this is why the above matters.

## Behaviour on flaky links

Streams use 512 KiB range chunks with one chunk of read-ahead and up to 6 retries with backoff.
If a stream still fails, Susic polls the server for ~2 minutes and resumes the same song from the same
position. Downloaded songs and local music are unaffected. The sidebar shows Connected/Offline.

## Testing status

Local-network streaming was exercised against `127.0.0.1` (desktop) and through `adb reverse`
(Android emulator). **Remote access over a real Tailscale tailnet has not been tested** in this
repository's CI or development run — the code path is identical (an HTTP URL), but please run the
checklist in `docs/RELEASING.md` before relying on it.

## What has been tested

* Server bound to a tailnet address only: reachable via the tailnet IP and the full MagicDNS name,
  refused on loopback and the LAN address.
* Susic Android (emulator) pairing, YouTube search, server import and streaming through the full
  MagicDNS URL.
* **Not yet tested:** a second physical device on the tailnet (phone on mobile data, another
  computer), `tailscale serve` HTTPS, and ACL rules. In these tests the client and server were the same
  machine, so traffic never crossed a real WireGuard link. Use the "Tailscale remote" row of the
  checklist in [RELEASING.md](RELEASING.md).

