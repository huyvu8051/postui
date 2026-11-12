## What and why

<!-- What does this change, and what problem does it solve? Link the issue: "Closes #123". -->

## How it was tested

<!-- Tests added/changed, and manual testing in a terminal for UI changes (which terminal/OS). -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Tests that build an `App` hold `test_support::isolated_storage()`
- [ ] Docs updated (`docs/`, keybindings) for user-visible changes
- [ ] Line added under **Unreleased** in `CHANGELOG.md` for user-visible changes
