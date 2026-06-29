//! Selection — caret/anchor/reversed-offset model with grapheme, word, and line movement.
//!
//! A single `Selection` covers both the collapsed caret and an extended range.
//! The fields are:
//!
//! - `head`: the actively moving end of the selection (caret when collapsed).
//! - `anchor`: the fixed end of the selection (`None` for a freshly placed caret).
//! - `reversed`: whether `head < anchor` (the user is dragging from end to start).
//!
//! The public `range_bounds()` method always returns a normalized `Range<usize>` with
//! `start <= end`, so callers don't need to care about direction. The `head`
//! and `anchor` accessors are kept for cases (like mouse drag) where direction
//! matters.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    head: usize,
    anchor: Option<usize>,
    reversed: bool,
}

impl Selection {
    /// Build a collapsed caret at `offset` (no anchor, no extension).
    pub fn caret(offset: usize) -> Self {
        Self {
            head: offset,
            anchor: None,
            reversed: false,
        }
    }

    /// Build a forward selection from `start` to `end` (collapsed if equal).
    pub fn range(start: usize, end: usize) -> Self {
        if start <= end {
            Self {
                head: end,
                anchor: Some(start),
                reversed: false,
            }
        } else {
            Self {
                head: start,
                anchor: Some(end),
                reversed: true,
            }
        }
    }

    /// The moving side of the selection. For a collapsed caret this is the
    /// caret offset.
    pub fn head(&self) -> usize {
        self.head
    }

    /// The fixed side of the selection, or `None` for a collapsed caret
    /// freshly placed without explicit anchor.
    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    /// True if the user extended the selection backwards (`head < anchor`).
    pub fn is_reversed(&self) -> bool {
        self.reversed
    }

    /// True if the selection is collapsed to a single offset.
    pub fn is_collapsed(&self) -> bool {
        self.anchor.is_none_or(|start| start == self.head)
    }

    /// Normalized range with `start <= end`. Collapsed carets return `head..head`.
    pub fn range_bounds(&self) -> std::ops::Range<usize> {
        match self.anchor {
            Some(anchor) if anchor <= self.head => anchor..self.head,
            Some(anchor) => self.head..anchor,
            None => self.head..self.head,
        }
    }

    /// Move the caret to `offset`, collapsing any extension. The anchor is cleared.
    pub fn set_caret(&mut self, offset: usize) {
        self.head = offset;
        self.anchor = None;
        self.reversed = false;
    }

    /// Extend the selection to `offset`. The anchor is fixed on first extension.
    pub fn select_to(&mut self, offset: usize) {
        // Set anchor if first extension.
        if self.anchor.is_none() {
            self.anchor = Some(self.head);
        }
        let anchor = self.anchor.unwrap();

        // Fully collapse when offset matches both ends (true no-op).
        if offset == anchor && offset == self.head {
            return;
        }
        // Move head to anchor: collapses the selection.
        if offset == anchor {
            self.head = offset;
            self.anchor = None;
            self.reversed = false;
            return;
        }
        if offset < anchor {
            self.head = anchor;
            self.anchor = Some(offset);
            self.reversed = true;
        } else if offset > self.head && self.reversed {
            // Crossing back from reversed into forward.
            self.anchor = Some(self.head);
            self.head = offset;
            self.reversed = false;
        } else if offset < self.head && !self.reversed {
            // Crossing into reversed direction.
            self.anchor = Some(self.head);
            self.head = offset;
            self.reversed = true;
        } else {
            self.head = offset;
        }
    }

    /// Replace the entire selection with an explicit start..end range and direction.
    pub fn set_range(&mut self, start: usize, end: usize) {
        if start == end {
            self.head = start;
            self.anchor = None;
            self.reversed = false;
        } else if start < end {
            self.head = end;
            self.anchor = Some(start);
            self.reversed = false;
        } else {
            self.head = start;
            self.anchor = Some(end);
            self.reversed = true;
        }
    }
}

/// Movement units supported by selection navigation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Movement {
    /// Previous grapheme cluster to the left of `offset`. Returns `0` if `offset` is already at the start.
    GraphemeLeft,
    /// Next grapheme cluster to the right of `offset`. Returns `text.len()` at the end.
    GraphemeRight,
    /// Start of the word containing or ending at `offset`.
    WordLeft,
    /// Start of the next word after `offset`.
    WordRight,
    /// Start of the visual line containing `offset`.
    LineStart,
    /// End of the visual line containing `offset`.
    LineEnd,
    /// Start of the document.
    DocumentStart,
    /// End of the document.
    DocumentEnd,
}

