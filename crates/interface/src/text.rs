use std::borrow::Cow;

pub fn reading_page(text: &str, page: usize, char_limit: usize) -> (&str, bool) {
    let char_limit = char_limit.clamp(1, 16000);
    let mut offset = 0;
    for index in 0..=page {
        let tail = &text[offset..];
        if tail.is_empty() {
            return ("", false);
        }
        let mut end = offset;
        let mut paragraphs = 0;
        for (count, (byte, ch)) in tail.char_indices().enumerate() {
            end = offset + byte + ch.len_utf8();
            paragraphs += usize::from(matches!(ch, '\n' | '\u{2028}' | '\u{2029}'));
            if count + 1 == char_limit || paragraphs == 64 {
                break;
            }
        }
        if index == page {
            return (&text[offset..end], end < text.len());
        }
        offset = end;
    }
    unreachable!()
}

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
    while low + 1 < high {
        let middle = (low + high) / 2;
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
