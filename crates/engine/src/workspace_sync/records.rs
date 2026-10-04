use super::*;
use chrono::{DateTime, Utc};
use protein::authority::{MutationTarget, Operation, Property};

impl Engine {
    pub(super) async fn workspace_record_change(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        hosted: &Hosted,
        change: &Change,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        let (record, operation, permission) = match change {
            Change::CreateRecord { draft, .. } => (&draft.uid, Operation::Create, "record:create"),
            Change::EditRecord { record, .. } => (record, Operation::Update, "record:update"),
            Change::RestoreRecord { record, .. } => (record, Operation::Restore, "record:update"),
            Change::ChangeRecord { record, .. } => (record, Operation::Update, "record:update"),
            Change::DeleteRecord { record } => (record, Operation::Delete, "record:delete"),
            _ => return Ok(vec![]),
        };
        self.require_permission_on(tx, actor, permission).await?;
        let targets = vec![record.clone()];
        let before = crate::access::policy_graph_on(tx, &targets)
            .await
            .map_err(crate::record_policy::denied)?;
        if operation != Operation::Create {
            let target = before
                .records
                .iter()
                .find(|target| {
                    &target.uid == record && (target.deleted == (operation == Operation::Restore))
                })
                .ok_or_else(|| invalid("Record unavailable"))?;
            if target.kind != nucleus::RecordKind::Plain || target.organ_uid.as_deref() != Some(store::sqlx::query_scalar::<_, String>("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL").fetch_one(&mut **tx).await?.as_str()) {
                return Err(invalid("Shared Record actions support plain Records owned by this host"));
            }
            if operation != Operation::Restore
                && !self.readable_on(tx, actor, &before).await?.contains(record)
            {
                return Err(crate::record_policy::denied("Record unavailable"));
            }
        }
        let checkpoint = self
            .record_checkpoint_on(tx, actor, targets.clone(), operation)
            .await?
            .map(crate::record_policy::Checkpoint::bounded);
        let mut properties = BTreeSet::new();
        let mut facts = Vec::new();
        match change {
            Change::CreateRecord { draft, .. } => {
                let quantity = draft.validate()?;
                if before
                    .records
                    .iter()
                    .any(|existing| existing.uid == draft.uid)
                {
                    return Err(invalid("Record identity already exists"));
                }
                let organ: String = store::sqlx::query_scalar("SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL").fetch_one(&mut **tx).await?;
                store::records::create_with_uid_on(
                    tx,
                    &draft.uid,
                    store::records::NewRecord {
                        slug: draft.slug.as_deref(),
                        kind: nucleus::RecordKind::Plain,
                        head: &draft.head,
                        body: &draft.body,
                        quantity: store::exact::zero(),
                    },
                    &organ,
                    None,
                )
                .await?;
                let readable = self.readable_on(tx, actor, &before).await?;
                for assertion in &draft.assertions {
                    let predicate: String =
                        store::sqlx::query_scalar("SELECT uid FROM concept WHERE uid=?")
                            .bind(&assertion.predicate)
                            .fetch_optional(&mut **tx)
                            .await?
                            .ok_or_else(|| invalid("Use an existing assertion predicate UID"))?;
                    if assertion
                        .object
                        .as_ref()
                        .is_some_and(|object| !readable.contains(object))
                    {
                        return Err(crate::record_policy::denied(
                            "Assertion reference unavailable",
                        ));
                    }
                    let value = assertion
                        .quantity
                        .as_ref()
                        .map(|value| {
                            nucleus::DecimalValue::parse_inferred(value)
                                .map_err(|error| invalid(error.to_string()))
                        })
                        .transpose()?;
                    if let Some(unit) = &assertion.unit {
                        if !before.concepts.iter().any(|concept| &concept.uid == unit) {
                            return Err(invalid("Unit unavailable"));
                        }
                    }
                    store::assertions::insert_tx(
                        tx,
                        &nucleus::new_uid("a"),
                        store::assertions::NewAssertion {
                            subject_uid: record,
                            predicate_uid: &predicate,
                            object_uid: assertion.object.as_deref(),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: value,
                            unit_uid: assertion.unit.as_deref(),
                            asserted_by: actor,
                        },
                    )
                    .await?;
                }
                properties.extend([
                    Property::Kind,
                    Property::Organ,
                    Property::Head,
                    Property::Body,
                    Property::Quantity,
                ]);
                if draft.slug.is_some() {
                    properties.insert(Property::Slug);
                }
                let signer = self.signer.lock().await.clone();
                if let Some(fact) = crate::append::append_one_in_transaction(
                    tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: record.clone(),
                        delta: quantity,
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(json!({"created":true,"workspace":hosted.uid}).to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?
                {
                    facts.push(fact);
                }
            }
            Change::ChangeRecord { changes, .. } => {
                self.workspace_recipe_ceiling(tx, &hosted.policy, changes)
                    .await?;
                let readable = self.readable_on(tx, actor, &before).await?;
                if changes
                    .assign
                    .iter()
                    .chain(&changes.unassign)
                    .any(|person| !readable.contains(person))
                {
                    return Err(crate::record_policy::denied(
                        "Assignment reference unavailable",
                    ));
                }
                let (touched, committed) = self
                    .stage_workspace_changes(tx, record, changes, actor, now)
                    .await?;
                properties = touched;
                facts = committed;
            }
            Change::EditRecord { edits, .. } => {
                properties = self.stage_workspace_edits(tx, record, edits, actor).await?;
                let signer = self.signer.lock().await.clone();
                if let Some(fact) = crate::append::append_one_in_transaction(
                    tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: record.clone(),
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(json!({"edited":true,"workspace":hosted.uid}).to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?
                {
                    facts.push(fact);
                }
            }
            Change::RestoreRecord { slug, .. } => {
                store::records::restore_on(tx, record, slug.as_deref()).await?;
                properties.insert(Property::Slug);
                let signer = self.signer.lock().await.clone();
                if let Some(fact) = crate::append::append_one_in_transaction(
                    tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: record.clone(),
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(json!({"restored":true,"workspace":hosted.uid}).to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?
                {
                    facts.push(fact);
                }
            }
            Change::DeleteRecord { .. } => {
                store::records::mark_deleted_on(tx, record).await?;
                let signer = self.signer.lock().await.clone();
                if let Some(fact) = crate::append::append_one_in_transaction(
                    tx,
                    nucleus::NewFact {
                        uid: None,
                        record_uid: record.clone(),
                        delta: store::exact::zero(),
                        at: None,
                        actor_uid: actor.map(str::to_owned),
                        cause: nucleus::Cause::user_edit(),
                        payload: Some(json!({"deleted":true,"workspace":hosted.uid}).to_string()),
                    },
                    now,
                    signer.as_ref(),
                )
                .await?
                {
                    facts.push(fact);
                }
            }
            _ => unreachable!(),
        }
        let after = crate::access::policy_graph_on(tx, &targets)
            .await
            .map_err(crate::record_policy::denied)?;
        let mut available = before
            .records
            .iter()
            .filter(|record| !record.deleted)
            .map(|record| record.uid.clone())
            .collect::<BTreeSet<_>>();
        if matches!(operation, Operation::Create | Operation::Restore) {
            available.insert(record.clone());
        }
        crate::record_policy::check(
            &hosted.policy.ceiling,
            &before,
            &after,
            &crate::record_policy::ceiling(&after, available),
            &[MutationTarget {
                record_uid: record.clone(),
                touched_properties: properties.clone(),
            }],
        )?;
        crate::record_policy::check_workspace_dependencies_on(tx, &before, &after, &targets)
            .await?;
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(tx, properties).await?;
        }
        Ok(facts)
    }
}
