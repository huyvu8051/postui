# Environments and scripting

## Environments

An environment is a named list of variables. One environment is active at a
time; its variables replace `{{name}}` placeholders when a request is sent.

`space e` opens the environment manager:

| Key | Action |
|---|---|
| `n` | New environment |
| `Enter` | Make the selected environment active |
| `v` | Edit its variables (`n` new, `i`/`Enter` edit, `d` delete, `Esc` back) |
| `d` | Delete the environment |
| `Esc` | Close |

### Substitution rules

- Placeholders are replaced in the URL, path-param values, header keys and
  values, the raw body, form-data keys and text values, and file paths
  (form-data File fields and the Binary body).
- Replacement is plain text: every `{{name}}` whose `name` is a variable of
  the active environment becomes its value. Unknown placeholders stay as they
  are, which usually makes the request fail visibly (e.g. "not a valid URL").
- Scripts and the response are not substituted.

While typing in those inputs, `{{` opens a completion menu of the active
environment's variables (see [editing](editing.md#variable-completion)).

## Scripts

Each request has a **pre-request** and a **post-response** script, written in
Lua 5.4 with a Postman-like `pm` API. Edit them in the Script pane
(`space s`; `[` / `]` in Navigate mode switches between the two). Output from
`pm.console.log` and test results appear in the Console (`space c`).

- The pre-request script runs before every send and can change the request.
- The post-response script runs after every successful response. `x` in the
  Response pane runs it again on demand, against the response on screen.

### API

| API | Pre | Post | Description |
|---|---|---|---|
| `pm.request.method` | ✓ | | HTTP method (read-only) |
| `pm.request.url` | ✓ | | URL; assign to change it |
| `pm.request.body` | ✓ | | Raw body; assign to change it |
| `pm.request.headers` | ✓ | | Table of enabled headers; edit it to change what's sent |
| `pm.response.code` | | ✓ | Status code |
| `pm.response.body` | | ✓ | Body as a string |
| `pm.response.headers` | | ✓ | Table of response headers |
| `pm.response.json()` | | ✓ | Body parsed as JSON into Lua tables (errors if not JSON) |
| `pm.environment.get(k)` | ✓ | ✓ | Variable value or `nil` |
| `pm.environment.set(k, v)` | ✓ | ✓ | Create or update a variable (numbers are converted to strings) |
| `pm.environment.has(k)` | ✓ | ✓ | `true` if the variable exists |
| `pm.environment.unset(k)` | ✓ | ✓ | Remove a variable |
| `pm.test(name, fn)` | | ✓ | Runs `fn`; logs `PASS name` or `FAIL name: error` |
| `pm.console.log(...)` | ✓ | ✓ | Print to the Console |
| `pm.clipboard.set(text)` | ✓ | ✓ | Copy text to the system clipboard |

`pm.environment` works with both the Postman-style dot (`pm.environment.set(…)`)
and Lua's method colon (`pm.environment:set(…)`). It exists only when an
environment is active.

### Examples

Store a token from a login response:

```lua
local body = pm.response.json()
pm.environment.set("token", body.accessToken)
pm.test("logged in", function()
  assert(pm.response.code == 200, "status " .. pm.response.code)
end)
```

Add a header before sending:

```lua
pm.request.headers["X-Request-Id"] = tostring(math.random(1e9))
```

### Sandbox and limits

- Only Lua's `table`, `string` and `math` libraries are loaded: no file
  system, no `os`, no network, no `require`.
- A script is stopped after roughly five million Lua instructions, so an
  endless loop can't freeze the UI.
- A failing pre-request script cancels the send; a failing post-response
  script is reported in the status line and the Console.

### Known limitations

- Variables changed by a script update the active environment in memory and
  are written to disk with the next workspace save (opening a request,
  editing environments, saving).
- Postman collections imported elsewhere may carry JavaScript scripts; those
  need rewriting in Lua.
