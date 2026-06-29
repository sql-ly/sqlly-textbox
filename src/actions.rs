//! Actions and default keybindings used by `TextBox`.
//!
//! Use [`install_text_box_keybindings`] once at app startup to register the
//! default editing keymap under the `TextBox` key context. Container views
//! drawing text inputs should set `.key_context("TextBox")` on the parent
//! `div` for these bindings to fire on focus.

use gpui::{actions, App, KeyBinding};

// Single namespace so all actions live under `text_box::*`.
actions!(
    text_box,
    [
        // Editing
        Backspace,
        Delete,
        // Movement
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        DocumentStart,
        DocumentEnd,
        // Selection-extension movement
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        // Selection
        SelectAll,
        // Clipboard
        Copy,
        Cut,
        Paste,
        // Undo/redo
        Undo,
        Redo,
        // IME composition commit
        InsertNewline,
        // Form-field commit (Enter on single-line, Cmd-Enter on multiline)
        Commit,
        // Convenience for "show character palette" on macOS.
        ShowCharacterPalette,
    ]
);

/// Install the default keybindings used by `TextBox` into the global keymap.
///
/// Safe to call multiple times — bindings are deduplicated by `(action, keystroke)`.
pub fn install_text_box_keybindings(cx: &mut App) {
    #[cfg(target_os = "macos")]
    let platform_bindings: Vec<KeyBinding> = vec![
        KeyBinding::new("alt-left", WordLeft, None),
        KeyBinding::new("alt-right", WordRight, None),
        KeyBinding::new("alt-shift-left", SelectWordLeft, None),
        KeyBinding::new("alt-shift-right", SelectWordRight, None),
        KeyBinding::new("cmd-up", DocumentStart, None),
        KeyBinding::new("cmd-down", DocumentEnd, None),
        KeyBinding::new("cmd-shift-up", SelectDocumentStart, None),
        KeyBinding::new("cmd-shift-down", SelectDocumentEnd, None),
        KeyBinding::new("cmd-shift-left", SelectWordLeft, None),
        KeyBinding::new("cmd-shift-right", SelectWordRight, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),
        KeyBinding::new("enter", InsertNewline, None),
        KeyBinding::new("cmd-enter", Commit, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
    ];

    #[cfg(not(target_os = "macos"))]
    let platform_bindings: Vec<KeyBinding> = vec![
        KeyBinding::new("ctrl-left", WordLeft, None),
        KeyBinding::new("ctrl-right", WordRight, None),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, None),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, None),
        KeyBinding::new("ctrl-home", DocumentStart, None),
        KeyBinding::new("ctrl-end", DocumentEnd, None),
        KeyBinding::new("ctrl-shift-home", SelectDocumentStart, None),
        KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, None),
        KeyBinding::new("ctrl-a", SelectAll, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("ctrl-x", Cut, None),
        KeyBinding::new("ctrl-v", Paste, None),
        KeyBinding::new("ctrl-z", Undo, None),
        KeyBinding::new("ctrl-y", Redo, None),
        KeyBinding::new("enter", InsertNewline, None),
        KeyBinding::new("ctrl-enter", Commit, None),
    ];

    let always_bindings: Vec<KeyBinding> = vec![
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("home", LineStart, None),
        KeyBinding::new("end", LineEnd, None),
        KeyBinding::new("shift-home", SelectLineStart, None),
        KeyBinding::new("shift-end", SelectLineEnd, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
    ];

    cx.bind_keys(always_bindings.into_iter().chain(platform_bindings));
}
