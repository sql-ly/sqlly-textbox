//! Custom `Element` implementation that paints a `TextBox` field.
//!
//! The element handles shaping, layout, selection painting, caret painting,
//! IME marked-text underline, placeholder rendering, and registers an
//! `ElementInputHandler` so platform IME and accessibility services can drive
//! the field through the GPUI input pipeline.

use std::ops::Range;

use gpui::{
    fill, font, px, relative, size, App, Bounds, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hsla, IntoElement, LayoutId, Pixels, Point, ShapedLine, SharedString, Size,
    Style, TextAlign, TextRun, UnderlineStyle, Window, WrappedLine,
};
use smallvec::SmallVec;

use crate::mode::{Mode, TextWrap};
use crate::TextBox;

/// Layout cache produced by the element each paint and replicated back to the
/// entity so mouse handlers and `EntityInputHandler` can map positions.
#[derive(Clone)]
pub enum LastLayout {
    /// Single-line mode. No soft wrapping.
    Single {
        shaped: ShapedLine,
        original_len: usize,
    },
    /// Multi-line mode. Each entry corresponds to one `\n`-separated logical
    /// line; wrap boundaries inside it are visual row breaks.
    Multi {
        lines: SmallVec<[WrappedLine; 1]>,
        line_height: Pixels,
    },
}

impl LastLayout {
    pub fn line_height(&self) -> Pixels {
        match self {
            LastLayout::Single { shaped, .. } => line_height_for(shaped),
            LastLayout::Multi { line_height, .. } => *line_height,
        }
    }

    /// Map a window-coordinate point to a UTF-8 byte offset in `text`.
    pub fn utf8_index_at_position(
        &self,
        bounds: &Bounds<Pixels>,
        position: Point<Pixels>,
        text: &str,
    ) -> usize {
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return text.len();
        }
        let local = Point::new(position.x - bounds.left(), position.y - bounds.top());
        match self {
            LastLayout::Single {
                shaped,
                original_len,
            } => {
                if *original_len != text.len() {
                    return 0;
                }
                shaped.closest_index_for_x(local.x)
            }
            LastLayout::Multi { lines, line_height } => {
                let mut byte_idx = 0usize;
                let mut row_y = Pixels::ZERO;
                for wline in lines.iter() {
                    let vis_rows = wline.wrap_boundaries().len() as f32 + 1.0;
                    let row_end_y = row_y + *line_height * vis_rows;
                    if local.y <= row_end_y {
                        let pos_result = wline.closest_index_for_position(local, *line_height);
                        return byte_idx + pos_result.unwrap_or_else(|i| i);
                    }
                    byte_idx = advance_byte_idx(byte_idx, wline, text);
                    row_y = row_end_y;
                }
                text.len()
            }
        }
    }

    /// Bounds (in window coords) of the given UTF-8 byte range.
    pub fn bounds_for_utf8_range(
        &self,
        bounds: &Bounds<Pixels>,
        text: &str,
        range: Range<usize>,
    ) -> Option<Bounds<Pixels>> {
        match self {
            LastLayout::Single {
                shaped,
                original_len,
            } => {
                if *original_len != text.len() {
                    return None;
                }
                let s = shaped.x_for_index(range.start.min(text.len()));
                let e = shaped.x_for_index(range.end.min(text.len()));
                let line_h = line_height_for(shaped);
                Some(Bounds::from_corners(
                    Point::new(bounds.left() + s, bounds.top()),
                    Point::new(bounds.left() + e, bounds.top() + line_h),
                ))
            }
            LastLayout::Multi { lines, line_height } => {
                let mut byte_idx = 0usize;
                let mut row_y = bounds.top();
                let mut start_point: Option<Point<Pixels>> = None;
                let mut end_point: Option<Point<Pixels>> = None;
                for wline in lines.iter() {
                    let line_logical_end = wline.len()
                        + if byte_idx + wline.len() < text.len() {
                            1
                        } else {
                            0
                        };
                    if start_point.is_none() && range.start <= byte_idx + wline.len() {
                        let rel = range.start.saturating_sub(byte_idx);
                        if let Some(pos) = wline.position_for_index(rel, *line_height) {
                            start_point = Some(Point::new(
                                bounds.left() + pos.x,
                                bounds.top() + row_y + pos.y,
                            ));
                        }
                    }
                    if end_point.is_none() && range.end <= byte_idx + wline.len() {
                        let rel = range.end.saturating_sub(byte_idx);
                        if let Some(pos) = wline.position_for_index(rel, *line_height) {
                            end_point = Some(Point::new(
                                bounds.left() + pos.x,
                                bounds.top() + row_y + pos.y,
                            ));
                        }
                    }
                    if start_point.is_some() && end_point.is_some() {
                        break;
                    }
                    let vis_rows = wline.wrap_boundaries().len() as f32 + 1.0;
                    byte_idx += line_logical_end;
                    row_y += *line_height * vis_rows;
                }
                match (start_point, end_point) {
                    (Some(s), Some(e)) => Some(Bounds::from_corners(s, e)),
                    _ => None,
                }
            }
        }
    }
}

