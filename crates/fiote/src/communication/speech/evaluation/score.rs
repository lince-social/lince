//! Word normalization and edit-distance scoring for speech transcripts.

use serde::{Deserialize, Serialize};
use std::{iter::from_fn, mem::replace};

/// Aggregate word errors and exact matches across evaluated utterances.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Score {
    /// Number of normalized words in the reference transcripts.
    pub reference_words: usize,
    /// Insertions, deletions, and substitutions needed to match the references.
    pub edits: usize,
    /// Number of transcript pairs compared.
    pub utterances: usize,
    /// Number of transcript pairs with identical normalized words.
    pub exact_utterances: usize,
}

impl Score {
    /// Scores one reference and hypothesis after word normalization.
    pub fn compare(reference: &str, hypothesis: &str) -> Self {
        let reference = reference.to_lowercase();
        let hypothesis = hypothesis.to_lowercase();
        let actual: Vec<_> = normalized_words(&hypothesis).collect();
        let mut distances: Vec<_> = (1..=actual.len()).collect();
        let mut reference_words = 0;
        let mut edits = actual.len();
        for expected in normalized_words(&reference) {
            let mut diagonal = reference_words;
            reference_words += 1;
            edits = reference_words;
            for (above, &actual) in distances.iter_mut().zip(&actual) {
                // Keep the previous row's value for the next diagonal.
                edits = (diagonal + usize::from(expected != actual))
                    .min(*above + 1)
                    .min(edits + 1);
                diagonal = replace(above, edits);
            }
        }
        Self {
            reference_words,
            edits,
            utterances: 1,
            exact_utterances: usize::from(edits == 0),
        }
    }

    /// Accumulates another score.
    pub fn merge(&mut self, other: &Self) {
        self.reference_words += other.reference_words;
        self.edits += other.edits;
        self.utterances += other.utterances;
        self.exact_utterances += other.exact_utterances;
    }

    /// Returns the word error rate, using a denominator of one for empty references.
    pub fn wer(&self) -> f64 {
        self.edits as f64 / self.reference_words.max(1) as f64
    }
}

/// Lowercases text, splits punctuation and ASCII digit runs, and normalizes spoken numbers.
pub fn words(text: &str) -> Vec<String> {
    normalized_words(&text.to_lowercase())
        .map(str::to_owned)
        .collect()
}

fn normalized_words(text: &str) -> impl Iterator<Item = &str> {
    let mut chars = text.char_indices().peekable();
    from_fn(move || {
        let (start, first) = chars.find(|&(_, ch)| ch.is_alphanumeric())?;
        let digit = first.is_ascii_digit();
        while chars
            .next_if(|&(_, ch)| ch.is_alphanumeric() && ch.is_ascii_digit() == digit)
            .is_some()
        {}
        // Both bounds come from char_indices or the end of the string.
        let end = chars.peek().map_or(text.len(), |&(end, _)| end);
        Some(match &text[start..end] {
            "zero" => "0",
            "one" => "1",
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            "seven" => "7",
            "eight" => "8",
            "nine" => "9",
            "ten" => "10",
            "mister" => "mr",
            other => other,
        })
    })
}
