# Architecture

postui is a single binary crate. This page is a map for contributors.

## Event flow

```
input thread (crossterm) ──► AppEvent::Key / Paste / Tick ──┐
HTTP task (reqwest)      ──► AppEvent::Response ────────────┤
                                                            ▼
                                  main.rs: run() loop
                                    │  event::handle_key / handle_paste
                                    │  app.deliver_response, autosave on Tick
                                    ▼
                                  App (state)  ──► ui::draw (ratatui)
```

- The input thread polls the terminal every 100 ms and sends a `Tick` when
  idle; ticks only trigger the debounced autosave and never redraw.
- Sending clones the request, runs the pre-request script, and spawns a tokio
  task; the response comes back as an event tagged with the request it
  belongs to.

## Modules (`src/`)

| Module | Responsibility |
|---|---|
| `main.rs` | Terminal setup, event loop, send/receive wiring |
| `app.rs` | `App`: all UI and session state, and the operations on it (load/save requests, tabs, quick slots, drafts, response history) |
| `event.rs` | Key routing: popups, leader keys, Navigate/Interact handling per pane |
| `ui.rs` | Rendering with ratatui; one `draw_*` function per pane or popup |
| `completion.rs` | `{{variable}}` completion on top of an edtui buffer |
| `model.rs` | Serializable data: `Workspace`, `Collection`, `RequestItem`, `Environment`, response/history records |
| `session.rs` | Drafts persisted across restarts, including undo/redo snapshots |
| `storage.rs` | Reading and writing the files in the data directory |
| `script.rs` | Sandboxed Lua runtime and the `pm` API |
| `http.rs` | Building and sending the request (substitution, body types) |
| `format.rs` | JSON pretty-printing |
| `logger.rs` | Append-only error log |
| `test_support.rs` | Test helpers (isolated storage) |

## Conventions worth knowing

- **Which inputs get variable completion** is decided in one place,
  `App::var_input()`. Key handling for those inputs goes through
  `event::feed_var_input`, which hands the key to `completion::handle_key`.
- **Drafts**: `App::current` plus the four live `EditorState`s are the
  request on screen; every other edited request lives in `App::drafts`
  (`RequestDraft`, including its editor buffers so undo survives). `is_dirty`
  compares a draft with the saved copy in the workspace.
- **Persistence**: `App::persist()` writes `workspace.json`;
  `save_session()` writes `drafts.json` and is called from the debounced
  autosave and on quit.

## Vendored edtui

`vendor/edtui` is edtui 0.11.7, used through `[patch.crates-io]` in
`Cargo.toml`. The only change is in `vendor/edtui/src/state/undo.rs`:
`UndoSnapshot`, `EditorState::undo_history()` and `set_undo_history()` expose
the undo/redo stacks so drafts can store them. When upgrading edtui, copy the
new release over `vendor/edtui` and re-apply that patch (keep
`vendor/edtui/LICENSE.md`).

Because of the patch the crate is marked `publish = false`; install from git.

## Tests

`cargo test` runs unit tests in `app.rs`, `event.rs`, `completion.rs`,
`session.rs` and `script.rs`. Anything that builds an `App` can write to the
data directory, so such tests must hold `test_support::isolated_storage()`,
which points `HOME` at a temporary directory and serializes those tests.
