//! `sqlly-textbox` — a reusable, full-featured text input component for [GPUI].
//!
//! The crate exposes a [`TextBox`] entity with single-line and multi-line
//! modes, focus states, clipboard/undo/redo, IME support, validation state,
//! and a default key map. The component is built on top of a pure, GPUI-free
//! [`state::TextBoxState`] so the editing model can be unit-tested without
//! opening a window.
//!
//! ## Quick start
//!
//! ```ignore
//! use sqlly_textbox::{TextBox, install_text_box_keybindings};
//!
//! install_text_box_keybindings(cx);
//!
//! let field = cx.new(|cx| {
//!     TextBox::new(cx)
//!         .placeholder("Email")
//!         .on_change(Arc::new(|text, _cx| {
//!             println!("changed: {text}");
//!         }))
//! });
//! ```
//!
//! Use a `TextBox` anywhere you would render any other GPUI entity: nest it
//! inside a `div`, put a label above it, or stack several in a column.
//!
//! ## Feature overview
//!
//! - **Modes**: Single-line (default) or [multi-line](mode::MultiLine) with
//!   configurable minimum visible lines and wrap policy.
//! - **Editing**: Insert, delete, backspace, forward-delete, undo, redo.
//! - **Selection**: grapheme and word movement, line/document jumps,
//!   select-all, extend-with-shift.
//! - **Clipboard**: copy, cut, paste (paste allowed even for password
//!   fields; copy/cut of selected password contents is suppressed).
//! - **IME**: full `EntityInputHandler` integration with marked-text
//!   composition, UTF-16 offset conversion, and candidate window placement.
//! - **Validation**: external state via `validation_state(...)` and
//!   optional sync/async validators with debouncing.
//! - **Focus/disabled/read-only/password**: native fields.
//!
//! [GPUI]: https://github.com/zed-industries/zed/tree/main/crates/gpui

pub mod actions;
pub mod history;
pub mod mode;
pub mod selection;
pub mod state;
pub mod text_box;
pub mod text_box_element;
pub mod utf;
pub mod validation;

pub use actions::install_text_box_keybindings;
/// Re-exported to expose the full set of editing actions at the crate root.
pub use actions::{
    Backspace, Commit, Copy, Cut, Delete, DocumentEnd, DocumentStart, Down, InsertNewline, Left,
    LineEnd, LineStart, Paste, Redo, Right, SelectAll, SelectDocumentEnd, SelectDocumentStart,
    SelectDown, SelectLeft, SelectLineEnd, SelectLineStart, SelectRight, SelectUp, SelectWordLeft,
    SelectWordRight, ShowCharacterPalette, Undo, Up, WordLeft, WordRight,
};
pub use mode::{Mode, Placeholder, TextWrap};
pub use selection::{Movement, Selection};
pub use state::TextBoxState;
pub use text_box::TextBox;
pub use text_box::{AsyncValidator, ChangeCallback, CommitCallback, ComponentStyle};
pub use text_box_element::LastLayout;
pub use validation::{sync_validator, SyncValidator, ValidationState};
