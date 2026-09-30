use icu_segmenter::WordSegmenter;
use ropey::Rope;

pub fn boundary(rope: &Rope, position: usize, backwards: bool) -> usize {
    let position = position.min(rope.len_chars());
    let start = position.saturating_sub(4096);
    let end = (position + 4096).min(rope.len_chars());
    let text = rope.slice(start..end).to_string();
    let byte = rope.char_to_byte(position) - rope.char_to_byte(start);
    let segmenter = WordSegmenter::new_dictionary(Default::default());
    let mut previous = 0;
    let mut candidate = 0;
    for next in segmenter.segment_str(&text) {
        if next == previous {
            continue;
        }
        let whitespace = text[previous..next].chars().all(char::is_whitespace);
        if backwards {
            if previous >= byte {
                break;
            }
            if !whitespace {
                candidate = previous;
            }
        } else if next > byte && !whitespace {
            candidate = next;
            break;
        } else {
            candidate = next;
        }
        previous = next;
    }
    start + text[..candidate].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_words_and_unicode_offsets_are_kept_together() {
        let rope = Rope::from_str("こんにちは世界 猫");
        assert_eq!(boundary(&rope, 0, false), 5);
        assert_eq!(boundary(&rope, 5, false), 7);
        assert_eq!(boundary(&rope, 7, true), 5);
        assert_eq!(boundary(&rope, 5, true), 0);
        assert_eq!(boundary(&rope, 8, true), 5);
        assert_eq!(boundary(&rope, 7, false), 9);
    }

    #[test]
    fn word_navigation_skips_space_and_stays_bounded_on_long_lines() {
        let rope = Rope::from_str("hello_world  next");
        assert_eq!(boundary(&rope, 0, false), 11);
        assert_eq!(boundary(&rope, 13, true), 0);
        assert_eq!(boundary(&rope, 11, false), 17);
        let long = Rope::from_str(&"x".repeat(100_000));
        assert_eq!(boundary(&long, 50_000, false), 54_096);
        assert_eq!(boundary(&long, 50_000, true), 45_904);
    }
}
