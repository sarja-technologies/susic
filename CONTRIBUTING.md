# Contributing to Susic

Thanks for helping! Susic is a local-first music player with an optional self-hosted server.

## Getting started

1. Install the prerequisites listed in the [README](README.md#build-from-source).
2. `cd apps/susic-app && npm ci && npm run tauri dev`.
3. Make test audio: `scripts/make-fixtures.sh ./fixtures` (requires ffmpeg), then drag the folder in.
4. To work on server features, run `susic-server` against the fixtures (see README → Run locally).

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd apps/susic-app && npm run check && npm test
```

* Add or update tests with behaviour changes. Server and client changes should be covered by the
  real-socket integration tests in `crates/susic-server/tests` and `crates/susic-client/tests`
  (don't mock the network where a real socket is cheap).
* Keep the app responsive: no blocking network or disk work on the UI thread; Tauri sync commands run
  on the main thread, so use `async` + `spawn_blocking` for I/O.
* Keep the library schema changes **append-only** migrations in `susic-library/src/migrations.rs`.
* UI: use the CSS tokens in `src/app.css` (warm white surfaces, charcoal text, orange accent). No
  purple/violet, no decorative gradients. Every button must do something; keyboard and screen-reader
  access matter.
* Don't add dependencies lightly (the server targets old hardware). New dependencies must be
  permissively licensed (MIT, Apache-2.0, BSD, Zlib, MPL-2.0 or similar) and must not impose copyleft on
  Susic's own code; update `THIRD_PARTY.md`.
* Never claim a platform behaviour is verified unless you ran it; say what you tested in the PR.

## Reporting bugs and security issues

Bugs: GitHub issues with OS, version and steps. Security: see [docs/SECURITY.md](docs/SECURITY.md) —
please report privately.

## Licence

Susic is dedicated to the public domain under [CC0 1.0 Universal](LICENSE). By contributing you confirm
that you wrote the contribution (or otherwise have the right to submit it) and that you dedicate it under
CC0 1.0. Do not submit code or assets copied from other projects unless their licence permits it, and
include the original notice so it can be listed in `THIRD_PARTY.md`.
