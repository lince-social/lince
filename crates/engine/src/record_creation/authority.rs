// non-lean-yet

use super::{Assertion, Draft, invalid};
use crate::{Engine, EngineError};
use lince_resources::reference::is_uri;
use nucleus::{DecimalValue, RecordKind, performance::Span, sync::PERSONAL_FILE_ORIGIN};
use protein::{
    Predicate,
    authority::{
        AssertionGrant, AssertionProperty, AssertionRole, AssertionTarget, Operation, Property,
        RolePolicy,
    },
    read_rules::effective_predicate,
    role_authority::policy_for,
};
use std::collections::{BTreeSet, HashSet};

impl Draft {
    pub(crate) fn matches(
        &self,
        predicate: &Predicate,
        family: &HashSet<String>,
        quantity: DecimalValue,
    ) -> Option<bool> {
        self.matches_kind(predicate, family, quantity, RecordKind::Plain)
    }

    fn matches_kind(
        &self,
        predicate: &Predicate,
        family: &HashSet<String>,
        quantity: DecimalValue,
        kind: RecordKind,
    ) -> Option<bool> {
        let _evaluation = Span::new("record-draft-selector-evaluation");
        self.evaluate(predicate, family, quantity, kind)
    }

    fn evaluate(
        &self,
        predicate: &Predicate,
        family: &HashSet<String>,
        quantity: DecimalValue,
        kind: RecordKind,
    ) -> Option<bool> {
        use Predicate::*;
        Some(match predicate {
            All(children) => children.iter().try_fold(true, |matched, child| {
                self.evaluate(child, family, quantity, kind)
                    .map(|value| matched && value)
            })?,
            Any(children) => children.iter().try_fold(false, |matched, child| {
                self.evaluate(child, family, quantity, kind)
                    .map(|value| matched || value)
            })?,
            Not(child) => !self.evaluate(child, family, quantity, kind)?,
            ConceptIn(uid) => family.contains(uid),
            KindEq(value) => value == kind.as_str(),
            UidEq(uid) => uid == &self.uid,
            SlugEq(slug) => self.slug.as_ref() == Some(slug),
            FileOriginEq(origin) => {
                self.file_origin.as_deref().unwrap_or(PERSONAL_FILE_ORIGIN) == origin
            }
            TextContains(text) => {
                text.is_empty() || {
                    let query = text.to_lowercase();
                    self.head.to_lowercase().contains(&query)
                        || self.body.to_lowercase().contains(&query)
                }
            }
            QuantityEq(value) => quantity.exact_numeric_cmp(*value).is_eq(),
            QuantityLt(value) => quantity.exact_numeric_cmp(*value).is_lt(),
            QuantityLte(value) => !quantity.exact_numeric_cmp(*value).is_gt(),
            QuantityGt(value) => quantity.exact_numeric_cmp(*value).is_gt(),
            QuantityGte(value) => !quantity.exact_numeric_cmp(*value).is_lt(),
            _ => return None,
        })
    }

    fn role_allows(
        &self,
        policy: &RolePolicy,
        matches: impl Fn(&Predicate) -> bool,
    ) -> Result<bool, EngineError> {
        let grants: Vec<_> = policy
            .grants
            .iter()
            .filter(|grant| grant.operation == Operation::Create && matches(&grant.selector))
            .collect();
        let _properties = Span::new("record-draft-role-properties");
        let allowed: BTreeSet<_> = grants.iter().flat_map(|grant| &grant.properties).collect();
        let work = self
            .work
            .as_object()
            .ok_or_else(|| invalid("Record work must be an object"))?;
        // The set counts each permitted work key once across matching grants.
        let work_fields = allowed.iter().filter(|property| {
            matches!(property, Property::Extension(field) if field.namespace == "work" && work.contains_key(&field.field))
        });
        Ok([
            Property::Kind,
            Property::Head,
            Property::Body,
            Property::Quantity,
        ]
        .iter()
        .all(|property| allowed.contains(property))
            && (self.slug.is_none() || allowed.contains(&Property::Slug))
            && (work.is_empty() || work_fields.count() == work.len())
            && self.assertions.iter().all(|assertion| {
                grants
                    .iter()
                    .flat_map(|grant| &grant.assertions_add)
                    .any(|grant| covers(grant, assertion))
            }))
    }
}

fn covers(grant: &AssertionGrant, assertion: &Assertion) -> bool {
    grant.predicate_uid == assertion.predicate
        && grant.role == AssertionRole::Ordinary
        && match (&grant.target, assertion.object.as_deref()) {
            (AssertionTarget::Unary, None) => true,
            (AssertionTarget::Record(expected) | AssertionTarget::Uri(expected), Some(actual)) => {
                expected == actual
            }
            (AssertionTarget::AnyReadableRecord, Some(actual)) => !is_uri(actual),
            _ => false,
        }
        && (assertion.quantity.is_none() || grant.properties.contains(&AssertionProperty::Quantity))
        && (assertion.unit.is_none() || grant.properties.contains(&AssertionProperty::Unit))
}

impl Engine {
    pub(super) async fn authorize_record_draft(
        &self,
        draft: &Draft,
        kind: RecordKind,
        actor: &str,
        quantity: DecimalValue,
        family: &HashSet<String>,
    ) -> Result<(), EngineError> {
        let matches = |predicate: &Predicate| {
            draft.matches_kind(predicate, family, quantity, kind) == Some(true)
        };
        if let Some(filter) = effective_predicate(&self.store, actor).await?
            && !matches(&filter)
        {
            return Err(EngineError::Forbidden(
                "Choose assertions that satisfy your access rules".into(),
            ));
        }
        if let Some(policy) = policy_for(&self.store, actor, Some(Operation::Create)).await?
            && !draft.role_allows(&policy, matches)?
        {
            return Err(EngineError::Forbidden(
                "Your role cannot create this Record with these properties".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use protein::authority::{ExtensionProperty, MutationGrant};
    use serde_json::json;

    #[test]
    fn matching_grants_cover_each_work_field_once() {
        let draft = Draft {
            work: json!({"due": "2026-11-21", "estimate_min": 30}),
            ..Default::default()
        };
        let extension = |namespace: &str, field: &str| {
            Property::Extension(ExtensionProperty {
                namespace: namespace.into(),
                field: field.into(),
            })
        };
        let matching = MutationGrant {
            operation: Operation::Create,
            selector: Predicate::KindEq("plain".into()),
            properties: BTreeSet::from([
                Property::Kind,
                Property::Head,
                Property::Body,
                Property::Quantity,
                extension("work", "due"),
                extension("other", "estimate_min"),
            ]),
            assertions_add: Vec::new(),
            assertions_remove: Vec::new(),
        };
        let mut unrelated = matching.clone();
        unrelated.selector = Predicate::KindEq("organ".into());
        unrelated
            .properties
            .insert(extension("work", "estimate_min"));
        let mut policy = RolePolicy {
            read: matching.selector.clone(),
            grants: vec![matching.clone(), matching, unrelated],
        };
        let family = HashSet::new();
        let quantity = draft.validate().unwrap();
        let matches =
            |predicate: &Predicate| draft.matches(predicate, &family, quantity) == Some(true);

        assert!(!draft.role_allows(&policy, matches).unwrap());
        policy.grants[0]
            .properties
            .insert(extension("work", "estimate_min"));
        assert!(draft.role_allows(&policy, matches).unwrap());
    }
}
