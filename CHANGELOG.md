# Changelog

## 0.1.0

Initial release of `sqlly-textbox`.

### Features

- Single-line and multi-line modes with configurable `min_lines`/`max_lines`
- Soft wrap and no-wrap modes
- Grapheme-aware cursor movement and deletion
- Word, line, and document navigation
- Select-all, shift-extend selection, mouse drag, double/triple-click
- Copy, cut, paste with password-mode clipboard policy
- Bounded undo/redo (cap 256) with per-run coalescing
- IME composition support (marked text, candidate positioning)
- Validation: external state, sync validator, async debounced validator
- Disabled, read-only, and password modes
- Custom `ComponentStyle` for colors, borders, padding, font sizing
- `#![forbid(unsafe_code)]`

### Architecture

- Pure, GPUI-free `TextBoxState` core (UTF-8 byte offsets)
- UTF-16 conversion only at the `EntityInputHandler` boundary
- Custom `Element` for text shaping, painting, and hit testing
- 56 unit tests + 10 GPUI integration tests