/// Apply a movement from a starting offset on `text`, returning the new offset.
///
/// All returned offsets are guaranteed to land on a UTF-8 character boundary.
pub fn apply_movement(text: &str, start: usize, movement: Movement) -> usize {
    let start = start.min(text.len());
    match movement {
        Movement::GraphemeLeft => text
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < start).then_some(idx))
            .unwrap_or(0),
        Movement::GraphemeRight => text
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > start).then_some(idx))
            .unwrap_or(text.len()),
        Movement::WordLeft => {
            // Find the start of the word at or preceding `start`.
            let mut last_match: Option<usize> = None;
            for (idx, word) in text.unicode_word_indices() {
                let end = idx + word.len();
                if end <= start {
                    last_match = Some(idx);
                } else if idx < start {
                    // Carret is inside or at end of this word — go to word start.
                    last_match = Some(idx);
                } else {
                    break;
                }
            }
            last_match.unwrap_or(0)
        }
        Movement::WordRight => {
            // Find the start of the next word strictly after `start`.
            for (idx, _) in text.unicode_word_indices() {
                if idx > start {
                    return idx;
                }
            }
            text.len()
        }
        Movement::LineStart => text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0),
        Movement::LineEnd => text[start..]
            .find('\n')
            .map(|i| start + i)
            .unwrap_or(text.len()),
        Movement::DocumentStart => 0,
        Movement::DocumentEnd => text.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_round_trip() {
        let s = Selection::caret(7);
        assert!(s.is_collapsed());
        assert_eq!(s.head(), 7);
        assert_eq!(s.range_bounds(), 7..7);
    }

    #[test]
    fn forward_and_reversed_selections() {
        let s = Selection::range(2, 5);
        assert!(!s.is_collapsed());
        assert!(!s.is_reversed());
        assert_eq!(s.range_bounds(), 2..5);

        let r = Selection::range(7, 3);
        assert!(r.is_reversed());
        assert_eq!(r.range_bounds(), 3..7);
    }

    #[test]
    fn select_to_extends_then_collapses() {
        let mut s = Selection::caret(5);
        s.select_to(10);
        assert_eq!(s.range_bounds(), 5..10);
        assert!(!s.is_reversed());
        s.select_to(2);
        assert!(s.is_reversed());
        assert_eq!(s.range_bounds(), 2..5);
        s.select_to(8);
        assert!(!s.is_reversed());
        assert_eq!(s.range_bounds(), 5..8);
    }

    #[test]
    fn select_to_doesnt_collapse_when_re_extending() {
        let mut s = Selection::caret(2);
        s.select_to(3);
        assert_eq!(s.range_bounds(), 2..3);
        // Re-extending to the same offset must NOT collapse.
        s.select_to(3);
        assert_eq!(s.range_bounds(), 2..3);
        s.select_to(4);
        assert_eq!(s.range_bounds(), 2..4);
    }

    #[test]
    fn grapheme_movement_skips_emoji() {
        let text = "a😀b";
        assert_eq!(apply_movement(text, 0, Movement::GraphemeRight), 1);
        assert_eq!(apply_movement(text, 1, Movement::GraphemeRight), 5);
        assert_eq!(apply_movement(text, 5, Movement::GraphemeRight), 6);
        assert_eq!(apply_movement(text, 6, Movement::GraphemeRight), 6);
        assert_eq!(apply_movement(text, 6, Movement::GraphemeLeft), 5);
        assert_eq!(apply_movement(text, 5, Movement::GraphemeLeft), 1);
        assert_eq!(apply_movement(text, 1, Movement::GraphemeLeft), 0);
    }

    #[test]
    fn line_movement_with_multi_line_text() {
        let text = "first\nsecond\nthird";
        assert_eq!(apply_movement(text, 9, Movement::LineStart), 6);
        assert_eq!(apply_movement(text, 9, Movement::LineEnd), 12);
        assert_eq!(apply_movement(text, 0, Movement::LineStart), 0);
        // Last line has no trailing newline: LineEnd lands on document end.
        assert_eq!(apply_movement(text, 17, Movement::LineEnd), 18);
        assert_eq!(apply_movement(text, 13, Movement::LineEnd), 18);
    }

    #[test]
    fn line_end_single_line_goes_to_text_end() {
        let text = "hello world";
        assert_eq!(apply_movement(text, 0, Movement::LineEnd), text.len());
        assert_eq!(apply_movement(text, 5, Movement::LineEnd), text.len());
    }

    #[test]
    fn word_movement_bouncing() {
        let text = "hello world there";
        assert_eq!(apply_movement(text, 5, Movement::WordRight), 6);
        // From inside "world" (offset 7), WordRight goes to start of "there".
        assert_eq!(apply_movement(text, 7, Movement::WordRight), 12);
        // From inside "there" (offset 12 is exact start), WordLeft goes back to "world".
        assert_eq!(apply_movement(text, 12, Movement::WordLeft), 6);
        // From exact start of "world" we go back to start of "hello".
        assert_eq!(apply_movement(text, 6, Movement::WordLeft), 0);
    }

    #[test]
    fn document_movement() {
        let text = "abc";
        assert_eq!(apply_movement(text, 2, Movement::DocumentStart), 0);
        assert_eq!(apply_movement(text, 0, Movement::DocumentEnd), 3);
    }
}
