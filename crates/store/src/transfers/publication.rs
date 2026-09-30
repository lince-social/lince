use super::*;

pub async fn publication_input(
    pool: &SqlitePool,
    transfer: &str,
    revision: u64,
    person: &str,
    request: &str,
) -> Result<WholeDraftRevisionInput, StoreError> {
    let mut tx = pool.begin().await?;
    let snapshot = revision_snapshot(&mut tx, transfer).await?;
    if snapshot.revision != revision {
        return Err(sqlx::Error::Protocol("transfer_revision_stale".into()));
    }
    let creator = snapshot
        .parties
        .iter()
        .find(|party| party.kind == "creator")
        .map(|party| party.person_uid.clone())
        .ok_or_else(|| sqlx::Error::Protocol("Transfer has no creator Person".into()))?;
    let terms = snapshot.transfer;
    let promises = snapshot
        .promises
        .into_iter()
        .filter(|promise| promise.state != "withdrawn")
        .map(|promise| DraftPromiseRevisionInput {
            uid: Some(promise.uid),
            item: promise.item,
            source_promise_uid: None,
            record_uid: promise.record_uid,
            concept_uid: promise.concept_uid,
            unit_uid: promise.unit_uid,
            person_uid: promise.person_uid,
            open: promise.state == "open",
            delta: promise.delta,
            window_start: promise.window_start,
            window_end: promise.window_end,
            location: promise.location,
            condition: promise.condition,
            reserve_from: promise.reserve_from,
            open_reuse_policy: promise.open_reuse_policy,
        })
        .collect();
    let dependencies = snapshot
        .dependencies
        .into_iter()
        .map(|dependency| TransferDependencyInput {
            uid: Some(dependency.uid),
            scope: dependency.scope,
            promise_uid: dependency.promise_uid,
            upstream_kind: dependency.upstream_kind,
            upstream_uid: dependency.upstream_uid,
            required_state: dependency.required_state,
        })
        .collect();
    let result = WholeDraftRevisionInput {
        transfer_uid: transfer.into(),
        expected_revision: revision,
        idempotency_key: request.into(),
        creator_person_uid: creator,
        proposal_author_person_uid: person.into(),
        terms: TransferDraftTermsInput {
            slug: terms.slug,
            head: terms.head,
            agreement_type: terms.agreement_type,
            agreement_pct: terms.agreement_pct,
            settlement: terms.settlement,
            visibility: "public".into(),
            max_proximity: None,
            satiation: terms.satiation,
            parent_uid: terms.parent_uid,
            source_uid: terms.source_uid,
            reserve_default: terms.reserve_default.unwrap_or_else(|| "none".into()),
            require_confirmation: terms.require_confirmation,
            default_place: terms.default_place,
        },
        retained_invitation_uids: snapshot
            .invitations
            .into_iter()
            .filter(|invitation| invitation.status == "pending")
            .map(|invitation| invitation.uid)
            .collect(),
        promises,
        dependencies,
        successor: None,
        authorization_intent_uid: None,
    };
    tx.rollback().await?;
    Ok(result)
}
