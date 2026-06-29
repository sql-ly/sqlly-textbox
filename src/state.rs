//! Pure text-editing state — text, selection, IME marked text, mode, validation,
//! and history. No GPUI types; fully unit-testable.
//!
//! All offsets are UTF-8 byte offsets. Conversion to UTF-16 happens at the
//! `EntityInputHandler` boundary in the GPUI entity layer.

use std::ops::Range;

use crate::history::History;
use crate::mode::{Mode, TextWrap};
use crate::selection::{apply_movement, Movement, Selection};
use crate::utf::{
    ceil_char_boundary, clamp_range, floor_char_boundary, utf16_offset_to_utf8, utf16_range_to_utf8,
};
use crate::validation::ValidationState;

/// Classifies a mutation for undo coalescing. Consecutive same-kind edits at
/// contiguous offsets fold into a single undo step; `Other` always breaks the
/// run (paste, replace, newline, IME, programmatic set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditKind {
    Insert,
    Delete,
    Other,
}

/// Pure, GPUI-agnostic text input state.
///
/// Holds the canonical text and selection, plus mode, validation, IME marked
/// text, and history. All mutations go through methods on this struct so we
/// guarantee clamping, newline normalization, and undo recording in one place.
#[derive(Clone, Debug)]
pub struct TextBoxState {
    text: String,
    selection: Selection,
    marked_range: Option<Range<usize>>,
    mode: Mode,
    validation: ValidationState,
    disabled: bool,
    read_only: bool,
    password: bool,
    history: History,
    /// Kind of the most recent recorded edit (drives undo coalescing).
    last_edit: EditKind,
    /// Caret offset immediately after the last coalescable edit. A new edit
    /// coalesces only when its anchor offset equals this value.
    coalesce_caret: Option<usize>,
    /// Monotonically increasing counter incremented after every mutation.
    /// Used by the renderer to invalidate cached layout.
    pub version: u64,
}

impl TextBoxState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: Mode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub fn mode(&self) -> &Mode {
        &self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn set_selection_range(&mut self, start: usize, end: usize) {
        let start = self.clamp_offset(start);
        let end = self.clamp_offset(end);
        self.selection.set_range(start, end);
        self.break_coalescing();
    }

    pub fn select_to(&mut self, offset: usize) {
        let offset = self.clamp_offset(offset);
        self.selection.select_to(offset);
        self.break_coalescing();
    }

    pub fn marked_range(&self) -> Option<&Range<usize>> {
        self.marked_range.as_ref()
    }

    /// Clear the IME marked range *without* deleting the composed text. Used by
    /// `unmark_text` when the IME accepts the composition.
    pub fn clear_marked_range(&mut self) {
        self.marked_range = None;
    }

    pub fn validation(&self) -> &ValidationState {
        &self.validation
    }

