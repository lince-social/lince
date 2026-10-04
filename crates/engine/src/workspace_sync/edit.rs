use super::*;
use protein::authority::Property;

pub(super) fn validate(edits: &[RecordEdit]) -> Result<(), EngineError> {
    if edits.is_empty() || edits.len() > 32 {
        return Err(invalid("Use 1–32 typed Record edits"));
    }
    for edit in edits {
        let valid = match edit {
            RecordEdit::Text { head, body } => {
                (head.is_some() || body.is_some())
                    && head.as_ref().is_none_or(|text| text.len() <= 65536)
                    && body.as_ref().is_none_or(|text| text.len() <= 131072)
            }
            RecordEdit::Slug { slug } => slug.as_ref().is_none_or(|slug| nucleus::valid_slug(slug)),
            RecordEdit::Unit { unit } => unit
                .as_ref()
                .is_none_or(|unit| nucleus::valid_uid(unit, "c")),
            RecordEdit::Extension { namespace, value } => {
                !namespace.is_empty()
                    && namespace.len() <= 128
                    && !namespace.starts_with("lince.")
                    && namespace != "work"
                    && value.as_object().is_some_and(|fields| fields.len() <= 128)
                    && serde_json::to_vec(value)?.len() <= 65536
            }
            RecordEdit::Assert {
                predicate,
                object,
                quantity,
                unit,
            } => {
                nucleus::valid_uid(predicate, "c")
                    && object
                        .as_ref()
                        .is_none_or(|object| nucleus::valid_uid(object, "r"))
                    && unit
                        .as_ref()
                        .is_none_or(|unit| nucleus::valid_uid(unit, "c"))
                    && quantity.as_ref().is_none_or(|quantity| {
                        nucleus::DecimalValue::parse_inferred(quantity).is_ok()
                    })
            }
            RecordEdit::Retract { assertion } => nucleus::valid_uid(assertion, "a"),
            RecordEdit::AssertionValue {
                assertion,
                quantity,
                unit,
            } => {
                nucleus::valid_uid(assertion, "a")
                    && unit
                        .as_ref()
                        .is_none_or(|unit| nucleus::valid_uid(unit, "c"))
                    && quantity.as_ref().is_none_or(|quantity| {
                        nucleus::DecimalValue::parse_inferred(quantity).is_ok()
                    })
            }
            RecordEdit::Refine { predicate, object } => {
                nucleus::valid_uid(predicate, "c") && nucleus::valid_uid(object, "r")
            }
            RecordEdit::Identity { predicate } => predicate
                .as_ref()
                .is_none_or(|predicate| nucleus::valid_uid(predicate, "c")),
        };
        if !valid {
            return Err(invalid(
                "Invalid Record edit. Reserved extensions require their dedicated editors.",
            ));
        }
    }
    Ok(())
}

pub(super) fn references(edits: &[RecordEdit]) -> BTreeSet<String> {
    edits
        .iter()
        .filter_map(|edit| match edit {
            RecordEdit::Assert { object, .. } => object.clone(),
            RecordEdit::Refine { object, .. } => Some(object.clone()),
            _ => None,
        })
        .collect()
}

async fn assertion_subject(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    assertion: &str,
) -> Result<(), EngineError> {
    let subject: Option<String> = store::sqlx::query_scalar(
        "SELECT subject_uid FROM record_assertion WHERE uid=? AND retracted_at IS NULL",
    )
    .bind(assertion)
    .fetch_optional(&mut **tx)
    .await?;
    if subject.as_deref() != Some(record) {
        return Err(invalid("Assertion unavailable on this Record"));
    }
    Ok(())
}

async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    predicate: &str,
    object: Option<&str>,
    quantity: Option<&str>,
    unit: Option<&str>,
    actor: Option<&str>,
) -> Result<(), EngineError> {
    store::assertions::insert_tx(
        tx,
        &nucleus::new_uid("a"),
        store::assertions::NewAssertion {
            subject_uid: record,
            predicate_uid: predicate,
            object_uid: object,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: quantity
                .map(nucleus::DecimalValue::parse_inferred)
                .transpose()
                .map_err(|error| invalid(error.to_string()))?,
            unit_uid: unit,
            asserted_by: actor,
        },
    )
    .await?;
    Ok(())
}

