// non-lean-yet
// Scope: struct Expansion.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   33.
// - [Clippy probe: clippy::as_conversions] Use From or TryFrom when it preserves the conversion
//   contract. Sites 43.

use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Expansion<'a> {
    records: &'a BTreeMap<String, BTreeSet<String>>,
    by_reader: BTreeMap<&'a str, Vec<&'a str>>,
    additions: BTreeMap<&'a str, BTreeSet<&'a str>>,
}

impl<'a> Expansion<'a> {
    pub(super) fn new(records: &'a BTreeMap<String, BTreeSet<String>>) -> Self {
        let _prepare = crate::performance::Span::new("record-value-dependency-graph");
        let mut by_reader: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut edges = 0;
        for (record, readers) in records {
            for reader in readers {
                by_reader.entry(reader).or_default().push(record);
                edges += 1;
            }
        }
        crate::performance::add("record-value-dependency-edges", edges);
        Self {
            records,
            by_reader,
            additions: BTreeMap::new(),
        }
    }

    pub(super) fn inherit(&mut self, reader: &'a str, dependency: &str) {
        let Some(records) = self.by_reader.get(dependency) else {
            return;
        };
        for &record in records {
            if !self.records[record].contains(reader) {
                self.additions.entry(record).or_default().insert(reader);
            }
        }
    }

    pub(super) fn into_additions(self) -> Vec<(String, BTreeSet<String>)> {
        let Self { additions, .. } = self;
        crate::performance::add(
            "record-value-dependency-additions",
            additions.values().map(|readers| readers.len() as u64).sum(),
        );
        additions
            .into_iter()
            .map(|(record, readers)| {
                (
                    record.to_owned(),
                    readers.into_iter().map(str::to_owned).collect(),
                )
            })
            .collect()
    }
}
