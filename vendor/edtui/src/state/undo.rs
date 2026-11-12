//! This module contains a "brute-force" implementation of undo and redo.
//! It stores the entire editor state at each action.
//!
//! This approach works for basic undo/redo needs, but its not the most efficient.
//! In the long run, this should be replaced with an action-based mechanism.
use crate::{EditorState, Index2, Lines};

#[derive(Debug, Clone)]
pub(crate) struct Stack {
    inner: Vec<UndoState>,
    max_size: usize,
}

impl Stack {
    pub(crate) fn new() -> Self {
        Self {
            inner: Vec::new(),
            max_size: 100,
        }
    }

    pub(crate) fn pop(&mut self) -> Option<UndoState> {
        self.inner.pop()
    }

    pub(crate) fn push(&mut self, value: UndoState) {
        self.inner.push(value);
        if self.len() > self.max_size {
            self.remove(0);
        }
    }

    fn len(&mut self) -> usize {
        self.inner.len()
    }

    fn remove(&mut self, index: usize) {
        self.inner.remove(index);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct UndoState {
    lines: Lines,
    cursor: Index2,
}

/// Plain-text copy of one undo/redo entry, for persisting history outside the editor.
/// (postui patch — not part of upstream edtui.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoSnapshot {
    pub text: String,
    pub row: usize,
    pub col: usize,
}

impl UndoState {
    fn to_snapshot(&self) -> UndoSnapshot {
        UndoSnapshot {
            text: self.lines.to_string(),
            row: self.cursor.row,
            col: self.cursor.col,
        }
    }

    fn from_snapshot(s: &UndoSnapshot) -> Self {
        Self {
            lines: Lines::from(s.text.as_str()),
            cursor: Index2::new(s.row, s.col),
        }
    }
}

impl EditorState {
    /// The undo and redo stacks, oldest entry first.
    pub fn undo_history(&self) -> (Vec<UndoSnapshot>, Vec<UndoSnapshot>) {
        (
            self.undo.inner.iter().map(UndoState::to_snapshot).collect(),
            self.redo.inner.iter().map(UndoState::to_snapshot).collect(),
        )
    }

    /// Replace the undo and redo stacks (oldest entry first), e.g. after a restart.
    pub fn set_undo_history(&mut self, undo: &[UndoSnapshot], redo: &[UndoSnapshot]) {
        self.undo = Stack::new();
        for s in undo {
            self.undo.push(UndoState::from_snapshot(s));
        }
        self.redo = Stack::new();
        for s in redo {
            self.redo.push(UndoState::from_snapshot(s));
        }
    }

    pub(crate) fn capture(&mut self) {
        let editor_state = UndoState {
            lines: self.lines.clone(),
            cursor: self.cursor,
        };
        self.undo.push(editor_state);
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let current = UndoState {
                lines: self.lines.clone(),
                cursor: self.cursor,
            };
            self.lines = prev.lines;
            self.cursor = prev.cursor;
            self.redo.push(current);
        }
    }

    pub fn redo(&mut self) {
        if let Some(prev) = self.redo.pop() {
            let current = UndoState {
                lines: self.lines.clone(),
                cursor: self.cursor,
            };
            self.lines = prev.lines;
            self.cursor = prev.cursor;
            self.undo.push(current);
        }
    }
}
