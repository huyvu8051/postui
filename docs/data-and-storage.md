# Data and storage

postui keeps everything in one directory:

| OS | Location |
|---|---|
| macOS | `~/Library/Application Support/postui/` |
| Linux | `$XDG_DATA_HOME/postui/` (usually `~/.local/share/postui/`) |
| Windows | `%APPDATA%\postui\` (untested) |

## Files

| File | Contents | Written |
|---|---|---|
| `workspace.json` | Collections and saved requests, environments, open tabs, quick slots, recent requests | On every structural change (save, create, delete, opening a request, environment edits) |
| `drafts.json` | Per request: unsaved edits and the undo/redo history of its buffers | About 1 s after you stop typing (at most ~5 s while typing continuously) and on quit, only if something changed |
| `responses.jsonl` | Response history, one JSON object per line. Only a summary of each is kept in memory; a body is read from disk when you view it, and the latest one is shown again when you reopen the request | Appended after every response; compacted at startup |
| `history.json` | Sent-request log (time, method, URL, name, status, duration) | After every send |
| `postui.log` | Errors (failed requests, failed saves) | When something goes wrong |

`drafts.json` and `responses.jsonl` are written through a temporary file and
a rename, so a crash can't leave them half-written.

## Limits

| What | Limit |
|---|---|
| Undo history per buffer | 100 steps, and at most ~2 million characters in total (each step stores a full copy of the text, so large bodies keep fewer steps) |
| Response history | 30 per request, 1000 in total |
| Response body kept in history | 2 MB (longer bodies are truncated and marked) |
| Sent-request log | 500 entries |

## Privacy and security

Everything is stored as plain JSON. That includes environment values (often
tokens and passwords), request headers, and the full text of responses in
`drafts.json` and `responses.jsonl`. Protect the directory like you would a
`.env` file, and don't sync it to shared storage.

## Backups

The files are plain JSON and safe to copy while postui is closed. To start
from scratch, quit postui and move the directory away.
