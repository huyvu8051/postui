# postui

A terminal HTTP client — a Postman alternative that lives in your terminal. Built with Rust, `ratatui`, `edtui`, and `reqwest`.

## Run

```bash
cargo run
```

Requests, collections, and environments are persisted to
`~/Library/Application Support/postui/workspace.json` (macOS) automatically after each change.

## Layout

- **Collections**: browse/search saved collections and requests in a popup (`space f`).
- **Method / URL** (top right): request line. URL is a real vim buffer (`edtui`).
- **Path Params**: auto-detected `:name` segments from the URL, per request.
- **Headers** (middle row, left) and **Scripts** (middle row, right): request
  headers beside the Pre-request / Post-response Lua editor and its Console.
  Focus the Script pane (without entering it) and press `[` / `]` to switch
  between Pre-request and Post-response.
- **Body / Response** (bottom row, side by side): a Raw/Form Data/Binary type selector, the body editor
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
navigate to Method/Headers/Path Params/Body-in-Form-Data-or-Binary first.

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
| `space z` / `F3` | Maximize the focused pane full-screen; again to restore the layout (`space z` works while selecting a pane; `F3` also works inside a vim buffer) |
| `Ctrl+Enter` / `F5` | Send the current request |
| `Ctrl+S` | Save the loaded request back to its collection |
| `q` | Quit (Headers/Path Params/Method focus only — see Editing model) |
| `e` | Open the environment manager (same restriction) |

### Leader shortcuts (`space` + key)

Work whenever you're not typing inside a vim buffer. Pane shortcuts focus the
pane **and enter it** (vim buffers land in Normal mode; `Esc` in Normal goes
back to pane navigation).

| Key | Action |
|---|---|
| `space u` | URL |
| `space m` | Method |
| `space p` | Path Params |
| `space h` | Headers |
| `space b` | Body |
| `space s` | Script editor |
| `space c` | Script Console (read-only vim buffer: `hjkl`, `gg`/`G`, `/` search, `v`+`y` copy) |
| `space r` | Response |
| `space e` | Environment manager |
| `space f` | Collection browser |
| `space t` | Show the open-tabs sidebar and jump into it; again (while in it) to hide it |
| `space z` | Maximize / restore the focused pane |

### Open tabs sidebar (`space t`)
Every request you open becomes a tab. The list is saved in `workspace.json`,
so it survives restarts (unsaved edits and responses don't, yet). The sidebar
is hidden by default; `space t` shows it on the left and focuses it, and
pressing it again while it's focused hides it. With it visible, `h` from
Method/Path Params/Headers/Body also reaches it.

`▶` marks the loaded request, `●` marks unsaved edits (not yet `Ctrl+S`).

| Key | Action |
|---|---|
| `j`/`k`, `g`/`G` | Move selection |
| `Enter` / `l` | Open the selected tab |
| `d` / `x` | Close the tab (asks `y/n` first if it has unsaved edits, which are then discarded) |
| `Shift+J` / `Shift+K` | Move the selected tab down / up |

`Shift+H` / `Shift+L` switch to the previous / next tab from anywhere outside
a vim buffer — the sidebar doesn't need to be visible. It wraps around at the
ends.

Closing the loaded request switches to the next tab, or clears the editor if
it was the last one. Deleting a request in the collection browser also
closes its tab.

### Collection browser (`space f`)
A 90%×90% popup with a search input on top and the collection list below.
It opens with the search input focused; an empty query lists **Recent**
requests, typing fuzzy-filters requests.

| Key (search input) | Action |
|---|---|
| type | Filter requests |
| `↑`/`↓`, `Ctrl+N`/`Ctrl+P` | Move selection |
| `Enter` | Load the selected request (or expand/collapse a collection) |
| `Esc` | Leave the input and browse the list |

| Key (list / browse) | Action |
|---|---|
| `j`/`k`, `↓`/`↑` | Move selection (empty query shows the full collection tree) |
| `g` / `G` | First / last entry |
| `l` / `→` | Expand a collection, or load a request |
| `h` / `←` | Collapse a collection, or jump to a request's collection |
| `Enter` | Expand/collapse a collection, or load a request |
| `a` | New request in the selected collection |
| `A` | New collection |
| `d` | Delete selected collection/request (asks `y/n` first; written to disk immediately) |
| `/` or `i` | Back to the search input |
| `Esc` | Close |

`Ctrl+R` (new request), `Ctrl+A` (new collection) and `Ctrl+D` (delete, also asks `y/n`) work
in both modes.

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

The body type lives inside the Body pane itself (its title shows
`Raw [Form Data] Binary`, active one bracketed). Focus the Body pane (just
below Headers) *without* entering it, then press `[` / `]` to cycle to the
previous / next type. It changes what the Body pane is and does:

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
