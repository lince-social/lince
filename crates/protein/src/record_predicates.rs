use super::{DateComparison, LinkDirection, Predicate, ProteinError, Store, WorkDateField};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use store::records::RecordBody;

pub(super) struct Context<'a> {
    person: Option<String>,
    concept_families: HashMap<&'a str, HashSet<String>>,
    record_concepts: Option<HashMap<String, Vec<String>>>,
    anchors: HashMap<&'a str, Option<nucleus::place::Place>>,
    locations: HashMap<String, nucleus::vocabulary::SavedLocation>,
    relations: HashMap<(&'a str, LinkDirection, Option<&'a str>), HashSet<String>>,
    branches: HashMap<(&'a str, &'a str, bool), HashSet<String>>,
    work: Option<HashMap<String, Value>>,
    organs: HashMap<&'a str, Option<String>>,
    text_queries: HashMap<&'a str, String>,
}

impl<'a> Context<'a> {
    pub(super) async fn prepare(
        store: &Store,
        preds: &'a [Predicate],
        visible: Option<&HashSet<String>>,
        subject: Option<&str>,
    ) -> Result<Self, ProteinError> {
        let _preparation =
            nucleus::performance::Span::new("protein-record-predicate-prepare-await");
        let mut leaves = Vec::new();
        let mut pending: Vec<_> = preds.iter().rev().collect();
        while let Some(predicate) = pending.pop() {
            match predicate {
                Predicate::All(children) | Predicate::Any(children) => {
                    pending.extend(children.iter().rev())
                }
                Predicate::Not(child) => pending.push(child),
                _ => leaves.push(predicate),
            }
        }
        let mut ctx = Context {
            person: subject.map(str::to_owned),
            concept_families: HashMap::new(),
            record_concepts: None,
            anchors: HashMap::new(),
            locations: if leaves
                .iter()
                .any(|predicate| matches!(predicate, Predicate::Near { .. }))
            {
                store::places::saved_locations_for_person(&store.pool, visible, subject).await?
            } else {
                HashMap::new()
            },
            relations: HashMap::new(),
            branches: HashMap::new(),
            work: None,
            organs: HashMap::new(),
            text_queries: HashMap::new(),
        };
        for p in leaves {
            ctx.prepare_one(store, p).await?;
        }
        Ok(ctx)
    }

