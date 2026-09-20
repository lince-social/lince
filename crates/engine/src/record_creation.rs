use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, Utc};
use nucleus::{Cause, DecimalValue, NewFact, RecordKind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Engine, EngineError, actions::ActionOutcome};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Assertion {
    pub predicate: String,
    pub object: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub uid: String,
    pub head: String,
    pub body: String,
    pub slug: Option<String>,
    pub quantity: String,
    pub assertions: Vec<Assertion>,
    pub work: Value,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            uid: nucleus::new_uid("r"),
            head: String::new(),
            body: String::new(),
            slug: None,
            quantity: "0".into(),
            assertions: Vec::new(),
            work: json!({}),
        }
    }
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

impl Draft {
    fn matches(
        &self,
        predicate: &protein::Predicate,
        family: &HashSet<String>,
        quantity: DecimalValue,
    ) -> Option<bool> {
        use protein::Predicate;
        Some(match predicate {
            Predicate::All(children) => children
                .iter()
                .map(|child| self.matches(child, family, quantity))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .all(|value| value),
            Predicate::Any(children) => children
                .iter()
                .map(|child| self.matches(child, family, quantity))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .any(|value| value),
            Predicate::Not(child) => !self.matches(child, family, quantity)?,
            Predicate::ConceptIn(uid) => family.contains(uid),
            Predicate::KindEq(kind) => kind == "plain",
            Predicate::UidEq(uid) => uid == &self.uid,
            Predicate::SlugEq(slug) => self.slug.as_ref() == Some(slug),
            Predicate::TextContains(text) => {
                self.head.to_lowercase().contains(&text.to_lowercase())
                    || self.body.to_lowercase().contains(&text.to_lowercase())
            }
            Predicate::QuantityEq(value) => quantity.exact_numeric_cmp(*value).is_eq(),
            Predicate::QuantityLt(value) => quantity.exact_numeric_cmp(*value).is_lt(),
            Predicate::QuantityLte(value) => !quantity.exact_numeric_cmp(*value).is_gt(),
            Predicate::QuantityGt(value) => quantity.exact_numeric_cmp(*value).is_gt(),
            Predicate::QuantityGte(value) => !quantity.exact_numeric_cmp(*value).is_lt(),
            _ => return None,
        })
    }

    pub fn validate(&self) -> Result<DecimalValue, EngineError> {
        if !nucleus::valid_uid(&self.uid, "r")
            || self.head.len() > 65536
            || self.body.len() > 262144
            || self.assertions.len() > 40
            || self.quantity.len() > 128
            || self
                .slug
                .as_ref()
                .is_some_and(|slug| slug.is_empty() || slug.len() > 128 || slug.trim() != slug)
            || self.assertions.iter().any(|assertion| {
                [&assertion.predicate]
                    .into_iter()
                    .chain(assertion.object.iter())
                    .chain(assertion.unit.iter())
                    .chain(assertion.quantity.iter())
                    .any(|value| value.is_empty() || value.len() > 128)
            })
        {
            return Err(invalid(
                "Check the Record fields and use at most 40 assertions",
            ));
        }
        crate::private_work::WorkMetadata::parse(&self.work)
            .map_err(|error| invalid(error.to_string()))?;
        DecimalValue::parse_inferred(&self.quantity)
            .map_err(|_| invalid("Enter an exact decimal quantity"))
    }
}

