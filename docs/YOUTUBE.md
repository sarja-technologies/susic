# YouTube discovery: what is and isn't possible

## What Susic does

With your own **YouTube Data API v3** key, Search can query YouTube's catalogue (`search.list`,
`type=video`, music category) and show title, channel and thumbnail. Each result is labelled
**Discovery only**, can open in YouTube, or can be used to search your own library. Nothing is
downloaded, cached or stored.

## Why it stops there

* **Playback / audio extraction.** YouTube's API Services Terms and Developer Policies do not permit
  downloading content or separating audio from video, and the only sanctioned playback is YouTube's
  own embedded player (with video shown). Susic has no embedded-player view, and implementing
  extraction would bypass those terms (and, for protected content, DRM). So catalogue results are
  never presented as playable or downloadable.
* **YouTube Music.** There is no official public YouTube Music API; unofficial endpoints violate the
  terms, so they are not used.
* **Quota.** The default project quota is 10,000 units/day and `search.list` costs 100 units, i.e. about
  100 searches per day. Google may change this; check the API console. Susic therefore searches YouTube
  only on explicit request (Enter or the button), never while typing.
* **Authentication.** Public search needs only an API key (no OAuth). Create a key in Google Cloud,
  enable *YouTube Data API v3*, restrict the key, and paste it in Settings.
* **Metadata** shown is whatever YouTube returns; it is not merged into your library.

## Getting music you may legally keep

Use **Add files / Add folder** for music you own or may use, or put it on your server. The desktop app
also has an optional URL import that calls `yt-dlp` if you installed it; it is off by default, shows a
warning, and you are responsible for only importing content you have the right to download.
