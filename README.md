# sqlly-textbox

A reusable, full-featured `TextBox` component for [GPUI] — Zed's
GPU-accelerated UI framework. Provides single-line and multi-line text editing
with validation, focus handling, selection state, clipboard, IME, undo/redo,
and the standard editing key bindings.

[GPUI]: https://github.com/zed-industries/zed/tree/main/crates/gpui

## What is this?

[GPUI] is the GPU-accelerated UI framework that powers the [Zed] editor. It's
fast and delightful, but it doesn't ship a batteries-included text field —
you get primitives (text shaping, input handlers, focus), not a form-ready
component. **`sqlly-textbox` fills that gap** with one drop-in `TextBox` you
can configure for almost any form-field use case.

[Zed]: https://zed.dev

It is built around a **pure, GPUI-free editing core** (`TextBoxState`) so all
text semantics — insertion, deletion, Unicode navigation, selection, undo/redo
— are fully unit-testable without opening a window. GPUI-specific code (focus,
rendering, the custom element that shapes/paints text and handles IME) lives in
a thin outer layer.

### When to reach for it

- You're building a GPUI app and need a text field (login form, search box,
  settings panel, notes area).
- You want validation (sync, async/debounced, or externally controlled) wired
  in, not bolted on later.
- You need correct Unicode editing (graphemes, combining marks, emoji, CJK +
  IME composition) without hand-rolling it.
- You want browser-grade editing key bindings (Cmd/Ctrl-A/C/X/V/Z, word +
  line + document movement, double/triple-click selection) out of the box.

### What you get

One type, [`TextBox`](src/text_box.rs), configured entirely through builder
methods:

```rust
let email = TextBox::new(cx)
    .placeholder("you@example.com")
    .validator(sync_validator(|t: &str| {
        if t.contains('@') { ValidationState::Valid }
        else { ValidationState::Invalid("must contain '@'".into()) }
    }))
    .on_change(Arc::new(|text, _cx| println!("{text}")));
```

Run the tabbed demo to see every configuration live:

```bash
cargo run --example demo
```

## Features

| Capability | Status |
| --- | --- |
| Single-line mode | ✅ |
| Multi-line mode (configurable min/max lines) | ✅ |
| Soft wrap (multi-line) | ✅ |
| No-wrap mode | ✅ |
| Grapheme-aware movement and deletion | ✅ |
| Word movement (Cmd/Ctrl+arrow) | ✅ |
| Line / document jump (Home/End, Cmd/Ctrl+Home/End) | ✅ |
| Select-all, shift-extend movement | ✅ |
| Copy / Cut / Paste | ✅ |
| Undo / Redo (with selection restoration) | ✅ |
| Backspace / Delete (forward) | ✅ |
| Focus state, focus ring styling | ✅ |
| Disabled / Read-only | ✅ |
| Password mode (masked glyphs, paste allowed) | ✅ |
| Placeholder | ✅ |
| Validation: external `ValidationState` | ✅ |
| Validation: sync `validator` callback | ✅ |
| Validation: async/debounced `async_validator` (with stale-result suppression) | ✅ |
| IME composition underline + caret | ✅ |
| Mouse click to place caret | ✅ |
| Shift-click to extend selection | ✅ |
| Mouse drag to select | ✅ |
| Double-click (word) / triple-click (line) selection | ✅ |
| Vertical caret movement (Up/Down) in multi-line | ✅ |
| `max_lines` viewport with vertical scroll + caret-follow | ✅ |
| UTF-8 byte offsets with UTF-16 conversion at GPUI boundary | ✅ |
| Pure, GPUI-free editing core with full unit tests | ✅ |

## Quick start

```toml
[dependencies]
gpui = "0.2.2"
sqlly-textbox = "0.1"
unicode-segmentation = "1"   # already a transitive dep
```

