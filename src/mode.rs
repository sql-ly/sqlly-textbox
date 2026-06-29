//! Text input mode — single-line or multi-line — with line and wrap configuration.

use gpui::SharedString;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Single-line input. Pasted or IME newlines are normalized to spaces.
    #[default]
    SingleLine,
    /// Multi-line input with a configurable visible line range and optional wrapping.
    MultiLine {
        min_lines: usize,
        max_lines: Option<usize>,
        wrap: TextWrap,
    },
}

impl Mode {
    pub fn is_multiline(&self) -> bool {
        matches!(self, Mode::MultiLine { .. })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextWrap {
    /// Soft wrap on whitespace/word boundary at the available width. Logical
    /// text remains unchanged.
    #[default]
    Soft,
    /// No wrap — long lines may overflow horizontally. The element should clip.
    None,
}

/// Placeholder shown when the field is empty and not composing IME text.
#[derive(Clone, Debug, Default)]
pub struct Placeholder(pub SharedString);

impl Placeholder {
    pub fn new(s: impl Into<SharedString>) -> Self {
        Placeholder(s.into())
    }
}

impl From<String> for Placeholder {
    fn from(s: String) -> Self {
        Placeholder(SharedString::from(s))
    }
}

impl From<SharedString> for Placeholder {
    fn from(s: SharedString) -> Self {
        Placeholder(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_single_line() {
        assert_eq!(Mode::default(), Mode::SingleLine);
        assert!(!Mode::default().is_multiline());
    }

    #[test]
    fn multi_line_default_wrap_is_soft() {
        let m = Mode::MultiLine {
            min_lines: 1,
            max_lines: None,
            wrap: TextWrap::default(),
        };
        assert!(m.is_multiline());
        assert_eq!(
            m,
            Mode::MultiLine {
                min_lines: 1,
                max_lines: None,
                wrap: TextWrap::Soft
            }
        );
    }
}
