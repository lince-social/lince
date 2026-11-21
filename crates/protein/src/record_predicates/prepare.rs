use std::collections::HashSet;

use nucleus::performance::{Span, add};
use store::{
    Store, assertions::binary_of_predicates, records::resolve_uid, vocabulary::meaning_family,
};

use super::{Context, RelationKey};
use crate::{LinkDirection, Predicate, ProteinError};

impl<'a> Context<'a> {
    pub(crate) async fn prepare(
        store: &Store,
        predicates: &'a [Predicate],
        visible: Option<&HashSet<String>>,
        subject: Option<&str>,
    ) -> Result<Self, ProteinError> {
        let _preparation = Span::new("protein-record-predicate-prepare-await");
        let mut leaves = Vec::new();
        let mut pending: Vec<_> = predicates.iter().rev().collect();
        while let Some(predicate) = pending.pop() {
            match predicate {
                Predicate::All(children) | Predicate::Any(children) => {
                    pending.extend(children.iter().rev())
                }
                Predicate::Not(child) => pending.push(child),
                _ => leaves.push(predicate),
            }
        }
        let mut ctx = Self {
            predicates,
            ..Self::default()
        };
        if leaves
            .iter()
            .any(|predicate| matches!(predicate, Predicate::Near { .. }))
        {
            ctx.locations =
                store::places::saved_locations_for_person(&store.pool, visible, subject).await?;
        }
        for predicate in leaves {
            ctx.prepare_one(store, predicate, subject).await?;
        }
        Ok(ctx)
    }

    async fn prepare_one(
        &mut self,
        store: &Store,
        predicate: &'a Predicate,
        subject: Option<&str>,
    ) -> Result<(), ProteinError> {
        match predicate {
            Predicate::ConceptIn(name) if !self.concept_families.contains_key(name.as_str()) => {
                if self.record_concepts.is_none() {
                    self.record_concepts = Some(
                        store::ledger::all_record_concepts(&store.pool)
                            .await?
                            .into_iter()
                            .collect(),
                    );
                }
                let family = match store::vocabulary::resolve(&store.pool, name, subject).await? {
                    Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                        .await?
                        .into_iter()
                        .collect(),
                    None => HashSet::new(),
                };
                self.concept_families.insert(name, family);
            }
            Predicate::Near { of, .. } if !self.anchors.contains_key(of.as_str()) => {
                let place = reference_uid(store, of)
                    .await?
                    .and_then(|uid| self.location(&uid));
                self.anchors.insert(of, place);
            }
            Predicate::Under {
                record,
                kind,
                include_self,
            } => {
                let key = (record.as_str(), kind.as_str(), *include_self);
                if self.branches.contains_key(&key) {
                    return Ok(());
                }
                let root = reference_uid(store, record).await?;
                let family = meaning_family(&store.pool, kind, subject).await?;
                let members = match root {
                    Some(root) => store::assertions::record_descendants(
                        &store.pool,
                        &root,
                        &family,
                        *include_self,
                    )
                    .await?
                    .into_iter()
                    .collect(),
                    _ => HashSet::new(),
                };
                self.branches.insert(key, members);
            }
            Predicate::Relation {
                kind,
                direction,
                other,
            } => {
                let key = (kind.as_str(), *direction, other.as_deref());
                if self.relations.contains_key(&key) {
                    return Ok(());
                }
                let matches = relation_members(store, key, subject).await?;
                self.relations.insert(key, matches);
            }
            Predicate::WorkDate { .. } if self.work.is_none() => {
                self.work = Some(store::records::all_extensions(&store.pool, "work").await?);
            }
            Predicate::OrganEq(token) => self.prepare_organ(store, token).await?,
            Predicate::OrganIn(tokens) => {
                for token in tokens {
                    self.prepare_organ(store, token).await?;
                }
            }
            Predicate::TextContains(query) => {
                self.text_queries
                    .entry(query)
                    .or_insert_with(|| query.to_lowercase());
            }
            _ => {}
        }
        Ok(())
    }

    async fn prepare_organ(&mut self, store: &Store, token: &'a str) -> Result<(), ProteinError> {
        if !self.organs.contains_key(token) {
            self.organs
                .insert(token, reference_uid(store, token).await?);
        }
        Ok(())
    }
}

async fn relation_members(
    store: &Store,
    (kind, direction, other): RelationKey<'_>,
    subject: Option<&str>,
) -> Result<HashSet<String>, ProteinError> {
    let predicates = meaning_family(&store.pool, kind, subject).await?;
    if predicates.is_empty() {
        return Ok(HashSet::new());
    }
    let other_uid = if let Some(other) = other {
        let Some(uid) = reference_uid(store, other).await? else {
            return Ok(HashSet::new());
        };
        Some(uid)
    } else {
        None
    };
    let mut matches = HashSet::new();
    for link in binary_of_predicates(&store.pool, &predicates).await? {
        let outgoing = matches!(direction, LinkDirection::Out | LinkDirection::Both)
            && other_uid.as_ref().is_none_or(|uid| uid == &link.object_uid);
        let incoming = matches!(direction, LinkDirection::In | LinkDirection::Both)
            && other_uid
                .as_ref()
                .is_none_or(|uid| uid == &link.subject_uid);
        if outgoing {
            matches.insert(link.subject_uid);
        }
        if incoming {
            matches.insert(link.object_uid);
        }
    }
    Ok(matches)
}

async fn reference_uid(store: &Store, token: &str) -> Result<Option<String>, ProteinError> {
    add("protein-record-reference-lookups", 1);
    resolve_uid(&store.pool, token).await
}
