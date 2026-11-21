//! Settlement blockers, the authorized application formula, and a quote for the remaining amount.

use super::Occurrences;
use crate::{
    ProteinError,
    transfers::{
        application::effect_amounts,
        status::{EffectiveSettlementApplication, effective_settlement_application},
    },
};
use nucleus::{DecimalValue, PromiseState, transfer::application::amount};
use serde_json::{Value, json};
use store::{
    Store,
    misc::PromiseRow,
    records::RecordRow,
    sqlx::Error::Protocol,
    transfer_accounting::{Binding, applied, bound_record},
    transfer_effects,
    transfers::{
        OccurrenceSettlementProgress, TransferOccurrenceRow, effective_occurrence_remainder_policy,
    },
};

pub(super) struct Source<'a> {
    pub origin_promise: Option<&'a PromiseRow>,
    pub origin_record: Option<&'a RecordRow<Option<String>>>,
    pub source_owner: Option<&'a str>,
    pub viewer_is_source_owner: bool,
    pub viewer_owns_origin: bool,
}

pub(super) struct Settlement {
    pub blockers: Vec<&'static str>,
    pub application: Option<PrivateApplication>,
    pub preview: Option<Value>,
}

pub(super) struct PrivateApplication {
    effective: EffectiveSettlementApplication,
    prior_local: DecimalValue,
}

impl PrivateApplication {
    pub(super) fn into_row(self) -> Value {
        json!({
            "formula": self.effective.formula,
            "formula_hash": self.effective.formula_hash,
            "version": self.effective.version,
            "source": self.effective.source,
            "applied_local_delta": self.prior_local,
            "error": self.effective.error,
        })
    }
}

pub(super) async fn project(
    store: &Store,
    input: &Occurrences<'_>,
    occurrence: &TransferOccurrenceRow,
    progress: &OccurrenceSettlementProgress,
    mut settlement_blockers: Vec<&'static str>,
    source: &Source<'_>,
) -> Result<Settlement, ProteinError> {
    let &Source {
        origin_promise,
        origin_record,
        source_owner,
        viewer_is_source_owner,
        viewer_owns_origin,
    } = source;
    let concrete_record_matches = origin_promise
        .and_then(|promise| promise.record_uid.as_deref())
        .is_some_and(|record_uid| {
            occurrence.record_uid.as_deref() == Some(record_uid) && origin_record.is_some()
        });
    if !concrete_record_matches {
        settlement_blockers.push("concrete_source_record_required");
    }
    if origin_record.is_some_and(|record| {
        record
            .organ_uid
            .as_deref()
            .is_some_and(|organ| Some(organ) != input.local_organ_uid)
    }) {
        settlement_blockers.push("source_record_not_local");
    }
    if !origin_promise.is_some_and(|promise| promise.state == PromiseState::Active) {
        settlement_blockers.push("occurrence_not_active");
    }
    let source_direction_matches = origin_promise.is_some_and(|promise| {
        promise.delta.is_finite()
            && ((promise.delta < 0.0 && source_owner == Some(occurrence.giver_person_uid.as_str()))
                || (promise.delta > 0.0
                    && source_owner == Some(occurrence.receiver_person_uid.as_str())))
    });
    if !source_direction_matches {
        settlement_blockers.push("occurrence_source_direction_invalid");
    }
    if !occurrence.delivery_claimed || !occurrence.receipt_claimed {
        settlement_blockers.push("occurrence_confirmation_required");
    }
    if occurrence.disputed {
        settlement_blockers.push("occurrence_disputed");
    }
    if progress.remaining_quantity <= 0.0 {
        settlement_blockers.push("occurrence_already_settled");
    }

    let application = if viewer_owns_origin
        && let (Some(promise), Some(owner)) = (origin_promise, source_owner)
    {
        let effective = effective_settlement_application(store, occurrence, promise, owner).await?;
        let prior_local = applied(&store.pool, &occurrence.uid, owner).await?.local;
        Some(PrivateApplication {
            effective,
            prior_local,
        })
    } else {
        None
    };

    let mut preview = None;
    if viewer_is_source_owner && let Some(owner) = source_owner {
        match &application {
            Some(application) if application.effective.error.is_some() => {
                settlement_blockers.push("application_formula_invalid");
            }
            Some(application) if progress.remaining_quantity > 0.0 => {
                preview = quote(
                    store,
                    input.uid,
                    occurrence,
                    progress,
                    application,
                    &Binding {
                        transfer: &occurrence.transfer_uid,
                        exchange: occurrence
                            .exchange_uid
                            .as_deref()
                            .unwrap_or(&occurrence.promise_uid),
                        occurrence: Some(&occurrence.uid),
                        person: owner,
                        record: None,
                        unit: occurrence.unit_uid.as_deref(),
                        outgoing: origin_promise.is_some_and(|promise| promise.delta < 0.0),
                    },
                )
                .await?;
                if preview.is_none() {
                    settlement_blockers.push("application_formula_invalid");
                }
            }
            None => settlement_blockers.push("application_formula_unavailable"),
            Some(_) => {}
        }
    }
    Ok(Settlement {
        blockers: settlement_blockers,
        application,
        preview,
    })
}

async fn quote(
    store: &Store,
    uid: &str,
    occurrence: &TransferOccurrenceRow,
    progress: &OccurrenceSettlementProgress,
    application: &PrivateApplication,
    binding: &Binding<'_>,
) -> Result<Option<Value>, ProteinError> {
    let quantity = amount(progress.settled_quantity + progress.remaining_quantity)
        .map_err(|error| Protocol(error.to_string()))?;
    let Ok(group) = transfer_effects::quote(&store.pool, binding, quantity).await else {
        return Ok(None);
    };
    let Ok((delta, after)) = effect_amounts(
        group.as_ref(),
        &application.effective.formula,
        quantity,
        application.prior_local,
    ) else {
        return Ok(None);
    };
    let local_delta = delta.to_f64();
    if !local_delta.is_finite() {
        return Ok(None);
    }
    let remainder_policy =
        effective_occurrence_remainder_policy(&store.pool, &occurrence.uid, binding.person).await?;
    let local_record = bound_record(
        &store.pool,
        binding.transfer,
        binding.exchange,
        binding.person,
        occurrence.record_uid.as_deref(),
    )
    .await?;
    Ok(Some(json!({
        "transfer": uid,
        "occurrence": occurrence.uid,
        "promise": occurrence.promise_uid,
        "person": binding.person,
        "local_record": local_record,
        "canonical_quantity": progress.remaining_quantity,
        "remaining_quantity": progress.remaining_quantity,
        "settled_quantity": progress.settled_quantity,
        "local_delta": local_delta,
        "local_cumulative_after": after.to_f64(),
        "effects": group.as_ref().map(|group| &group.effects),
        "expected_effects_hash": group.as_ref().map(|group| &group.hash),
        "application_formula_hash": application.effective.formula_hash,
        "application_formula_version": application.effective.version,
        "remainder_policy": remainder_policy.as_str(),
    })))
}
