//! `TextBox` GPUI entity — full-featured text input component.
//!
//! Start with [`TextBox::new`], configure via builder methods, then either
//! render it directly (it implements [`Render`]) or nest it inside another
//! `Render` view. Don't forget to call
//! [`crate::actions::install_text_box_keybindings`] at app startup so the
//! default editing keymap is active.

use std::future::Future;
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    hsla, prelude::*, px, App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler,
    FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Task, UTF16Selection, Window,
};

use crate::actions::{
    Backspace, Commit, Copy, Cut, Delete, DocumentEnd, DocumentStart, Down, InsertNewline, Left,
    LineEnd, LineStart, Paste, Redo, Right, SelectAll, SelectDocumentEnd, SelectDocumentStart,
    SelectDown, SelectLeft, SelectLineEnd, SelectLineStart, SelectRight, SelectUp, SelectWordLeft,
    SelectWordRight, ShowCharacterPalette, Undo, Up, WordLeft, WordRight,
};
use crate::mode::{Mode, TextWrap};
use crate::state::TextBoxState;
use crate::text_box_element::{LastLayout, TextBoxElement};
use crate::validation::ValidationState;

/// Optional callback invoked whenever the value changes.
pub type ChangeCallback = Arc<dyn Fn(&str, &mut App) + Send + Sync>;

/// Optional callback invoked when the field is committed (Enter on
/// single-line, Cmd/Ctrl-Enter on multi-line, or external `.commit()`).
pub type CommitCallback = Arc<dyn Fn(&str, &mut App) + Send + Sync>;

/// Async validator: a future returning a `ValidationState`.
pub type AsyncValidator = Arc<
    dyn Fn(String) -> std::pin::Pin<Box<dyn Future<Output = ValidationState> + Send>> + Send + Sync,
>;

/// Vertical alignment of text within the field's content box.
///
/// Only visible when the field is taller than the text content (e.g. via
/// `min_height` or when a parent stretches the field). Defaults to `Middle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

pub struct ComponentStyle {
    /// Background color when idle.
    pub background: gpui::Hsla,
    /// Background color when focused.
    pub background_focused: gpui::Hsla,
    /// Border color when idle and not disabled.
    pub border: gpui::Hsla,
    /// Border color when focused.
    pub border_focused: gpui::Hsla,
    /// Border color when the field has an `Invalid` validation state.
    pub border_invalid: gpui::Hsla,
    /// Border color when the field has a `Warning` validation state.
    pub border_warning: gpui::Hsla,
    /// Border color when disabled.
    pub border_disabled: gpui::Hsla,
    /// Text color for typed content.
    pub text: gpui::Hsla,
    /// Text color for placeholder.
    pub placeholder: gpui::Hsla,
    /// Text color when disabled.
    pub text_disabled: gpui::Hsla,
    /// Color for selection rectangle.
    pub selection: gpui::Hsla,
    /// Color for the caret.
    pub caret: gpui::Hsla,
    /// Color for validation message text.
    pub invalid_message: gpui::Hsla,
    /// Color for warning message text.
    pub warning_message: gpui::Hsla,
    /// Corner radius.
    pub corner_radius: Pixels,
    /// Border width in pixels (1 by default — use `border_width(px(N))`).
    pub border_width_px: f32,
    /// Inner padding.
    pub padding: Pixels,
    /// Minimum height of the field (including padding and border).
    /// When this exceeds the text content height, `vertical_align`
    /// determines where the text sits. Defaults to 0 (no effect).
    pub min_height: Pixels,
    /// Vertical alignment of text within the field.
    pub vertical_align: VerticalAlign,
    /// Line height.
    pub line_height: Pixels,
    /// Font size.
    pub font_size: Pixels,
}

