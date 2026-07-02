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
fn tab_index_builder_marks_field_as_tab_stop(cx: &mut TestAppContext) {
    let window = cx.add_window(|_w, cx| TextBox::new(cx).tab_index(7));
    let (tab_stop, tab_index) = window
        .update(cx, |input, _, _| {
            let h = input.focus_handle_ref();
            (h.tab_stop, h.tab_index)
        })
        .unwrap();
    assert!(tab_stop, "tab_index() must set tab_stop=true");
    assert_eq!(tab_index, 7, "tab_index() must record the index");
}

#[gpui::test]
fn tab_stop_builder_can_disable_tab_navigation(cx: &mut TestAppContext) {
    let window = cx.add_window(|_w, cx| TextBox::new(cx).tab_index(3).tab_stop(false));
    let (tab_stop, tab_index) = window
        .update(cx, |input, _, _| {
            let h = input.focus_handle_ref();
            (h.tab_stop, h.tab_index)
        })
        .unwrap();
    assert!(
        !tab_stop,
        "tab_stop(false) must remove the field from tab order"
    );
    assert_eq!(tab_index, 3, "tab_index must persist after tab_stop(false)");
}

#[gpui::test]
fn set_disabled_mutator_toggles_editability_at_runtime(cx: &mut TestAppContext) {
    let window = cx.add_window(|_w, cx| TextBox::new(cx).value("v0"));
    // Initially enabled: select-all + replace lands the new value.
    window
        .update(cx, |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.replace_text_in_range(None, "v1", window, cx);
        })
        .unwrap();
    let after_edit = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(after_edit, "v1");

    // Toggle disabled on at runtime.
    window
        .update(cx, |input, _, cx| input.set_disabled(true, cx))
        .unwrap();
    window
        .update(cx, |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.replace_text_in_range(None, "v2", window, cx);
        })
        .unwrap();
    let blocked = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(blocked, "v1", "set_disabled(true) must block edits");

    // Toggle back off: edits flow again.
    window
        .update(cx, |input, _, cx| input.set_disabled(false, cx))
        .unwrap();
    window
        .update(cx, |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.replace_text_in_range(None, "v3", window, cx);
        })
        .unwrap();
    let reenabled = window
        .update(cx, |input, _, _| input.text().to_string())
        .unwrap();
    assert_eq!(reenabled, "v3", "set_disabled(false) must restore edits");
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

#[gpui::test]
#[allow(clippy::reversed_empty_ranges)]
fn text_for_range_with_out_of_bounds_utf16_does_not_panic(cx: &mut TestAppContext) {
    // Regression test for Bug 0001: `text_for_range` must not panic when the
    // platform passes a stale/out-of-bounds UTF-16 range (common during IME
    // composition after the text buffer has been modified).
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            input.replace_text_in_range(None, "hello", window, cx);
        })
        .unwrap();

    // Request a range far beyond the text length.
    let result = window
        .update(cx, |input, window, cx| {
            let mut actual = None;
            let text = input.text_for_range(0..999, &mut actual, window, cx);
            (text, actual)
        })
        .unwrap();
    assert_eq!(
        result.0.as_deref(),
        Some("hello"),
        "out-of-bounds range clamps to text"
    );
    assert!(result.1.is_some(), "actual_range is populated");

    // Request a reversed range (start > end).
    let result = window
        .update(cx, |input, window, cx| {
            let mut actual = None;
            let text = input.text_for_range(4..1, &mut actual, window, cx);
            (text, actual)
        })
        .unwrap();
    let text = result.0.as_deref().unwrap_or("");
    assert!(
        text == "ell" || text == "el",
        "reversed range returns text between the two offsets, got {text:?}"
    );

    // Request a range that starts beyond text length.
    let result = window
        .update(cx, |input, window, cx| {
            let mut actual = None;
            let text = input.text_for_range(100..200, &mut actual, window, cx);
            (text, actual)
        })
        .unwrap();
    assert_eq!(
        result.0.as_deref(),
        Some(""),
        "range entirely beyond text returns empty"
    );
}

#[gpui::test]
fn text_for_range_after_ime_shrinks_text(cx: &mut TestAppContext) {
    // Simulate the IME scenario: text grows during composition, then the
    // platform asks for a range based on the old (longer) text.
    let window = single_line(cx);
    window
        .update(cx, |input, window, cx| {
            // Start with long text.
            input.replace_text_in_range(None, "hello world", window, cx);
        })
        .unwrap();
    // Shrink the text.
    window
        .update(cx, |input, window, cx| {
            input.select_all(&SelectAll, window, cx);
            input.replace_text_in_range(None, "hi", window, cx);
        })
        .unwrap();
    // Now request the old range (0..11) against the new short text ("hi", len 2).
    // This would have panicked before the fix.
    let result = window
        .update(cx, |input, window, cx| {
            let mut actual = None;
            input.text_for_range(0..11, &mut actual, window, cx)
        })
        .unwrap();
    assert_eq!(
        result.as_deref(),
        Some("hi"),
        "stale range after shrink clamps to current text"
    );
}
