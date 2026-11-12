# Keybindings

postui has two layers of keys:

- **Pane navigation** (*Navigate* mode): `hjkl` moves focus between panes, the
  focused border is blue. `Enter` enters the pane (*Interact* mode, green
  border); `Esc` leaves it again.
- **Inside a vim buffer** (URL, raw Body, Script, Console, Response) every key
  goes to the editor. `Esc` in Normal mode returns to pane navigation. Only
  `F5`/`Ctrl+Enter` (send) and `F3` (maximize) work from inside a buffer.

## Global

| Key | Action |
|---|---|
| `h` `j` `k` `l` / arrows | Move focus between panes |
| `Enter` | Enter the focused pane |
| `Esc` | Leave the pane (from Normal mode in a vim buffer) |
| `F5` / `Ctrl+Enter` | Send the request (see [requests](requests.md#sending) about `Ctrl+Enter` in tmux) |
| `F3` | Maximize / restore the focused pane |
| `Shift+H` / `Shift+L` | Previous / next open tab (wraps) |
| `q` | Quit |

## Leader (`space` + key)

Available whenever you are not inside a vim buffer. Pane shortcuts focus the
pane **and enter it**; vim buffers open in Normal mode.

| Key | Action |
|---|---|
| `space u` | URL |
| `space m` | Method |
| `space p` | Path params |
| `space h` | Headers |
| `space b` | Body |
| `space s` | Script editor |
| `space c` | Script console |
| `space r` | Response |
| `space a` | Show the Actual request pane (what was really sent) and enter it; again (while in it) to hide it |
| `space t` | Show the tabs sidebar and enter it; again (while in it) to hide it |
| `space f` | Collection browser |
| `space e` | Environment manager |
| `space H` | Request history |
| `space 1` … `space 9` | Open the request in that quick slot |
| `space w` | Save the loaded request |
| `space W` | Save all requests with unsaved edits |
| `space z` | Maximize / restore the focused pane |

## Pane-specific (Navigate mode, pane focused but not entered)

| Pane | Key | Action |
|---|---|---|
| Body | `[` / `]` | Previous / next body type (Raw, Form Data, Binary) |
| Script | `[` / `]` | Switch Pre-request / Post-response |
| Response | `[` / `]` | Older / newer response from this request's history |
| Response | `x` | Run the post-response script on the response on screen |
| Response | `o` | Open the whole body (pretty-printed JSON) in `$PAGER` (default `less`) |
| Response | `S` | Save the body, byte for byte, to a file (see [saving bodies](requests.md#saving-a-response-body)) |

## Vim buffers

Standard [edtui](https://docs.rs/edtui) vim keys. The most used:

| Key | Action |
|---|---|
| `i` `a` `A` `o` | Insert mode |
| `Esc` | Normal mode |
| `h` `j` `k` `l`, `w` `b` `e`, `0` `$`, `gg` `G` | Motions |
| `v` / `V` | Visual mode; `y` yanks to the system clipboard |
| `dd` `dw` `x`, `p` | Delete, paste |
| `/pattern` `Enter`, `n` / `N` | Search |
| `u` / `Ctrl+R` | Undo / redo |

Extras added by postui:

| Where | Key (Normal mode) | Action |
|---|---|---|
| Raw body | `f` | Pretty-print the body if it is valid JSON |
| Response | `F4` | Open the first `http(s)://` link on the cursor line in the browser |

### `{{variable}}` completion (Insert mode)

In the URL, raw Body, header/form Key/Value popup and path-param popup:

| Key | Action |
|---|---|
| `{{` | Auto-closes to `{{│}}` and opens the variable menu |
| `Ctrl+N` / `Ctrl+P`, `↓` / `↑` | Move in the menu |
| `Tab` / `Enter` | Insert the variable, jump past `}}` |
| `Esc` | Close the menu (stay in Insert) |
| `Ctrl+Space` | Reopen the menu inside `{{…}}` |
| `Backspace` | In an empty `{{│}}`, delete the whole pair |

## Lists (Interact mode)

| Pane | Keys |
|---|---|
| Method | `h` / `l` cycle the HTTP method |
| Headers | `j`/`k` move, `n` new, `i`/`Enter` edit, `d` delete, `x` enable/disable |
| Body: Form Data | as Headers, plus `t` toggles the value between Text and File |
| Body: Binary | `i`/`Enter` set the file path, `d` clear it |
| Path params | `j`/`k` move, `i`/`Enter` edit the value |

## Tabs sidebar (`space t`)

| Key | Action |
|---|---|
| `j` / `k`, `g` / `G` | Move |
| `Enter` / `l` | Open the tab |
| `d` / `x` | Close the tab (asks `y/n` if it has unsaved edits) |
| `Shift+J` / `Shift+K` | Move the tab down / up |
| `m` then `1`–`9` | Assign the tab to a quick slot (`space N` opens it) |
| `m` then `0` | Clear the tab's quick slot |

## Collection browser (`space f`)

Search input (opens focused):

| Key | Action |
|---|---|
| typing | Fuzzy-filter requests (empty query lists recent requests) |
| `↑` / `↓`, `Ctrl+N` / `Ctrl+P` | Move |
| `Enter` | Open the request / expand or collapse the collection |
| `Esc` | Go to the list |

List:

| Key | Action |
|---|---|
| `j` / `k`, `g` / `G` | Move (empty query shows the whole tree) |
| `l` / `→`, `h` / `←` | Expand / collapse (on a request: open it / jump to its collection) |
| `Enter` | Open / toggle |
| `a` / `A` | New request in the collection / new collection |
| `d` | Delete (asks `y/n`) |
| `/` or `i` | Back to the search input |
| `Esc` | Close |

`Ctrl+R` (new request), `Ctrl+A` (new collection) and `Ctrl+D` (delete) work
in both parts.

## Popups

| Popup | Keys |
|---|---|
| Key/Value (header, form field, env variable) | `hjkl` choose field, `Enter`/`i` edit it (vim), `Esc` in Normal back to choosing, `Ctrl+S`/`Ctrl+Enter` confirm, `Esc` while choosing cancels |
| Single input (names, file path, path-param value) | vim buffer opening in Normal; `Enter` confirms, `Esc` in Normal cancels |
| Environment manager | `j`/`k`, `n` new, `Enter` activate, `v` edit variables, `d` delete, `Esc` close |
| Environment variables | `j`/`k`, `n` new, `i`/`Enter` edit, `d` delete, `Esc` back |
| Request history (`space H`) | `j`/`k`, `g`/`G`, `Enter` open the request, `Esc`/`q` close |
| Save response body (`S`) | vim path input; `Tab` completes folders/files (again to cycle), `Enter` saves; if the file exists: `o` overwrite, `n` new name, `Esc` back |