    pub fn set_validation(&mut self, state: ValidationState) {
        self.validation = state;
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub fn is_password(&self) -> bool {
        self.password
    }

    pub fn set_password(&mut self, password: bool) {
        self.password = password;
    }

    pub fn wrap(&self) -> TextWrap {
        match self.mode {
            Mode::SingleLine => TextWrap::None,
            Mode::MultiLine { wrap, .. } => wrap,
        }
    }

    pub fn min_lines(&self) -> usize {
        match self.mode {
            Mode::SingleLine => 1,
            Mode::MultiLine { min_lines, .. } => min_lines,
        }
    }

    pub fn max_lines(&self) -> Option<usize> {
        match self.mode {
            Mode::SingleLine => Some(1),
            Mode::MultiLine { max_lines, .. } => max_lines,
        }
    }

    pub fn can_edit(&self) -> bool {
        !self.disabled && !self.read_only
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Replace the entire text, collapsing the selection to the end and clearing history.
    /// Owned entry point used by external `.value(...)` calls.
    pub fn set_text(&mut self, text: impl Into<String>) {
        let mut text = text.into();
        text = self.normalize_text(text);
        self.text = text.clone();
        let end = text.len();
        self.selection.set_caret(end);
        self.marked_range = None;
        self.history.clear();
        self.break_coalescing();
        self.version += 1;
    }

    /// Insert raw text at the current caret (or replace current selection).
    /// Caller is responsible for normalization; usually use [`Self::replace_range`].
    pub fn insert(&mut self, text: &str) {
        self.replace_range(None, text);
    }

    /// Replace a UTF-8 byte range. When `range` is `None`, the current selection is used.
    /// When `range` is `Some`, supplied ranges are clamped to char boundaries and mode rules.
    /// Inserted text is normalized before being applied.
    ///
    /// Single non-whitespace character insertions into a collapsed selection
    /// coalesce into the current undo step; all other edits start a new step.
    pub fn replace_range(&mut self, range: Option<Range<usize>>, text: &str) {
        if !self.can_edit() {
            return;
        }
        let text = self.normalize_text(text.to_string());
        let range_is_none = range.is_none();

        let target = match range {
            Some(r) => clamp_range(&self.text, &r),
            None => self.selection.range_bounds(),
        };

        // Classify for undo coalescing: a single non-whitespace char typed into
        // a collapsed selection extends the current run; everything else breaks.
        let is_typing = range_is_none
            && self.selection.is_collapsed()
            && text.chars().count() == 1
            && !text.chars().next().map(char::is_whitespace).unwrap_or(true);
        let kind = if is_typing {
            EditKind::Insert
        } else {
            EditKind::Other
        };

        self.record_snapshot(kind, target.start);

        self.text.replace_range(target.clone(), &text);
        let caret = target.start + text.len();
        self.selection.set_caret(caret);
        self.marked_range = None;
        self.coalesce_caret = if kind == EditKind::Insert {
            Some(caret)
        } else {
            None
        };
        self.version += 1;
    }

    /// Replace a UTF-8 byte range without recording an undo snapshot.
    /// Used by IME composition to keep marked-text edits inside the same
    /// coalesced history entry. When `mark` is true, the inserted (normalized)
    /// text span is recorded as the IME marked range.
    pub fn replace_range_silent(&mut self, range: Range<usize>, text: &str, mark: bool) {
        if !self.can_edit() {
            return;
        }
        let text = self.normalize_text(text.to_string());
        let target = clamp_range(&self.text, &range);
        self.text.replace_range(target.clone(), &text);
        let new_caret = target.start + text.len();
        self.selection.set_caret(new_caret);
        self.marked_range = if mark && !text.is_empty() {
            Some(target.start..new_caret)
        } else {
            None
        };
        self.last_edit = EditKind::Other;
        self.coalesce_caret = None;
        self.version += 1;
    }

    /// Record a pre-mutation undo snapshot unless this edit coalesces with the
    /// previous one (same kind, contiguous offset).
    fn record_snapshot(&mut self, kind: EditKind, anchor: usize) {
        let coalesce = kind != EditKind::Other
            && self.last_edit == kind
            && self.coalesce_caret == Some(anchor);
        if !coalesce {
            self.history.push(self.text.clone(), self.selection.clone());
        }
        self.last_edit = kind;
    }

    /// Break the current coalescing run so the next typed character starts a
    /// fresh undo step. Called after any non-edit caret/selection change.
    fn break_coalescing(&mut self) {
        self.last_edit = EditKind::Other;
        self.coalesce_caret = None;
    }

    /// Clamp an offset to a valid UTF-8 char boundary within the text buffer.
    fn clamp_offset(&self, offset: usize) -> usize {
        floor_char_boundary(&self.text, offset.min(self.text.len()))
    }

    /// Move the caret/selection by a movement unit. If `extend` is true the
    /// selection is extended rather than collapsed.
    pub fn move_by(&mut self, movement: Movement, extend: bool) {
        let text_len = self.text.len();
        let start = match (extend, self.selection.anchor()) {
            (false, _) => self.selection.head(),
            (true, Some(anchor)) => {
                if self.selection.is_reversed() {
                    anchor
                } else {
                    self.selection.head().min(anchor)
                }
            }
            (true, None) => self.selection.head(),
        };
        let start = floor_char_boundary(&self.text, start.min(text_len));
        let new_offset = apply_movement(&self.text, start, movement).min(text_len);
        let new_offset = ceil_char_boundary(&self.text, new_offset.min(text_len));
        if extend {
            self.selection.select_to(new_offset);
        } else {
            self.selection.set_caret(new_offset);
        }
        self.marked_range = None;
        self.break_coalescing();
    }

    pub fn line_start(&mut self, extend: bool) {
        self.move_by(Movement::LineStart, extend);
    }

    pub fn line_end(&mut self, extend: bool) {
        self.move_by(Movement::LineEnd, extend);
    }

    pub fn document_start(&mut self, extend: bool) {
        self.move_by(Movement::DocumentStart, extend);
    }

    pub fn document_end(&mut self, extend: bool) {
        self.move_by(Movement::DocumentEnd, extend);
    }

    pub fn select_all(&mut self) {
        let end = self.text.len();
        self.selection.set_range(0, end);
        self.marked_range = None;
        self.break_coalescing();
    }

    pub fn collapse_to(&mut self, offset: usize) {
        let offset = floor_char_boundary(&self.text, offset.min(self.text.len()));
        self.selection.set_caret(offset);
        self.marked_range = None;
        self.break_coalescing();
    }

    pub fn move_to(&mut self, offset: usize) {
        self.collapse_to(offset);
    }

    /// Delete the current selection if non-empty, otherwise delete the
    /// grapheme before (or, if `forward=true`, after) the caret.
    pub fn delete(&mut self, forward: bool) {
        if !self.can_edit() {
            return;
        }
        if !self.selection.is_collapsed() {
            self.replace_range(None, "");
            return;
        }
        let cursor = self.selection.head();
        let target = if forward {
            let next = apply_movement(&self.text, cursor, Movement::GraphemeRight);
            cursor..next
        } else {
            let prev = apply_movement(&self.text, cursor, Movement::GraphemeLeft);
            prev..cursor
        };
        if target.start == target.end {
            return;
        }
        // Coalesce consecutive single-grapheme deletions. The compare anchor is
        // the caret *before* this delete; it equals the caret left after the
        // previous delete for a contiguous run (backspace walks left, forward
        // delete stays put).
        self.record_snapshot(EditKind::Delete, cursor);
        self.text.replace_range(target.clone(), "");
        let new_offset = target.start;
        self.selection.set_caret(new_offset);
        self.marked_range = None;
        self.coalesce_caret = Some(new_offset);
        self.version += 1;
    }

    /// Delete the current selection (no-op if collapsed or not editable).
    pub fn delete_selection(&mut self) {
        if self.selection.is_collapsed() || !self.can_edit() {
            return;
        }
        self.replace_range(None, "");
    }

    /// Snapshot current text/selection for undo without performing a mutation.
    /// Useful before bulk operations like paste. Forces a new undo step.
    pub fn snapshot_for_undo(&mut self) {
        self.history.push(self.text.clone(), self.selection.clone());
        self.break_coalescing();
    }

    pub fn undo(&mut self) -> bool {
        let Some(entry) = self
            .history
            .pop_undo((self.text.clone(), self.selection.clone()))
        else {
            return false;
        };
        self.text = entry.text;
        self.selection = entry.selection;
        self.marked_range = None;
        self.break_coalescing();
        self.version += 1;
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(entry) = self
            .history
            .pop_redo((self.text.clone(), self.selection.clone()))
        else {
            return false;
        };
        self.text = entry.text;
        self.selection = entry.selection;
        self.marked_range = None;
        self.break_coalescing();
        self.version += 1;
        true
    }

    pub fn selected_text(&self) -> String {
        let r = self.selection.range_bounds();
        if r.start == r.end || r.start >= self.text.len() {
            return String::new();
        }
        let start = floor_char_boundary(&self.text, r.start.min(self.text.len()));
        let end = floor_char_boundary(&self.text, r.end.min(self.text.len()));
        if start >= end {
            return String::new();
        }
        self.text[start..end].to_string()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    fn normalize_text(&self, text: String) -> String {
        let to_space = matches!(self.mode, Mode::SingleLine);
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    // Consume a following `\n` so CRLF maps to a single replacement.
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    out.push(if to_space { ' ' } else { '\n' });
                }
                '\n' => out.push(if to_space { ' ' } else { '\n' }),
                other => out.push(other),
            }
        }
        out
    }