impl Default for ComponentStyle {
    fn default() -> Self {
        Self {
            background: hsla(0., 0., 1., 1.),
            background_focused: hsla(0., 0., 1., 1.),
            border: hsla(0., 0., 0., 0.15),
            border_focused: hsla(0.6, 0.7, 0.5, 1.),
            border_invalid: hsla(0., 0.8, 0.5, 1.),
            border_warning: hsla(0.13, 0.8, 0.5, 1.),
            border_disabled: hsla(0., 0., 0., 0.07),
            text: hsla(0., 0., 0., 1.),
            placeholder: hsla(0., 0., 0., 0.35),
            text_disabled: hsla(0., 0., 0., 0.35),
            selection: hsla(0.6, 0.7, 0.85, 0.4),
            caret: hsla(0.6, 0.5, 0.4, 1.),
            invalid_message: hsla(0., 0.8, 0.4, 1.),
            warning_message: hsla(0.13, 0.8, 0.4, 1.),
            corner_radius: px(6.),
            border_width_px: 1.0,
            padding: px(8.),
            min_height: px(0.),
            vertical_align: VerticalAlign::Middle,
            line_height: px(24.),
            font_size: px(14.),
        }
    }
}

/// The text input entity.
pub struct TextBox {
    focus_handle: FocusHandle,
    state: TextBoxState,
    placeholder: gpui::SharedString,
    style: ComponentStyle,
    on_change: Option<ChangeCallback>,
    on_commit: Option<CommitCallback>,
    sync_validator: Option<crate::validation::SyncValidator>,
    async_validator: Option<AsyncValidator>,
    debounce_ms: u64,
    validation_generation: u64,
    /// True while the primary mouse button is held down for drag-selection.
    is_selecting: bool,
    /// Vertical scroll offset (multi-line). Always `>= 0`; subtracted from row
    /// positions during paint so the caret stays visible.
    pub(crate) scroll_offset: Pixels,
    /// Set by [`TextBoxElement`] during paint so mouse handlers can map click
    /// position back to text offsets.
    pub(crate) last_layout: Option<LastLayout>,
    pub(crate) last_bounds: Option<Bounds<Pixels>>,
    /// Toggles every ~500ms while focused to produce a blinking caret.
    pub(crate) caret_blink_on: bool,
    /// Active blink timer task; dropped on blur to stop blinking.
    blink_task: Option<Task<()>>,
}