impl Engine {
    pub(crate) async fn create_record_draft(
        &self,
        mut draft: Draft,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let quantity = draft.validate()?;
        let payload = serde_json::to_string(&draft).map_err(EngineError::Json)?;
        let mut family = HashSet::new();
        for assertion in &mut draft.assertions {
            assertion.predicate = store::concepts::resolve(&self.store.pool, &assertion.predicate)
                .await?
                .ok_or_else(|| invalid("Choose an existing assertion"))?;
            if assertion.object.is_none() {
                family.extend(
                    store::concepts::ancestors_including(&self.store.pool, &assertion.predicate)
                        .await?,
                );
            }
            if let Some(object) = &mut assertion.object {
                *object = self.resolve(object).await?;
                if !self.may_read_record(actor.as_deref(), object).await? {
                    return Err(EngineError::Forbidden(
                        "The related Record is not visible".into(),
                    ));
                }
            }
            if let Some(unit) = &mut assertion.unit {
                *unit = store::concepts::resolve(&self.store.pool, unit)
                    .await?
                    .ok_or_else(|| invalid("Choose an existing unit"))?;
            }
        }
        if let Some(actor) = actor.as_deref() {
            let matches = |predicate: &protein::Predicate| {
                draft.matches(predicate, &family, quantity) == Some(true)
            };
            if let Some(filter) =
                protein::read_rules::effective_predicate(&self.store, actor).await?
            {
                if !matches(&filter) {
                    return Err(EngineError::Forbidden(
                        "Choose assertions that satisfy your access rules".into(),
                    ));
                }
            }
            if let Some(role) = store::auth::person_access(&self.store.pool, actor)
                .await?
                .and_then(|person| person.role_id)
            {
                if let Some(policy) = store::role_policies::get(&self.store.pool, role)
                    .await?
                    .and_then(|row| row.policy)
                {
                    use protein::authority::{
                        AssertionProperty, AssertionRole, AssertionTarget, ExtensionProperty,
                        Operation, Property, RolePolicy,
                    };
                    let policy: RolePolicy =
                        serde_json::from_value(policy).map_err(EngineError::Json)?;
                    let grants: Vec<_> = policy
                        .grants
                        .iter()
                        .filter(|grant| {
                            grant.operation == Operation::Create && matches(&grant.selector)
                        })
                        .collect();
                    let allowed: BTreeSet<_> = grants
                        .iter()
                        .flat_map(|grant| grant.properties.iter().cloned())
                        .collect();
                    let mut required = BTreeSet::from([
                        Property::Kind,
                        Property::Head,
                        Property::Body,
                        Property::Quantity,
                    ]);
                    if draft.slug.is_some() {
                        required.insert(Property::Slug);
                    }
                    required.extend(draft.work.as_object().unwrap().keys().map(|field| {
                        Property::Extension(ExtensionProperty {
                            namespace: "work".into(),
                            field: field.clone(),
                        })
                    }));
                    if !required.is_subset(&allowed)
                        || draft.assertions.iter().any(|assertion| {
                            !grants
                                .iter()
                                .flat_map(|grant| &grant.assertions_add)
                                .any(|grant| {
                                    grant.predicate_uid == assertion.predicate
                                        && grant.role == AssertionRole::Ordinary
                                        && match (&grant.target, &assertion.object) {
                                            (AssertionTarget::Unary, None)
                                            | (AssertionTarget::AnyReadableRecord, Some(_)) => true,
                                            (AssertionTarget::Record(a), Some(b)) => a == b,
                                            _ => false,
                                        }
                                        && (assertion.quantity.is_none()
                                            || grant
                                                .properties
                                                .contains(&AssertionProperty::Quantity))
                                        && (assertion.unit.is_none()
                                            || grant.properties.contains(&AssertionProperty::Unit))
                                })
                        })
                    {
                        return Err(EngineError::Forbidden(
                            "Your role cannot create this Record with these properties".into(),
                        ));
                    }
                }
            }
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The local Organ is unavailable"))?;
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let previous: Option<(String, String)> = store::sqlx::query_as("SELECT payload, record_uid FROM record_change_receipt WHERE actor = ? AND change_uid = ?")
            .bind(actor.as_deref().unwrap_or("")).bind(&draft.uid).fetch_optional(&mut *tx).await?;
        if let Some((previous, uid)) = previous {
            if previous != payload {
                return Err(invalid(
                    "This creation was already saved with different fields",
                ));
            }
            return Ok(ActionOutcome {
                created: Some(uid),
                ..Default::default()
            });
        }
        store::records::create_with_uid_on(
            &mut tx,
            &draft.uid,
            store::records::NewRecord {
                slug: draft.slug.as_deref(),
                kind: RecordKind::Plain,
                head: &draft.head,
                body: &draft.body,
                quantity: store::exact::zero(),
            },
            &organ.uid,
            None,
        )
        .await?;
        for assertion in draft.assertions {
            store::assertions::insert_tx(
                &mut tx,
                &nucleus::new_uid("a"),
                store::assertions::NewAssertion {
                    subject_uid: &draft.uid,
                    predicate_uid: &assertion.predicate,
                    object_uid: assertion.object.as_deref(),
                    role: store::assertions::AssertionRole::Ordinary,
                    quantity: assertion
                        .quantity
                        .as_deref()
                        .map(DecimalValue::parse_inferred)
                        .transpose()
                        .map_err(|_| invalid("Enter an exact assertion quantity"))?,
                    unit_uid: assertion.unit.as_deref(),
                    asserted_by: actor.as_deref(),
                },
            )
            .await?;
        }
        store::records::set_extension_on(&mut tx, &draft.uid, "work", &draft.work).await?;
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            NewFact {
                actor_uid: actor.clone(),
                ..NewFact::quantity(draft.uid.clone(), quantity, Cause::user_edit())
            },
            now,
            signer.as_ref(),
        )
        .await?;
        store::sqlx::query("INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)")
            .bind(actor.as_deref().unwrap_or("")).bind(&draft.uid).bind(&draft.uid).bind(payload).bind(json!({"created":draft.uid}).to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        let facts = if let Some(fact) = fact {
            self.observe_committed_fact(fact, now).await?
        } else {
            vec![]
        };
        self.query_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        Ok(ActionOutcome {
            created: Some(draft.uid),
            facts,
            ..Default::default()
        })
    }
}
