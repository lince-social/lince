use nucleus::{performance::add, place::near, sync::PERSONAL_FILE_ORIGIN};
use store::{
    records::{RecordBody, RecordRow},
    sqlx::Error::Protocol,
};

use super::{Context, unsupported};
use crate::{DateComparison, Predicate, ProteinError, WorkDateField};

impl Context<'_> {
    pub(crate) fn matches_record<Body: RecordBody>(
        &self,
        r: &RecordRow<Body>,
    ) -> Result<bool, ProteinError> {
        let mut candidate = Candidate {
            row: r,
            head: None,
            body: None,
        };
        for p in self.predicates {
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
                let query = self
                    .text_queries
                    .get(query.as_str())
                    .ok_or_else(|| Protocol("Record text predicate was not prepared".into()))?;
                candidate.text_contains(query)?
            }
            Predicate::WorkDate { field, op, value } => {
                let key = match field {
                    WorkDateField::Start => "start",
                    WorkDateField::Due => "due",
                };
                let date = self
                    .work
                    .as_ref()
                    .and_then(|work| work.get(&r.uid))
                    .and_then(|work| work[key].as_str()?.get(..10));
                date_comparison(date, *op, value.as_deref())
            }
            Predicate::Near { of, meters } => self
                .anchors
                .get(of.as_str())
                .copied()
                .flatten()
                .zip(self.location(&r.uid))
                .is_some_and(|(anchor, here)| near(here, anchor, *meters)),
            Predicate::OrganEq(token) => r
                .organ_uid
                .as_deref()
                .is_some_and(|uid| self.organ(token) == Some(uid)),
            Predicate::OrganIn(tokens) => r
                .organ_uid
                .as_deref()
                .is_some_and(|uid| tokens.iter().any(|token| self.organ(token) == Some(uid))),
            Predicate::FileOriginEq(origin) => {
                r.file_origin.as_deref().unwrap_or(PERSONAL_FILE_ORIGIN) == origin
            }
            _ => return Err(unsupported()),
        })
    }

    fn organ(&self, token: &str) -> Option<&str> {
        self.organs.get(token)?.as_deref()
    }
}

fn date_comparison(date: Option<&str>, op: DateComparison, value: Option<&str>) -> bool {
    if op == DateComparison::Exists {
        return date.is_some();
    }
    date.zip(value).is_some_and(|(a, b)| match op {
        DateComparison::Eq => a == b,
        DateComparison::Lt => a < b,
        DateComparison::Lte => a <= b,
        DateComparison::Gt => a > b,
        DateComparison::Gte => a >= b,
        DateComparison::Exists => true,
    })
}

struct Candidate<'a, Body> {
    row: &'a RecordRow<Body>,
    head: Option<String>,
    body: Option<String>,
}

impl<Body: RecordBody> Candidate<'_, Body> {
    fn text_contains(&mut self, query: &str) -> Result<bool, ProteinError> {
        add("protein-record-text-predicates", 1);
        let head = self.head.get_or_insert_with(|| {
            add("protein-record-match-head-folds", 1);
            self.row.head.to_lowercase()
        });
        if head.contains(query) {
            return Ok(true);
        }
        let body = match &mut self.body {
            Some(body) => body,
            slot @ None => {
                let body = self.row.body.required()?;
                add("protein-record-match-body-folds", 1);
                slot.insert(body.to_lowercase())
            }
        };
        Ok(body.contains(query))
    }
}
