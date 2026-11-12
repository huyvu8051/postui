//! Unsaved work that survives restarts: per-request drafts (edited request,
//! last response/error, and the editors' undo/redo stacks), stored in drafts.json.
use std::collections::HashMap;

use edtui::{EditorState, Lines, UndoSnapshot};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::{RequestItem, ResponseData};

/// The four vim buffers that make up one request's editable text.
#[derive(Clone)]
pub struct EditorBuffers {
    pub url: EditorState,
    pub body: EditorState,
    pub pre: EditorState,
    pub post: EditorState,
}

impl EditorBuffers {
    pub fn from_item(item: &RequestItem) -> Self {
        let mut url = EditorState::new(Lines::from(item.url.as_str()));
        url.set_single_line(true);
        Self {
            url,
            body: EditorState::new(Lines::from(item.body.as_str())),
            pre: EditorState::new(Lines::from(item.pre_request_script.as_str())),
            post: EditorState::new(Lines::from(item.post_response_script.as_str())),
        }
    }

    fn states(&self) -> [&EditorState; 4] {
        [&self.url, &self.body, &self.pre, &self.post]
    }

    pub fn to_stored(&self) -> StoredBuffers {
        StoredBuffers::capture(self.states())
    }

    pub fn from_stored(item: &RequestItem, stored: &StoredBuffers) -> Self {
        let mut buffers = Self::from_item(item);
        stored.url.restore(&mut buffers.url);
        stored.body.restore(&mut buffers.body);
        stored.pre.restore(&mut buffers.pre);
        stored.post.restore(&mut buffers.post);
        buffers
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredSnapshot {
    pub text: String,
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredHistory {
    #[serde(default)]
    pub undo: Vec<StoredSnapshot>,
    #[serde(default)]
    pub redo: Vec<StoredSnapshot>,
}

impl StoredHistory {
    fn capture(state: &EditorState) -> Self {
        let (undo, redo) = state.undo_history();
        let conv = |v: Vec<UndoSnapshot>| {
            v.into_iter()
                .map(|s| StoredSnapshot {
                    text: s.text,
                    row: s.row,
                    col: s.col,
                })
                .collect()
        };
        Self {
            undo: conv(undo),
            redo: conv(redo),
        }
    }

    fn restore(&self, state: &mut EditorState) {
        let conv = |v: &[StoredSnapshot]| -> Vec<UndoSnapshot> {
            v.iter()
                .map(|s| UndoSnapshot {
                    text: s.text.clone(),
                    row: s.row,
                    col: s.col,
                })
                .collect()
        };
        state.set_undo_history(&conv(&self.undo), &conv(&self.redo));
    }

    fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredBuffers {
    #[serde(default)]
    pub url: StoredHistory,
    #[serde(default)]
    pub body: StoredHistory,
    #[serde(default)]
    pub pre: StoredHistory,
    #[serde(default)]
    pub post: StoredHistory,
}

impl StoredBuffers {
    /// Undo/redo history of the URL, body, pre- and post-script buffers (in that order).
    pub fn capture(states: [&EditorState; 4]) -> Self {
        let [url, body, pre, post] = states.map(StoredHistory::capture);
        Self {
            url,
            body,
            pre,
            post,
        }
    }

    pub fn is_empty(&self) -> bool {
        [&self.url, &self.body, &self.pre, &self.post]
            .iter()
            .all(|h| h.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDraft {
    pub item: RequestItem,
    /// Only read, for drafts.json files from older versions: responses now live in
    /// responses.jsonl and are reloaded from there.
    #[serde(default, skip_serializing)]
    pub response: Option<ResponseData>,
    #[serde(default, skip_serializing)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub history: StoredBuffers,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredSession {
    #[serde(default)]
    pub drafts: HashMap<Uuid, StoredDraft>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use edtui::{EditorEventHandler, EditorMode};

    fn type_text(state: &mut EditorState, text: &str) {
        let mut events = EditorEventHandler::default();
        let key = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        // Real terminals send uppercase letters with SHIFT.
        events.on_key_event(
            KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT),
            state,
        );
        for c in text.chars() {
            events.on_key_event(key(c), state);
        }
        events.on_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), state);
        assert_eq!(state.mode, EditorMode::Normal);
    }

    #[test]
    fn undo_history_survives_a_json_round_trip() {
        let mut item = RequestItem::new("t");
        item.body = "x".into();
        let mut buffers = EditorBuffers::from_item(&item);
        type_text(&mut buffers.body, "hello");
        let edited = buffers.body.lines.to_string();
        assert_eq!(edited, "xhello");

        let mut stored_item = item.clone();
        stored_item.body = edited.clone();
        let draft = StoredDraft {
            item: stored_item,
            response: None,
            last_error: None,
            history: buffers.to_stored(),
        };
        let json = serde_json::to_string(&draft).unwrap();
        let back: StoredDraft = serde_json::from_str(&json).unwrap();

        let mut restored = EditorBuffers::from_stored(&back.item, &back.history);
        assert_eq!(restored.body.lines.to_string(), "xhello");
        restored.body.undo();
        assert_eq!(restored.body.lines.to_string(), "x");
        restored.body.redo();
        assert_eq!(restored.body.lines.to_string(), "xhello");
    }

    #[test]
    fn undo_history_of_a_large_buffer_is_bounded_by_size() {
        let line = "x".repeat(99) + "\n";
        let mut state = EditorState::new(Lines::from(line.repeat(3_000).as_str())); // 300k chars
        for _ in 0..30 {
            type_text(&mut state, "y"); // each insert session snapshots the whole buffer
        }
        let (undo, _) = state.undo_history();
        let chars: usize = undo
            .iter()
            .map(|s| s.text.chars().filter(|c| *c != '\n').count())
            .sum();
        assert!(!undo.is_empty(), "recent steps are still undoable");
        assert!(
            undo.len() < 30,
            "older snapshots were dropped ({} kept)",
            undo.len()
        );
        assert!(chars <= 2_000_000, "{chars} chars kept");
    }

    #[test]
    fn response_with_u128_duration_round_trips() {
        let resp = ResponseData {
            status: 200,
            headers: vec![("a".into(), "b".into())],
            body: "{}".into(),
            duration_ms: 1234,
        };
        let back: ResponseData =
            serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
        assert_eq!(back.duration_ms, 1234);
        assert!(StoredBuffers::default().is_empty());
    }
}