impl Engine {
    pub(super) async fn stage_workspace_edits(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        record: &str,
        edits: &[RecordEdit],
        actor: Option<&str>,
    ) -> Result<BTreeSet<Property>, EngineError> {
        validate(edits)?;
        let graph = crate::access::policy_graph_on(tx, &[])
            .await
            .map_err(crate::record_policy::denied)?;
        if !references(edits).is_subset(&self.readable_on(tx, actor, &graph).await?) {
            return Err(invalid("Assertion reference unavailable"));
        }
        let mut properties = BTreeSet::new();
        for edit in edits {
            match edit {
                RecordEdit::Text { head, body } => {
                    self.stage_record_text_on(tx, record, head.as_deref(), body.as_deref())
                        .await?;
                    if head.is_some() {
                        properties.insert(Property::Head);
                    }
                    if body.is_some() {
                        properties.insert(Property::Body);
                    }
                }
                RecordEdit::Slug { slug } => {
                    store::records::set_slug_on(tx, record, slug.as_deref()).await?;
                    properties.insert(Property::Slug);
                }
                RecordEdit::Unit { unit } => {
                    store::records::set_unit_on(tx, record, unit.as_deref()).await?;
                    properties.insert(Property::Unit);
                }
                RecordEdit::Extension { namespace, value } => {
                    store::records::set_extension_on(tx, record, namespace, value).await?;
                }
                RecordEdit::Assert {
                    predicate,
                    object,
                    quantity,
                    unit,
                } => {
                    if let Some(object) = object { super::effects::refuse_automation_assignment(tx, predicate, object).await?; }
                    insert(
                        tx,
                        record,
                        predicate,
                        object.as_deref(),
                        quantity.as_deref(),
                        unit.as_deref(),
                        actor,
                    )
                    .await?
                }
                RecordEdit::Retract { assertion } => {
                    assertion_subject(tx, record, assertion).await?;
                    store::assertions::retract_tx(tx, assertion, actor).await?;
                }
                RecordEdit::AssertionValue {
                    assertion,
                    quantity,
                    unit,
                } => {
                    assertion_subject(tx, record, assertion).await?;
                    store::assertions::set_quantity_tx(
                        tx,
                        assertion,
                        store::assertions::AssertionQuantity {
                            quantity: quantity
                                .as_ref()
                                .map(|quantity| nucleus::DecimalValue::parse_inferred(quantity))
                                .transpose()
                                .map_err(|error| invalid(error.to_string()))?,
                            unit_uid: unit.as_deref(),
                        },
                    )
                    .await?;
                }
                RecordEdit::Refine { predicate, object } => {
                    super::effects::refuse_automation_assignment(tx, predicate, object).await?;
                    let assertions: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid IS NULL AND role='ordinary' AND retracted_at IS NULL").bind(record).bind(predicate).fetch_all(&mut **tx).await?;
                    if assertions.is_empty() {
                        return Err(invalid(
                            "Refinement requires an existing ordinary unary assertion",
                        ));
                    }
                    for assertion in assertions {
                        store::assertions::retract_tx(tx, &assertion, actor).await?;
                    }
                    insert(tx, record, predicate, Some(object), None, None, actor).await?;
                }
                RecordEdit::Identity { predicate } => {
                    let existing: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid=? AND role='identity' AND retracted_at IS NULL").bind(record).fetch_all(&mut **tx).await?;
                    for assertion in existing {
                        store::assertions::retract_tx(tx, &assertion, actor).await?;
                    }
                    if let Some(predicate) = predicate {
                        let assertion: Option<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid IS NULL AND role='ordinary' AND retracted_at IS NULL ORDER BY uid LIMIT 1").bind(record).bind(predicate).fetch_optional(&mut **tx).await?;
                        if let Some(assertion) = assertion {
                            store::assertions::promote_identity_tx(tx, &assertion).await?;
                        } else {
                            store::assertions::insert_tx(
                                tx,
                                &nucleus::new_uid("a"),
                                store::assertions::NewAssertion {
                                    subject_uid: record,
                                    predicate_uid: predicate,
                                    object_uid: None,
                                    role: store::assertions::AssertionRole::Identity,
                                    quantity: None,
                                    unit_uid: None,
                                    asserted_by: actor,
                                },
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        Ok(properties)
    }
}