```rust
use std::sync::Arc;
use gpui::*;
use sqlly_textbox::{install_text_box_keybindings, sync_validator, TextBox, ValidationState};

fn main() {
    Application::new().run(|cx: &mut App| {
        install_text_box_keybindings(cx);

        let bounds = Bounds::centered(None, size(px(420.0), px(180.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| {
                TextBox::new(cx)
                    .placeholder("Email")
                    .validator(sync_validator(|text: &str| {
                        if text.contains('@') {
                            ValidationState::Valid
                        } else {
                            ValidationState::Invalid("must contain '@'".into())
                        }
                    }))
                    .on_change(Arc::new(|text, _cx| println!("{text}")))
            }),
        )
        .unwrap();
        cx.activate(true);
    });
}
```

## Demo

A tabbed showcase lives in `examples/demo.rs` — one tab per feature set, each
with several example configurations and the builder-chain code that produced
them. Run it with:

```bash
cargo run --example demo
```

Tabs: **Basics** · **Multi-line** · **Validation** · **Modes** · **Styling** ·
**Unicode & IME**.

## Recipes

Each recipe corresponds to a tab in the demo.

### Basics — single-line with callbacks

```rust
let field = TextBox::new(cx)
    .placeholder("Type here…")
    .value("Hello, world".to_string())
    .on_change(Arc::new(|text, _cx| println!("[change] {text}")))
    .on_commit(Arc::new(|text, _cx| println!("[commit] {text}")));
```

### Multi-line — soft wrap, viewport, scroll

```rust
let notes = TextBox::multi_line(3, cx)
    .mode(Mode::MultiLine {
        min_lines: 3,
        max_lines: Some(8),          // clamps height, enables vertical scroll
        wrap: TextWrap::Soft,        // wrap at the available width
    })
    .value("Hello\nfrom\na multi-line\nfield.".to_string());
```

`TextWrap::None` keeps long lines on one row (they overflow and are clipped).

### Validation — external, sync, and async

```rust
// Externally controlled state (e.g. from a server response):
TextBox::new(cx).value("bad".to_string())
    .validation_state(ValidationState::Invalid("rejected".into()));

// Sync validator (runs on every keystroke):
TextBox::new(cx).placeholder("user@example.com")
    .validator(sync_validator(|t: &str| {
        if t.contains('@') && t.contains('.') { ValidationState::Valid }
        else { ValidationState::Invalid("must contain '@' and a '.'".into()) }
    }));

// Async + debounced (shows "Validating…" then the result; stale results ignored):
TextBox::new(cx).placeholder("pick a username")
    .async_validator(Arc::new(|v: String| Box::pin(async move {
        if v == "admin" { ValidationState::Invalid("taken".into()) }
        else { ValidationState::Valid }
    })))
    .debounce_ms(400);
```

### Modes — password, disabled, read-only

```rust
TextBox::new(cx).password(true).placeholder("Secret");     // copy suppressed, paste allowed
TextBox::new(cx).value("locked".to_string()).disabled(true); // no focus / no edit
TextBox::new(cx).value("read me".to_string()).read_only(true); // focus + select + copy, no edit
```

### Styling — custom `ComponentStyle`

```rust
let dark = ComponentStyle {
    background: hsla(0., 0., 0.12, 1.),
    text: hsla(0., 0., 0.95, 1.),
    border_focused: hsla(0.6, 0.8, 0.65, 1.),
    corner_radius: px(20.),        // pill shape
    border_width_px: 3.,          // thick border (rounded to nearest 0–8 px)
    font_size: px(20.),
    line_height: px(30.),
    ..ComponentStyle::default()
};
TextBox::new(cx).value("styled".to_string()).style(dark);
```

### Unicode & IME

```rust
TextBox::new(cx).value("😀🚀✨ 漢字 ひらがな".to_string());
TextBox::new(cx).value("cafe\u{0301}".to_string()); // combining mark → "café"
```

IME composition (marked text) is handled by the `EntityInputHandler`
implementation — composition underline, candidate-window positioning, and
commit behavior all work out of the box.

## Builder reference

