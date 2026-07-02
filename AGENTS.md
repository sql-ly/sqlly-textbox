# AGENTS.md — contributor notes for `sqlly-textbox`

## Goals

Build a single, reusable `TextBox` GPUI component suitable for form-field-sized
inputs. Favor testable, pure editing logic in the core; push all GPUI-specific code
to the leaves (entity + element).

## Build & verify

```bash
cargo check --all-targets         # lib + example + tests
cargo clippy --all-targets -- -D warnings  # zero warnings
cargo test --lib                  # 58 unit tests for state, selection, utf, validation, history
cargo test --test text_box_gpui   # 12 GPUI integration tests
cargo run --example demo          # tabbed feature showcase (Basics, Multi-line, …)
```

## MANDATORY pre-push gate — NO EXCEPTIONS

Under **no circumstance** may you commit, tag, or push without first running
**both** of these commands to completion and confirming they succeed:

```bash
cargo fmt                            # format the code
cargo clippy --all-targets -- -D warnings   # zero warnings, zero errors
```

- If `cargo fmt` changes **any** file, re-stage and re-commit those changes
  before pushing. Never commit unformatted code.
- If `cargo clippy` reports **any** warning or error (even one), STOP. Fix it
  before proceeding. Do not push with clippy failures.
- This gate applies to **every** push: version bumps, tiny edits, doc-only
  changes, everything. No skipping, no "it's trivial", no exceptions.
- Only after both pass clean may you proceed to bump the version, tag, and push.

## Layering

- `state::TextBoxState` is GPUI-free. All text editing semantics live here.
  Strings are stored as UTF-8 byte offsets inside `String`. Mutations route
  through methods that clamp ranges, normalize `\n`, and record history.
- `text_box::TextBox` wraps `TextBoxState`; adds `FocusHandle`, builders,
  EntityInputHandler, Focusable, and Render.
- `text_box_element::TextBoxElement` is the custom `Element` that issues the
  `window.handle_input(...)` registry call and shapes/paints text. Mouse
  position-to-offset queries are answered by the entity's `last_layout` cache,
  which the element writes back to the entity during paint.

## Memory rules (to preserve)

| Rule | Why |
| --- | --- |
| All byte offsets are UTF-8. | Fast mutation; canonical. |
| UTF-16 conversion only at the EntityInputHandler boundary. | GPUI is UTF-16. |
| All `replace_range` callers clamp their arguments to char boundaries. | Avoid `text[range]` panics. |
| `text_for_range` clamps via `clamp_range` and handles reversed ranges. | Stale/out-of-bounds UTF-16 ranges from IME must not panic (Bug 0001). |
| `set_selection_range` / `select_to` clamp via `clamp_offset`. | Prevent mid-codepoint panics from stale offsets. |
| `selected_text` floors both range ends to char boundaries. | Defense-in-depth for public API misuse. |
| `replace_range_silent` computes mark range from normalized text. | IME mark spans the normalized, not raw, composition. |
| `new_selected_range_utf16` in IME is relative to composition text. | Per GPUI EntityInputHandler contract. |
| `set_text` replaces the text AND clears history. | Setting from outside should not re-enter undo. |
| Validation generation token suppresses stale async results. | Prevents races where an old validator overwrites a newer edit. |
| Password mode suppresses copy/cut but allows paste. | Standard browser-style policy; documented in README. |
| `#![forbid(unsafe_code)]` at crate root. | No unsafe, ever. |

## Adding a feature

1. Add the operation as a method on `TextBoxState` (with tests).
2. If the action needs GPUI awareness (e.g., async validator), wire it through
   the entity in `text_box.rs`.
3. If a new visual mode is needed, add it to `Mode` / `TextWrap` and to the
   element's `paint`.

## Visible follow-ups

- Hard-wrap as an explicit formatting feature (not a layout-driven mutation).
- Diff-only undo coalescing (the current model coalesces per run of single-char
  inserts / deletes; a finer diff-based model could merge more intelligently).
- BiDi/RTL test coverage (currently out of scope).

## Recently landed

- Crate renamed to `sqlly-textbox`; public type is `TextBox` (was `TextInput`).
- Tabbed demo showcase (`examples/demo.rs`): one tab per feature set, several
  configurations each, with the builder-chain code shown inline.
- Mouse-drag selection plus double-click (word) and triple-click (line).
- Real vertical caret movement (Up/Down) in multi-line mode via the cached layout.
- `max_lines` viewport clamping with vertical scroll and caret-follow.
- Bounded undo history (cap 256) with per-word / per-run coalescing.
- `border_width_px` honored; `unmark_text` no longer deletes committed IME text.
