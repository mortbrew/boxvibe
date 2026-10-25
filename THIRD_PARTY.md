# Third-party code and references

## Music Assistant (Apache-2.0)

Parts of `src/pandora/mod.rs` (and the retry logic it exports, used by
`src/backend.rs`) are adapted from Music Assistant's Pandora provider:
`music_assistant/providers/pandora/provider.py` and `helpers.py` in
<https://github.com/music-assistant/server> (dev branch, commit
`0c560a9e58adb7106137ad8c8502bf1694070bfd`).

- Copyright the Music Assistant contributors (the Pandora provider is by
  OzGav and others).
- Licensed under the Apache License, Version 2.0. A copy is in
  [LICENSE-APACHE](LICENSE-APACHE); the original is at
  <https://github.com/music-assistant/server/blob/dev/LICENSE>.
- Changes: translated from Python (aiohttp, asyncio) to Rust (ureq, blocking
  worker thread) and reworked. What was adapted: the order and headers of
  the login (`HEAD /` for the `csrftoken`, then `POST /api/v1/auth/login`
  with `username`, `password`, `keepLoggedIn`, `existingAuthToken`), the
  browser-like request headers, logging in again once on HTTP 401, taking
  the stream over with `playbackResumed {"forceActive": true}` once on a 429
  `STREAM_VIOLATION`, the `getFragment` request body, choosing `mp3-hifi` by
  the `highQualityStreamingAvailable` account flag, and skipping curator
  messages. Each adapted file says so at its top.
- Music Assistant's `NOTICE` file covers its bundled service credentials
  and third-party machine-learning models, none of which boxvibe uses, so no
  NOTICE text applies to the adapted parts.

The adapted parts stay under the Apache License 2.0; boxvibe as a whole is
offered as MIT OR Apache-2.0 (Apache-2.0 section 4 allows that as long as its
conditions are kept: the license copy, this notice and the change notes).

## Pandora API documentation

The endpoints and their JSON come from the community "Unofficial Pandora API
documentation", <https://6xq.net/pandora-apidoc/rest/>
(<https://github.com/PromyLOPh/pandora-apidoc>, WTFPL). Only the
documentation was used; no code.

The search, create-station and genre requests (`v3/sod/search`,
`v1/station/createStation`, `v1/music/genrecategories`, `v1/music/genres`) are
written from the community's notes on the website's API, including the
request / answer shapes that libepimetheusKotlin documents. Those shapes were
noted as facts only; none of libepimetheusKotlin's (GPL-3.0) code was copied.

## tubevibe

The Omarchy theme reader (`src/theme.rs`), the tolerant config loader
(`src/config.rs`), the hint bar and the self-update (`src/update.rs`) are
adapted from [tubevibe](https://github.com/mortbrew/tubevibe) (MIT OR
Apache-2.0).

## secret-service, zbus (Pandora password in the keychain)

`src/keychain.rs` uses the [`secret-service`](https://crates.io/crates/secret-service)
crate (MIT OR Apache-2.0) to talk to the desktop's Secret Service keychain,
which pulls in [`zbus`](https://crates.io/crates/zbus) and its helper crates
(`zvariant`, `zbus_names`, MIT) and about 60 more small crates, all under
permissive licenses (MIT, Apache-2.0, Unlicense, Zlib, BSD). No code was copied;
they are ordinary Cargo dependencies listed in `Cargo.lock`. `cargo audit`
reports no vulnerabilities; it does show three "unmaintained" notices
(RUSTSEC-2026-0370, -0371 and -0372, for `zvariant`, `zbus_names` and
`zvariant_derive`, which were merged into `zbus` 6) that no release of
`secret-service` can fix yet.