/// After processing a logical line, how many UTF-8 bytes does it consume in
/// the original text? Includes the trailing `\n` if there is one.
fn advance_byte_idx(byte_idx: usize, wline: &WrappedLine, text: &str) -> usize {
    let logical_len = wline.len()
        + if byte_idx + wline.len() < text.len()
            && text.as_bytes().get(byte_idx + wline.len()) == Some(&b'\n')
        {
            1
        } else {
            0
        };
    byte_idx + logical_len
}

// ----------------------------------------------------------------------------
// The custom element.
// ----------------------------------------------------------------------------

pub struct TextBoxElement {
    input: Entity<TextBox>,
}

impl TextBoxElement {
    pub fn new(input: Entity<TextBox>) -> Self {
        Self { input }
    }
}

impl IntoElement for TextBoxElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextBoxElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let state = input.state();
        let min_lines = state.min_lines().max(1);
        let max_lines = state.max_lines();
        let line_height = input.style_ref().line_height;

        // Grow with explicit logical lines (cheap, pre-shaping estimate), but
        // clamp to [min_lines, max_lines]. Soft-wrapped overflow scrolls inside
        // this viewport rather than expanding it.
        let logical_lines = state.text().split('\n').count().max(1);
        let mut rows = logical_lines.max(min_lines);
        if let Some(max) = max_lines {
            rows = rows.min(max.max(min_lines));
        }

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (line_height * rows as f32).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let input = self.input.read(cx);
        // Pull out focus handle by value (it's Clone) so we can use it after
        // the mutable borrow of `cx` for paint operations.
        let focus_handle = input.focus_handle_ref().clone();
        let text_color = input.effective_text_color();
        let placeholder_color = input.style_ref().placeholder;
        let selection_color = input.style_ref().selection;
        let caret_color = input.style_ref().caret;
        let line_height = input.style_ref().line_height;
        let font_size = input.style_ref().font_size;
        let mode = input.state().mode().clone();
        let real_text = input.state().text();
        let is_empty = real_text.is_empty();
        let show_placeholder_inputs = (
            is_empty,
            input.state().marked_range().is_none(),
            !input.placeholder_str().is_empty(),
        );

        // Register IME handler.
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let show_placeholder =
            show_placeholder_inputs.0 && show_placeholder_inputs.1 && show_placeholder_inputs.2;

        // Mask glyphs for password mode.
        let masked = input.state().is_password() && !is_empty;
        let display_text: SharedString = if show_placeholder {
            input.placeholder_str().clone()
        } else if masked {
            let n = real_text.chars().count().max(1);
            SharedString::from("•".repeat(n))
        } else {
            SharedString::from(real_text.to_string())
        };

        let base_color = if show_placeholder {
            placeholder_color
        } else {
            text_color
        };

        let marked = input.state().marked_range().cloned();
        let runs = build_text_runs(&display_text, base_color, marked.as_ref());

        // Shape text according to mode.
        let layout = match mode {
            Mode::SingleLine => {
                let shaped =
                    window
                        .text_system()
                        .shape_line(display_text.clone(), font_size, &runs, None);
                LastLayout::Single {
                    shaped,
                    original_len: real_text.len(),
                }
            }
            Mode::MultiLine { .. } => {
                // Use shape_text for both wrap modes; it returns SmallVec<[WrappedLine; 1]>.
                let wrap_width = match input.state().wrap() {
                    TextWrap::Soft => Some(bounds.size.width),
                    TextWrap::None => Some(Pixels::MAX),
                };
                let wrapped = window
                    .text_system()
                    .shape_text(display_text.clone(), font_size, &runs, wrap_width, Some(64))
                    .ok();
                match wrapped {
                    Some(w) => LastLayout::Multi {
                        lines: w,
                        line_height,
                    },
                    None => {
                        // Fallback: shape_line if shape_text failed.
                        let shaped = window.text_system().shape_line(
                            display_text.clone(),
                            font_size,
                            &runs,
                            None,
                        );
                        LastLayout::Single {
                            shaped,
                            original_len: real_text.len(),
                        }
                    }
                }
            }
        };

        // Selection & caret.
        let sel = input.state().selection().clone();

        // Determine vertical scroll (multi-line) to keep the caret visible.
        let viewport_h = bounds.size.height;
        let mut scroll = input.scroll_offset;
        let content_height = match &layout {
            LastLayout::Multi {
                lines,
                line_height: lh,
            } => {
                let rows: f32 = lines
                    .iter()
                    .map(|w| w.wrap_boundaries().len() as f32 + 1.0)
                    .sum();
                *lh * rows
            }
            LastLayout::Single { .. } => line_height,
        };
        // Caret position in unscrolled content coordinates drives scrolling.
        if let Some(cq) = build_caret_quad(
            &layout,
            &bounds,
            &display_text,
            sel.head(),
            caret_color,
            line_height,
        ) {
            let caret_top = cq.bounds.origin.y - bounds.top();
            let caret_bottom = caret_top + line_height;
            if caret_top < scroll {
                scroll = caret_top;
            } else if caret_bottom > scroll + viewport_h {
                scroll = caret_bottom - viewport_h;
            }
        }
        let max_scroll = (content_height - viewport_h).max(Pixels::ZERO);
        scroll = scroll.clamp(Pixels::ZERO, max_scroll);

        // All painting happens against bounds shifted up by the scroll offset.
        let scrolled_bounds = Bounds {
            origin: Point::new(bounds.left(), bounds.top() - scroll),
            size: bounds.size,
        };

        let selection_quads = build_selection_quads(
            &layout,
            &scrolled_bounds,
            &display_text,
            sel.range_bounds(),
            selection_color,
            line_height,
        );
        let caret_quad = build_caret_quad(
            &layout,
            &scrolled_bounds,
            &display_text,
            sel.head(),
            caret_color,
            line_height,
        );

        let focused = focus_handle.is_focused(window);

        // Clip all content (selection, text, caret) to the field viewport so
        // scrolled-away rows don't bleed outside the box.
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for q in &selection_quads {
                window.paint_quad(q.clone());
            }

            match &layout {
                LastLayout::Single { shaped, .. } => {
                    shaped
                        .paint(scrolled_bounds.origin, layout.line_height(), window, cx)
                        .ok();
                }
                LastLayout::Multi {
                    lines,
                    line_height: lh,
                } => {
                    let mut row_y = scrolled_bounds.top();
                    for wline in lines.iter() {
                        wline
                            .paint(
                                Point::new(bounds.left(), row_y),
                                *lh,
                                TextAlign::Left,
                                Some(Bounds {
                                    origin: Point::new(bounds.left(), row_y),
                                    size: Size {
                                        width: bounds.size.width,
                                        height: *lh,
                                    },
                                }),
                                window,
                                cx,
                            )
                            .ok();
                        let vis_rows = wline.wrap_boundaries().len() as f32 + 1.0;
                        row_y += *lh * vis_rows;
                    }
                }
            }

            if focused {
                if let Some(cq) = caret_quad.clone() {
                    window.paint_quad(cq);
                }
            }
        });

        // Persist layout + scroll for mouse & IME queries.
        self.input.update(cx, |this, _cx| {
            this.scroll_offset = scroll;
            this.record_layout(bounds, layout.clone());
        });
    }
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

