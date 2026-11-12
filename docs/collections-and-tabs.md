# Collections, tabs and saving

## Collections

Requests live in collections. The collection browser (`space f`) is a large
popup with a search box on top and the collection tree below.

- It opens in the search box: an empty query shows your recent requests,
  typing fuzzy-filters all requests. `Enter` opens the selection.
- `Esc` moves to the list, which shows the whole tree when the query is
  empty. Navigate it like vim: `j`/`k`, `g`/`G`, `l` expands, `h` collapses,
  `Enter` opens.
- `a` creates a request in the selected collection, `A` a collection, `d`
  deletes (after a `y/n` confirmation). Deleting is written to disk at once.
- `/` or `i` returns to the search box, `Esc` closes.

Renaming and moving requests are not implemented yet.

## Tabs

Every request you open becomes a tab. The list (and its order) is saved, so
your tabs come back after a restart, together with the request you had open.

`space t` shows the tabs sidebar and enters it; pressing it again while
you're in it hides it. With the sidebar visible, `h` from the left-hand panes
also reaches it.

```
┌ Tabs (4) ─────────────────────┐
│▶*1 POST   create basket       │   ▶  loaded request
│  3 PUT    upload document     │   *  unsaved edits
│ *  GET    list signers        │   1  quick slot (space 1)
│    DELETE cancel basket       │
└ Enter: open, d: close, … ─────┘
```

- `Enter` / `l` opens a tab, `d` / `x` closes it. Closing a tab with unsaved
  edits asks `y/n`; confirming discards the edits and the undo history.
- `Shift+J` / `Shift+K` reorder tabs.
- `Shift+H` / `Shift+L` cycle through tabs from anywhere outside a vim
  buffer, even with the sidebar hidden.

### Quick slots

Like vim marks: select a tab, press `m`, then a digit `1`–`9`. From then on
`space <digit>` opens that request from anywhere. `m` `0` clears the tab's
slot.

A request has at most one slot and a slot holds one request; assigning a
digit that's taken moves it (the status line says which request lost it).
Slots are saved with the workspace and dropped when their request is deleted.

## Unsaved edits and saving

Editing a request doesn't change the saved copy until you save:

- `space w` saves the loaded request.
- `space W` saves every request with unsaved edits.

Requests with unsaved edits show `*` in the tabs sidebar and the collection
browser. Unsaved edits are not at risk in the meantime: postui keeps a draft
per request, with its undo history and last response, and autosaves the
drafts to disk (see [data and storage](data-and-storage.md)). Switching tabs
or quitting keeps them; only closing a tab (with confirmation) discards them.
