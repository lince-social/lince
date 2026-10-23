use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Score {
    pub reference_words: usize,
    pub edits: usize,
    pub utterances: usize,
    pub exact_utterances: usize,
}

impl Score {
    pub fn compare(reference: &str, hypothesis: &str) -> Self {
        let expected = words(reference);
        let actual = words(hypothesis);
        let mut previous: Vec<usize> = (0..=actual.len()).collect();
        for (row, expected) in expected.iter().enumerate() {
            let mut next = Vec::with_capacity(actual.len() + 1);
            next.push(row + 1);
            for (column, actual) in actual.iter().enumerate() {
                next.push(
                    (previous[column] + usize::from(expected != actual))
                        .min(previous[column + 1] + 1)
                        .min(next[column] + 1),
                );
            }
            previous = next;
        }
        Self {
            reference_words: expected.len(),
            edits: previous[actual.len()],
            utterances: 1,
            exact_utterances: usize::from(expected == actual),
        }
    }

    pub fn merge(&mut self, other: &Self) {
        self.reference_words += other.reference_words;
        self.edits += other.edits;
        self.utterances += other.utterances;
        self.exact_utterances += other.exact_utterances;
    }

    pub fn wer(&self) -> f64 {
        self.edits as f64 / self.reference_words.max(1) as f64
    }
}

pub fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut numeric = None;
    let finish = |word: &mut String, words: &mut Vec<String>| {
        if word.is_empty() {
            return;
        }
        let normalized = match word.as_str() {
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
        };
        words.push(normalized.to_owned());
        word.clear();
    };
    for character in text.to_lowercase().chars() {
        if !character.is_alphanumeric() {
            finish(&mut word, &mut words);
            numeric = None;
            continue;
        }
        let digit = character.is_ascii_digit();
        if numeric.is_some_and(|previous| previous != digit) {
            finish(&mut word, &mut words);
        }
        numeric = Some(digit);
        word.push(character);
    }
    finish(&mut word, &mut words);
    words
}
