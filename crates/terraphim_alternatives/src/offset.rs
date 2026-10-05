//! Conversions between UTF-16 code-unit offsets and Rust byte offsets.
//!
//! Every offset stored in an [`Anchor`](crate::Anchor) and every offset taken by
//! the public [`Document`](crate::Document) API is a **UTF-16 code-unit offset
//! into the document body** (the Markdown text with the annotation block
//! removed). This is the unit JavaScript strings, `selectionStart` and the DOM
//! `Range` API use, so the browser side can pass offsets through unchanged.
//!
//! Rust strings are indexed by byte, so the crate converts at the boundary with
//! the helpers below. An offset that falls inside a character (for example
//! between the two halves of a surrogate pair) is rejected with `None` rather
//! than rounded.

/// Converts a UTF-16 code-unit offset into a byte offset within `text`.
///
/// Returns `None` when `utf16` is beyond the end of `text` or falls in the
/// middle of a character (between the two code units of a surrogate pair).
///
/// ```
/// use terraphim_alternatives::utf16_to_byte;
/// // "é" is 2 bytes / 1 unit; "𝄞" is 4 bytes / 2 units.
/// assert_eq!(utf16_to_byte("é𝄞x", 1), Some(2));
/// assert_eq!(utf16_to_byte("é𝄞x", 3), Some(6));
/// assert_eq!(utf16_to_byte("é𝄞x", 2), None);
/// ```
pub fn utf16_to_byte(text: &str, utf16: usize) -> Option<usize> {
    let mut units = 0usize;
    for (byte, ch) in text.char_indices() {
        if units == utf16 {
            return Some(byte);
        }
        if units > utf16 {
            return None;
        }
        units += ch.len_utf16();
    }
    (units == utf16).then_some(text.len())
}

/// Converts a byte offset within `text` into a UTF-16 code-unit offset.
///
/// Returns `None` when `byte` is beyond the end of `text` or not on a
/// character boundary.
///
/// ```
/// use terraphim_alternatives::byte_to_utf16;
/// assert_eq!(byte_to_utf16("é𝄞x", 6), Some(3));
/// assert_eq!(byte_to_utf16("é𝄞x", 1), None);
/// ```
pub fn byte_to_utf16(text: &str, byte: usize) -> Option<usize> {
    if byte > text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    Some(utf16_len(&text[..byte]))
}

/// Converts many UTF-16 offsets, sorted ascending, to byte offsets in one walk
/// of `text`. Each result is `None` exactly when [`utf16_to_byte`] would
/// return `None` for that offset.
pub(crate) fn utf16_to_bytes_sorted(text: &str, offsets: &[usize]) -> Vec<Option<usize>> {
    debug_assert!(offsets.is_sorted(), "offsets must be sorted");
    let mut out = Vec::with_capacity(offsets.len());
    let mut chars = text.char_indices();
    let (mut byte, mut units) = (0usize, 0usize);
    let mut next = chars.next();
    for &wanted in offsets {
        while units < wanted {
            match next {
                Some((_, ch)) => {
                    units += ch.len_utf16();
                    next = chars.next();
                    byte = next.map_or(text.len(), |(index, _)| index);
                }
                None => break,
            }
        }
        out.push((units == wanted).then_some(byte));
    }
    out
}

/// Converts the UTF-16 offset `target` to a byte offset by walking from a
/// known point: byte `from_byte`, a character boundary at UTF-16 offset
/// `from_units`. The cost is proportional to the distance walked, not to the
/// length of `text`. `None` exactly when [`utf16_to_byte`] would be `None`.
pub(crate) fn utf16_to_byte_from(
    text: &str,
    (from_units, from_byte): (usize, usize),
    target: usize,
) -> Option<usize> {
    let mut units = from_units;
    if target >= from_units {
        for (index, ch) in text[from_byte..].char_indices() {
            if units >= target {
                return (units == target).then_some(from_byte + index);
            }
            units += ch.len_utf16();
        }
        (units == target).then_some(text.len())
    } else {
        for (index, ch) in text[..from_byte].char_indices().rev() {
            units -= ch.len_utf16();
            if units <= target {
                return (units == target).then_some(index);
            }
        }
        None
    }
}

/// Length of `text` in UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "aé中𝄞b"; // 1 + 2 + 3 + 4 + 1 bytes; 1 + 1 + 1 + 2 + 1 units

    #[test]
    fn ascii_offsets_are_identical() {
        for i in 0..=5 {
            assert_eq!(utf16_to_byte("hello", i), Some(i));
            assert_eq!(byte_to_utf16("hello", i), Some(i));
        }
    }

    #[test]
    fn multi_byte_and_astral_characters_convert_both_ways() {
        let pairs = [(0, 0), (1, 1), (2, 3), (3, 6), (5, 10), (6, 11)];
        for (units, bytes) in pairs {
            assert_eq!(utf16_to_byte(MIXED, units), Some(bytes), "units {units}");
            assert_eq!(byte_to_utf16(MIXED, bytes), Some(units), "bytes {bytes}");
        }
        assert_eq!(utf16_len(MIXED), 6);
    }

    #[test]
    fn offsets_inside_a_character_are_rejected() {
        assert_eq!(utf16_to_byte(MIXED, 4), None, "mid surrogate pair");
        assert_eq!(byte_to_utf16(MIXED, 2), None, "mid two-byte char");
        assert_eq!(utf16_to_byte("𝄞", 1), None, "mid pair at end of text");
    }

    #[test]
    fn sorted_conversion_agrees_with_single_conversion() {
        let offsets: Vec<usize> = vec![0, 0, 1, 2, 3, 4, 5, 6, 7, 9];
        let expected: Vec<Option<usize>> =
            offsets.iter().map(|&o| utf16_to_byte(MIXED, o)).collect();
        assert_eq!(utf16_to_bytes_sorted(MIXED, &offsets), expected);
        assert_eq!(utf16_to_bytes_sorted("", &[0, 1]), vec![Some(0), None]);
        assert!(utf16_to_bytes_sorted("abc", &[]).is_empty());
    }

    #[test]
    fn conversion_from_a_known_point_agrees_with_single_conversion() {
        // Every character boundary of MIXED as a starting point, every offset
        // (including mid-pair and past the end) as a target.
        let points = [(0, 0), (1, 1), (2, 3), (3, 6), (5, 10), (6, 11)];
        for point in points {
            for target in 0..=8 {
                assert_eq!(
                    utf16_to_byte_from(MIXED, point, target),
                    utf16_to_byte(MIXED, target),
                    "from {point:?} to {target}"
                );
            }
        }
    }

    #[test]
    fn offsets_past_the_end_are_rejected() {
        assert_eq!(utf16_to_byte(MIXED, 7), None);
        assert_eq!(byte_to_utf16(MIXED, 12), None);
    }
}