impl TextBox {
    #[must_use]
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            state: TextBoxState::new(),
            placeholder: gpui::SharedString::default(),
            style: ComponentStyle::default(),
            on_change: None,
            on_commit: None,
            sync_validator: None,
            async_validator: None,
            debounce_ms: 300,
            validation_generation: 0,
            is_selecting: false,
            scroll_offset: px(0.),
            last_layout: None,
            last_bounds: None,
            caret_blink_on: true,
            blink_task: None,
        }
    }

    #[must_use]
    pub fn multi_line(min_lines: usize, cx: &mut Context<Self>) -> Self {
        let mut s = Self::new(cx);
        s.state.set_mode(Mode::MultiLine {
            min_lines,
            max_lines: None,
            wrap: TextWrap::Soft,
        });
        s
    }

    // -------------------- Builder methods --------------------

    #[must_use]
    pub fn value(mut self, text: impl Into<String>) -> Self {
        self.state.set_text(text);
        // Keep validation consistent regardless of builder order: if a sync
        // validator was already attached, re-run it against the new value.
        self.run_sync_validation();
        self
    }

    #[must_use]
    pub fn placeholder(mut self, text: impl Into<gpui::SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    #[must_use]
    pub fn mode(mut self, mode: Mode) -> Self {
        self.state.set_mode(mode);
        self
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Mark this field as a tab stop and place it at `index` in the window's
    /// tab order. Equivalent to `.tab_stop(true).tab_index(index)` on the
    /// underlying `FocusHandle`. Lower indices are visited first by Tab;
    /// Shift+Tab reverses. Indices need not be contiguous.
    #[must_use]
    pub fn tab_index(mut self, index: isize) -> Self {
        self.focus_handle = self.focus_handle.tab_stop(true).tab_index(index);
        self
    }

    /// Toggle whether this field is reachable via Tab/Shift+Tab navigation.
    /// `true` includes it in the tab order (at its current `tab_index`);
    /// `false` removes it. Defaults to `false` (gpui's `FocusHandle` default).
    #[must_use]
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.focus_handle = self.focus_handle.tab_stop(tab_stop);
        self
    }

    #[must_use]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.state.set_read_only(read_only);
        self
    }

    #[must_use]
    pub fn password(mut self, password: bool) -> Self {
        self.state.set_password(password);
        self
    }

    #[must_use]
    pub fn style(mut self, style: ComponentStyle) -> Self {
        self.style = style;
        self
    }

    /// Set the vertical alignment of text within the field.
    #[must_use]
    pub fn vertical_align(mut self, align: VerticalAlign) -> Self {
        self.style.vertical_align = align;
        self
    }

    /// Set a minimum height for the field. When this exceeds the text content
    /// height, `vertical_align` controls where the text sits.
    #[must_use]
    pub fn min_height(mut self, height: Pixels) -> Self {
        self.style.min_height = height;
        self
    }

    /// Borrow the current component style.
    pub fn style_ref(&self) -> &ComponentStyle {
        &self.style
    }

    #[must_use]
    pub fn on_change(mut self, callback: ChangeCallback) -> Self {
        self.on_change = Some(callback);
        self
    }

    #[must_use]
    pub fn on_commit(mut self, callback: CommitCallback) -> Self {
        self.on_commit = Some(callback);
        self
    }

    #[must_use]
    pub fn validation_state(mut self, state: ValidationState) -> Self {
        self.state.set_validation(state);
        self
    }

    #[must_use]
    pub fn validator(mut self, validator: crate::validation::SyncValidator) -> Self {
        let v_state = validator(self.state.text());
        self.state.set_validation(v_state);
        self.sync_validator = Some(validator);
        self
    }

    #[must_use]
    pub fn async_validator(mut self, validator: AsyncValidator) -> Self {
        self.async_validator = Some(validator);
        self
    }

    #[must_use]
    pub fn debounce_ms(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    // -------------------- Public accessors --------------------

    pub fn state(&self) -> &TextBoxState {
        &self.state
    }

    pub fn text(&self) -> &str {
        self.state.text()
    }

    pub fn validation(&self) -> &ValidationState {
        self.state.validation()
    }

    pub fn set_value(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.state.set_text(text);
        self.run_sync_validation();
        self.after_mutation(cx);
    }

    /// Toggle the disabled state at runtime (mirrors the `.disabled(bool)`
    /// builder). A disabled field rejects editing and renders with the
    /// disabled style, but remains focusable. Notifies the view to repaint.
    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.state.set_disabled(disabled);
        cx.notify();
    }

    pub fn set_validation(&mut self, state: ValidationState, cx: &mut Context<Self>) {
        self.state.set_validation(state);
        cx.notify();
    }

    pub fn commit(&mut self, cx: &mut Context<Self>) {
        if let Some(callback) = &self.on_commit {
            callback(self.state.text(), cx);
        }
    }

    /// Read-only: the most recent layout (set by the renderer each frame).
    pub fn last_layout(&self) -> Option<&LastLayout> {
        self.last_layout.as_ref()
    }

    /// Render-ready display text: the underlying value, or masked glyphs for
    /// password fields, or empty when the placeholder is showing.
    pub fn value_for_render(&self) -> gpui::SharedString {
        if self.state.is_password() {
            let n = self.state.text().chars().count().max(1);
            gpui::SharedString::from("•".repeat(n))
        } else {
            gpui::SharedString::from(self.state.text().to_string())
        }
    }

    /// Effective text color (returns `text_disabled` if disabled).
    pub fn effective_text_color(&self) -> gpui::Hsla {
        if self.state.is_disabled() {
            self.style.text_disabled
        } else {
            self.style.text
        }
    }

    /// Borrow the underlying placeholder.
    pub fn placeholder_str(&self) -> &gpui::SharedString {
        &self.placeholder
    }

    /// Borrow the focus handle.
    pub fn focus_handle_ref(&self) -> &FocusHandle {
        &self.focus_handle
    }

    // -------------------- Helper used by element --------------------

    pub(crate) fn record_layout(&mut self, bounds: Bounds<Pixels>, layout: LastLayout) {
        self.last_bounds = Some(bounds);
        self.last_layout = Some(layout);
    }

    // -------------------- Action handlers --------------------

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.state.select_all();
        cx.notify();
    }

    pub fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.is_password() {
            return;
        }
        let selected = self.state.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
        }
    }

    pub fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.is_password() || !self.state.can_edit() {
            return;
        }
        let selected = self.state.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
            self.state.delete_selection();
            self.after_mutation(cx);
        }
    }

    pub fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if !self.state.can_edit() {
            return;
        }
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            // `replace_range` records its own (non-coalesced) undo snapshot for
            // multi-character inserts, so paste is a single undo step.
            self.state.insert(&text);
            self.after_mutation(cx);
        }
    }

    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if !self.state.can_edit() {
            return;
        }
        self.state.delete(false);
        self.after_mutation(cx);
    }

    pub fn delete_forward(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if !self.state.can_edit() {
            return;
        }
        self.state.delete(true);
        self.after_mutation(cx);
    }

    pub fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::GraphemeLeft, false);
        cx.notify();
    }

    pub fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::GraphemeRight, false);
        cx.notify();
    }

    pub fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, false, cx);
    }

    pub fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, false, cx);
    }

    pub fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::WordLeft, false);
        cx.notify();
    }

    pub fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::WordRight, false);
        cx.notify();
    }

    pub fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.state.line_start(false);
        cx.notify();
    }

    pub fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.state.line_end(false);
        cx.notify();
    }

    pub fn document_start(&mut self, _: &DocumentStart, _: &mut Window, cx: &mut Context<Self>) {
        self.state.document_start(false);
        cx.notify();
    }

    pub fn document_end(&mut self, _: &DocumentEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.state.document_end(false);
        cx.notify();
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::GraphemeLeft, true);
        cx.notify();
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::GraphemeRight, true);
        cx.notify();
    }

    pub fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(true, true, cx);
    }

    pub fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(false, true, cx);
    }

    pub fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.state
            .move_by(crate::selection::Movement::WordLeft, true);
        cx.notify();
    }

    pub fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state
            .move_by(crate::selection::Movement::WordRight, true);
        cx.notify();
    }

    pub fn select_line_start(
        &mut self,
        _: &SelectLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.line_start(true);
        cx.notify();
    }

    pub fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.state.line_end(true);
        cx.notify();
    }

    pub fn select_document_start(
        &mut self,
        _: &SelectDocumentStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.document_start(true);
        cx.notify();
    }

    pub fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.document_end(true);
        cx.notify();
    }

    pub fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        if !self.state.can_edit() {
            return;
        }
        match self.state.mode() {
            Mode::SingleLine => self.commit(cx),
            Mode::MultiLine { .. } => {
                self.state.insert("\n");
                self.after_mutation(cx);
            }
        }
    }

    pub fn commit_action(&mut self, _: &Commit, _: &mut Window, cx: &mut Context<Self>) {
        self.commit(cx);
    }

    pub fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.undo() {
            self.after_mutation(cx);
        }
    }

    pub fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.redo() {
            self.after_mutation(cx);
        }
    }

    pub fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // -------------------- Mouse handlers --------------------

    pub fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.is_disabled() {
            return;
        }
        window.focus(&self.focus_handle);
        let offset = self.mouse_position_to_offset(event.position);
        match event.click_count {
            // Double-click: select the word under the cursor.
            2 => self.select_word_at(offset),
            // Triple-click (or more): select the whole logical line.
            n if n >= 3 => self.select_line_at(offset),
            // Single click: place caret, or extend if shift is held.
            _ => {
                if event.modifiers.shift {
                    self.state.select_to(offset);
                } else {
                    self.state.collapse_to(offset);
                }
            }
        }
        // Begin a potential drag-selection.
        self.is_selecting = event.click_count <= 1;
        // Reset blink so the caret is immediately visible after clicking.
        self.caret_blink_on = true;
        cx.notify();
    }

    pub fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only extend selection while the primary button is held (drag).
        if !self.is_selecting || !event.dragging() {
            return;
        }
        let offset = self.mouse_position_to_offset(event.position);
        self.state.select_to(offset);
        cx.notify();
    }

    pub fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    // -------------------- Internal helpers --------------------

    /// Move the caret up or down one visual row, using the cached layout for
    /// geometry. Falls back to logical line start/end when no layout exists yet
    /// (first frame) or in single-line mode (where up/down jump to the field
    /// extremes).
    fn move_vertical(&mut self, up: bool, extend: bool, cx: &mut Context<Self>) {
        if matches!(self.state.mode(), Mode::SingleLine) {
            if up {
                self.state.document_start(extend);
            } else {
                self.state.document_end(extend);
            }
            cx.notify();
            return;
        }

        let caret = self.state.selection().head();
        let mut moved = false;
        if let (Some(layout), Some(bounds)) = (self.last_layout.clone(), self.last_bounds) {
            let line_height = layout.line_height();
            if let Some(caret_bounds) =
                layout.bounds_for_utf8_range(&bounds, self.state.text(), caret..caret)
            {
                let x = caret_bounds.left();
                let y = if up {
                    caret_bounds.top() - line_height * 0.5
                } else {
                    caret_bounds.bottom() + line_height * 0.5
                };
                let target =
                    layout.utf8_index_at_position(&bounds, Point::new(x, y), self.state.text());
                if extend {
                    self.state.select_to(target);
                } else {
                    self.state.collapse_to(target);
                }
                moved = true;
            }
        }
        if !moved {
            // No geometry available yet: approximate with logical line bounds.
            if up {
                self.state.line_start(extend);
            } else {
                self.state.line_end(extend);
            }
        }
        cx.notify();
    }

    /// Select the word (unicode word boundary) containing `offset`.
    fn select_word_at(&mut self, offset: usize) {
        let text = self.state.text();
        let start =
            crate::selection::apply_movement(text, offset, crate::selection::Movement::WordLeft);
        // WordRight from just inside the word lands on the next word; instead
        // find the end of the current word from `start`.
        let from = start.min(offset);
        let end = {
            let right =
                crate::selection::apply_movement(text, from, crate::selection::Movement::WordRight);
            // WordRight gives the start of the *next* word; trim trailing
            // whitespace back to the end of the current word.
            text[..right.min(text.len())].trim_end().len().max(offset)
        };
        if end > start {
            self.state.set_selection_range(start, end);
        } else {
            self.state.collapse_to(offset);
        }
    }

    /// Select the entire logical line containing `offset`.
    fn select_line_at(&mut self, offset: usize) {
        let text = self.state.text();
        let start =
            crate::selection::apply_movement(text, offset, crate::selection::Movement::LineStart);
        let end =
            crate::selection::apply_movement(text, offset, crate::selection::Movement::LineEnd);
        self.state.set_selection_range(start, end);
    }

    fn after_mutation(&mut self, cx: &mut Context<Self>) {
        self.run_sync_validation();
        if self.async_validator.is_some() {
            self.run_async_validation(cx);
        }
        if let Some(callback) = &self.on_change {
            callback(self.state.text(), cx);
        }
        // Reset blink so the caret is immediately visible after any edit.
        self.caret_blink_on = true;
        cx.notify();
    }

    fn run_sync_validation(&mut self) {
        if let Some(validator) = &self.sync_validator {
            let new_state = validator(self.state.text());
            self.state.set_validation(new_state);
        }
    }

    fn run_async_validation(&mut self, cx: &mut Context<Self>) {
        let Some(validator) = self.async_validator.clone() else {
            return;
        };
        let validation_generation = self.validation_generation + 1;
        // Mutate directly — we already hold the entity lease via the caller
        // (after_mutation). Going through entity.update(cx, ...) here would
        // attempt a second lease on the same entity and panic
        // ("cannot update TextBox while it is already being updated").
        let value = self.state.text().to_string();
        let debounce_ms = self.debounce_ms;
        let entity = cx.entity();
        self.validation_generation = validation_generation;
        self.state.set_validation(ValidationState::Validating);
        let task = cx.spawn(async move |_weak, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(debounce_ms))
                .await;
            let result = validator(value).await;
            let _ = cx.update(|cx| {
                entity.update(cx, |this, cx| {
                    if this.validation_generation == validation_generation {
                        this.state.set_validation(result);
                        cx.notify();
                    }
                });
            });
        });
        task.detach();
    }

    fn mouse_position_to_offset(&self, position: Point<Pixels>) -> usize {
        match (&self.last_layout, self.last_bounds) {
            (Some(layout), Some(bounds)) => layout.utf8_index_at_position(
                &self.scrolled_bounds(bounds),
                position,
                self.state.text(),
            ),
            _ => self.state.selection().head(),
        }
    }

    /// The last painted bounds shifted up by the current vertical scroll
    /// offset, matching the coordinate space the element painted text in.
    fn scrolled_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        Bounds {
            origin: Point::new(bounds.origin.x, bounds.origin.y - self.scroll_offset),
            size: bounds.size,
        }
    }
}

