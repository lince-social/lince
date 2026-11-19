use super::{Include, Predicate, Protein, Source, is_ephemeral};
use nucleus::Fact;
use std::{cell::OnceCell, collections::HashSet};

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

impl<'a> FactChanges<'a> {
    pub fn new(facts: &'a [Fact], subject: Option<&str>) -> Self {
        Self {
            facts,
            local: subject.is_none(),
            text_records: OnceCell::new(),
        }
    }

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

pub fn affects(protein: &Protein, fact: &Fact, subject: Option<&str>) -> bool {
    FactChanges::new(std::slice::from_ref(fact), subject).affects(protein)
}

fn canonical_text_record(fact: &Fact) -> Option<&str> {
    if fact.actor_uid.is_some()
        || fact.cause.kind != nucleus::CauseKind::UserEdit
        || fact.cause.uid.is_some()
        || fact.delta != nucleus::fact::zero_delta()
        || fact.payload.as_deref() != Some("{\"collab\":true}")
        || fact.record_uid.is_empty()
    {
        return None;
    }
    Some(&fact.record_uid)
}

fn text_records(facts: &[Fact]) -> TextRecords<'_> {
    let Some(first) = facts.first().and_then(canonical_text_record) else {
        return TextRecords::All;
    };
    let mut records = HashSet::new();
    for fact in &facts[1..] {
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
        || protein.fields.as_ref().is_none_or(|fields| {
            fields.iter().any(|field| {
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
        })
        || !matches!(
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
    {
        return None;
    }
    Some(uid)
}
