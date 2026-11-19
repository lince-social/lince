use nucleus::sync::{Outcome, Summary};
use serde::{Serialize, Serializer, ser::SerializeSeq};

use crate::StoreError;

const SUBJECTS: usize = 32;

fn prefix(text: &str, count: usize) -> &str {
    if text.len() <= count {
        return text;
    }
    text.char_indices()
        .nth(count)
        .map_or(text, |(end, _)| &text[..end])
}

struct Subjects<'a>(&'a [String]);

impl Serialize for Subjects<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len().min(SUBJECTS)))?;
        for subject in self.0.iter().take(SUBJECTS) {
            sequence.serialize_element(prefix(subject, 256))?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct SummaryView<'a> {
    outcome: Outcome,
    count: u64,
    subjects: Subjects<'a>,
    message: Option<&'a str>,
}

pub(super) fn summary(summary: &Summary) -> Result<String, StoreError> {
    let Summary {
        outcome,
        count,
        subjects,
        message,
    } = summary;
    let encoded = serde_json::to_string(&SummaryView {
        outcome: *outcome,
        count: *count,
        subjects: Subjects(subjects),
        message: message.as_deref().map(|message| prefix(message, 1024)),
    })
    .map_err(|error| StoreError::Protocol(error.to_string()))?;
    nucleus::performance::add(
        "sync-history-subjects-encoded",
        subjects.len().min(SUBJECTS) as u64,
    );
    Ok(encoded)
}
