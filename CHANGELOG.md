# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

First public release candidate (0.1.0).

### Added

- Requests with any method, headers, and Raw / Form Data (text and file
  fields) / Binary bodies; Postman-style `:path` params.
- Collections with a fuzzy collection browser (`space f`) and vim-style list
  navigation; delete with confirmation.
- Environments with `{{variable}}` substitution and `{{` completion of
  variable names in the URL, body, key/value and path-param inputs.
- Vim editing everywhere through edtui: URL, body, scripts, console and
  response, plus popups that open in Normal mode.
- Leader shortcuts: `space u/m/p/h/b/s/c/r` jump into panes, `space e`
  environments, `space H` request history, `space z` maximize.
- Tabs sidebar (`space t`) with unsaved-change markers, reordering, and quick
  slots (`m` + digit, then `space 1`–`9`).
- Lua pre-request / post-response scripts with a Postman-like `pm` API
  (`pm.request`, `pm.response.json()`, `pm.environment`, `pm.test`,
  `pm.console.log`, `pm.clipboard.set`), sandboxed and step-limited.
- Drafts: unsaved edits, last response and undo/redo history survive tab
  switches and restarts (autosaved to `drafts.json`); `space w` saves the
  current request, `space W` saves all.
- Response history per request: `[` / `]` in the Response pane, `x` reruns
  the post-response script on the response on screen.
- JSON pretty-printing and syntax highlighting; `F4` opens links from the
  response.
- `scripts/bench.sh` to measure memory and CPU.

### Fixed

- A response that arrives after switching tabs is stored on the request that
  sent it.