| Method | Effect |
| --- | --- |
| `value(text)` | Initial value (also resets history). |
| `placeholder(text)` | Placeholder shown when empty. |
| `mode(Mode::MultiLine { min_lines, max_lines, wrap })` or `multi_line(3, cx)` | Set edit mode. |
| `disabled(true)` | Block focus + editing. |
| `read_only(true)` | Block editing but keep focusable. |
| `password(true)` | Render masked glyphs; suppress copy/cut; allow paste. |
| `validator(SyncValidator)` | Sync validator runs on every change. Build with `sync_validator(\|t\| …)`. |
| `async_validator(Arc<Fn(String) -> Pin<Box<dyn Future<Output = ValidationState> + Send>>>)` | Debounced async validator; stale results are discarded. |
| `debounce_ms(ms)` | Debounce window for async validation (default 300). |
| `validation_state(state)` | Set externally controlled validation state. |
| `style(ComponentStyle)` | Override color palette, padding, radius, etc. |
| `on_change(Arc<Fn(&str, &mut App)>)` | Fires after each accepted mutation. |
| `on_commit(Arc<Fn(&str, &mut App)>)` | Fires on Enter (single-line) or Cmd/Ctrl+Enter (multi-line). |

`MultiLine { wrap: TextWrap::Soft }` enables soft wrapping at the available
width. `TextWrap::None` keeps long lines overflowing (the element clips them).

## Actions

Action structs live under the `text_box` namespace:

```
Backspace, Delete, Left, Right, Up, Down,
WordLeft, WordRight, LineStart, LineEnd, DocumentStart, DocumentEnd,
SelectLeft, SelectRight, SelectUp, SelectDown,
SelectWordLeft, SelectWordRight, SelectLineStart, SelectLineEnd,
SelectDocumentStart, SelectDocumentEnd,
SelectAll, Copy, Cut, Paste, Undo, Redo,
InsertNewline, Commit, ShowCharacterPalette
```

`install_text_box_keybindings(cx)` registers the default key bindings for
macOS (Cmd-A/C/X/V/Z/Shift-Z, etc.) and other platforms (Ctrl-…). Place views
inside a `div()` with `.key_context("TextBox")` for those bindings to fire
while the field is focused.

## Architecture

```
src/
├── lib.rs              — public exports
├── utf.rs              — UTF-8 ↔ UTF-16 offset conversion
├── selection.rs        — caret/anchor/reversed selection + movement units
├── history.rs          — bounded snapshot undo/redo
├── mode.rs             — Mode / TextWrap / Placeholder types
├── validation.rs       — ValidationState, SyncValidator, helpers
├── state.rs            — TextBoxState: text, selection, mode, edit ops, history
├── actions.rs          — actions! + install_text_box_keybindings
├── text_box.rs       — GPUI entity (Focusable, Render, EntityInputHandler, builders)
└── text_box_element.rs — custom Element: shaping, painting, hit testing, IME
```

The pure `TextBoxState` is unit-tested without GPUI. The GPUI-side
`TextBox` entity wraps it and adds focus, rendering, and event plumbing.

## Testing

```bash
cargo test --lib                  # 56 unit tests: state, selection, utf, validation, history
cargo test --test text_box_gpui   # 10 GPUI integration tests: input handler, clipboard, validation
cargo check --all-targets         # lib + example + tests all compile
cargo clippy --all-targets -- -D warnings  # zero warnings
cargo run --example demo          # open the demo window
```

## Limitations

- Box layout of bordered error messages: the validation message is rendered
  below the field, with `text_xs` styling.
- `min_lines` / `max_lines` count **logical** lines (split on `\n`), not
  visual wrapped rows. Soft-wrapped content may show more visual rows.
- UTF-8/UTF-16 conversion and grapheme movement are O(n) — fine for
  form-field-sized inputs but not optimized for editor-sized documents.
- BiDi/RTL text is not explicitly supported (no tested layout paths).
- Touch/mobile: not exercised.
- Hard wrap (`TextWrap::Hard`) is intentionally **not** implemented because it
  mutates user data on layout. Wrap is purely a visual concern.

## License

Dual-licensed under MIT or Apache-2.0 at your option. See `LICENSE-MIT` and
`LICENSE-APACHE`.