fn build_text_runs(
    text: &str,
    color: Hsla,
    marked: Option<&Range<usize>>,
) -> SmallVec<[TextRun; 4]> {
    let mut runs: SmallVec<[TextRun; 4]> = SmallVec::new();
    let f = font("");
    let push = |runs: &mut SmallVec<[TextRun; 4]>, len: usize, underline: bool| {
        runs.push(TextRun {
            len,
            font: f.clone(),
            color,
            background_color: None,
            underline: if underline {
                Some(UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.0),
                    wavy: false,
                })
            } else {
                None
            },
            strikethrough: None,
        });
    };

    if let Some(marked_range) = marked {
        if marked_range.start > 0 {
            push(&mut runs, marked_range.start, false);
        }
        push(
            &mut runs,
            marked_range.end.saturating_sub(marked_range.start),
            true,
        );
        if marked_range.end < text.len() {
            push(
                &mut runs,
                text.len().saturating_sub(marked_range.end),
                false,
            );
        }
    } else {
        push(&mut runs, text.len(), false);
    }
    runs
}

fn build_selection_quads(
    layout: &LastLayout,
    bounds: &Bounds<Pixels>,
    text: &str,
    range: Range<usize>,
    color: Hsla,
    line_height: Pixels,
) -> Vec<gpui::PaintQuad> {
    let mut quads: Vec<gpui::PaintQuad> = Vec::new();
    if range.start == range.end {
        return quads;
    }
    match layout {
        LastLayout::Single {
            shaped,
            original_len,
        } => {
            if *original_len != text.len() {
                return quads;
            }
            let s = shaped.x_for_index(range.start.min(text.len()));
            let e = shaped.x_for_index(range.end.min(text.len()));
            quads.push(fill(
                Bounds::from_corners(
                    Point::new(bounds.left() + s, bounds.top()),
                    Point::new(bounds.left() + e, bounds.top() + line_height),
                ),
                color,
            ));
        }
        LastLayout::Multi {
            lines,
            line_height: lh,
        } => {
            let mut byte_idx = 0usize;
            let mut row_y = bounds.top();
            for wline in lines.iter() {
                let line_logical_end = wline.len()
                    + if byte_idx + wline.len() < text.len()
                        && text.as_bytes().get(byte_idx + wline.len()) == Some(&b'\n')
                    {
                        1
                    } else {
                        0
                    };
                if range.start < byte_idx + wline.len() && range.end > byte_idx {
                    let rel_start = range.start.max(byte_idx).saturating_sub(byte_idx);
                    let rel_end = range
                        .end
                        .min(byte_idx + wline.len())
                        .saturating_sub(byte_idx);
                    if rel_start != rel_end {
                        let s_pos = wline
                            .position_for_index(rel_start, *lh)
                            .unwrap_or(Point::new(Pixels::ZERO, Pixels::ZERO));
                        let e_pos = wline
                            .position_for_index(rel_end, *lh)
                            .unwrap_or(Point::new(Pixels::ZERO, *lh));
                        quads.push(fill(
                            Bounds::from_corners(
                                Point::new(bounds.left() + s_pos.x, bounds.top() + row_y + s_pos.y),
                                Point::new(bounds.left() + e_pos.x, bounds.top() + row_y + e_pos.y),
                            ),
                            color,
                        ));
                    }
                }
                byte_idx += line_logical_end;
                let vis_rows = wline.wrap_boundaries().len() as f32 + 1.0;
                row_y += *lh * vis_rows;
            }
        }
    }
    quads
}

