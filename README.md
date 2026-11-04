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
- **Body / Response**: a Raw/Form Data/Binary type selector, the body editor
  for whichever type is selected, and the last response. Raw body and
  Response are real vim buffers. A JSON response body is auto-detected, pretty-printed, and
  syntax-highlighted (`edtui`'s built-in `syntect`-based highlighter); the
  response title shows ", JSON" when that kicked in. The same detector
  (`src/format.rs`) backs the Body panel's `f` pretty-print shortcut.
- Bottom line: status bar / hints.

## Editing model

**URL, Response, and Body-in-Raw-mode are full `edtui` vim buffers** — real
Normal/Insert/Visual modes, not a plain text box (Body in Form Data/Binary
mode is a list instead — see "Body Type" below — and behaves like Headers).
While a vim buffer is focused, *every* key goes to that buffer (including
bare letters like `q`/`e`, which are global shortcuts everywhere else —
`edtui`'s own Normal mode uses those for real vim motions). `Esc` in Normal mode
leaves the buffer back to pane navigation (`hjkl`); `Ctrl+Enter`/`F5` (send) and
`Ctrl+S` (save) always work too. To quit or open the environment manager,
navigate to Sidebar/Headers/Path Params/Body Type/Body-in-Form-Data-or-Binary first.

The key/value popup (env variable, request header, form-field) is a 90%×90%
panel that works like the main UI: `hjkl` selects Key/Value, `Enter`/`i`
edits that field as a vim buffer (opens in Normal mode), `Esc` in Normal
returns to selection, `Ctrl+S`/`Ctrl+Enter` saves, `Esc` (while selecting)
cancels. The single-input popups (new collection/request/environment name,
binary file path) are vim buffers that open in Normal mode; `Enter`
confirms, `Esc` in Normal cancels. The path-param popup stays a plain
type-and-Enter box.

## Keybindings

| Key | Action |
|---|---|
| `h` `j` `k` `l` | Move focus between panes |
| `F3` | Maximize the focused pane full-screen; `F3` again to restore the layout |
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

Switching to another request and back doesn't lose anything: unsaved edits
and the last response/error are kept in memory per request for the rest of
the session, restored exactly as you left them — `Ctrl+S` is only needed to
persist those edits to disk (`workspace.json`), not to avoid losing them
while you work. (One edge case: if a request is still in flight when you
switch away, its response lands wherever `current` is when it arrives, not
necessarily back on the request that sent it.)

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

Body-only (Raw mode, see below): `f` (Normal mode) pretty-prints the body in
place if it's valid JSON. Response-only: `F4` (Normal mode) opens the first
`http(s)://` link on the cursor's line in your browser — not a vim thing,
our own addition.

The **Response pane is always a live buffer** — no separate "enter/exit view
mode" step, just focus it and navigate. It's rebuilt fresh only when
a *new* response arrives, so search/selection state naturally survives
losing and regaining focus in between.

### Body Type (Raw / Form Data / Binary)

A small selector sits above the Body pane — navigate to it (just below
Headers), `h`/`l` to cycle, like Method. It changes what the Body pane below
it is and does:

- **Raw** — the vim buffer described above. Free-form text, usually JSON.
- **Form Data** (`multipart/form-data`) — a list, just like Headers:
  | Key | Action |
  |---|---|
  | `n` | New field |
  | `i` / `Enter` | Edit selected field's key/value |
  | `d` | Delete selected field |
  | `space` | Toggle field enabled/disabled |
  | `t` | Toggle the value between **Text** and **File** (a path to upload) |

  A field's value is always typed as plain text in the edit popup — `t` just
  decides whether that string is sent as a literal value or read from disk
  as a file at send time. `{{var}}` substitution applies to both the key and
  the value (so a file path can use an environment variable too).
- **Binary** — sends one file's raw bytes as the entire body. `i`/`Enter`
  opens a popup to type the file path (pre-filled if one's already set);
  `d` clears it. The pane shows a ✓/✗ existence check against disk.

Switching modes doesn't lose data — the Raw text, Form Data fields, and
Binary path are all kept independently, so flipping back to a previous mode
shows what you had there.

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
- A value can itself use `{{variable}}`, resolved from the active
  environment at send time — same substitution as the URL/headers/body.

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
