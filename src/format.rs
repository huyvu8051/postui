//! Reusable JSON formatting: detect-and-pretty-print, shared by the Response
//! pane's auto-formatting and the Body panel's `f` pretty-print shortcut.
//!
//! Syntax *coloring* used to live here too (hand-rolled `ratatui` spans),
//! but that's now handled natively by `edtui::SyntaxHighlighter` wherever
//! these panes are rendered (see `ui.rs`) — it colors JSON while you
//! navigate it, which the hand-rolled version couldn't do.

use serde_json::Value;

/// Parse `body` as JSON and re-serialize it pretty-printed (2-space indent,
/// original key order). Returns `None` if it isn't valid JSON.
pub fn pretty_json(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    serde_json::to_string_pretty(&value).ok()
}
