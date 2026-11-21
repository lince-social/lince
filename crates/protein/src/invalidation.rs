//! Chooses which Protein reads need refreshing after committed Facts.

use crate::{Include, Predicate, Protein, Source, is_ephemeral};
use nucleus::{CauseKind, Fact, fact::zero_delta};
use std::{cell::OnceCell, collections::HashSet, fmt, slice::from_ref};

/// A batch of persisted Facts evaluated against queries for one read scope.
pub struct FactChanges<'a> {
    facts: &'a [Fact],
    local: bool,
    text_records: OnceCell<TextRecords<'a>>,
}

enum TextRecords<'a> {
    All,
    One(&'a str),
    Many(HashSet<&'a str>),
}

impl fmt::Debug for FactChanges<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FactChanges")
            .field("count", &self.facts.len())
            .field("local", &self.local)
            .finish_non_exhaustive()
    }
}

impl<'a> FactChanges<'a> {
    /// Returns a batch for a local read or a read scoped to the given subject.
    pub fn new(facts: &'a [Fact], subject: Option<&str>) -> Self {
        Self {
            facts,
            local: subject.is_none(),
            text_records: OnceCell::new(),
        }
    }

    /// Reports whether the batch may affect the query's result.
    pub fn affects(&self, protein: &Protein) -> bool {
        if self.facts.is_empty() || is_ephemeral(protein) {
            return false;
        }
        if !self.local {
            return true;
        }
        let Some(uid) = direct_record_target(protein) else {
            return true;
        };
        match self.text_records.get_or_init(|| text_records(self.facts)) {
            TextRecords::All => true,
            TextRecords::One(record) => uid == *record,
            TextRecords::Many(records) => records.contains(uid),
        }
    }
}

/// Reports whether one committed Fact may affect the query's result.
pub fn affects(protein: &Protein, fact: &Fact, subject: Option<&str>) -> bool {
    FactChanges::new(from_ref(fact), subject).affects(protein)
}

fn canonical_text_record(fact: &Fact) -> Option<&str> {
    if fact.actor_uid.is_some()
        || fact.cause.kind != CauseKind::UserEdit
        || fact.cause.uid.is_some()
        || fact.delta != zero_delta()
        || fact.payload.as_deref() != Some("{\"collab\":true}")
        || fact.record_uid.is_empty()
    {
        return None;
    }
    Some(&fact.record_uid)
}

fn text_records(facts: &[Fact]) -> TextRecords<'_> {
    let Some((first, rest)) = facts.split_first() else {
        return TextRecords::All;
    };
    let Some(first) = canonical_text_record(first) else {
        return TextRecords::All;
    };
    let mut records = HashSet::new();
    for fact in rest {
        let Some(uid) = canonical_text_record(fact) else {
            return TextRecords::All;
        };
        if uid != first {
            records.insert(uid);
        }
    }
    if records.is_empty() {
        TextRecords::One(first)
    } else {
        records.insert(first);
        TextRecords::Many(records)
    }
}

fn direct_record_target(protein: &Protein) -> Option<&str> {
    if protein.source != Source::Record || protein.aggregate.is_some() {
        return None;
    }
    let [Predicate::UidEq(uid)] = protein.filter.as_slice() else {
        return None;
    };
    if uid.is_empty()
        || protein.fields.as_ref()?.iter().any(|field| {
            !matches!(
                field.as_str(),
                "uid"
                    | "slug"
                    | "kind"
                    | "head"
                    | "body"
                    | "quantity"
                    | "created_at"
                    | "updated_at"
                    | "created_hlc"
            )
        })
    {
        return None;
    }
    matches!(
            &protein.include,
            Include {
                relation_context: None,
                record_after: None,
                facts: None,
                promises: None,
                links: None,
                threads: None,
                availability: false,
                extension: None,
                shader_effect: false,
                numeric_fields,
                numeric_extensions: false,
                projection: None,
                contact: false,
                conversations: false,
                reference_reads: false,
            } if numeric_fields.is_empty()
    )
    .then_some(uid.as_str())
}
