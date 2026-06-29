//! Bounded undo/redo history.
//!
//! Records a snapshot of `(text, selection)` before each meaningful mutation.
//! The undo stack is capped at [`History::DEFAULT_MAX_ENTRIES`]; once exceeded,
//! the oldest entries are dropped. Coalescing of consecutive typing/deletion
//! edits into a single undoable step is decided by [`crate::state::TextBoxState`]
//! (it owns the edit semantics) — this struct is a plain bounded stack pair.

use crate::selection::Selection;

#[derive(Clone, Debug)]
pub struct Entry {
    pub text: String,
    pub selection: Selection,
}

#[derive(Clone, Debug)]
pub struct History {
    undo_stack: Vec<Entry>,
    redo_stack: Vec<Entry>,
    max_entries: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_entries: Self::DEFAULT_MAX_ENTRIES,
        }
    }
}

impl History {
    /// Maximum number of undo snapshots retained. Older entries are dropped
    /// once this cap is exceeded, bounding worst-case memory under heavy edits.
    pub const DEFAULT_MAX_ENTRIES: usize = 256;

    pub fn new() -> Self {
        Self::default()
    }

    /// Construct with a custom cap (minimum 1).
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            max_entries: max_entries.max(1),
            ..Self::default()
        }
    }

    /// Number of snapshots currently on the undo stack.
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Push a new snapshot onto the undo stack, dropping the oldest entries if
    /// the cap is exceeded. Clears the redo stack.
    pub fn push(&mut self, text: String, selection: Selection) {
        self.undo_stack.push(Entry { text, selection });
        self.redo_stack.clear();
        if self.undo_stack.len() > self.max_entries {
            let overflow = self.undo_stack.len() - self.max_entries;
            self.undo_stack.drain(0..overflow);
        }
    }

    /// True if there is an entry to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// True if there is an entry to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Pop an undo snapshot, pushing the given current `(text, selection)` onto
    /// the redo stack. Returns `None` if there's nothing to undo.
    pub fn pop_undo(&mut self, current: (String, Selection)) -> Option<Entry> {
        let entry = self.undo_stack.pop()?;
        self.redo_stack.push(Entry {
            text: current.0,
            selection: current.1,
        });
        Some(entry)
    }

    /// Pop a redo snapshot, pushing the given current `(text, selection)` onto
    /// the undo stack. Returns `None` if there's nothing to redo.
    pub fn pop_redo(&mut self, current: (String, Selection)) -> Option<Entry> {
        let entry = self.redo_stack.pop()?;
        self.undo_stack.push(Entry {
            text: current.0,
            selection: current.1,
        });
        Some(entry)
    }

    /// Discard all history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel() -> Selection {
        Selection::caret(0)
    }

    #[test]
    fn push_then_undo_redo() {
        let mut h = History::new();
        h.push("a".into(), sel());
        h.push("ab".into(), sel());
        let entry = h.pop_undo(("ab".into(), sel())).unwrap();
        assert_eq!(entry.text, "ab");
        let entry = h.pop_undo(("a".into(), sel())).unwrap();
        assert_eq!(entry.text, "a");
        assert!(!h.can_undo());
        let entry = h.pop_redo(("ab".into(), sel())).unwrap();
        assert_eq!(entry.text, "a");
        let entry = h.pop_redo(("foo".into(), sel())).unwrap();
        assert_eq!(entry.text, "ab");
        assert!(!h.can_redo());
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut h = History::new();
        h.push("a".into(), sel());
        h.pop_undo(("".into(), sel())).unwrap();
        assert!(h.can_redo());
        h.push("z".into(), sel());
        assert!(!h.can_redo());
    }

    #[test]
    fn empty_undo_redo_is_safe() {
        let mut h = History::new();
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert!(h.pop_undo(("x".into(), sel())).is_none());
        assert!(h.pop_redo(("x".into(), sel())).is_none());
    }

    #[test]
    fn undo_stack_is_bounded() {
        let mut h = History::with_capacity(3);
        for i in 0..10 {
            h.push(format!("v{i}"), sel());
        }
        assert_eq!(h.undo_depth(), 3);
        // Oldest entries dropped; the most recent pre-state survives.
        let entry = h.pop_undo(("v10".into(), sel())).unwrap();
        assert_eq!(entry.text, "v9");
    }
}
