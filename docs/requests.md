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

If you switch to another tab while a request is in flight, its response is
stored on the request that sent it, not on the one on screen.

## Response

The Response pane shows status, timing, headers and body. JSON bodies are
pretty-printed and highlighted. It is a vim buffer: `gg`/`G`, `/search`,
`v`…`y` to copy, `F4` opens the first link on the cursor line.

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
