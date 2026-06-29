//! UTF-8 ↔ UTF-16 offset conversions for GPUI's `EntityInputHandler` boundary.
//!
//! GPUI's platform text input and IME APIs operate on UTF-16 offsets. Internally,
//! we keep text in `String` storage and use UTF-8 byte offsets for fast mutation.
//! Conversion only happens on the boundary between our state and GPUI.

use std::ops::Range;

/// Convert a UTF-8 byte offset into the corresponding UTF-16 code unit offset.
pub fn utf8_offset_to_utf16(text: &str, utf8_offset: usize) -> usize {
    if utf8_offset >= text.len() {
        return text.encode_utf16().count();
    }
    let mut utf16_offset = 0;
    let mut utf8_seen = 0;
    for ch in text.chars() {
        if utf8_seen >= utf8_offset {
            break;
        }
        utf8_seen += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }
    utf16_offset
}

/// Convert a UTF-16 code unit offset into the corresponding UTF-8 byte offset.
///
/// If the offset falls inside a surrogate pair or beyond the end of the text,
/// the result is clamped to the nearest UTF-8 character boundary.
pub fn utf16_offset_to_utf8(text: &str, utf16_offset: usize) -> usize {
    let mut utf16_seen = 0;
    let mut utf8_seen = 0;
    for ch in text.chars() {
        let ch_utf16_len = ch.len_utf16();
        if utf16_seen + ch_utf16_len > utf16_offset {
            break;
        }
        utf16_seen += ch_utf16_len;
        utf8_seen += ch.len_utf8();
    }
    utf8_seen
}

/// Convert a UTF-8 byte range to a UTF-16 code-unit range.
pub fn utf8_range_to_utf16(text: &str, range: Range<usize>) -> Range<usize> {
    utf8_offset_to_utf16(text, range.start)..utf8_offset_to_utf16(text, range.end)
}

/// Convert a UTF-16 code-unit range to a UTF-8 byte range.
pub fn utf16_range_to_utf8(text: &str, range: Range<usize>) -> Range<usize> {
    utf16_offset_to_utf8(text, range.start)..utf16_offset_to_utf8(text, range.end)
}