    pub fn utf16_to_utf8(&self, offset: usize) -> usize {
        utf16_offset_to_utf8(&self.text, offset)
    }

    pub fn utf8_to_utf16(&self, offset: usize) -> usize {
        crate::utf::utf8_offset_to_utf16(&self.text, offset)
    }

    pub fn utf8_range_to_utf16(&self, range: Range<usize>) -> Range<usize> {
        crate::utf::utf8_range_to_utf16(&self.text, range)
    }

    pub fn utf16_range_to_utf8(&self, range_utf16: &Range<usize>) -> Range<usize> {
        utf16_range_to_utf8(&self.text, range_utf16.clone())
    }
}

impl Default for TextBoxState {
    fn default() -> Self {
        Self {
            text: String::new(),
            selection: Selection::caret(0),
            marked_range: None,
            mode: Mode::default(),
            validation: ValidationState::default(),
            disabled: false,
            read_only: false,
            password: false,
            history: History::new(),
            last_edit: EditKind::Other,
            coalesce_caret: None,
            version: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> TextBoxState {
        TextBoxState::new()
    }

    #[test]
    fn insert_replaces_selection_and_collapses() {
        let mut st = s();
        st.set_text("hello");
        st.collapse_to(2);
        st.move_by(Movement::GraphemeRight, true);
        st.move_by(Movement::GraphemeRight, true);
        st.replace_range(None, "X");
        assert_eq!(st.text(), "heXlo");
        assert_eq!(st.selection().head(), 3);
        assert_eq!(st.selection().range_bounds(), 3..3);
    }

    #[test]
    fn insert_via_two_extends() {
        // Manual grapheme selection: from 2, extend through 3 and 4 = "llo".
        // Should collapse caret to 4 (since re-extending to the same offset is
        // a no-op under our fixed select_to).
        let mut st = s();
        st.set_text("hello");
        st.collapse_to(2);
        st.select_to(2);
        st.select_to(3);
        st.select_to(4);
        // Should produce range 2..4.
        assert_eq!(st.selection().range_bounds(), 2..4);
        st.replace_range(None, "X");
        assert_eq!(st.text(), "heXo");
    }

    #[test]
    fn single_line_normalizes_newlines_to_spaces() {
        let mut st = s();
        st.set_text("");
        st.insert("a\nb\rc\r\nd");
        assert_eq!(st.text(), "a b c d");
    }

    #[test]
    fn multi_line_normalizes_crlf_to_lf() {
        let mut st = TextBoxState::with_mode(Mode::MultiLine {
            min_lines: 1,
            max_lines: None,
            wrap: TextWrap::Soft,
        });
        st.insert("a\r\nb\rc");
        assert_eq!(st.text(), "a\nb\nc");
    }

    #[test]
    fn read_only_blocks_mutation() {
        let mut st = s();
        st.set_read_only(true);
        st.set_text("hi");
        st.insert("more");
        assert_eq!(st.text(), "hi");
        st.delete(false);
        assert_eq!(st.text(), "hi");
    }

    #[test]
    fn disabled_blocks_mutation_but_allows_navigation() {
        let mut st = s();
        st.set_disabled(true);
        st.set_text("hi");
        st.insert("more");
        assert_eq!(st.text(), "hi");
        st.move_by(Movement::DocumentEnd, false);
        assert_eq!(st.selection().head(), 2);
        st.select_all();
        assert_eq!(st.selection().range_bounds(), 0..2);
    }

    #[test]
    fn backspace_collapses_then_deletes() {
        let mut st = s();
        st.set_text("hello");
        st.collapse_to(5);
        st.delete(false);
        assert_eq!(st.text(), "hell");
        assert_eq!(st.selection().head(), 4);
        st.delete(false);
        assert_eq!(st.text(), "hel");
    }

    #[test]
    fn forward_delete_removes_next_grapheme() {
        let mut st = s();
        st.set_text("hello");
        st.collapse_to(0);
        st.delete(true);
        assert_eq!(st.text(), "ello");
        assert_eq!(st.selection().head(), 0);
    }

    #[test]
    fn delete_with_selection_clears_all() {
        let mut st = s();
        st.set_text("hello world");
        st.collapse_to(6);
        st.select_all();
        st.delete(false);
        assert_eq!(st.text(), "");
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut st = s();
        // A caret move between each insert breaks coalescing, so every insert
        // becomes its own undo step (consecutive typing would otherwise merge).
        st.insert("a");
        st.move_to(0);
        st.move_to(st.text().len());
        st.insert("b");
        st.move_to(0);
        st.move_to(st.text().len());
        st.insert("c");
        assert_eq!(st.text(), "abc");
        assert!(st.undo());
        assert_eq!(st.text(), "ab");
        assert!(st.undo());
        assert_eq!(st.text(), "a");
        assert!(st.undo());
        assert_eq!(st.text(), "");
        assert!(!st.undo());
        assert!(st.redo());
        assert_eq!(st.text(), "a");
        assert!(st.redo());
        assert_eq!(st.text(), "ab");
        assert!(st.redo());
        assert_eq!(st.text(), "abc");
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut st = s();
        st.insert("a");
        st.insert("b");
        st.undo();
        st.undo();
        assert!(st.can_redo());
        st.insert("X");
        assert!(!st.can_redo());
    }

    #[test]
    fn line_and_document_movement() {
        let mut st = TextBoxState::with_mode(Mode::MultiLine {
            min_lines: 1,
            max_lines: None,
            wrap: TextWrap::Soft,
        });
        st.set_text("alpha\nbeta\ngamma");
        st.collapse_to(7);
        st.line_start(false);
        assert_eq!(st.selection().head(), 6);
        st.line_end(false);
        assert_eq!(st.selection().head(), 10);
        st.document_end(false);
        assert_eq!(st.selection().head(), 16);
        st.document_start(false);
        assert_eq!(st.selection().head(), 0);
    }

    #[test]
    fn utf16_offset_offsets_strictly_before() {
        let mut st = s();
        st.set_text("a你b");
        // 你 is in the BMP, so 1 utf-16 unit.
        assert_eq!(st.utf16_to_utf8(0), 0);
        assert_eq!(st.utf16_to_utf8(1), 1);
        assert_eq!(st.utf16_to_utf8(2), 4);
        assert_eq!(st.utf16_to_utf8(3), 5);
        assert_eq!(st.utf8_to_utf16(0), 0);
        assert_eq!(st.utf8_to_utf16(1), 1);
        assert_eq!(st.utf8_to_utf16(4), 2);
        assert_eq!(st.utf8_to_utf16(5), 3);
    }

    #[test]
    fn extend_movement_with_select_all() {
        let mut st = s();
        st.set_text("alpha\nbeta");
        st.collapse_to(0);
        st.move_by(Movement::DocumentEnd, true);
        assert_eq!(st.selection().range_bounds(), 0..10);
        st.select_all();
        assert_eq!(st.selection().range_bounds(), 0..10);
    }

    #[test]
    fn marked_range_reset_on_replace() {
        let mut st = s();
        st.set_text("hello");
        st.replace_range_silent(1..2, "u", true);
        assert!(st.marked_range().is_some());
        st.insert("X");
        assert!(st.marked_range().is_none());
    }

    #[test]
    fn version_increments_on_mutation() {
        let mut st = s();
        let v = st.version();
        st.set_text("hi");
        assert!(st.version() > v);
    }

    #[test]
    fn password_flag_does_not_affect_text() {
        let mut st = s();
        st.set_password(true);
        st.set_text("secret");
        assert_eq!(st.text(), "secret");
        assert!(st.is_password());
    }

    #[test]
    fn utf16_offset_round_trip_through_state() {
        let mut st = s();
        st.set_text("a你b");
        // 你 is in the BMP, so 1 utf-16 unit.
        assert_eq!(st.utf16_to_utf8(0), 0);
        assert_eq!(st.utf16_to_utf8(1), 1);
        assert_eq!(st.utf16_to_utf8(2), 4);
        assert_eq!(st.utf8_to_utf16(0), 0);
        assert_eq!(st.utf8_to_utf16(1), 1);
        assert_eq!(st.utf8_to_utf16(4), 2);
    }

    #[test]
    fn unicode_aware_delete() {
        let mut st = s();
        st.set_text("a😀b");
        st.collapse_to(5); // after the emoji
        st.delete(false);
        assert_eq!(st.text(), "ab");
    }

    #[test]
    fn typing_coalesces_into_one_undo_step_per_word() {
        let mut st = s();
        // Type "ab cd" one char at a time via single-char inserts.
        for ch in ["a", "b", " ", "c", "d"] {
            st.insert(ch);
        }
        assert_eq!(st.text(), "ab cd");
        // Word-level undo granularity: "cd" run, then the space, then "ab".
        assert!(st.undo());
        assert_eq!(st.text(), "ab ");
        assert!(st.undo());
        assert_eq!(st.text(), "ab");
        assert!(st.undo());
        assert_eq!(st.text(), "");
        assert!(!st.undo());
    }

    #[test]
    fn caret_move_breaks_coalescing() {
        let mut st = s();
        st.insert("a");
        st.insert("b");
        // Move caret: the next char must start a fresh undo step.
        st.collapse_to(0);
        st.insert("X");
        assert_eq!(st.text(), "Xab");
        assert!(st.undo());
        assert_eq!(st.text(), "ab");
        assert!(st.undo());
        assert_eq!(st.text(), "");
    }

    #[test]
    fn consecutive_backspaces_coalesce() {
        let mut st = s();
        st.set_text("hello");
        st.collapse_to(5);
        st.delete(false);
        st.delete(false);
        st.delete(false);
        assert_eq!(st.text(), "he");
        // All three deletions fold into one undo step.
        assert!(st.undo());
        assert_eq!(st.text(), "hello");
    }

    #[test]
    fn paste_is_its_own_undo_step() {
        let mut st = s();
        st.insert("a");
        st.insert("b");
        st.insert("c"); // "abc" — one coalesced run
        st.insert(" "); // breaks run (whitespace -> Other)
        st.replace_range(None, "PASTED"); // multi-char -> Other
        assert_eq!(st.text(), "abc PASTED");
        assert!(st.undo());
        assert_eq!(st.text(), "abc ");
        assert!(st.undo());
        assert_eq!(st.text(), "abc");
        assert!(st.undo());
        assert_eq!(st.text(), "");
    }

    #[test]
    fn clear_marked_range_keeps_text() {
        let mut st = s();
        st.set_text("hello");
        st.replace_range_silent(1..2, "u", true);
        assert!(st.marked_range().is_some());
        st.clear_marked_range();
        assert!(st.marked_range().is_none());
        // The composed text survives.
        assert_eq!(st.text(), "hullo");
    }

    #[test]
    fn line_end_single_line_goes_to_text_end() {
        let mut st = s();
        st.set_text("hello world");
        st.collapse_to(0);
        st.line_end(false);
        assert_eq!(st.selection().head(), 11);
    }

    #[test]
    fn delete_selection_noop_when_read_only() {
        let mut st = s();
        st.set_text("hello");
        st.select_all();
        st.set_read_only(true);
        st.delete_selection();
        assert_eq!(st.text(), "hello", "read-only must block deletion");
    }

    #[test]
    fn delete_selection_noop_when_disabled() {
        let mut st = s();
        st.set_text("hello");
        st.select_all();
        st.set_disabled(true);
        st.delete_selection();
        assert_eq!(st.text(), "hello", "disabled must block deletion");
    }

    #[test]
    fn selection_offsets_clamp_to_utf8_boundaries() {
        let mut st = s();
        // "a" (1 byte) + "é" (2 bytes) + "🦀" (4 bytes) + "z" (1 byte) = 8 bytes
        st.set_text("aé🦀z");
        // Offset 2 is inside "é" (bytes 1-2). Clamping should floor to 1.
        st.set_selection_range(2, 6);
        let r = st.selection().range_bounds();
        // Must be at char boundaries, not mid-codepoint.
        let _ = st.text()[r.clone()].to_string(); // must not panic
        assert!(r.start == 1 || r.start == 3, "start clamped to boundary");
    }

    #[test]
    fn selected_text_handles_stale_range() {
        let mut st = s();
        st.set_text("abc");
        // Set a selection then shrink the text so the selection is stale.
        st.set_selection_range(0, 10);
        let text = st.selected_text();
        assert_eq!(text, "abc", "selected_text clamps to available text");
    }

    #[test]
    fn selected_text_handles_mid_codepoint_range() {
        let mut st = s();
        st.set_text("aé🦀z");
        // Force a range that starts mid-codepoint via set_selection_range
        // (which should clamp), then verify selected_text doesn't panic.
        st.set_selection_range(2, 5);
        let _ = st.selected_text(); // must not panic
    }

    #[test]
    fn ime_mark_range_uses_normalized_text_length() {
        // In single-line mode, CRLF normalizes to a single space.
        // The mark range must span the normalized text, not the raw input.
        let mut st = s(); // SingleLine
        st.set_text("hello");
        st.collapse_to(5);
        st.replace_range_silent(5..5, "a\r\nb", true);
        let mark = st.marked_range().expect("mark should be set");
        // "a\r\nb" normalizes to "a b" (3 bytes) in single-line mode.
        // Mark should be 5..8, NOT 5..9 (raw "a\r\nb" is 4 bytes).
        assert_eq!(mark.end - mark.start, 3, "mark spans normalized text");
        assert_eq!(&st.text()[mark.clone()], "a b");
    }
}
