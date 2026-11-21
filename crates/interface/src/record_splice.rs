//! Borrowed replacements for a single contiguous change to editable text.

pub(crate) struct TextSplice<'a> {
    pub(crate) start: usize,
    pub(crate) removed: usize,
    pub(crate) inserted: &'a str,
}

impl<'a> TextSplice<'a> {
    /// Computes one replacement using Unicode scalar offsets and a slice of `after`.
    pub(crate) fn between(before: &str, after: &'a str) -> Self {
        let (start, prefix_bytes) = before
            .chars()
            .zip(after.chars())
            .take_while(|(old, new)| old == new)
            .fold((0, 0), |(count, bytes), (character, _)| {
                (count + 1, bytes + character.len_utf8())
            });
        // Matched scalars give the same byte boundaries in both strings.
        let old = &before[prefix_bytes..];
        let new = &after[prefix_bytes..];
        let suffix_bytes: usize = old
            .chars()
            .rev()
            .zip(new.chars().rev())
            .take_while(|(old, new)| old == new)
            .map(|(character, _)| character.len_utf8())
            .sum();
        Self {
            start,
            removed: old[..old.len() - suffix_bytes].chars().count(),
            inserted: &new[..new.len() - suffix_bytes],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TextSplice;

    #[test]
    fn finds_the_replacement_in_unicode_scalars() {
        for (before, after, expected) in [
            ("", "😀", (0, 0, "😀")),
            ("é", "", (0, 1, "")),
            ("aé", "aé", (2, 0, "")),
            ("aéé", "aêé", (1, 1, "ê")),
            ("α👩‍💻Z", "α😀Z", (1, 3, "😀")),
            ("abcd", "abx", (2, 2, "x")),
            ("e\u{301}", "é", (0, 2, "é")),
            ("ab😀cd", "ab😀d", (3, 1, "")),
            ("banana", "bananana", (6, 0, "na")),
        ] {
            let splice = TextSplice::between(before, after);
            assert_eq!((splice.start, splice.removed, splice.inserted), expected);
        }
    }
}
