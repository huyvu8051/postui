# postui

A keyboard-driven, vim-style HTTP client for the terminal. Think Postman, but
it starts in milliseconds, idles at ~0% CPU, uses tens of megabytes instead
of hundreds, and never makes you reach for the mouse.

```
┌ Method ┐┌ URL [INSERT] — {{: variables, :param for path params ────────────┐
│  POST  ││{{baseUrl}}/:tenantId/document-baskets                            │
└────────┘└───────────────────────────────────────────────────────────────────┘
┌ Path Params ─────────────────────────────────────────────────────────────────┐
│ :tenantId = {{tenantId}}                                                    │
└─────────────────────────────────────────────────────────────────────────────┘
┌ Headers ───────────────────────────┐┌ Script [Pre-request] Post-response ───┐
│ [x] Authorization: Bearer {{token}}││ pm.environment.set("basketId",        │
│ [x] Content-Type: application/json ││   pm.response.json().id)              │
└────────────────────────────────────┘└───────────────────────────────────────┘
┌ Body [Raw] Form Data Binary ───────┐┌ Response 7/7 — 200 (184 ms, JSON) ────┐
│ {                                  ││ { "id": "5a8c…", "status": "OPEN" }   │
│   "profileId": 171                 ││                                       │
└────────────────────────────────────┘└───────────────────────────────────────┘
```

## Features

- **Vim everywhere.** URL, body, scripts and every text field are real vim
  buffers (Normal/Insert/Visual, motions, search, `u`/`Ctrl+R`), powered by
  [`edtui`](https://github.com/preiter93/edtui).
- **Leader-key navigation.** `space u` URL, `space b` body, `space r` response,
  `space f` find a request, `space 1`–`9` jump to pinned requests.
- **Collections, tabs and quick slots.** Fuzzy collection browser, an
  open-tabs sidebar with unsaved-change markers, vim-mark-style quick slots.
- **Environments with `{{variable}}` completion.** Type `{{` and pick a
  variable from the active environment.
- **Postman-style scripting in Lua.** Pre-request and post-response scripts
  with a `pm.*` API (`pm.environment`, `pm.response.json()`, `pm.test`, …),
  sandboxed.
- **Nothing gets lost.** Unsaved edits, undo history and responses survive tab
  switches and restarts. Browse a request's past responses with `[` / `]`.
- **Body types.** Raw (JSON pretty-print + highlighting), multipart form data
  (text and file fields), binary file upload. Postman-style `:path` params.

## Install

Requires Rust 1.88 or newer.

```bash
cargo install --git https://github.com/huyvu8051/postui --locked
postui
```

Or from a clone:

```bash
git clone https://github.com/huyvu8051/postui && cd postui
cargo run --release
```

postui is developed on macOS. Linux should work (CI runs the test suite
there); Windows is untested.

## Quick start

1. `space e` opens the environment manager: `n` creates an environment,
   `Enter` activates it, `v` edits its variables (e.g. `baseUrl`).
2. `space f` opens the collection browser: `Esc` to the list, `A` creates a
   collection, `a` a request in it.
3. `space u` jumps into the URL (Normal mode): `i`, type
   `{{baseUrl}}/users`, `Esc`.
4. `F5` (or `Ctrl+Enter`) sends. `space r` focuses the response; it is a vim
   buffer, so `/` searches and `v`…`y` copies.
5. `space w` saves the request; `space W` saves every request marked `*`.

`Esc` out of a vim buffer before using `space` shortcuts or `q` (quit).

## Documentation

- [Keybindings](docs/keybindings.md): every key, by context
- [Editing model](docs/editing.md): panes, vim buffers, popups, `{{variable}}` completion
- [Building requests](docs/requests.md): method, URL, path params, headers, body types, sending, responses
- [Collections, tabs and saving](docs/collections-and-tabs.md): browser, tabs, quick slots, drafts
- [Environments and scripting](docs/environments-and-scripting.md): variables and the Lua `pm.*` API
- [Data and storage](docs/data-and-storage.md): where files live, what is kept, limits
- [Performance](docs/performance.md): measured memory and CPU, compared with GUI clients
- [Architecture](docs/architecture.md): code map for contributors

## Performance at a glance

Measured with [`scripts/bench.sh`](scripts/bench.sh) on an Apple-silicon Mac
(release build, 701 requests in the workspace):

| Scenario | Memory (RSS) | CPU |
|---|---|---|
| Startup | 13 MB | 0.03 s total |
| Idle | 13 MB | ~0.1% of one core |
| After two ~5 MB JSON responses | ~180 MB | 0.1% idle |

GUI clients such as Postman are commonly reported at several hundred MB when
idle. See [docs/performance.md](docs/performance.md) for the full numbers,
sources and caveats.

## Status

Usable daily, still young (0.x). Not implemented yet: Postman collection
import, auth helpers beyond raw headers, cookies UI, GraphQL/WebSocket.
See [CHANGELOG.md](CHANGELOG.md).

## Contributing

Bug reports, ideas and PRs are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md)
and our [Code of Conduct](CODE_OF_CONDUCT.md). Security issues: see
[SECURITY.md](SECURITY.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

### Third-party code

[`vendor/edtui`](vendor/edtui) is a copy of [edtui](https://github.com/preiter93/edtui)
0.11.7 (MIT, see [its license](vendor/edtui/LICENSE.md)) with a small patch
that exposes the undo/redo history so it can be saved.
