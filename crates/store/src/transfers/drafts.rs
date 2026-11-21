use std::collections::HashSet;

use chrono::{DateTime, Utc};
use nucleus::{Fact, PromiseState, RecordKind, transfer::OpenPromiseReusePolicy};
use sqlx::{Row, SqlitePool};

use super::{
    NewTransferCorrectionLink, TransferDependencyInput, TransferLocationSnapshot,
    correction_link_for_request,
    dependencies::{replace_dependencies, validate_dependency_inputs, validate_transfer_parent},
    ensure_request_not_used_by_invitation_event,
    invitations::events::{NewInvitationEvent, insert_invitation_event},
    item_terms::{encode_item, validate_location},
    open_claim_pair_for_request, promise_successor_for_request,
    revisions::{
        RevisionAuthorization, RevisionWrite, append_revision, fact_has_transfer_revision_action,
        snapshot::validated_snapshot,
    },
};
use crate::{StoreError, records};

/// The proposed terms and owner of a promise in a new Transfer.
#[derive(Debug, Clone)]
pub struct DraftPromise {
    pub uid: Option<String>,
    pub item: Option<nucleus::transfer::disclosure::TransferItem>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub open: bool,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub condition: Option<String>,
    pub reserve_from: String,
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

/// Initial Transfer terms and the people, promises, and dependencies they introduce.
#[derive(Debug, Clone)]
pub struct NewTransferDraft {
    pub public_history: Option<nucleus::transfer::public_history::Terms>,
    pub idempotency_key: String,
    pub slug: Option<String>,
    pub head: String,
    pub agreement_type: String,
    pub agreement_pct: Option<i64>,
    pub satiation: Option<String>,
    pub parent_uid: Option<String>,
    pub source_uid: Option<String>,
    pub visibility: String,
    pub max_proximity: Option<i64>,
    pub reserve_default: String,
    pub require_confirmation: bool,
    pub default_place: Option<TransferLocationSnapshot>,
    pub creator_person: String,
    pub invitees: Vec<String>,
    pub promises: Vec<DraftPromise>,
    pub dependencies: Vec<TransferDependencyInput>,
    pub organ_uid: Option<String>,
    pub evidence_action: String,
    pub correction: Option<NewTransferCorrectionLink>,
    pub authorization_intent_uid: Option<String>,
}

/// The stored draft and evidence for its first revision and invitations.
#[derive(Debug, Clone)]
pub struct CreatedTransferDraft {
    pub transfer_uid: String,
    pub revision: u64,
    pub replayed: bool,
    pub party_uids: Vec<String>,
    pub invitation_uids: Vec<String>,
    pub promise_uids: Vec<String>,
    pub fact: Fact,
    pub invitation_event_facts: Vec<Fact>,
    pub parent_facts: Vec<Fact>,
}

/// Creates the Transfer, first revision, and invitations in one transaction.
///
/// # Errors
///
/// Returns an error for invalid terms, unavailable dependencies, a conflicting
/// request, missing authorization, or a storage failure.
pub async fn create_draft<F>(
    pool: &SqlitePool,
    draft: NewTransferDraft,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    visibility_actor: Option<String>,
    sign: F,
) -> Result<CreatedTransferDraft, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_key = draft.idempotency_key.trim();
    if request_key.is_empty() || request_key.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer draft request key must contain 1 to 200 characters".into(),
        ));
    }
    ensure_request_not_used_by_invitation_event(pool, request_key).await?;
    let now_string = now.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    if let Some(row) = sqlx::query(
        "SELECT transfer_uid, revision, fact_uid FROM transfer_revision
         WHERE idempotency_key = ?",
    )
    .bind(request_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let transfer_uid: String = row.get("transfer_uid");
        let revision = row.get::<i64, _>("revision") as u64;
        let fact_uid: String = row.get("fact_uid");
        let party_uids = sqlx::query_scalar(
            "SELECT uid FROM transfer_party WHERE transfer_uid = ? ORDER BY uid",
        )
        .bind(&transfer_uid)
        .fetch_all(&mut *tx)
        .await?;
        let invitation_uids = sqlx::query_scalar(
            "SELECT uid FROM transfer_invitation WHERE transfer_uid = ? ORDER BY uid",
        )
        .bind(&transfer_uid)
        .fetch_all(&mut *tx)
        .await?;
        let promise_uids =
            sqlx::query_scalar("SELECT uid FROM promise WHERE transfer_uid = ? ORDER BY uid")
                .bind(&transfer_uid)
                .fetch_all(&mut *tx)
                .await?;
        tx.rollback().await?;
        let correction = correction_link_for_request(pool, request_key).await?;
        let successor = promise_successor_for_request(pool, request_key).await?;
        let open_claim = open_claim_pair_for_request(pool, request_key).await?;
        match (&draft.correction, correction.as_ref()) {
            (Some(requested), Some(existing))
                if successor.is_none()
                    && open_claim.is_none()
                    && existing.kind == requested.kind
                    && existing.source_transfer_uid == requested.source_transfer_uid
                    && existing.source_occurrence_uid == requested.source_occurrence_uid
                    && existing.source_revision == requested.source_revision
                    && (existing.canonical_quantity - requested.canonical_quantity).abs()
                        < 1e-9
                    && existing.created_transfer_uid == transfer_uid
                    && existing.actor_person_uid == draft.creator_person => {}
            (None, None) if successor.is_none() && open_claim.is_none() => {}
            _ => {
                return Err(sqlx::Error::Protocol(
                    "transfer request id was replayed as a different action".into(),
                ));
            }
        }
        let fact = crate::facts::get(pool, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if !fact_has_transfer_revision_action(&fact, &draft.evidence_action) {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different revision action".into(),
            ));
        }
        let event_fact_uids: Vec<String> = sqlx::query_scalar(
            "SELECT fact_uid FROM transfer_invitation_event
             WHERE transfer_uid = ? AND kind = 'addressed' AND revision = ?
             ORDER BY created_at, uid",
        )
        .bind(&transfer_uid)
        .bind(revision as i64)
        .fetch_all(pool)
        .await?;
        let mut invitation_event_facts = Vec::with_capacity(event_fact_uids.len());
        for uid in event_fact_uids {
            invitation_event_facts.push(
                crate::facts::get(pool, &uid)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound)?,
            );
        }
        return Ok(CreatedTransferDraft {
            transfer_uid,
            revision,
            replayed: true,
            party_uids,
            invitation_uids,
            promise_uids,
            fact,
            invitation_event_facts,
            parent_facts: Vec::new(),
        });
    }
    if let Some(slug) = draft.slug.as_deref()
        && !nucleus::valid_slug(slug)
    {
        return Err(sqlx::Error::Protocol(format!("invalid slug `{slug}`")));
    }
    validate_location(draft.default_place.as_ref())?;
    if let Some(terms) = &draft.public_history {
        terms.validate().map_err(sqlx::Error::Protocol)?;
    }
    let supplied_promise_uids = draft
        .promises
        .iter()
        .filter_map(|promise| promise.uid.as_ref())
        .collect::<HashSet<_>>();
    if supplied_promise_uids.len() != draft.promises.iter().filter(|p| p.uid.is_some()).count() {
        return Err(sqlx::Error::Protocol(
            "transfer draft repeats a promise uid".into(),
        ));
    }
    for promise in &draft.promises {
        if promise.record_uid.is_none() && promise.concept_uid.is_none() && promise.item.is_none() {
            return Err(sqlx::Error::Protocol(
                "transfer draft promise requires a Record or concept".into(),
            ));
        }
        if !promise.delta.is_finite() || promise.delta == 0.0 {
            return Err(sqlx::Error::Protocol(
                "transfer draft promise delta must be finite and non-zero".into(),
            ));
        }
        validate_location(promise.location.as_ref())?;
    }
    validate_dependency_inputs(&draft.dependencies)?;
    let transfer_uid = nucleus::new_uid("r");
    validate_transfer_parent(&mut tx, &transfer_uid, draft.parent_uid.as_deref()).await?;
    sqlx::query(
        "INSERT INTO record
            (uid, slug, kind, head, body, quantity_mantissa, quantity_scale, organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', '0', 0, ?, ?, ?)",
    )
    .bind(&transfer_uid)
    .bind(&draft.slug)
    .bind(RecordKind::Transfer.as_str())
    .bind(&draft.head)
    .bind(&draft.organ_uid)
    .bind(&now_string)
    .bind(&now_string)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO transfer
            (record_uid, agreement_type, agreement_pct, visibility, max_proximity,
             satiation, parent_uid, source_uid, reserve_default, require_confirmation,
             default_location_lat, default_location_lon, default_location_address, revision)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)",
    )
    .bind(&transfer_uid)
    .bind(&draft.agreement_type)
    .bind(draft.agreement_pct)
    .bind(&draft.visibility)
    .bind(draft.max_proximity)
    .bind(&draft.satiation)
    .bind(&draft.parent_uid)
    .bind(&draft.source_uid)
    .bind(&draft.reserve_default)
    .bind(draft.require_confirmation as i64)
    .bind(draft.default_place.as_ref().and_then(|value| value.lat))
    .bind(draft.default_place.as_ref().and_then(|value| value.lon))
    .bind(
        draft
            .default_place
            .as_ref()
            .and_then(|value| value.address.as_deref()),
    )
    .execute(&mut *tx)
    .await?;

    records::set_transfer_public_history_on(&mut tx, &transfer_uid, draft.public_history.as_ref())
        .await?;
    let party_uid = nucleus::new_uid("y");
    sqlx::query(
        "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
         VALUES (?, ?, ?, 'creator')",
    )
    .bind(&party_uid)
    .bind(&transfer_uid)
    .bind(&draft.creator_person)
    .execute(&mut *tx)
    .await?;
    let party_uids = vec![party_uid];

    let mut invitation_uids = Vec::with_capacity(draft.invitees.len());
    for addressed_person_uid in &draft.invitees {
        let invitation_uid = nucleus::new_uid("ti");
        sqlx::query(
            "INSERT INTO transfer_invitation
                (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
                 status, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'pending', ?, ?)",
        )
        .bind(&invitation_uid)
        .bind(&transfer_uid)
        .bind(addressed_person_uid)
        .bind(&draft.creator_person)
        .bind(&now_string)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
        invitation_uids.push(invitation_uid);
    }

    let mut promise_uids = Vec::with_capacity(draft.promises.len());
    for promise in &draft.promises {
        if promise.person_uid.is_none() {
            return Err(sqlx::Error::Protocol(
                "every Transfer promise requires an owning Person".into(),
            ));
        }
        if promise.open && promise.person_uid.as_deref() != Some(draft.creator_person.as_str()) {
            return Err(sqlx::Error::Protocol(
                "a newly created OPEN promise must belong to the Transfer creator".into(),
            ));
        }
        let promise_uid = promise.uid.clone().unwrap_or_else(|| nucleus::new_uid("p"));
        sqlx::query(
            "INSERT INTO promise
                (uid, record_uid, concept_uid, unit_uid, delta, window_start, window_end,
                 party_uid, state, condition, transfer_uid, reserve_from, revision,
                 location_lat, location_lon, location_address, open_reuse_policy,
                 created_at, updated_at, item_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&promise_uid)
        .bind(&promise.record_uid)
        .bind(&promise.concept_uid)
        .bind(&promise.unit_uid)
        .bind(promise.delta)
        .bind(&promise.window_start)
        .bind(&promise.window_end)
        .bind(&promise.person_uid)
        .bind(if promise.open {
            PromiseState::Open.as_str()
        } else {
            PromiseState::Proposed.as_str()
        })
        .bind(&promise.condition)
        .bind(&transfer_uid)
        .bind(&promise.reserve_from)
        .bind(promise.location.as_ref().and_then(|value| value.lat))
        .bind(promise.location.as_ref().and_then(|value| value.lon))
        .bind(
            promise
                .location
                .as_ref()
                .and_then(|value| value.address.as_deref()),
        )
        .bind(promise.open_reuse_policy.as_str())
        .bind(&now_string)
        .bind(&now_string)
        .bind(encode_item(promise.item.as_ref())?)
        .execute(&mut *tx)
        .await?;
        promise_uids.push(promise_uid);
    }

    replace_dependencies(&mut tx, &transfer_uid, 1, &draft.dependencies).await?;

    if let Some(subject) = visibility_actor.as_deref() {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'actor', ?, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(subject)
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }
    if draft.visibility == "public" {
        sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'public', NULL, ?, 'visible')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(&transfer_uid)
        .execute(&mut *tx)
        .await?;
    }

    let snapshot = validated_snapshot(&mut tx, &transfer_uid).await?;
    let fact = append_revision(
        &mut tx,
        snapshot,
        RevisionWrite {
            transfer_uid: &transfer_uid,
            revision: 1,
            previous: None,
            request_key,
            action: &draft.evidence_action,
            actor: fact_actor,
            delta: crate::exact::one(),
            authorization: RevisionAuthorization::Required {
                intent: draft.authorization_intent_uid.as_deref(),
                missing: "Transfer draft requires a signing key or verified Action intent",
            },
        },
        now,
        &now_string,
        &sign,
    )
    .await?;
    if let Some(correction) = &draft.correction {
        if !matches!(correction.kind.as_str(), "remainder" | "reversal")
            || !correction.canonical_quantity.is_finite()
            || correction.canonical_quantity <= 0.0
        {
            return Err(sqlx::Error::Protocol(
                "invalid Transfer correction lineage".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO transfer_correction_link
                (uid, kind, source_transfer_uid, source_occurrence_uid,
                 created_transfer_uid, source_revision, canonical_quantity,
                 actor_person_uid, fact_uid, idempotency_key, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(nucleus::new_uid("tcl"))
        .bind(&correction.kind)
        .bind(&correction.source_transfer_uid)
        .bind(&correction.source_occurrence_uid)
        .bind(&transfer_uid)
        .bind(correction.source_revision as i64)
        .bind(correction.canonical_quantity)
        .bind(&draft.creator_person)
        .bind(&fact.uid)
        .bind(request_key)
        .bind(&now_string)
        .execute(&mut *tx)
        .await?;
    }
    let mut invitation_event_facts = Vec::with_capacity(invitation_uids.len());
    for invitation_uid in &invitation_uids {
        let event_request = format!("{request_key}:invitation:{invitation_uid}:addressed");
        invitation_event_facts.push(
            insert_invitation_event(
                &mut tx,
                &transfer_uid,
                &NewInvitationEvent {
                    invitation_uid,
                    attempt: 1,
                    kind: "addressed",
                    from_status: None,
                    actor_uid: Some(&draft.creator_person),
                    revision: Some(1),
                    request_id: &event_request,
                },
                now,
                &sign,
            )
            .await?,
        );
    }
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::one(), &now_string).await?;

    let mut parent_facts = Vec::new();
    if let Some(parent) = &draft.parent_uid {
        let key = nucleus::fact::sha256_hex(
            format!("child-membership:{request_key}:{parent}").as_bytes(),
        );
        parent_facts.push(
            crate::transfer_children::amend(
                &mut tx,
                parent,
                &draft.creator_person,
                &key,
                draft.authorization_intent_uid.as_deref(),
                now,
                &sign,
            )
            .await?,
        );
    }
    tx.commit().await?;
    Ok(CreatedTransferDraft {
        transfer_uid,
        revision: 1,
        replayed: false,
        party_uids,
        invitation_uids,
        promise_uids,
        fact,
        invitation_event_facts,
        parent_facts,
    })
}
