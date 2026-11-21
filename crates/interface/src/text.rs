//! Text slicing and selection utilities for editable and read-only UI.

use std::borrow::Cow;

/// Returns up to `char_limit` Unicode scalar values from the start of the text.
pub fn prefix(text: &str, char_limit: usize) -> &str {
    if text.len() <= char_limit {
        return text;
    }
    text.char_indices()
        .nth(char_limit)
        .map_or(text, |(byte, _)| &text[..byte])
}

fn map_positions(positions: [usize; 2], mut read: impl FnMut(usize) -> usize) -> [usize; 2] {
    let [anchor, focus] = positions;
    let first = read(anchor.min(focus));
    if anchor == focus {
        return [first, first];
    }
    let second = read(anchor.max(focus));
    if anchor < focus {
        [first, second]
    } else {
        [second, first]
    }
}

/// Converts anchor and focus byte offsets to scalar offsets, rounding up within a scalar.
/// Positions past the end clamp to the text length.
pub fn scalar_positions(mut chars: impl Iterator<Item = char>, bytes: [usize; 2]) -> [usize; 2] {
    let (mut byte, mut scalar) = (0, 0);
    map_positions(bytes, |target| {
        while byte < target {
            let Some(character) = chars.next() else {
                break;
            };
            byte += character.len_utf8();
            scalar += 1;
        }
        scalar
    })
}

/// Converts anchor and focus scalar offsets to byte offsets, clamping to the text length.
pub fn byte_positions(mut chars: impl Iterator<Item = char>, scalars: [usize; 2]) -> [usize; 2] {
    let (mut byte, mut scalar) = (0, 0);
    map_positions(scalars, |target| {
        while scalar < target {
            let Some(character) = chars.next() else {
                break;
            };
            byte += character.len_utf8();
            scalar += 1;
        }
        byte
    })
}

const MAX_PAGE_SCALARS: usize = 16_000;
const MAX_PAGE_BREAKS: usize = 64;

/// Returns a zero-based page and whether additional text follows.
/// Each page caps both the scalar count and the number of paragraph breaks.
pub fn reading_page(text: &str, page: usize, char_limit: usize) -> (&str, bool) {
    let char_limit = char_limit.clamp(1, MAX_PAGE_SCALARS);
    let mut remaining = text;
    for index in 0..=page {
        if remaining.is_empty() {
            break;
        }
        let mut end = 0;
        let mut paragraphs = 0;
        for (count, (byte, ch)) in remaining.char_indices().enumerate() {
            end = byte + ch.len_utf8();
            paragraphs += usize::from(matches!(ch, '\n' | '\u{2028}' | '\u{2029}'));
            if count + 1 == char_limit || paragraphs == MAX_PAGE_BREAKS {
                break;
            }
        }
        // The end comes from a complete scalar in `remaining`.
        let (part, next) = remaining.split_at(end);
        if index == page {
            return (part, !next.is_empty());
        }
        remaining = next;
    }
    ("", false)
}

/// Uses the measured width to fit text, adding an ellipsis when space permits.
pub fn truncate<'a>(text: &'a str, width: f32, measure: impl Fn(&str) -> f32) -> Cow<'a, str> {
    if !width.is_finite() || width <= 0.0 {
        return Cow::Borrowed("");
    }
    if measure(text) <= width {
        return Cow::Borrowed(text);
    }
    let ellipsis = "…";
    let remaining = width - measure(ellipsis);
    if remaining < 0.0 {
        return Cow::Borrowed("");
    }
    let boundaries: Vec<_> = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .collect();
    let mut low = 0;
    let mut high = boundaries.len();
    // Low is an index into the nonempty boundary list. High is exclusive.
    while low + 1 < high {
        let middle = low.midpoint(high);
        if measure(&text[..boundaries[middle]]) <= remaining {
            low = middle;
        } else {
            high = middle;
        }
    }
    Cow::Owned(format!("{}{ellipsis}", &text[..boundaries[low]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_pages_preserve_multilingual_paragraphs_and_unicode_boundaries() {
        let text = "العربية لغة.\nAn English paragraph.\n\n日本語の語彙。\u{2029}".repeat(100);
        let mut restored = String::new();
        for page in 0.. {
            let (part, more) = reading_page(&text, page, 13);
            assert!(part.chars().count() <= 13);
            restored.push_str(part);
            if !more {
                break;
            }
        }
        assert_eq!(restored, text);
        assert_eq!(reading_page(&text, usize::MAX, 16000), ("", false));
        assert_eq!(reading_page("", 0, 0), ("", false));
    }

    #[test]
    fn reading_pages_bound_paragraph_widgets_and_oversized_character_limits() {
        let text = "日本語\n".repeat(200);
        let (first, more) = reading_page(&text, 0, 16000);
        assert_eq!(first.lines().count(), 64);
        assert!(more);
        assert_eq!(reading_page(&text, 3, 16000).0.lines().count(), 8);
        assert_eq!(reading_page(&text, 4, 16000), ("", false));
        let long = "é".repeat(40000);
        assert_eq!(reading_page(&long, 0, usize::MAX).0.chars().count(), 16000);
        assert_eq!(reading_page(&long, 2, usize::MAX).0.chars().count(), 8000);
        assert_eq!(reading_page("é", 0, 0), ("é", false));
    }

    #[test]
    fn measured_truncation_keeps_fitting_text_and_unicode_boundaries() {
        let measure = |text: &str| text.chars().count() as f32;
        assert!(matches!(
            truncate("abc", 3.0, measure),
            Cow::Borrowed("abc")
        ));
        assert_eq!(truncate("é水🙂long", 4.0, measure), "é水🙂…");
        assert_eq!(truncate("long", 1.0, measure), "…");
        assert_eq!(truncate("long", 0.5, measure), "");
        assert_eq!(truncate("long", f32::NAN, measure), "");
        assert_eq!(truncate("", 10.0, measure), "");
    }
}