fn build_caret_quad(
    layout: &LastLayout,
    bounds: &Bounds<Pixels>,
    text: &str,
    offset: usize,
    color: Hsla,
    line_height: Pixels,
) -> Option<gpui::PaintQuad> {
    match layout {
        LastLayout::Single {
            shaped,
            original_len,
        } => {
            if *original_len != text.len() {
                return None;
            }
            let x = shaped.x_for_index(offset.min(text.len()));
            Some(fill(
                Bounds::new(
                    Point::new(bounds.left() + x, bounds.top()),
                    size(px(2.0), line_height),
                ),
                color,
            ))
        }
        LastLayout::Multi {
            lines,
            line_height: lh,
        } => {
            let mut byte_idx = 0usize;
            let mut row_y = bounds.top();
            for wline in lines.iter() {
                let line_logical_end = wline.len()
                    + if byte_idx + wline.len() < text.len()
                        && text.as_bytes().get(byte_idx + wline.len()) == Some(&b'\n')
                    {
                        1
                    } else {
                        0
                    };
                if offset <= byte_idx + wline.len() {
                    let rel = offset.min(byte_idx + wline.len()).saturating_sub(byte_idx);
                    if let Some(pos) = wline.position_for_index(rel, *lh) {
                        return Some(fill(
                            Bounds::new(
                                Point::new(bounds.left() + pos.x, bounds.top() + row_y + pos.y),
                                size(px(2.0), *lh),
                            ),
                            color,
                        ));
                    }
                }
                let vis_rows = wline.wrap_boundaries().len() as f32 + 1.0;
                byte_idx += line_logical_end;
                row_y += *lh * vis_rows;
            }
            Some(fill(
                Bounds::new(Point::new(bounds.left(), row_y), size(px(2.0), line_height)),
                color,
            ))
        }
    }
}

fn line_height_for(shaped: &ShapedLine) -> Pixels {
    // LineLayout exposes `ascent`/`descent` as public fields.
    // Deref chains: ShapedLine -> Arc<LineLayout> -> LineLayout (then field access).
    shaped.ascent + shaped.descent
}
