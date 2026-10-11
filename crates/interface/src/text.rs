use std::borrow::Cow;

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