impl Focusable for TextBox {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for TextBox {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.state.utf16_range_to_utf8(&range_utf16);
        let text = self.state.text();
        // Clamp both endpoints to valid char boundaries and text length.
        // The IME/platform may pass stale or out-of-bounds UTF-16 ranges after
        // the text buffer has been modified; without clamping, `text[range]`
        // would panic.
        let clamped = crate::utf::clamp_range(text, &range);
        // Handle reversed ranges (start > end) by swapping so the slice is valid.
        let (start, end) = if clamped.start <= clamped.end {
            (clamped.start, clamped.end)
        } else {
            (clamped.end, clamped.start)
        };
        let utf16 = self.state.utf8_range_to_utf16(start..end);
        actual_range.replace(utf16);
        Some(text[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let r = self.state.selection().range_bounds();
        Some(UTF16Selection {
            range: self.state.utf8_range_to_utf16(r),
            reversed: self.state.selection().is_reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.state
            .marked_range()
            .map(|r| self.state.utf8_range_to_utf16(r.clone()))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The IME has accepted (committed) the composition. Clear the marked
        // range flag only — the composed text stays in the buffer. Deleting it
        // here would discard accepted input.
        self.state.clear_marked_range();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16.map(|r| self.state.utf16_range_to_utf8(&r));
        // `replace_range` records its own undo snapshot and coalesces single
        // character insertions; no explicit snapshot here (that would break
        // typing coalescing).
        self.state.replace_range(range, new_text);
        self.after_mutation(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = range_utf16
            .map(|r| self.state.utf16_range_to_utf8(&r))
            .or_else(|| self.state.marked_range().cloned())
            .unwrap_or_else(|| self.state.selection().range_bounds());

        self.state.snapshot_for_undo();
        self.state
            .replace_range_silent(target.clone(), new_text, !new_text.is_empty());

        // `new_selected_range_utf16` is relative to `new_text` (the composition
        // string), per the GPUI EntityInputHandler contract. Convert the UTF-16
        // offsets against the raw composition text, then offset by target.start.
        if let Some(new_sel_utf16) = new_selected_range_utf16 {
            let local_utf8_start = crate::utf::utf16_offset_to_utf8(new_text, new_sel_utf16.start);
            let local_utf8_end = crate::utf::utf16_offset_to_utf8(new_text, new_sel_utf16.end);
            self.state.set_selection_range(
                target.start + local_utf8_start,
                target.start + local_utf8_end,
            );
        }
        self.after_mutation(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.state.utf16_range_to_utf8(&range_utf16);
        let scrolled = self.scrolled_bounds(element_bounds);
        layout.bounds_for_utf8_range(&scrolled, self.state.text(), range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let layout = self.last_layout.as_ref()?;
        let bounds = self.last_bounds?;
        let scrolled = self.scrolled_bounds(bounds);
        let utf8_offset = layout.utf8_index_at_position(&scrolled, point, self.state.text());
        Some(self.state.utf8_to_utf16(utf8_offset))
    }
}

impl Render for TextBox {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(_window);

        // Manage caret blink task: spawn when gaining focus, drop when losing it.
        if focused && self.blink_task.is_none() {
            self.caret_blink_on = true;
            let task = cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
                let Ok(()) = this.update(cx, |this, cx| {
                    this.caret_blink_on = !this.caret_blink_on;
                    cx.notify();
                }) else {
                    break;
                };
            });
            self.blink_task = Some(task);
            cx.notify(); // immediate repaint with caret visible
        } else if !focused && self.blink_task.is_some() {
            self.blink_task = None;
            self.caret_blink_on = true;
        }
        let border_color = if self.state.is_disabled() {
            self.style.border_disabled
        } else if focused {
            self.style.border_focused
        } else {
            match self.state.validation() {
                ValidationState::Invalid(_) => self.style.border_invalid,
                ValidationState::Warning(_) => self.style.border_warning,
                _ => self.style.border,
            }
        };
        let bg = if focused {
            self.style.background_focused
        } else {
            self.style.background
        };

        let validation_msg = match self.state.validation() {
            ValidationState::Invalid(m) => Some((m.clone(), self.style.invalid_message)),
            ValidationState::Warning(m) => Some((m.clone(), self.style.warning_message)),
            _ => None,
        };

        let cursor_style = if self.state.is_disabled() {
            CursorStyle::Arrow
        } else {
            CursorStyle::IBeam
        };

        let element = TextBoxElement::new(cx.entity());

        let mut field = apply_border_width(
            gpui::div()
                .id(("TextBox", cx.entity().entity_id()))
                .key_context("TextBox")
                .track_focus(&self.focus_handle)
                .cursor(cursor_style)
                .bg(bg)
                .border_color(border_color),
            self.style.border_width_px,
        )
        .rounded(self.style.corner_radius)
        // Flex + cross-axis alignment position the text element within the
        // field's content box. The element's height is `line_height * rows`;
        // when the field is taller (min_height or parent stretch), the
        // vertical_align controls where the text sits.
        .flex()
        .p(self.style.padding);

        field = match self.style.vertical_align {
            VerticalAlign::Top => field.items_start(),
            VerticalAlign::Middle => field.items_center(),
            VerticalAlign::Bottom => field.items_end(),
        };

        if self.style.min_height > px(0.) {
            field = field.min_h(self.style.min_height);
        }

        let field = field
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            // Editing
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete_forward))
            // Movement
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::document_start))
            .on_action(cx.listener(Self::document_end))
            // Select movement
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::select_document_start))
            .on_action(cx.listener(Self::select_document_end))
            // Selection
            .on_action(cx.listener(Self::select_all))
            // Clipboard
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            // Undo/redo
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            // IME/commit
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::commit_action))
            .on_action(cx.listener(Self::show_character_palette))
            .child(element);

        if let Some((msg, color)) = validation_msg {
            gpui::div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field)
                .child(
                    gpui::div()
                        .text_xs()
                        .text_color(color)
                        .child(gpui::SharedString::from(msg)),
                )
                .into_any_element()
        } else {
            field.into_any_element()
        }
    }
}

/// Apply a border width (in pixels) to a styled element. GPUI only exposes
/// discrete border-width helpers, so the requested width is rounded to the
/// nearest supported step (0–8 px, saturating above).
fn apply_border_width<E: gpui::Styled>(el: E, width_px: f32) -> E {
    match width_px.round().max(0.) as u32 {
        0 => el.border_0(),
        1 => el.border_1(),
        2 => el.border_2(),
        3 => el.border_3(),
        4 => el.border_4(),
        5 => el.border_5(),
        6 => el.border_6(),
        7 => el.border_7(),
        _ => el.border_8(),
    }
}
