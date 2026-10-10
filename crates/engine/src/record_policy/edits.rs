use super::{Operation, Property, denied};
use crate::{
    Engine, EngineError,
    actions::{Action, ActionOutcome},
};
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::BTreeSet;

impl Engine {
    pub(crate) async fn grant_record_visibility_as(
        &self,
        subject_kind: &str,
        subject: Option<&str>,
        target: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let target = self.resolve(target).await?;
        let subject = match (subject_kind, subject) {
            ("actor" | "organ", Some(subject)) => Some(self.resolve(subject).await?),
            (_, subject) => subject.map(str::to_owned),
        };
        let signer = self.signer.lock().await.clone();
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "permission:assign")
            .await?;
        self.require_permission_on(&mut tx, actor, "record:update")
            .await?;
        let graph = crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&target))
            .await
            .map_err(denied)?;
        if !self
            .readable_on(&mut tx, actor, &graph)
            .await?
            .contains(&target)
        {
            return Err(denied(
                "Visibility cannot be granted for an unreadable Record",
            ));
        }
        let uid = nucleus::new_uid("v");
        store::sqlx::query("INSERT INTO visibility_rule(uid,subject_kind,subject_uid,target_uid,grant_level) VALUES(?,?,?,?,'visible')")
            .bind(&uid).bind(subject_kind).bind(&subject).bind(&target).execute(&mut *tx).await?;
        let fact = crate::append::append_one_in_transaction(&mut tx, nucleus::NewFact {
            uid: None, record_uid: target, delta: store::exact::zero(), at: None, actor_uid: actor.map(str::to_owned), cause: nucleus::Cause::user_edit(),
            payload: Some(json!({"visibility_granted":{"uid":uid,"subject_kind":subject_kind,"subject":subject}}).to_string()),
        }, now, signer.as_ref()).await?;
        tx.commit().await?;
        Ok(ActionOutcome {
            created: Some(uid),
            facts: match fact {
                Some(fact) => self.observe_committed_fact(fact, now).await?,
                None => vec![],
            },
            ..Default::default()
        })
    }

    pub(crate) async fn edit_record_relations_as(
        &self,
        action: Action,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let action = match action {
            Action::SetIdentity { subject, predicate } => Action::SetIdentity {
                subject: self.resolve(&subject).await?,
                predicate: self.resolve_concept_opt(predicate).await?,
            },
            Action::RefineAssertion {
                subject,
                predicate,
                object,
            } => Action::RefineAssertion {
                subject: self.resolve(&subject).await?,
                predicate: self
                    .resolve_concept_opt(Some(predicate))
                    .await?
                    .ok_or_else(|| denied("Choose an Assertion predicate"))?,
                object: self.resolve(&object).await?,
            },
            Action::RetractRecord {
                subject,
                predicate,
                object,
            } => Action::RetractRecord {
                subject: self.resolve(&subject).await?,
                predicate: self
                    .resolve_concept_opt(Some(predicate))
                    .await?
                    .ok_or_else(|| denied("Choose an Assertion predicate"))?,
                object: match object {
                    Some(object) => Some(self.resolve(&object).await?),
                    None => None,
                },
            },
            Action::SetAssertionOrder {
                predicate,
                ordered,
                reverse,
            } => {
                if ordered.len() > 128 {
                    return Err(denied("Order at most 128 Records"));
                }
                let mut resolved = Vec::new();
                for token in ordered {
                    let uid = self.resolve(&token).await?;
                    if !resolved.contains(&uid) {
                        resolved.push(uid);
                    }
                }
                Action::SetAssertionOrder {
                    predicate: self
                        .resolve_concept_opt(Some(predicate))
                        .await?
                        .ok_or_else(|| denied("Choose an Assertion predicate"))?,
                    ordered: resolved,
                    reverse,
                }
            }
            Action::SetPlace {
                target,
                lat,
                lon,
                address,
            } => Action::SetPlace {
                target: self.resolve(&target).await?,
                lat,
                lon,
                address,
            },
            _ => return Err(EngineError::Consequence("Unsupported relation edit".into())),
        };
        let references = self.record_targets_of(&action).await?;
        let mut targets = references.clone();
        if matches!(
            action,
            Action::RefineAssertion { .. } | Action::RetractRecord { .. }
        ) {
            targets.truncate(1);
        }
        targets.sort();
        targets.dedup();
        if targets.is_empty() || targets.len() > 128 {
            return Err(denied("Declare between one and 128 Records to edit"));
        }
        for target in &targets {
            self.reject_direct_transfer_record_mutation(target).await?;
        }
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:update")
            .await?;
        let mut checkpoint = self
            .record_checkpoint_on(&mut tx, actor, targets.clone(), Operation::Update)
            .await?;
        if actor.is_some() {
            let graph = crate::access::policy_graph_on(&mut tx, &references)
                .await
                .map_err(denied)?;
            let readable = self.readable_on(&mut tx, actor, &graph).await?;
            if references.iter().any(|uid| !readable.contains(uid)) {
                return Err(denied("An edit references an unreadable Record"));
            }
        }
        let mut created = None;
        let mut properties = BTreeSet::new();
        match &action {
            Action::SetIdentity { subject, predicate } => {
                created = store::assertions::set_identity_tx(
                    &mut tx,
                    subject,
                    predicate.as_deref(),
                    actor,
                )
                .await?;
            }
            Action::RefineAssertion {
                subject,
                predicate,
                object,
            } => {
                created = Some(
                    store::assertions::refine_tx(&mut tx, subject, predicate, object, actor)
                        .await?,
                );
            }
            Action::RetractRecord {
                subject,
                predicate,
                object,
            } => {
                let ids: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid IS ? AND retracted_at IS NULL LIMIT 129")
                    .bind(subject).bind(predicate).bind(object).fetch_all(&mut *tx).await?;
                if ids.len() > 128 {
                    return Err(denied("Too many Assertions to retract"));
                }
                for uid in ids {
                    store::assertions::retract_tx(&mut tx, &uid, actor).await?;
                }
            }
            Action::SetAssertionOrder {
                predicate,
                ordered,
                reverse,
            } => {
                let resolved = ordered;
                if resolved.len() < 2 {
                    return Err(denied("Ordering requires at least two Records"));
                }
                for subject in resolved {
                    let rows: Vec<(String, String)> = store::sqlx::query_as("SELECT uid, object_uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid IS NOT NULL AND retracted_at IS NULL LIMIT 129")
                        .bind(subject).bind(&predicate).fetch_all(&mut *tx).await?;
                    if rows.len() > 128 {
                        return Err(denied("Too many Assertions to reorder"));
                    }
                    for (uid, object) in rows {
                        if resolved.contains(&object) {
                            store::assertions::retract_tx(&mut tx, &uid, actor).await?;
                        }
                    }
                }
                for pair in resolved.windows(2) {
                    let (subject, object) = if *reverse {
                        (&pair[1], &pair[0])
                    } else {
                        (&pair[0], &pair[1])
                    };
                    store::assertions::insert_tx(
                        &mut tx,
                        &nucleus::new_uid("a"),
                        store::assertions::NewAssertion {
                            subject_uid: subject,
                            predicate_uid: predicate,
                            object_uid: Some(object),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor,
                        },
                    )
                    .await?;
                }
            }
            Action::SetPlace {
                target,
                lat,
                lon,
                address,
            } => {
                if !lat.is_finite()
                    || !lon.is_finite()
                    || !(-90.0..=90.0).contains(lat)
                    || !(-180.0..=180.0).contains(lon)
                    || address.as_ref().is_some_and(|value| value.len() > 4096)
                {
                    return Err(EngineError::Consequence(
                        "Choose valid coordinates and an address of at most 4096 bytes".into(),
                    ));
                }
                let place = nucleus::new_uid("pl");
                store::sqlx::query("INSERT INTO place(uid,lat,lon,address) VALUES(?,?,?,?)")
                    .bind(&place)
                    .bind(lat)
                    .bind(lon)
                    .bind(address)
                    .execute(&mut *tx)
                    .await?;
                store::sqlx::query(
                    "UPDATE record SET place_uid=? WHERE uid=? AND deleted_at IS NULL",
                )
                .bind(&place)
                .bind(target)
                .execute(&mut *tx)
                .await?;
                if store::data_visibility::ensure_private_on(&mut tx, target, nucleus::visibility::Data::Place).await? {
                    let saved = nucleus::visibility::SavedPolicy { controller_uid: String::new(), revision: 1, policy: Default::default() };
                    store::sync_ops::log_local_tx(&mut tx, "data_visibility", target, "place", store::sync_ops::OpKind::Set, Some(serde_json::to_string(&saved)?)).await?;
                }
                store::sync_ops::log_local_tx(
                    &mut tx,
                    "record",
                    target,
                    "place_uid",
                    store::sync_ops::OpKind::Set,
                    Some(json!(place).to_string()),
                )
                .await?;
                if let Some(checkpoint) = checkpoint.as_mut() {
                    checkpoint.include_place(place.clone());
                }
                created = Some(place);
                properties.insert(Property::Place);
            }
            _ => return Err(EngineError::Consequence("Unsupported relation edit".into())),
        }
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(&mut tx, properties).await?;
        }
        let mut facts = Vec::new();
        for uid in targets {
            if let Some(fact) = crate::append::append_one_in_transaction(
                &mut tx,
                nucleus::NewFact {
                    uid: None,
                    record_uid: uid,
                    delta: store::exact::zero(),
                    at: None,
                    actor_uid: actor.map(str::to_owned),
                    cause: nucleus::Cause::user_edit(),
                    payload: Some(json!({"relation_edit":action}).to_string()),
                },
                now,
                signer.as_ref(),
            )
            .await?
            {
                facts.push(fact);
            }
        }
        tx.commit().await?;
        drop(serial);
        let mut outcome = ActionOutcome {
            created,
            ..Default::default()
        };
        for fact in facts {
            outcome
                .facts
                .extend(self.observe_committed_fact(fact, now).await?);
        }
        Ok(outcome)
    }
}
