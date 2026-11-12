# Building requests

## Method

Focus the Method box and enter it (`space m`), then `h` / `l` cycle through
GET, POST, PUT, PATCH, DELETE, HEAD and OPTIONS.

## URL

A single-line vim buffer (`space u`). Use `{{variable}}` placeholders from the
active environment and `:name` segments for path parameters:

```
{{baseUrl}}/:tenantId/document-baskets?expand=signers
```

## Path params

Postman-style path variables. Typing a `:name` segment into the URL adds a
row to the Path Params panel (and removing it removes the row). Only a `:`
that starts a whole `/`-separated segment counts, so `https://host:8080/x` is
a port, not a parameter.

`i` / `Enter` on a row edits its value in a one-line vim popup. Values are
stored per request and may contain `{{variables}}`. The name comes from the
URL and is not editable.

## Headers

A list (`space h`): `n` new, `i`/`Enter` edit (Key/Value popup), `d` delete,
`x` enable/disable. Disabled headers are kept but not sent. Keys and values
may contain `{{variables}}`.

## Body

The Body pane's title shows the body type, e.g. `Body [Raw] Form Data Binary`.
With the pane focused in Navigate mode, `[` / `]` switch types. Each type keeps
its own data, so switching back shows what you had.

- **Raw**: a vim buffer, sent as-is after `{{variable}}` substitution. `f` in
  Normal mode pretty-prints valid JSON. JSON is syntax-highlighted when a
  `Content-Type` header mentions JSON or the text starts with `{` or `[`.
  postui does not add a `Content-Type` header for you.
- **Form Data** (`multipart/form-data`): a list like Headers. `t` toggles a
  field between **Text** and **File**; a File field's value is a path read
  from disk at send time (its file name is sent as the part's file name).
- **Binary**: the raw bytes of one file. `i` / `Enter` sets the path, `d`
  clears it; the pane shows whether the file exists.

## Sending

`F5` or `Ctrl+Enter`. Before sending, the pre-request script runs (see
[scripting](environments-and-scripting.md)); then `{{variables}}` and path
params are substituted.

`Ctrl+Enter` needs a terminal that reports it differently from `Enter`
(kitty keyboard protocol: kitty, WezTerm, Ghostty, iTerm2 with "Report
modifiers using CSI u", …). Inside **tmux** it usually arrives as a plain
`Enter`; either use `F5` or enable extended keys in `~/.tmux.conf`:

```
set -s extended-keys always
set -s extended-keys-format csi-u
set -as terminal-features 'xterm*:extkeys'
```

then restart the tmux server.

Requests are sent in the background, so you can keep working while one is in
flight, including switching to other requests. Only the request that is
sending shows `Response (sending...)`; when its response arrives it is stored
on that request (the status line tells you if it isn't the one on screen),
never on whatever is open at the time.

## Response

The Response pane shows status, timing, headers and body. JSON bodies are
pretty-printed and highlighted. It is a vim buffer: `gg`/`G`, `/search`,
`v`…`y` to copy, `F4` opens the first link on the cursor line.

### Large responses (light mode)

Bodies over 1 MB are shown in light mode: the Response pane loads only the
first 1 MB, raw (not pretty-printed or highlighted) and without undo, and the
title says `LIGHT MODE: first 1.0 MB of 4.7 MB`. A fully pretty-printed buffer
would take roughly twenty times the body size in memory. To see everything,
from the Response pane in Navigate mode:

- `o` opens the whole body, pretty-printed if it's JSON, in `$PAGER` (default
  `less`; e.g. `export PAGER="less -S"`). Quit the pager to return.
- `S` saves the body exactly as received (see below).

Both keys work for any response, not only large ones.

Independently of size, lines longer than 1000 characters (minified JSON or
HTML, base64 fields) are split for display, and the title says `long lines
split`; `o` and `S` still give you the original.

### Binary responses

Responses that aren't text (by `Content-Type`, or an `attachment`
`Content-Disposition`), and text that isn't valid UTF-8, are streamed to a
temp file while they download instead of being held in memory, so a multi-GB
zip costs no RAM. The Response pane shows `Binary body · 2.3 MB ·
application/zip — S: save to file`. Temp files live in the data directory's
`bodies/` folder and are removed when the response is replaced, its tab is
closed, or postui quits. Past binary responses in the history keep only their
size and type.

### Saving a response body

`S` in the Response pane (Navigate mode) opens a path input:

- It suggests the folder this request's body was last saved to (else your
  Downloads folder) and the file name from the server's `Content-Disposition`
  (else the name used last time, else `<request name>.<extension for the
  Content-Type>`). Server-provided names are reduced to a plain file name, so a
  response can't make postui write outside the folder you chose.
- `Tab` completes folder and file names like a shell; pressing it again cycles
  through the matches. A path ending in `/` (or an existing folder) saves the
  suggested file name into it.
- `Enter` saves. If the file exists you get a choice: `o` overwrites, `n` fills
  in a free name such as `documents (1).zip` (edit it if you like, then
  `Enter`), `Esc` goes back to editing the path.

The folder is remembered per request (in `workspace.json`), so the next save of
the same endpoint lands in the same place.

### Response history

Every response (or error) is kept per request: the last 30 per request, 1000
overall, with bodies over 2 MB truncated in the history. With the Response
pane focused in Navigate mode:

- `[` / `]` step to an older / newer response. The title shows
  `Response 3/7 · time · status` and the border turns yellow on a past
  response. Only the Response pane changes; the request is untouched.
- `x` runs the request's current post-response script against the response on
  screen (a past one, or the latest).

`space H` lists every request you sent (method, URL, status, duration);
`Enter` opens that request.
