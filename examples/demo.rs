//! `sqlly-textbox` demo — a tabbed showcase of every configurable feature.
//!
//! Each tab covers one feature set with several example configurations, each
//! shown alongside the builder-chain code that produced it.
//!
//! Run with `cargo run --example demo`.

use std::sync::Arc;

use gpui::{
    div, hsla, prelude::*, px, size, AnyElement, App, Application, Bounds, Context, Entity, Hsla,
    MouseButton, Render, SharedString, Window, WindowBounds, WindowOptions,
};

use sqlly_textbox::{
    install_text_box_keybindings, sync_validator, AsyncValidator, ComponentStyle, Mode, TextBox,
    TextWrap, ValidationState,
};

fn main() {
    Application::new().run(|cx: &mut App| {
        install_text_box_keybindings(cx);

        let bounds = Bounds::centered(None, size(px(860.0), px(940.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|cx| Demo::new(cx)),
        )
        .unwrap();
        cx.activate(true);
    });
}

// ---------------------------------------------------------------------------
// Colors used across the demo chrome.
// ---------------------------------------------------------------------------

// `hsla` is not a const fn, so colors are exposed as zero-arg functions.

fn ink() -> Hsla {
    hsla(0., 0., 0.12, 1.)
}
fn muted() -> Hsla {
    hsla(0., 0., 0., 0.5)
}
fn paper() -> Hsla {
    hsla(0., 0., 0.96, 1.)
}
fn code_bg() -> Hsla {
    hsla(0., 0., 0., 0.06)
}
fn tab_bg() -> Hsla {
    hsla(0.6, 0.6, 0.55, 1.)
}
fn line() -> Hsla {
    hsla(0., 0., 0., 0.1)
}

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Build a `TextBox` entity from a builder closure (avoids repeating `cx.new`).
fn field<F: FnOnce(&mut Context<TextBox>) -> TextBox>(
    cx: &mut Context<Demo>,
    build: F,
) -> Entity<TextBox> {
    cx.new(|cx| build(cx))
}

struct Example {
    label: &'static str,
    code: &'static str,
    field: Entity<TextBox>,
}

struct Tab {
    title: &'static str,
    examples: Vec<Example>,
}

struct Demo {
    active_tab: usize,
    tabs: Vec<Tab>,
}

impl Demo {
    fn new(cx: &mut Context<Self>) -> Self {
        // Shared callbacks.
        let on_change: Arc<dyn Fn(&str, &mut App) + Send + Sync> =
            Arc::new(|text, _cx| println!("[change] {text}"));
        let on_commit: Arc<dyn Fn(&str, &mut App) + Send + Sync> =
            Arc::new(|text, _cx| println!("[commit] {text}"));

        // Sync email validator.
        let email_validator = sync_validator(|t: &str| {
            if t.is_empty() {
                ValidationState::None
            } else if t.contains('@') && t.contains('.') {
                ValidationState::Valid
            } else {
                ValidationState::Invalid("must contain '@' and a '.'".to_string())
            }
        });

        // Async username validator (the entity's debounce provides the
        // "Validating…" window; this closure just maps text -> state).
        let async_validator: AsyncValidator = Arc::new(|v: String| {
            Box::pin(async move {
                if v.is_empty() {
                    ValidationState::None
                } else if v.len() < 3 {
                    ValidationState::Invalid("too short".to_string())
                } else if v == "admin" || v == "root" {
                    ValidationState::Invalid("name already taken".to_string())
                } else {
                    ValidationState::Valid
                }
            })
        });

        // ----- Tab 0: Basics --------------------------------------------------
        let basics = vec![
            Example {
                label: "Placeholder only",
                code: "TextBox::new(cx).placeholder(\"Type here…\")",
                field: field(cx, |cx| TextBox::new(cx).placeholder("Type here…")),
            },
            Example {
                label: "Prefilled value",
                code: "TextBox::new(cx).value(\"Hello, world\")",
                field: field(cx, |cx| TextBox::new(cx).value("Hello, world".to_string())),
            },
            Example {
                label: "on_change (logs to stdout)",
                code: "TextBox::new(cx).placeholder(\"logs each keystroke\").on_change(on_change)",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .placeholder("logs each keystroke")
                        .on_change(on_change.clone())
                }),
            },
            Example {
                label: "on_commit (press Enter)",
                code: "TextBox::new(cx).placeholder(\"press Enter\").on_commit(on_commit)",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .placeholder("press Enter to commit")
                        .on_commit(on_commit.clone())
                }),
            },
        ];

        // ----- Tab 1: Multi-line ---------------------------------------------
        let multi = vec![
            Example {
                label: "Soft-wrap notes (min 3 / max 8)",
                code: "TextBox::multi_line(3, cx)\n  .mode(MultiLine{min_lines:3,max_lines:Some(8),wrap:TextWrap::Soft})\n  .value(\"Hello\\nfrom\\na multi-line\\nfield.\")",
                field: field(cx, |cx| {
                    TextBox::multi_line(3, cx)
                        .mode(Mode::MultiLine {
                            min_lines: 3,
                            max_lines: Some(8),
                            wrap: TextWrap::Soft,
                        })
                        .value("Hello\nfrom\na multi-line\nfield.".to_string())
                }),
            },
            Example {
                label: "No-wrap (long lines overflow / clip)",
                code: "TextBox::multi_line(2, cx)\n  .mode(MultiLine{min_lines:2,wrap:TextWrap::None})\n  .value(\"A very long single line that does not wrap …\")",
                field: field(cx, |cx| {
                    TextBox::multi_line(2, cx)
                        .mode(Mode::MultiLine {
                            min_lines: 2,
                            max_lines: Some(4),
                            wrap: TextWrap::None,
                        })
                        .value("A very long single line that does not wrap — it overflows and is clipped by the viewport.".to_string())
                }),
            },
            Example {
                label: "Scrollable viewport (min 4 / max 6)",
                code: "TextBox::multi_line(4, cx)\n  .mode(MultiLine{min_lines:4,max_lines:Some(6),wrap:TextWrap::Soft})\n  .value(LONG_TEXT)",
                field: field(cx, |cx| {
                    TextBox::multi_line(4, cx)
                        .mode(Mode::MultiLine {
                            min_lines: 4,
                            max_lines: Some(6),
                            wrap: TextWrap::Soft,
                        })
                        .value(LONG_TEXT.to_string())
                }),
            },
            Example {
                label: "Large textarea (min 6 / max 12)",
                code: "TextBox::multi_line(6, cx)\n  .mode(MultiLine{min_lines:6,max_lines:Some(12),wrap:TextWrap::Soft})",
                field: field(cx, |cx| {
                    TextBox::multi_line(6, cx).mode(Mode::MultiLine {
                        min_lines: 6,
                        max_lines: Some(12),
                        wrap: TextWrap::Soft,
                    })
                }),
            },
        ];

        // ----- Tab 2: Validation ----------------------------------------------
        let validation = vec![
            Example {
                label: "External Invalid state",
                code: "TextBox::new(cx).value(\"bad\")\n  .validation_state(ValidationState::Invalid(\"rejected\".into()))",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .value("bad".to_string())
                        .validation_state(ValidationState::Invalid("rejected by server".to_string()))
                }),
            },
            Example {
                label: "External Warning state",
                code: "TextBox::new(cx).value(\"meh\")\n  .validation_state(ValidationState::Warning(\"check spelling\".into()))",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .value("meh".to_string())
                        .validation_state(ValidationState::Warning("check spelling".to_string()))
                }),
            },
            Example {
                label: "Sync validator (email)",
                code: "TextBox::new(cx).placeholder(\"user@example.com\")\n  .validator(sync_validator(|t| /* @ and . */))",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .placeholder("user@example.com")
                        .validator(email_validator.clone())
                }),
            },
            Example {
                label: "Async + debounced (400ms)",
                code: "TextBox::new(cx).placeholder(\"pick a username\")\n  .async_validator(av).set_debounce_ms(400)",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .placeholder("pick a username (try \"admin\")")
                        .async_validator(async_validator.clone())
                        .set_debounce_ms(400)
                }),
            },
        ];

        // ----- Tab 3: Modes ---------------------------------------------------
        let modes = vec![
            Example {
                label: "Password (masked, paste allowed, copy suppressed)",
                code: "TextBox::new(cx).password(true).placeholder(\"Secret\")",
                field: field(cx, |cx| TextBox::new(cx).password(true).placeholder("Secret")),
            },
            Example {
                label: "Disabled (no focus / no edit)",
                code: "TextBox::new(cx).value(\"locked\").disabled(true)",
                field: field(cx, |cx| TextBox::new(cx).value("locked".to_string()).disabled(true)),
            },
            Example {
                label: "Read-only (focus + select + copy allowed)",
                code: "TextBox::new(cx).value(\"read me\").read_only(true)",
                field: field(cx, |cx| TextBox::new(cx).value("read me".to_string()).read_only(true)),
            },
            Example {
                label: "Read-only selectable text",
                code: "TextBox::new(cx).value(\"copy me with Cmd/Ctrl-A → Cmd/Ctrl-C\").read_only(true)",
                field: field(cx, |cx| {
                    TextBox::new(cx)
                        .value("copy me with Cmd/Ctrl-A then Cmd/Ctrl-C".to_string())
                        .read_only(true)
                }),
            },
        ];

        // ----- Tab 4: Styling -------------------------------------------------
        let dark = {
            let s = ComponentStyle::default();
            ComponentStyle {
                background: hsla(0., 0., 0.12, 1.),
                background_focused: hsla(0., 0., 0.16, 1.),
                border: hsla(0., 0., 1., 0.15),
                border_focused: hsla(0.6, 0.8, 0.65, 1.),
                text: hsla(0., 0., 0.95, 1.),
                placeholder: hsla(0., 0., 1., 0.4),
                selection: hsla(0.6, 0.6, 0.5, 0.5),
                caret: hsla(0.6, 0.8, 0.75, 1.),
                ..s
            }
        };
        let accent_ring = {
            let s = ComponentStyle::default();
            ComponentStyle {
                border_focused: hsla(0.55, 0.9, 0.6, 1.),
                ..s
            }
        };
        let thick_border = {
            let s = ComponentStyle::default();
            ComponentStyle {
                border_width_px: 3.,
                border: hsla(0., 0., 0., 0.4),
                ..s
            }
        };
        let pill = {
            let s = ComponentStyle::default();
            ComponentStyle {
                corner_radius: px(20.),
                padding: px(12.),
                ..s
            }
        };
        let large_font = {
            let s = ComponentStyle::default();
            ComponentStyle {
                font_size: px(20.),
                line_height: px(30.),
                padding: px(12.),
                ..s
            }
        };

        let styling = vec![
            Example {
                label: "Dark theme",
                code: "TextBox::new(cx).value(\"dark mode\")\n  .style(ComponentStyle{ background:#1e1e1e, text:#eee, … })",
                field: field(cx, |cx| TextBox::new(cx).value("dark mode".to_string()).style(dark)),
            },
            Example {
                label: "Accent focus ring",
                code: "TextBox::new(cx).placeholder(\"focus me\")\n  .style(ComponentStyle{ border_focused: hsla(0.55,0.9,0.6,1.), … })",
                field: field(cx, |cx| TextBox::new(cx).placeholder("focus me").style(accent_ring)),
            },
            Example {
                label: "Thick border (3px)",
                code: "TextBox::new(cx).value(\"bold\")\n  .style(ComponentStyle{ border_width_px: 3., … })",
                field: field(cx, |cx| TextBox::new(cx).value("bold".to_string()).style(thick_border)),
            },
            Example {
                label: "Rounded pill",
                code: "TextBox::new(cx).placeholder(\"pill\")\n  .style(ComponentStyle{ corner_radius: px(20.), … })",
                field: field(cx, |cx| TextBox::new(cx).placeholder("pill").style(pill)),
            },
            Example {
                label: "Large font (20px / 30px line)",
                code: "TextBox::new(cx).value(\"big\")\n  .style(ComponentStyle{ font_size: px(20.), line_height: px(30.), … })",
                field: field(cx, |cx| TextBox::new(cx).value("big".to_string()).style(large_font)),
            },
        ];

        // ----- Tab 5: Unicode & IME -------------------------------------------
        let unicode = vec![
            Example {
                label: "Emoji",
                code: "TextBox::new(cx).value(\"😀🚀✨🎉\")",
                field: field(cx, |cx| TextBox::new(cx).value("😀🚀✨🎉".to_string())),
            },
            Example {
                label: "CJK (Han, Hiragana, Katakana)",
                code: "TextBox::new(cx).value(\"漢字 ひらがな カタカナ 漢字検査\")",
                field: field(cx, |cx| {
                    TextBox::new(cx).value("漢字 ひらがな カタカナ 漢字検査".to_string())
                }),
            },
            Example {
                label: "Combining marks (e + U+0301 = é)",
                code: "TextBox::new(cx).value(\"cafe\\u{0301}\")",
                field: field(cx, |cx| TextBox::new(cx).value("cafe\u{0301}".to_string())),
            },
            Example {
                label: "Mixed paragraph (soft-wrap)",
                code: "TextBox::multi_line(3, cx).mode(MultiLine{…Soft}).value(MIXED)",
                field: field(cx, |cx| {
                    TextBox::multi_line(3, cx)
                        .mode(Mode::MultiLine {
                            min_lines: 3,
                            max_lines: Some(7),
                            wrap: TextWrap::Soft,
                        })
                        .value(MIXED.to_string())
                }),
            },
        ];

        Self {
            active_tab: 0,
            tabs: vec![
                Tab {
                    title: "Basics",
                    examples: basics,
                },
                Tab {
                    title: "Multi-line",
                    examples: multi,
                },
                Tab {
                    title: "Validation",
                    examples: validation,
                },
                Tab {
                    title: "Modes",
                    examples: modes,
                },
                Tab {
                    title: "Styling",
                    examples: styling,
                },
                Tab {
                    title: "Unicode & IME",
                    examples: unicode,
                },
            ],
        }
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut buttons: Vec<AnyElement> = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let active = i == self.active_tab;
            let mut el = div()
                .px_4()
                .py_2()
                .text_sm()
                .rounded_md()
                .child(SharedString::from(tab.title));
            if active {
                el = el.bg(tab_bg()).text_color(hsla(0., 0., 1., 1.));
            } else {
                el = el.text_color(muted()).bg(hsla(0., 0., 1., 0.5));
            }
            let idx = i;
            el = el.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _ev, _window, _cx| {
                    this.active_tab = idx;
                }),
            );
            buttons.push(el.into_any_element());
        }
        div()
            .flex()
            .flex_row()
            .gap_1()
            .px_3()
            .border_b_1()
            .border_color(line())
            .bg(paper())
            .children(buttons)
            .into_any_element()
    }

    fn render_example(ex: &Example) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .text_color(ink())
                    .child(SharedString::from(ex.label)),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted())
                    .bg(code_bg())
                    .p_2()
                    .rounded_sm()
                    .child(SharedString::from(ex.code)),
            )
            .child(ex.field.clone())
            .into_any_element()
    }
}

