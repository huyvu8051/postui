# Contributing to postui

Thanks for helping! Bug reports, feature ideas, docs fixes and code are all
welcome.

## Before you start

- **Bugs**: open an issue with the bug template; include your OS, terminal
  (and whether you use tmux), and steps to reproduce.
- **Features**: open an issue first for anything bigger than a small fix, so
  we can agree on the behaviour (especially new keybindings) before you write
  code.
- **Security issues**: don't open a public issue; see [SECURITY.md](SECURITY.md).

## Development setup

You need Rust 1.88 or newer (`rustup update`).

```bash
git clone https://github.com/huyvu8051/postui && cd postui
cargo run            # debug build
cargo run --release  # what users run
```

Your real data directory is used when you run postui locally (see
[docs/data-and-storage.md](docs/data-and-storage.md)). To experiment without
touching it, point `HOME` somewhere else:

```bash
HOME=$(mktemp -d) cargo run
```

## Checks

CI runs these on every push and pull request; run them before opening a PR:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

For changes to rendering, drafts, response history or persistence, also run
`scripts/audit.sh` (macOS/Linux): it stresses postui with large responses and
bodies and fails if it gets stuck at high CPU.

## Code layout

[docs/architecture.md](docs/architecture.md) maps the modules and explains the
event flow, drafts and persistence. Rules of thumb:

- Keep state changes in `App` methods (`app.rs`), key routing in `event.rs`,
  and drawing in `ui.rs`.
- Self-contained features go in their own module with a doc comment at the top
  describing the behaviour (see `completion.rs`).
- Keybindings are user-facing API: update [docs/keybindings.md](docs/keybindings.md)
  and the relevant docs page in the same PR.

## Tests

- Add a test for new behaviour and for every bug fix.
- Tests that build an `App` must start with
  `let _storage = crate::test_support::isolated_storage();`. Many `App`
  methods write to the data directory; the guard redirects `HOME` to a
  temporary directory so tests never touch your real workspace.
- Key handling can be tested end to end by calling `event::handle_key` with
  `KeyEvent`s (see the tests in `event.rs` and `completion.rs`). Uppercase
  letters need `KeyModifiers::SHIFT`, as real terminals send them.

## Vendored edtui

`vendor/edtui` is a patched copy of edtui; see
[docs/architecture.md](docs/architecture.md#vendored-edtui). Keep changes there
minimal, mark them in `vendor/edtui/Cargo.toml`, and prefer contributing
general fixes upstream to [edtui](https://github.com/preiter93/edtui).

## Pull requests

- One topic per PR; keep unrelated refactors out.
- Describe what changed and how you tested it (including manual testing in a
  terminal for UI changes).
- Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md) for
  user-visible changes.

## License

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed under the MIT and Apache-2.0 licenses, without
any additional terms or conditions.
