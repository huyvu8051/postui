# postui

A terminal HTTP client — a Postman alternative that lives in your terminal. Built with Rust, `ratatui`, `edtui`, and `reqwest`.

## Run

```bash
cargo run
```

Requests, collections, and environments are persisted to
`~/Library/Application Support/postui/workspace.json` (macOS) automatically after each change.

## Layout

- **Collections** (left): saved collections and requests.
- **Method / URL** (top right): request line. URL is a real vim buffer (`edtui`).
- **Path Params**: auto-detected `:name` segments from the URL, per request.
- **Headers**: request headers.
- **Body / Response**: request body editor and the last response — both real
  vim buffers. A JSON response body is auto-detected, pretty-printed, and
  syntax-highlighted (`edtui`'s built-in `syntect`-based highlighter); the
  response title shows ", JSON" when that kicked in. The same detector
  (`src/format.rs`) backs the Body panel's `f` pretty-print shortcut.
- Bottom line: status bar / hints.

## Editing model

**URL, Body, and Response are full `edtui` vim buffers** — real Normal/
Insert/Visual modes, not a plain text box. While one of them is focused,
*every* key goes to that buffer (including bare letters like `q`/`e`, which
are global shortcuts everywhere else — `edtui`'s own Normal mode uses those
for real vim motions). `Tab`/`Shift+Tab` always work to switch panes;
`Ctrl+Enter`/`F5` (send) and `Ctrl+S` (save) always work too. To quit or
open the environment manager, `Tab` to Sidebar/Headers/Path Params first.

Small popups (new collection/request/environment name, header key/value,
path param value) are **not** full vim buffers — they open ready to type
into immediately and stay that way; `Enter` confirms, `Esc` cancels. Full
vim editing there would be more surprising than useful for "type a name and
hit Enter".

## Keybindings

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | Cycle focus between panes |
| `Ctrl+Enter` / `F5` | Send the current request |
| `Ctrl+S` | Save the loaded request back to its collection |
| `q` | Quit (Sidebar/Headers/Path Params focus only — see Editing model) |
| `e` | Open the environment manager (same restriction) |

### Sidebar
| Key | Action |
|---|---|
| `j`/`k`, `↓`/`↑` | Move selection |
| `Enter` | Expand/collapse a collection, or load a request into the editor |
| `a` | New request in the selected collection |
| `A` | New collection |
| `d` | Delete selected collection/request |

### Method
`h`/`l` or `←`/`→` — cycle HTTP method.

### URL / Body / Response (edtui vim buffers)

`{{variable}}` in the URL/headers/body is substituted from the active
environment. Standard `edtui` vim keybindings apply (full list: run
`cargo doc --open -p edtui` or see its
[crate docs](https://docs.rs/edtui)); the ones you'll use most:

| Key | Action |
|---|---|
| `i` | Insert mode (type normally) |
| `Esc` | Back to Normal mode |
| `h`/`j`/`k`/`l`, arrows | Move |
| `gg` / `G` | Jump to top / bottom |
| `w`/`b`/`e` | Word motions |
| `v` | Visual mode; `y` yanks the selection to the **system clipboard** |
| `dd`/`dw`/`x` | Delete line / word / character |
| `/pattern` `Enter` | Search; `n`/`N` next/previous match |
| `u` / `Ctrl+r` | Undo / redo |

Body-only: `f` (Normal mode) pretty-prints the body in place if it's valid
JSON. Response-only: `F4` (Normal mode) opens the first `http(s)://` link on
the cursor's line in your browser — not a vim thing, our own addition.

The **Response pane is always a live buffer** — no separate "enter/exit view
mode" step, just focus it (`Tab`) and navigate. It's rebuilt fresh only when
a *new* response arrives, so search/selection state naturally survives
losing and regaining focus in between.

### Path Params
Postman-style path variables, e.g. `:tenantId` in
`{{baseUrl}}/:tenantId/document-baskets`. This is a dedicated per-request
panel between URL and Headers — separate from environments:

- Typing a `:name` segment into the URL adds it here automatically (live,
  as you type), empty by default.
- Removing it from the URL removes it from this panel.
- `j`/`k` to move selection, `i`/`Enter` opens a small popup to edit the
  value (the name itself isn't editable — it's derived from the URL).
- Values are per-request and saved with the request (`Ctrl+S`).

Only a `:` that starts a whole `/`-delimited URL segment counts, so
`https://host:8080/path` is untouched — that's a port, not a param.

### Headers
| Key | Action |
|---|---|
| `n` | New header |
| `i` / `Enter` | Edit selected header |
| `d` | Delete selected header |
| `space` | Toggle header enabled/disabled |

### Environment manager (`e`)
| Key | Action |
|---|---|
| `n` | New environment |
| `Enter` | Set as active environment |
| `v` | Edit variables (`n` new, `i`/`Enter` edit, `d` delete) |
| `d` | Delete environment |
| `Esc` | Close |

## Scope

v1 covers: sending requests (any method), custom headers, request body, a
vim-navigable/searchable/syntax-highlighted response viewer
(status/headers/body/timing), collections of saved requests, and
environments with `{{var}}` substitution.

Not yet implemented: scripting/tests, auth helpers beyond raw headers,
Postman collection import, request history view.
