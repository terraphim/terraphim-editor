//! Byte offsets to UTF-16 code-unit offsets, in one pass.
//!
//! Marks are computed on byte offsets (Rust's `str` indexing) and returned as
//! UTF-16 code-unit offsets (the unit of JavaScript strings, `selectionStart`
//! and the DOM `Range` API), so the editor can use them unchanged.

/// Convert every byte offset in `offsets` (each on a `char` boundary of
/// `text`) to a UTF-16 offset, in place. Runs in `O(n + k log k)` for a text of
/// `n` bytes and `k` offsets: the offsets are visited in sorted order while
/// the text is scanned once.
pub(crate) fn bytes_to_utf16(text: &str, offsets: &mut [&mut usize]) {
    offsets.sort_unstable_by_key(|o| **o);
    let mut units = 0usize;
    let mut chars = text.char_indices().peekable();
    for off in offsets.iter_mut() {
        let target = **off;
        while let Some(&(byte, ch)) = chars.peek() {
            if byte >= target {
                break;
            }
            units += ch.len_utf16();
            chars.next();
        }
        **off = units;
    }
}

/// Convert UTF-16 offsets into `text` to byte offsets, in one pass. An
/// offset past the end of `text`, or one that falls between the two halves
/// of a surrogate pair, maps to `None`. The result is parallel to `offsets`.
pub(crate) fn utf16_to_bytes(text: &str, offsets: &[usize]) -> Vec<Option<usize>> {
    let mut order: Vec<usize> = (0..offsets.len()).collect();
    order.sort_unstable_by_key(|&i| offsets[i]);
    let mut out = vec![None; offsets.len()];
    let mut units = 0usize;
    let mut chars = text.char_indices().peekable();
    for i in order {
        let target = offsets[i];
        while let Some(&(_, ch)) = chars.peek() {
            if units + ch.len_utf16() > target {
                break;
            }
            units += ch.len_utf16();
            chars.next();
        }
        if units == target {
            out[i] = Some(chars.peek().map_or(text.len(), |&(byte, _)| byte));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_back_to_bytes() {
        // "é" is 2 bytes / 1 unit, "𝄞" is 4 bytes / 2 units.
        let text = "é𝄞x";
        assert_eq!(
            utf16_to_bytes(text, &[4, 0, 1, 3, 2, 5]),
            vec![Some(7), Some(0), Some(2), Some(6), None, None]
        );
    }

    #[test]
    fn multibyte_and_surrogate_pairs() {
        // "é" is 2 bytes / 1 unit, "𝄞" is 4 bytes / 2 units.
        let text = "é𝄞x";
        let (mut a, mut b, mut c, mut d) = (6usize, 0usize, 2usize, 7usize);
        bytes_to_utf16(text, &mut [&mut a, &mut b, &mut c, &mut d]);
        assert_eq!((a, b, c, d), (3, 0, 1, 4));
    }
}
