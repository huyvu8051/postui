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
bare letters like `q`, which is a global shortcut everywhere else —
`edtui`'s own Normal mode uses those for real vim motions). `Esc` in Normal mode
leaves the buffer back to pane navigation (`hjkl`); `Ctrl+Enter`/`F5` (send)
always work too. To quit (`q`) or use a `space` shortcut (e.g. `space e` for
environments), `Esc` out of the vim buffer first.

The key/value popup (env variable, request header, form-field) is a 90%×90%
panel that works like the main UI: `hjkl` selects Key/Value, `Enter`/`i`
edits that field as a vim buffer (opens in Normal mode), `Esc` in Normal
returns to selection, `Ctrl+S`/`Ctrl+Enter` confirms the popup, `Esc` (while selecting)
cancels. The single-input popups (new collection/request/environment name,
binary file path, path-param value) are vim buffers that open in Normal
mode; `Enter` confirms, `Esc` in Normal cancels.

### `{{variable}}` completion
In the inputs that get environment substitution — URL, Body (Raw), the
header/form-field Key/Value popup, and the path-param value popup — typing
`{{` in Insert mode auto-closes it to `{{|}}` and opens a menu of the active
environment's variables (value preview on the right), filtered as you type.

| Key (menu open) | Action |
|---|---|
| `Ctrl+N` / `Ctrl+P`, `↓` / `↑` | Move |
| `Tab` / `Enter` | Insert the variable and jump past `}}` |
| `Esc` | Close the menu (stay in Insert; `Esc` again leaves it) |

`Ctrl+Space` reopens the menu while the cursor is inside `{{…}}`.
`Backspace` inside an empty `{{|}}` deletes the whole pair. One `u` undoes
the whole completion. The logic lives in `src/completion.rs`.

## Keybindings

| Key | Action |
|---|---|
| `h` `j` `k` `l` | Move focus between panes |
| `space z` / `F3` | Maximize the focused pane full-screen; again to restore the layout (`space z` works while selecting a pane; `F3` also works inside a vim buffer) |
| `Ctrl+Enter` / `F5` | Send the current request |
| `space w` | Save the loaded request back to its collection |
| `space W` | Save all requests with unsaved edits |
| `q` | Quit (Headers/Path Params/Method focus only — see Editing model) |

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
| `space 1`–`space 9` | Open the request in that quick slot (assign with `m` + digit in the tabs sidebar) |
| `space z` | Maximize / restore the focused pane |
| `space w` / `space W` | Save current request / save all unsaved requests |
| `space H` | Request history |

### Open tabs sidebar (`space t`)
Every request you open becomes a tab. The list is saved in `workspace.json`,
so it survives restarts (unsaved edits and responses don't, yet). The sidebar
is hidden by default; `space t` shows it on the left and focuses it, and
pressing it again while it's focused hides it. With it visible, `h` from
Method/Path Params/Headers/Body also reaches it.

Quick slots work like vim marks: select a tab, press `m` then a digit, and
`space <digit>` opens that request from anywhere (the sidebar can be hidden).
The digit shows in front of the tab. A request has at most one slot and a
slot one request — assigning a taken digit moves it. Slots are saved in
`workspace.json` and dropped when the request is deleted.

`▶` marks the loaded request, `*` marks unsaved edits (not yet saved with
`space w`/`space W`). The collection browser shows the same `*`.

| Key | Action |
|---|---|
| `j`/`k`, `g`/`G` | Move selection |
| `Enter` / `l` | Open the selected tab |
| `d` / `x` | Close the tab (asks `y/n` first if it has unsaved edits, which are then discarded) |
| `Shift+J` / `Shift+K` | Move the selected tab down / up |
| `m` then `1`–`9` | Assign the selected tab to quick slot N (`space N` opens it) |
| `m` then `0` | Clear the selected tab's quick slot |

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

### Unsaved work, undo history, and request history
Nothing is lost by switching requests or restarting: per request, postui
keeps the unsaved edits, the last response/error, and the undo/redo history
of its URL/Body/Script buffers (`u` / `Ctrl+R` in Normal mode). These are
autosaved to `drafts.json` next to `workspace.json` about a second after you
stop typing (at most ~5 s while typing continuously) and on quit; on start,
postui restores them and reopens the most recently used open tab. Saving
(`space w` current, `space W` all) only writes the edits into
`workspace.json` — undo history stays. Closing a tab with `d` discards its
draft and history. Undo depth is 100 steps per buffer (edtui's limit).

Every sent request is recorded in `history.json` (last 500). `space H`
opens it newest first — time, status (`ERR` for failures), duration, method,
name, URL — and `Enter` opens that request.

`edtui` is vendored in `vendor/edtui` with a small patch
(`src/state/undo.rs`) that exposes the undo/redo stacks so they can be
saved; see the note at the top of `vendor/edtui/Cargo.toml`.

### Response history (`[` / `]` in the Response pane)
Every response (or failure) is kept per request in `responses.jsonl` — the
last 30 per request, 1000 overall, bodies over 2 MB cut (marked "body
truncated"). Only the response is stored, not the request's inputs. With the
Response pane focused (Navigate mode, i.e. not inside the vim buffer), `[`
steps to an older response and `]` back towards the latest; the title shows
`Response 3/7 · time · status`, and the border turns yellow while you're
looking at a past response. This only changes what the Response pane shows —
none of the request's inputs change. Sending again jumps back to the latest.

`x` (same place) runs the request's post-response script against the
response on screen — the past one you're looking at, or the latest. It uses
the script as it is in the Script pane right now (unsaved edits included);
output goes to the Console and `pm.environment.set(...)` (or `:set`) updates the active
environment, just like after a real send.

A response always lands on the request that sent it: if you switch tabs while
a request is in flight, the response is stored on the original request (and
shown when you switch back), not on whatever is open when it arrives.

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
- Values are per-request and saved with the request (`space w`).
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

### Environment manager (`space e`)
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
