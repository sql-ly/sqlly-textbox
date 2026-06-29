//! GPUI integration tests for the `TextBox` entity.
//!
//! These exercise the entity wiring that the pure-state unit tests cannot:
//! focus, the `EntityInputHandler` (IME) boundary, action handlers, clipboard
//! policy, and validation rendering state. They run on the headless GPUI test
//! harness via `#[gpui::test]`, which provides a `TestAppContext`.

use gpui::{EntityInputHandler, TestAppContext};
use sqlly_textbox::{sync_validator, Copy, Cut, Paste, SelectAll, TextBox, ValidationState};

/// Build a window whose root view is a fresh single-line `TextBox`.
fn single_line(cx: &mut TestAppContext) -> gpui::WindowHandle<TextBox> {
    cx.add_window(|_window, cx| TextBox::new(cx))
}

#[gpui::test]
fn typing_inserts_text_through_input_handler(cx: &mut TestAppContext) {
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            // Simulate the platform IME committing text (no marked range).
            input.replace_text_in_range(None, "hello", window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "hello");
}

#[gpui::test]
fn single_line_normalizes_newlines_on_insert(cx: &mut TestAppContext) {
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "a\nb\r\nc", window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    // Single-line mode collapses newlines to spaces.
    assert_eq!(text, "a b c");
}

#[gpui::test]
fn select_all_then_copy_and_paste(cx: &mut TestAppContext) {
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "abc", window, cx);
            input.select_all(&SelectAll, window, cx);
            input.copy(&Copy, window, cx);
            // Caret to end, then paste the clipboard contents after it.
            input.replace_text_in_range(None, "", window, cx); // no-op insert keeps caret
            input.paste(&Paste, window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    // "abc" selected+copied, selection replaced by paste of "abc" => "abc".
    assert_eq!(text, "abc");
}

#[gpui::test]
fn cut_removes_selection_and_fills_clipboard(cx: &mut TestAppContext) {
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "abcdef", window, cx);
            input.select_all(&SelectAll, window, cx);
            input.cut(&Cut, window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "");
    // Clipboard now holds the cut text; paste restores it.
    window
        .update(cx, |input, window, cx| {
            input.paste(&Paste, window, cx);
        })
        .unwrap();
    let restored = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(restored, "abcdef");
}

#[gpui::test]
fn password_mode_suppresses_copy_but_allows_paste(cx: &mut TestAppContext) {
    // First, put a known value on the clipboard via a normal field.
    let plain = single_line(cx);
    plain
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "fromclipboard", window, cx);
            input.select_all(&SelectAll, window, cx);
            input.copy(&Copy, window, cx);
        })
        .unwrap();

    let pw = cx.add_window(|_w, cx| TextBox::new(cx).password(true).value("secret"));
    // Copy must NOT overwrite the clipboard in password mode.
    pw.update(cx, |input, window, cx| {
        input.select_all(&SelectAll, window, cx);
        input.copy(&Copy, window, cx);
    })
    .unwrap();
    let clip = cx
        .read(|cx| cx.read_from_clipboard().and_then(|i| i.text()))
        .unwrap_or_default();
    assert_eq!(
        clip, "fromclipboard",
        "password copy must not touch clipboard"
    );

    // Paste IS allowed in password mode.
    pw.update(cx, |input, window, cx| {
        input.select_all(&SelectAll, window, cx);
        input.paste(&Paste, window, cx);
    })
    .unwrap();
    let text = pw
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "fromclipboard");
}

#[gpui::test]
fn sync_validator_runs_on_value_builder(cx: &mut TestAppContext) {
    // Validator order-independence: building with `.value()` AFTER `.validator()`
    // must still produce the validated state.
    let window = cx.add_window(|_w, cx| {
        TextBox::new(cx)
            .validator(sync_validator(|t: &str| {
                if t.contains('@') {
                    ValidationState::Valid
                } else {
                    ValidationState::Invalid("must contain @".into())
                }
            }))
            .value("nope")
    });
    let invalid = window
        .update(cx, |input, _, _| input.validation().is_invalid())
        .unwrap();
    assert!(invalid, "validator should have flagged the seeded value");
}

#[gpui::test]
fn marked_text_round_trip_keeps_committed_text(cx: &mut TestAppContext) {
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            // Begin an IME composition.
            input.replace_and_mark_text_in_range(None, "abc", Some(0..3), window, cx);
        })
        .unwrap();
    let marked = window
        .update(cx, |input, window, cx| {
            input.marked_text_range(window, cx).is_some()
        })
        .unwrap();
    assert!(marked, "composition should set a marked range");

    window
        .update(cx, |input, window, cx| {
            // IME accepts the composition.
            input.unmark_text(window, cx);
        })
        .unwrap();

    let (text, still_marked) = window
        .update(cx, |input, window, cx| {
            (
                input.text().to_string(),
                input.marked_text_range(window, cx).is_some(),
            )
        })
        .unwrap();
    assert_eq!(
        text, "abc",
        "committed composition text must survive unmark"
    );
    assert!(!still_marked, "unmark clears the marked range flag");
}

#[gpui::test]
fn disabled_field_rejects_edits(cx: &mut TestAppContext) {
    let window = cx.add_window(|_w, cx| TextBox::new(cx).value("locked").disabled(true));
    window
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "x", window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "locked", "disabled field must not mutate");
}

#[gpui::test]
fn read_only_blocks_mutation_but_allows_copy(cx: &mut TestAppContext) {
    let window = cx.add_window(|_w, cx| TextBox::new(cx).value("readme").read_only(true));
    window
        .update(cx, |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.copy(&Copy, window, cx);
            // Mutation must be blocked.
            input.replace_text_in_range(None, "x", window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "readme");
    let clip = cx
        .read(|cx| cx.read_from_clipboard().and_then(|i| i.text()))
        .unwrap_or_default();
    assert_eq!(clip, "readme", "read-only copy should still work");
}

#[gpui::test]
fn ime_mark_with_non_ascii_composition(cx: &mut TestAppContext) {
    let window = single_line(cx);
    // Compose a CJK string with a relative selection inside it.
    // "你好" = 6 UTF-8 bytes, 2 UTF-16 code units.
    window
        .update(cx, |input, window, cx| {
            input.replace_and_mark_text_in_range(None, "你好", Some(0..1), window, cx);
        })
        .unwrap();
    let (text, marked) = window
        .update(cx, |input, window, cx| {
            (
                input.text().to_string(),
                input.marked_text_range(window, cx).is_some(),
            )
        })
        .unwrap();
    assert_eq!(text, "你好");
    assert!(marked, "composition should set marked range");
    // Unmark (IME accepts).
    window
        .update(cx, |input, window, cx| {
            input.unmark_text(window, cx);
        })
        .unwrap();
    let text = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(text, "你好", "committed CJK composition survives unmark");
}
