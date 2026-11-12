# Editing model

## Layout

```
┌ Tabs ┐ ┌ Method ┐┌ URL ──────────────────────────────────────┐
│ (opt)│ └────────┘└───────────────────────────────────────────┘
│      │ ┌ Path Params ───────────────────────────────────────────┐
│      │ └────────────────────────────────────────────────────────┘
│      │ ┌ Headers ──────────────┐┌ Script ───────────────────────┐
│      │ │                       ││                               │
│      │ │                       │├ Console ──────────────────────┤
│      │ └───────────────────────┘└───────────────────────────────┘
│      │ ┌ Body ─────────────────┐┌ Actual request (opt) ─────────┐
│      │ │                       │├ Response ─────────────────────┤
│      │ │                       ││                               │
└──────┘ └───────────────────────┘└───────────────────────────────┘
 status line
```

The tabs sidebar is hidden until you press `space t`, and the Actual request
pane until you press `space a`. `F3` / `space z`
maximizes the focused pane.

## Navigate and Interact

Every pane has two states:

- **Navigate** (blue border): the pane is selected. `hjkl` moves to a
  neighbour, `space` shortcuts and `q` work, and a few panes have extra keys
  (`[`/`]` on Body, Script and Response; `x` on Response).
- **Interact** (green border): you are inside the pane. Lists take their own
  keys (`j`/`k`, `n`, `d`, …); vim buffers take every key.

`Enter` goes from Navigate to Interact, `Esc` comes back. In a vim buffer,
`Esc` first leaves Insert/Visual mode; `Esc` in Normal mode leaves the pane.
`space` + letter jumps straight into a pane in Interact mode.

## Vim buffers

The URL, raw Body, Script, Console and Response are
[edtui](https://github.com/preiter93/edtui) editors with real Normal, Insert,
Visual and Search modes. Multi-line buffers show the mode in a status line;
the single-line URL shows it in its title (`URL [INSERT]`) while you edit.

- Yanking (`y`) goes to the system clipboard.
- The **Response** is an editable buffer so you can search, select and copy;
  edits don't change anything and are replaced by the next response.
- The **Console** is read-only: motions, search and yank work, edits are
  reverted.
- Undo history is kept per request and survives tab switches and restarts
  (see [data and storage](data-and-storage.md)).

## Popups

- **Key/Value** (headers, form fields, environment variables): a large panel
  that works like the main UI. `hjkl` picks the Key or Value field, `Enter`
  or `i` edits it as a vim buffer, `Esc` in Normal mode goes back to picking,
  `Ctrl+S` / `Ctrl+Enter` confirms, `Esc` while picking cancels.
- **Single inputs** (new collection/request/environment name, binary file
  path, path-param value): one-line vim buffers that open in Normal mode.
  `Enter` confirms; `Esc` in Normal mode cancels. The title shows the mode.

## `{{variable}}` completion

Inputs whose text gets environment substitution (URL, raw Body, the
Key/Value popup, the path-param popup) complete variable names:

1. In Insert mode type `{{`. It becomes `{{│}}` and a menu lists the active
   environment's variables with a preview of each value.
2. Keep typing to fuzzy-filter. `Ctrl+N`/`Ctrl+P` or the arrows move.
3. `Tab` or `Enter` inserts the name and puts the cursor after `}}`.

`Esc` closes the menu without leaving Insert mode, `Ctrl+Space` reopens it
while the cursor is inside `{{…}}`, and `Backspace` in an empty `{{│}}`
removes the whole pair. The inserted text belongs to the current insert
session, so a single `u` undoes it.

Without an active environment the menu says so; activate one with `space e`.