const LONG_TEXT: &str = "The longest recognized English word is \
pneumonoultramicroscopicsilicovolcanoconiosis, a 45-letter word coined in 1935 \
to describe a lung disease caused by silica dust. This field demonstrates soft \
wrapping, line navigation, selection painting across multiple visual rows, and \
vertical scrolling when content exceeds the max_lines viewport. Try selecting \
text, holding shift and clicking further out, or pressing Cmd/Ctrl-A to select all.";

const MIXED: &str = "Hello 世界! 😀 Here is some mixed text —漢字— with emoji 🚀 \
and combining marks (cafe\u{0301} = café) and a long line that should soft-wrap \
across the available width of the field. ひらがな カタカナ 한글";

impl Render for Demo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let tab_bar = self.render_tab_bar(_cx);
        let active = self.active_tab.min(self.tabs.len() - 1);
        let examples: Vec<AnyElement> = self.tabs[active]
            .examples
            .iter()
            .map(Self::render_example)
            .collect();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(paper())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .child(div().text_lg().text_color(ink()).child("sqlly-textbox"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted())
                            .child("Tabbed feature showcase — click a tab"),
                    ),
            )
            .child(tab_bar)
            .child(
                div()
                    .id("content")
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .size_full()
                    .overflow_y_scroll()
                    .children(examples),
            )
            .into_any_element()
    }
}