/// Clamp a UTF-8 byte offset to the nearest char boundary at or before it.
pub fn floor_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut i = offset.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Clamp a UTF-8 byte offset to the nearest char boundary at or after it.
pub fn ceil_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut i = offset;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Clamp a UTF-8 byte range so both endpoints sit on char boundaries.
///
/// Both endpoints are floored to the nearest preceding char boundary. This is
/// safe for `text.replace_range(clamped, ...)` which would panic on
/// mid-character offsets.
pub fn clamp_range(text: &str, range: &Range<usize>) -> Range<usize> {
    floor_char_boundary(text, range.start)..floor_char_boundary(text, range.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_round_trip() {
        let text = "hello world";
        assert_eq!(utf8_offset_to_utf16(text, 0), 0);
        assert_eq!(utf8_offset_to_utf16(text, 5), 5);
        assert_eq!(utf8_offset_to_utf16(text, 11), 11);
        assert_eq!(utf8_offset_to_utf16(text, 100), 11);
        assert_eq!(utf16_offset_to_utf8(text, 0), 0);
        assert_eq!(utf16_offset_to_utf8(text, 5), 5);
        assert_eq!(utf16_offset_to_utf8(text, 11), 11);
    }

    #[test]
    fn emoji_bmp_vs_supplementary() {
        let text = "a😀b";
        assert_eq!(text.len(), 1 + 4 + 1);
        assert_eq!(utf8_offset_to_utf16(text, 0), 0);
        assert_eq!(utf8_offset_to_utf16(text, 1), 1);
        assert_eq!(utf8_offset_to_utf16(text, 5), 3);
        assert_eq!(utf8_offset_to_utf16(text, 6), 4);
        assert_eq!(utf16_offset_to_utf8(text, 0), 0);
        assert_eq!(utf16_offset_to_utf8(text, 1), 1);
        assert_eq!(utf16_offset_to_utf8(text, 3), 5);
        assert_eq!(utf16_offset_to_utf8(text, 4), 6);
    }

    #[test]
    fn cjk_two_utf16_per_codepoint() {
        let text = "a你b";
        assert_eq!(text.len(), 1 + 3 + 1);
        assert_eq!(utf8_offset_to_utf16(text, 0), 0);
        assert_eq!(utf8_offset_to_utf16(text, 1), 1);
        // 你 is in the BMP, so it uses 1 UTF-16 code unit. utf8 byte 4 is
        // where 'b' starts, which is the 2nd utf-16 code unit.
        assert_eq!(utf8_offset_to_utf16(text, 4), 2);
        assert_eq!(utf8_offset_to_utf16(text, 5), 3);
    }

    #[test]
    fn cjk_full_char_double_utf16() {
        // Use a CJK character in a supplementary plane position which would
        // require 2 UTF-16 code units. Surrogate pair example: U+1F600.
        let text = "a😀b";
        // 😀 = U+1F600 = 4 UTF-8 bytes, 2 UTF-16 code units.
        assert_eq!(text.len(), 1 + 4 + 1);
        assert_eq!(utf8_offset_to_utf16(text, 0), 0);
        assert_eq!(utf8_offset_to_utf16(text, 1), 1);
        assert_eq!(utf8_offset_to_utf16(text, 5), 3);
        assert_eq!(utf8_offset_to_utf16(text, 6), 4);
    }

    #[test]
    fn range_round_trip() {
        let text = "a😀你b";
        let utf8 = 1..5;
        let utf16 = utf8_range_to_utf16(text, utf8.clone());
        // 'a' is 1 utf16 unit, emoji is 2 utf16, 你 is 1 utf16 unit.
        assert_eq!(utf16, 1..3);
        assert_eq!(utf16_range_to_utf8(text, utf16), utf8);
    }

    #[test]
    fn clamp_range_handles_invalid_offsets() {
        let text = "a😀b";
        // Both 2 and 4 are mid-emoji boundaries in "a😀b". clamp_range floors
        // both ends to the nearest valid char boundary.
        assert_eq!(clamp_range(text, &(0..2)), 0..1);
        assert_eq!(clamp_range(text, &(0..4)), 0..1);
        assert_eq!(clamp_range(text, &(0..999)), 0..text.len());
    }

    #[test]
    fn floor_and_ceil_char_boundary() {
        let text = "a😀b";
        assert_eq!(floor_char_boundary(text, 0), 0);
        assert_eq!(floor_char_boundary(text, 2), 1);
        assert_eq!(floor_char_boundary(text, 4), 1);
        assert_eq!(ceil_char_boundary(text, 2), 5);
        assert_eq!(ceil_char_boundary(text, 4), 5);
    }

    #[test]
    #[allow(clippy::reversed_empty_ranges)]
    fn clamp_range_handles_out_of_bounds_and_reversed() {
        let text = "a😀b";
        // Range entirely beyond text — both ends clamp to text.len().
        assert_eq!(clamp_range(text, &(100..200)), text.len()..text.len());
        // Range partially beyond text.
        assert_eq!(clamp_range(text, &(0..999)), 0..text.len());
        // Reversed range — clamp_range floors both ends independently.
        // 4 is mid-emoji, floors to 1. 2 is mid-emoji, floors to 1.
        let r = clamp_range(text, &(4..2));
        assert_eq!(r, 1..1);
    }

    #[test]
    fn utf16_range_to_utf8_with_out_of_bounds_offsets() {
        let text = "a😀b"; // 6 bytes, 4 UTF-16 code units
                           // Both offsets beyond text — clamps to text.len().
        assert_eq!(utf16_range_to_utf8(text, 10..20), text.len()..text.len());
        // Start within, end beyond.
        let r = utf16_range_to_utf8(text, 1..10);
        assert_eq!(r, 1..text.len());
    }
}