    async fn prepare_one(&mut self, store: &Store, p: &'a Predicate) -> Result<(), ProteinError> {
        match p {
            Predicate::ConceptIn(name) => {
                if self.concept_families.contains_key(name.as_str()) {
                    return Ok(());
                }
                if self.record_concepts.is_none() {
                    self.record_concepts = Some(
                        store::ledger::all_record_concepts(&store.pool)
                            .await?
                            .into_iter()
                            .collect(),
                    );
                }
                let family =
                    match store::vocabulary::resolve(&store.pool, name, self.person.as_deref())
                        .await?
                    {
                        Some(uid) => store::concepts::descendants_including(&store.pool, &uid)
                            .await?
                            .into_iter()
                            .collect(),
                        None => HashSet::new(),
                    };
                self.concept_families.insert(name, family);
            }
            Predicate::Near { of, .. } => {
                if self.anchors.contains_key(of.as_str()) {
                    return Ok(());
                }
                let place = match reference_uid(store, of).await? {
                    Some(uid) => self
                        .locations
                        .get(&uid)
                        .and_then(|location| location.selected.as_ref())
                        .map(|candidate| nucleus::place::Place {
                            lat: candidate.latitude,
                            lon: candidate.longitude,
                        }),
                    None => None,
                };
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
                let family =
                    store::vocabulary::meaning_family(&store.pool, kind, self.person.as_deref())
                        .await?;
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
                let predicates =
                    store::vocabulary::meaning_family(&store.pool, kind, self.person.as_deref())
                        .await?;
                if predicates.is_empty() {
                    self.relations.insert(key, HashSet::new());
                    return Ok(());
                };
                let other_uid = match other {
                    Some(token) => match reference_uid(store, token).await? {
                        Some(uid) => Some(uid),
                        None => {
                            self.relations.insert(key, HashSet::new());
                            return Ok(());
                        }
                    },
                    None => None,
                };
                let mut matches = HashSet::new();
                for link in
                    store::assertions::binary_of_predicates(&store.pool, &predicates).await?
                {
                    let outgoing = matches!(*direction, LinkDirection::Out | LinkDirection::Both)
                        && other_uid.as_ref().is_none_or(|uid| uid == &link.object_uid);
                    let incoming = matches!(*direction, LinkDirection::In | LinkDirection::Both)
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
                self.relations.insert(key, matches);
            }
            Predicate::WorkDate { .. } => {
                if self.work.is_none() {
                    self.work = Some(store::records::all_extensions(&store.pool, "work").await?);
                }
            }
            Predicate::OrganEq(token) => {
                if self.organs.contains_key(token.as_str()) {
                    return Ok(());
                }
                let resolved = reference_uid(store, token).await?;
                self.organs.insert(token, resolved);
            }
            Predicate::OrganIn(tokens) => {
                for token in tokens {
                    if self.organs.contains_key(token.as_str()) {
                        continue;
                    }
                    let resolved = reference_uid(store, token).await?;
                    self.organs.insert(token, resolved);
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

    pub(super) fn matches_record<Body: RecordBody>(
        &self,
        r: &store::records::RecordRow<Body>,
        preds: &[Predicate],
    ) -> Result<bool, ProteinError> {
        let mut candidate = Candidate {
            row: r,
            head: None,
            body: None,
        };
        for p in preds {
            if !self.matches_one(&mut candidate, p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn matches_one<Body: RecordBody>(
        &self,
        candidate: &mut Candidate<'_, Body>,
        p: &Predicate,
    ) -> Result<bool, ProteinError> {
        let r = candidate.row;
        Ok(match p {
            Predicate::ProjectionWindow(_) | Predicate::RollingWindow(_) => false,
            Predicate::All(ps) => {
                for p in ps {
                    if !self.matches_one(candidate, p)? {
                        return Ok(false);
                    }
                }
                true
            }
            Predicate::Any(ps) => {
                for p in ps {
                    if self.matches_one(candidate, p)? {
                        return Ok(true);
                    }
                }
                false
            }
            Predicate::Not(p) => !self.matches_one(candidate, p)?,
            Predicate::QuantityLt(n) => r.quantity.exact_numeric_cmp(*n).is_lt(),
            Predicate::QuantityLte(n) => !r.quantity.exact_numeric_cmp(*n).is_gt(),
            Predicate::QuantityGt(n) => r.quantity.exact_numeric_cmp(*n).is_gt(),
            Predicate::QuantityGte(n) => !r.quantity.exact_numeric_cmp(*n).is_lt(),
            Predicate::QuantityEq(n) => r.quantity.exact_numeric_cmp(*n).is_eq(),
            Predicate::UidEq(uid) => r.uid == *uid,
            Predicate::KindEq(k) => r.kind == *k,
            Predicate::SlugEq(s) => r.slug.as_deref() == Some(s.as_str()),
            Predicate::ConceptIn(name) => {
                self.concept_families
                    .get(name.as_str())
                    .is_some_and(|family| {
                        self.record_concepts
                            .as_ref()
                            .and_then(|records| records.get(&r.uid))
                            .is_some_and(|concepts| concepts.iter().any(|c| family.contains(c)))
                    })
            }
            Predicate::Under {
                record,
                kind,
                include_self,
            } => self
                .branches
                .get(&(record.as_str(), kind.as_str(), *include_self))
                .is_some_and(|members| members.contains(&r.uid)),
            Predicate::Relation {
                kind,
                direction,
                other,
            } => self
                .relations
                .get(&(kind.as_str(), *direction, other.as_deref()))
                .is_some_and(|records| records.contains(&r.uid)),
            Predicate::TextContains(query) => {
                let query = &self.text_queries[query.as_str()];
                candidate.text_contains(query)?
            }
            Predicate::WorkDate { field, op, value } => {
                let date = self
                    .work
                    .as_ref()
                    .and_then(|work| work.get(&r.uid))
                    .and_then(|work| {
                        let key = match field {
                            WorkDateField::Start => "start",
                            WorkDateField::Due => "due",
                        };
                        work.get(key)
                            .and_then(Value::as_str)
                            .filter(|date| !date.is_empty())
                            .and_then(|date| date.get(..10))
                    });
                match op {
                    DateComparison::Exists => date.is_some(),
                    DateComparison::Eq => date.zip(value.as_deref()).is_some_and(|(a, b)| a == b),
                    DateComparison::Lt => date.zip(value.as_deref()).is_some_and(|(a, b)| a < b),
                    DateComparison::Lte => date.zip(value.as_deref()).is_some_and(|(a, b)| a <= b),
                    DateComparison::Gt => date.zip(value.as_deref()).is_some_and(|(a, b)| a > b),
                    DateComparison::Gte => date.zip(value.as_deref()).is_some_and(|(a, b)| a >= b),
                }
            }
            Predicate::StateIn(_) => true,
            Predicate::RevisionEq(_)
            | Predicate::RevisionLt(_)
            | Predicate::RevisionLte(_)
            | Predicate::RevisionGt(_)
            | Predicate::RevisionGte(_)
            | Predicate::StatusIn(_)
            | Predicate::ViewerRoleIn(_)
            | Predicate::InvitationStateIn(_)
            | Predicate::PersonEq(_)
            | Predicate::UnitEq(_)
            | Predicate::WindowEndBefore(_)
            | Predicate::WindowEndAfter(_)
            | Predicate::OccurrenceIn(_) => true,
            Predicate::AtSince(_) | Predicate::CauseKindEq(_) | Predicate::RecordEq(_) => true,
            Predicate::AtBefore(_) | Predicate::ClassifiedIn(_) => true,
            Predicate::Near { of, meters } => {
                match (
                    self.anchors.get(of.as_str()).and_then(|a| *a),
                    self.locations
                        .get(&r.uid)
                        .and_then(|location| location.selected.as_ref())
                        .map(|candidate| nucleus::place::Place {
                            lat: candidate.latitude,
                            lon: candidate.longitude,
                        }),
                ) {
                    (Some(anchor), Some(here)) => nucleus::place::near(here, anchor, *meters),
                    _ => false,
                }
            }
            Predicate::OrganEq(token) => {
                match (
                    self.organs.get(token.as_str()).and_then(|o| o.as_deref()),
                    &r.organ_uid,
                ) {
                    (Some(resolved), Some(actual)) => resolved == actual,
                    _ => false,
                }
            }
            Predicate::OrganIn(tokens) => match &r.organ_uid {
                Some(actual) => tokens.iter().any(|token| {
                    self.organs.get(token.as_str()).and_then(|o| o.as_deref())
                        == Some(actual.as_str())
                }),
                None => false,
            },
        })
    }
}

async fn reference_uid(store: &Store, token: &str) -> Result<Option<String>, ProteinError> {
    nucleus::performance::add("protein-record-reference-lookups", 1);
    store::records::resolve_uid(&store.pool, token).await
}

struct Candidate<'a, Body> {
    row: &'a store::records::RecordRow<Body>,
    head: Option<String>,
    body: Option<String>,
}

impl<Body: RecordBody> Candidate<'_, Body> {
    fn text_contains(&mut self, query: &str) -> Result<bool, ProteinError> {
        nucleus::performance::add("protein-record-text-predicates", 1);
        let head = self.head.get_or_insert_with(|| {
            nucleus::performance::add("protein-record-match-head-folds", 1);
            self.row.head.to_lowercase()
        });
        if head.contains(query) {
            return Ok(true);
        }
        if self.body.is_none() {
            let body = self.row.body.required()?;
            nucleus::performance::add("protein-record-match-body-folds", 1);
            self.body = Some(body.to_lowercase());
        }
        Ok(self.body.as_ref().unwrap().contains(query))
    }
}
