// non-lean-yet

use super::{Assertion, Draft, invalid};
use crate::{
    Engine, EngineError, actions::ActionOutcome, append::append_one_in_transaction,
    file_sync::require_origin,
};
use chrono::{DateTime, Utc};
use lince_resources::{
    address,
    reference::{is_address, is_uri},
};
use nucleus::{Cause, DecimalValue, NewFact, RecordKind, new_uid, sync::PERSONAL_FILE_ORIGIN};
use protein::authority::Operation;
use serde_json::{json, to_string};
use std::{collections::HashSet, path::Path};
use store::{
    assertions::{AssertionRole, NewAssertion, insert_tx, insert_uri_tx},
    concepts::ancestors_including,
    exact::zero,
    organs::local,
    records::{NewRecord, create_with_uid_on, set_extension_on, set_file_origin_on},
    sqlx::{query, query_as},
    vocabulary::resolve,
    write_tx,
};

impl Engine {
    pub(crate) async fn create_record_draft(
        &self,
        draft: Draft,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.create_record_draft_kind(draft, RecordKind::Plain, actor, now)
            .await
    }

    pub(crate) async fn create_record_draft_kind(
        &self,
        mut draft: Draft,
        kind: RecordKind,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let actor_uid = actor.as_deref();
        self.require_permission(actor_uid, "record:create").await?;
        let quantity = draft.validate()?;
        let payload = to_string(&(kind, &draft))?;
        let family = self
            .resolve_draft_assertions(&mut draft.assertions, actor_uid)
            .await?;
        if let Some(actor) = actor_uid {
            self.authorize_record_draft(&draft, kind, actor, quantity, &family)
                .await?;
        }
        let organ = local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("The local Organ is unavailable"))?;
        let signer = self.signer.lock().await.clone();
        let mut tx = write_tx(&self.store.pool).await?;
        let checkpoint = self
            .record_checkpoint_on(
                &mut tx,
                actor_uid,
                vec![draft.uid.clone()],
                Operation::Create,
            )
            .await?;
        let receipt_actor = actor_uid.unwrap_or("");
        if let Some((previous, uid)) = query_as::<_, (String, String)>(
            "SELECT payload, record_uid FROM record_change_receipt WHERE actor = ? AND change_uid = ?",
        )
        .bind(receipt_actor)
        .bind(&draft.uid)
        .fetch_optional(&mut *tx)
        .await?
        {
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
        create_with_uid_on(
            &mut tx,
            &draft.uid,
            NewRecord {
                slug: draft.slug.as_deref(),
                kind,
                head: &draft.head,
                body: &draft.body,
                quantity: zero(),
            },
            &organ.uid,
            None,
        )
        .await?;
        if let Some(origin) = draft.file_origin.as_deref() {
            if actor.is_some() {
                return Err(EngineError::Forbidden(
                    "Only the local owner can assign a file sync origin".into(),
                ));
            }
            if origin != PERSONAL_FILE_ORIGIN {
                require_origin(&mut tx, origin).await?;
                set_file_origin_on(&mut tx, &draft.uid, origin).await?;
            }
        }
        for assertion in draft.assertions {
            let (uri, object_uid) = match assertion.object.as_deref() {
                Some(value) if is_uri(value) => (Some(value), None),
                value => (None, value),
            };
            let new = NewAssertion {
                subject_uid: &draft.uid,
                predicate_uid: &assertion.predicate,
                object_uid,
                role: AssertionRole::Ordinary,
                quantity: assertion
                    .quantity
                    .as_deref()
                    .map(DecimalValue::parse_inferred)
                    .transpose()
                    .map_err(|_| invalid("Enter an exact assertion quantity"))?,
                unit_uid: assertion.unit.as_deref(),
                asserted_by: actor_uid,
            };
            let uid = new_uid("a");
            if let Some(uri) = uri {
                insert_uri_tx(&mut tx, &uid, new, uri).await?;
            } else {
                insert_tx(&mut tx, &uid, new).await?;
            }
        }
        set_extension_on(&mut tx, &draft.uid, "work", &draft.work).await?;
        let fact = append_one_in_transaction(
            &mut tx,
            NewFact {
                actor_uid: actor.clone(),
                ..NewFact::quantity(draft.uid.clone(), quantity, Cause::user_edit())
            },
            now,
            signer.as_ref(),
        )
        .await?;
        query(
            "INSERT INTO record_change_receipt (actor, change_uid, record_uid, payload, result) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(receipt_actor)
        .bind(&draft.uid)
        .bind(&draft.uid)
        .bind(payload)
        .bind(json!({"created": draft.uid}).to_string())
        .execute(&mut *tx)
        .await?;
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(&mut tx, Default::default()).await?;
        }
        tx.commit().await?;
        let facts = if let Some(fact) = fact {
            self.observe_committed_fact(fact, now).await?
        } else {
            Vec::new()
        };
        self.notify_query_changed();
        Ok(ActionOutcome {
            created: Some(draft.uid),
            facts,
            ..Default::default()
        })
    }

    async fn resolve_draft_assertions(
        &self,
        assertions: &mut [Assertion],
        actor: Option<&str>,
    ) -> Result<HashSet<String>, EngineError> {
        let pool = &self.store.pool;
        let mut family = HashSet::new();
        for assertion in assertions {
            assertion.predicate = resolve(pool, &assertion.predicate, actor)
                .await?
                .ok_or_else(|| invalid("Choose an existing assertion"))?;
            if assertion.object.is_none() {
                family.extend(ancestors_including(pool, &assertion.predicate).await?);
            }
            if let Some(object) = &mut assertion.object {
                if is_address(object) {
                    if !is_uri(object) && !Path::new(object).is_absolute() {
                        return Err(invalid("A relative reference needs a declaring document"));
                    }
                    *object = address(object).map_err(invalid)?.to_string();
                } else {
                    *object = self.resolve(object).await?;
                    if !self.may_read_record(actor, object).await? {
                        return Err(EngineError::Forbidden(
                            "The related Record is not visible".into(),
                        ));
                    }
                }
            }
            if let Some(unit) = &mut assertion.unit {
                *unit = resolve(pool, unit, actor)
                    .await?
                    .ok_or_else(|| invalid("Choose an existing unit"))?;
            }
        }
        Ok(family)
    }
}
