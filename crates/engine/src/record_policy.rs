use crate::{Engine, EngineError, actions::Action};
use protein::authority::{
    self, GraphSnapshot, MutationTarget, Operation, Property, RolePolicy, VisibilityCeiling,
};
use std::collections::BTreeSet;
use store::sqlx::{Sqlite, Transaction};

mod edits;

pub(crate) const LOCAL_AUTOMATION_RECORD_QUERY: &str = "SELECT EXISTS(SELECT 1 FROM record_extension WHERE record_uid=? AND namespace='lince.fiote') OR EXISTS(SELECT 1 FROM record_assertion a JOIN concept c ON c.uid=a.predicate_uid JOIN record_extension e ON e.record_uid=a.object_uid AND e.namespace='lince.fiote' WHERE a.subject_uid=? AND a.retracted_at IS NULL AND c.canonical_name='assigned-to')";

pub(crate) struct Checkpoint {
    before: GraphSnapshot,
    policy: Option<RolePolicy>,
    read: RolePolicy,
    ceiling: VisibilityCeiling,
    targets: Vec<String>,
    actor: String,
    dependencies: Vec<protein::Predicate>,
    may_change_dependencies: bool,
}

pub(crate) enum ScalarChange<'a> {
    Unit(Option<&'a str>),
    Extension {
        namespace: &'a str,
        value: &'a serde_json::Value,
        expected: Option<&'a serde_json::Value>,
    },
    Delete,
}

impl Checkpoint {
    pub(crate) fn include_place(&mut self, uid: String) {
        self.before.places.insert(uid.clone());
        self.ceiling.places.insert(uid);
    }
    pub(crate) fn bounded(mut self) -> Self {
        self.may_change_dependencies = false;
        self
    }
    pub(crate) async fn finish(
        self,
        tx: &mut Transaction<'_, Sqlite>,
        properties: BTreeSet<Property>,
    ) -> Result<(), EngineError> {
        let after = crate::access::policy_graph_on(tx, &self.targets)
            .await
            .map_err(denied)?;
        for uid in &self.targets {
            let automated: bool = store::sqlx::query_scalar(LOCAL_AUTOMATION_RECORD_QUERY)
                .bind(uid).bind(uid).fetch_one(&mut **tx).await?;
            if automated { return Err(denied("Fiote configuration and data require the local interface session")); }
        }
        let previous_assertions: std::collections::BTreeMap<_, _> = self.before.assertions.iter().map(|assertion| (assertion.uid.as_str(), assertion)).collect();
        for assertion in &after.assertions {
            if self.targets.contains(&assertion.subject_uid) && previous_assertions.get(assertion.uid.as_str()).copied() != Some(assertion)
                && after.concepts.iter().any(|concept| concept.uid == assertion.predicate_uid && concept.name == "assigned-to")
                && let Some(object) = &assertion.object_uid
            {
                let automated: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension WHERE record_uid=? AND namespace='lince.fiote')")
                    .bind(object).fetch_one(&mut **tx).await?;
                if automated { return Err(denied("Fiote assignments require the local interface session")); }
            }
        }
        let targets = self
            .targets
            .iter()
            .map(|uid| MutationTarget {
                record_uid: uid.clone(),
                touched_properties: properties.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(policy) = self.policy {
            check(&policy, &self.before, &after, &self.ceiling, &targets)?;
        } else {
            let readable = authority::readable_records(
                Some(&self.read),
                &after,
                &self.ceiling,
                &Default::default(),
            )
            .map_err(denied)?;
            if after.records.iter().any(|record| {
                self.targets.contains(&record.uid)
                    && !record.deleted
                    && !readable.contains(&record.uid)
            }) {
                return Err(denied("The proposed state would remove your read access"));
            }
        }
        let readable = actor_readable_on(tx, &self.actor, &after).await?;
        if after.records.iter().any(|record| {
            self.targets.contains(&record.uid) && !record.deleted && !readable.contains(&record.uid)
        }) {
            return Err(denied("The proposed state would remove your read access"));
        }
        if !self.may_change_dependencies {
            let limits = authority::Limits {
                predicate_nodes: 65536,
                grants: 8192,
                ..Default::default()
            };
            let changed = authority::selector_membership_changes(
                &self.dependencies,
                &self.before,
                &after,
                &limits,
            )
            .map_err(denied)?;
            if changed.iter().any(|record| !self.targets.contains(record)) {
                return Err(denied(
                    "This operation would change access to Records outside its declared scope",
                ));
            }
        }
        Ok(())
    }
}

pub(crate) fn denied(error: impl std::fmt::Display) -> EngineError {
    EngineError::Forbidden(format!("Record policy: {error}"))
}

pub(crate) fn ceiling(graph: &GraphSnapshot, records: BTreeSet<String>) -> VisibilityCeiling {
    VisibilityCeiling {
        records,
        concepts: graph
            .concepts
            .iter()
            .map(|concept| concept.uid.clone())
            .collect(),
        places: graph.places.clone(),
    }
}

pub(crate) fn check(
    policy: &RolePolicy,
    before: &GraphSnapshot,
    after: &GraphSnapshot,
    ceiling: &VisibilityCeiling,
    targets: &[MutationTarget],
) -> Result<(), EngineError> {
    let limits = authority::Limits {
        grants: 8192,
        predicate_nodes: 65536,
        ..Default::default()
    };
    let current_ceiling = VisibilityCeiling {
        records: ceiling
            .records
            .iter()
            .filter(|uid| before.records.iter().any(|record| &record.uid == *uid))
            .cloned()
            .collect(),
        concepts: ceiling
            .concepts
            .iter()
            .filter(|uid| before.concepts.iter().any(|concept| &concept.uid == *uid))
            .cloned()
            .collect(),
        places: ceiling
            .places
            .intersection(&before.places)
            .cloned()
            .collect(),
    };
    authority::authorize_record_changes(
        Some(policy),
        before,
        after,
        &current_ceiling,
        ceiling,
        targets,
        &limits,
    )
    .map_err(denied)?;
    Ok(())
}

async fn actor_readable_on(
    tx: &mut Transaction<'_, Sqlite>,
    actor: &str,
    graph: &GraphSnapshot,
) -> Result<BTreeSet<String>, EngineError> {
    let filter = store::auth::person_access_on(tx, actor)
        .await?
        .and_then(|person| person.read_filter)
        .map(|raw| serde_json::from_str::<protein::Predicate>(&raw))
        .transpose()?;
    let mut result = BTreeSet::new();
    for role in store::person_roles::ids_on(tx, actor).await? {
        if !store::auth::role_permission_keys_by_id_on(tx, role)
            .await?
            .iter()
            .any(|key| key == "record:read")
        {
            continue;
        }
        let mut policy = store::role_policies::get_on(tx, role)
            .await?
            .and_then(|row| row.policy)
            .map(serde_json::from_value::<RolePolicy>)
            .transpose()?
            .unwrap_or(RolePolicy {
                read: protein::Predicate::All(vec![]),
                grants: vec![],
            });
        policy.grants.clear();
        if let Some(filter) = &filter {
            policy.read = protein::Predicate::All(vec![policy.read, filter.clone()]);
        }
        let records = store::visibility::role_targets_on(tx, actor, role)
            .await?
            .into_iter()
            .collect();
        result.extend(
            authority::readable_records(
                Some(&policy),
                graph,
                &ceiling(graph, records),
                &Default::default(),
            )
            .map_err(denied)?,
        );
    }
    Ok(result)
}

pub(crate) async fn check_workspace_dependencies_on(
    tx: &mut Transaction<'_, Sqlite>,
    before: &GraphSnapshot,
    after: &GraphSnapshot,
    targets: &[String],
) -> Result<(), EngineError> {
    let mut selectors = Vec::new();
    for row in store::role_policies::all_on(tx).await? {
        if let Some(raw) = row.policy {
            let policy: RolePolicy = serde_json::from_value(raw)?;
            selectors.push(policy.read);
            selectors.extend(policy.grants.into_iter().map(|grant| grant.selector));
        }
    }
    for person in store::auth::retained_person_access_on(tx).await? {
        if let Some(raw) = person.read_filter {
            selectors.push(serde_json::from_str(&raw)?);
        }
    }
    selectors.extend(workspace_selectors_on(tx).await?);
    let limits = authority::Limits {
        predicate_nodes: 65536,
        grants: 8192,
        ..Default::default()
    };
    let changed = authority::selector_membership_changes(&selectors, before, after, &limits)
        .map_err(denied)?;
    if changed.iter().any(|record| !targets.contains(record)) {
        return Err(denied(
            "This operation would change access to Records outside the workspace action's scope",
        ));
    }
    Ok(())
}

async fn workspace_selectors_on(tx: &mut Transaction<'_, Sqlite>) -> Result<Vec<protein::Predicate>, EngineError> {
    let mut selectors = Vec::new();
    let policies: Vec<String> =
        store::sqlx::query_scalar("SELECT policy FROM shared_workspace LIMIT 257")
            .fetch_all(&mut **tx)
            .await?;
    if policies.len() > 256 || policies.iter().map(String::len).sum::<usize>() > 16777216 {
        return Err(denied("Workspace policy catalogue exceeds its bounds"));
    }
    for raw in policies {
        let policy: crate::workspace_sync::Policy = serde_json::from_str(&raw)?;
        selectors.push(policy.ceiling.read);
        selectors.extend(
            policy
                .ceiling
                .grants
                .into_iter()
                .map(|grant| grant.selector),
        );
    }
    Ok(selectors)
}

impl Engine {
    pub(crate) async fn change_record_scalar_as(
        &self,
        uid: &str,
        change: ScalarChange<'_>,
        actor: Option<&str>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        let operation = if matches!(change, ScalarChange::Delete) {
            Operation::Delete
        } else {
            Operation::Update
        };
        if operation == Operation::Delete {
            if let Err(error) = self
                .require_permission_on(&mut tx, actor, "record:delete")
                .await
            {
                self.require_permission_on(&mut tx, actor, "record:delete_own")
                    .await?;
                let creator: Option<String> = store::sqlx::query_scalar(
                    "SELECT actor_uid FROM fact WHERE record_uid=? ORDER BY at,uid LIMIT 1",
                )
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                if creator.as_deref() != actor {
                    return Err(error);
                }
            }
        } else {
            self.require_permission_on(&mut tx, actor, "record:update")
                .await?;
        }
        let recoverable = !store::person_roles::recovery_people_on(&mut tx)
            .await?
            .is_empty();
        if operation == Operation::Delete
            && store::auth::person_access_on(&mut tx, uid).await?.is_some()
        {
            self.require_permission_on(&mut tx, actor, "user:delete")
                .await?;
            if let Some(actor) = actor {
                let caller = store::person_roles::permissions_on(&mut tx, actor).await?;
                if store::person_roles::permissions_on(&mut tx, uid)
                    .await?
                    .iter()
                    .any(|key| !caller.contains(key))
                {
                    return Err(denied("Actor management authority changed"));
                }
            }
        }
        let checkpoint = self
            .record_checkpoint_on(&mut tx, actor, vec![uid.into()], operation)
            .await?;
        let mut properties = BTreeSet::new();
        let payload = match change {
            ScalarChange::Unit(unit) => {
                properties.insert(Property::Unit);
                store::records::set_unit_on(&mut tx, uid, unit).await?;
                serde_json::json!({"unit":unit})
            }
            ScalarChange::Extension {
                namespace,
                value,
                expected,
            } => {
                if let Some(expected) = expected {
                    let raw: Option<String> = store::sqlx::query_scalar(
                        "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
                    )
                    .bind(uid)
                    .bind(namespace)
                    .fetch_optional(&mut *tx)
                    .await?;
                    let current = raw
                        .as_deref()
                        .map(serde_json::from_str::<serde_json::Value>)
                        .transpose()?
                        .unwrap_or_default();
                    if &current != expected {
                        return Err(denied(
                            "The extension changed while this response was being saved",
                        ));
                    }
                }
                store::records::set_extension_on(&mut tx, uid, namespace, value).await?;
                serde_json::json!({"extension":namespace})
            }
            ScalarChange::Delete => {
                let slug: Option<String> = store::sqlx::query_scalar(
                    "SELECT slug FROM record WHERE uid=? AND deleted_at IS NULL",
                )
                .bind(uid)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                store::records::mark_deleted_on(&mut tx, uid).await?;
                store::person_roles::require_recovery_on(&mut tx, recoverable).await?;
                serde_json::json!({"deleted":true,"slug":slug})
            }
        };
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(&mut tx, properties).await?;
        }
        let fact = crate::append::append_one_in_transaction(
            &mut tx,
            nucleus::NewFact {
                uid: None,
                record_uid: uid.into(),
                delta: store::exact::zero(),
                at: None,
                actor_uid: actor.map(str::to_owned),
                cause: nucleus::Cause::user_edit(),
                payload: Some(payload.to_string()),
            },
            now,
            signer.as_ref(),
        )
        .await?;
        tx.commit().await?;
        drop(serial);
        match fact {
            Some(fact) => self.observe_committed_fact(fact, now).await,
            None => Ok(vec![]),
        }
    }

    pub(crate) async fn inspect_record_authority(
        &self,
        caller: Option<&str>,
        person: &str,
        record: &str,
    ) -> Result<crate::actions::ActionOutcome, EngineError> {
        self.require_permission(caller, "permission:read").await?;
        self.require_permission(caller, "user:read").await?;
        let person = self.resolve(person).await?;
        self.require_manageable_person(caller, &person).await?;
        let record = self.resolve(record).await?;
        if !self.may_read_record(caller, &record).await? {
            return Err(denied("Record unavailable"));
        }
        let mut tx = self.store.pool.begin().await?;
        self.require_permission_on(&mut tx, caller, "permission:read")
            .await?;
        self.require_permission_on(&mut tx, caller, "user:read")
            .await?;
        let graph = crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&record))
            .await
            .map_err(denied)?;
        let readable = self
            .readable_on(&mut tx, Some(&person), &graph)
            .await
            .unwrap_or_default()
            .contains(&record);
        let permissions = store::person_roles::permissions_on(&mut tx, &person).await?;
        let mut properties = BTreeSet::new();
        let local_automation: bool = store::sqlx::query_scalar(LOCAL_AUTOMATION_RECORD_QUERY).bind(&record).bind(&record).fetch_one(&mut *tx).await?;
        if readable && !local_automation && permissions.iter().any(|key| key == "record:update") {
            let policy =
                protein::role_authority::policy_for_on(&mut tx, &person, Some(Operation::Update))
                    .await?;
            let checkpoint = self
                .record_checkpoint_on(
                    &mut tx,
                    Some(&person),
                    vec![record.clone()],
                    Operation::Update,
                )
                .await?
                .ok_or_else(|| denied("Actor unavailable"))?;
            let mut candidates = BTreeSet::from([
                Property::Head,
                Property::Body,
                Property::Quantity,
                Property::Slug,
                Property::Unit,
                Property::Place,
            ]);
            if let Some(policy) = &policy {
                candidates.extend(
                    policy
                        .grants
                        .iter()
                        .flat_map(|grant| grant.properties.iter().cloned()),
                );
            }
            for property in candidates {
                if policy.as_ref().is_none_or(|_| {
                    check(
                        checkpoint.policy.as_ref().unwrap(),
                        &graph,
                        &graph,
                        &checkpoint.ceiling,
                        &[MutationTarget {
                            record_uid: record.clone(),
                            touched_properties: BTreeSet::from([property.clone()]),
                        }],
                    )
                    .is_ok()
                }) {
                    properties.insert(property);
                }
            }
        }
        let mut supplied = Vec::new();
        for role in store::person_roles::ids_on(&mut tx, &person).await? {
            let name: String = store::sqlx::query_scalar("SELECT name FROM role WHERE id=?")
                .bind(role)
                .fetch_one(&mut *tx)
                .await?;
            supplied.push(serde_json::json!({"role":name,"permissions":store::auth::role_permission_keys_by_id_on(&mut tx, role).await?}));
        }
        tx.commit().await?;
        Ok(crate::actions::ActionOutcome {
            data: Some(
                serde_json::json!({"person":person,"record":record,"read":readable,"update_properties":properties,"role_sources":supplied,"explanation":"Matching Role grants add together. Every mutation checks its proposed state, references and workspace ceiling again."}),
            ),
            ..Default::default()
        })
    }

    pub(crate) async fn append_record_changes_as(
        &self,
        facts: Vec<nucleus::NewFact>,
        actor: Option<&str>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        let targets = facts
            .iter()
            .map(|fact| fact.record_uid.clone())
            .collect::<Vec<_>>();
        let signer = self.signer.lock().await.clone();
        let serial = self.import_lock.lock().await;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:update")
            .await?;
        let checkpoint = self
            .record_checkpoint_on(&mut tx, actor, targets, Operation::Update)
            .await?;
        let mut committed = Vec::new();
        for fact in facts {
            if let Some(fact) =
                crate::append::append_one_in_transaction(&mut tx, fact, now, signer.as_ref())
                    .await?
            {
                committed.push(fact);
            }
        }
        if let Some(checkpoint) = checkpoint {
            checkpoint
                .finish(&mut tx, BTreeSet::from([Property::Quantity]))
                .await?;
        }
        tx.commit().await?;
        drop(serial);
        let mut outcome = Vec::new();
        for fact in committed {
            outcome.extend(self.observe_committed_fact(fact, now).await?);
        }
        Ok(outcome)
    }

    pub(crate) async fn require_permission_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        permission: &str,
    ) -> Result<(), EngineError> {
        self.require_login_on(tx).await?;
        let Some(actor) = actor else {
            return Ok(());
        };
        let active = store::people::is_active_on(tx, actor).await?;
        let permissions = store::person_roles::permissions_on(tx, actor).await?;
        if !active || !permissions.iter().any(|key| key == permission) {
            return Err(EngineError::Forbidden(format!(
                "Missing {permission} permission or active Actor standing"
            )));
        }
        Ok(())
    }

    pub(crate) async fn readable_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        graph: &GraphSnapshot,
    ) -> Result<BTreeSet<String>, EngineError> {
        let Some(actor) = actor else {
            return Ok(graph
                .records
                .iter()
                .filter(|record| !record.deleted)
                .map(|record| record.uid.clone())
                .collect());
        };
        self.require_permission_on(tx, Some(actor), "record:read")
            .await?;
        actor_readable_on(tx, actor, graph).await
    }

    pub(crate) async fn record_checkpoint_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        actor: Option<&str>,
        targets: Vec<String>,
        operation: Operation,
    ) -> Result<Option<Checkpoint>, EngineError> {
        let Some(actor) = actor else {
            return Ok(None);
        };
        self.require_login_on(tx).await?;
        self.require_permission_on(tx, Some(actor), "record:read")
            .await?;
        for uid in &targets {
            let automated: bool = store::sqlx::query_scalar(LOCAL_AUTOMATION_RECORD_QUERY)
                .bind(uid).bind(uid).fetch_one(&mut **tx).await?;
            if automated { return Err(denied("Fiote configuration and assigned task data require the local interface session")); }
        }
        let mut policy = protein::role_authority::policy_for_on(tx, actor, Some(operation)).await?;
        let mut read = protein::role_authority::policy_for_on(tx, actor, None)
            .await?
            .unwrap_or(RolePolicy {
                read: protein::Predicate::All(vec![]),
                grants: vec![],
            });
        if let Some(filter) = store::auth::person_access_on(tx, actor)
            .await?
            .and_then(|person| person.read_filter)
        {
            let filter: protein::Predicate = serde_json::from_str(&filter)?;
            read.read = protein::Predicate::All(vec![read.read, filter.clone()]);
            if let Some(policy) = policy.as_mut() {
                policy.read = protein::Predicate::All(vec![policy.read.clone(), filter]);
            }
        }
        let before = crate::access::policy_graph_on(tx, &targets)
            .await
            .map_err(denied)?;
        let mut records = protein::role_authority::visibility_ceiling_on(tx, actor).await?;
        if matches!(operation, Operation::Create | Operation::Restore) {
            records.extend(targets.iter().cloned());
        }
        let ceiling = ceiling(&before, records);
        let mut dependencies = Vec::new();
        for row in store::role_policies::all_on(tx).await? {
            if let Some(raw) = row.policy {
                let policy: RolePolicy = serde_json::from_value(raw)?;
                dependencies.push(policy.read);
                dependencies.extend(policy.grants.into_iter().map(|grant| grant.selector));
            }
        }
        for person in store::auth::retained_person_access_on(tx).await? {
            if let Some(raw) = person.read_filter {
                dependencies.push(serde_json::from_str(&raw)?);
            }
        }
        dependencies.extend(workspace_selectors_on(tx).await?);
        let may_change_dependencies = store::person_roles::permissions_on(tx, actor)
            .await?
            .iter()
            .any(|key| key == "permission:assign");
        Ok(Some(Checkpoint {
            before,
            policy,
            read,
            ceiling,
            targets,
            actor: actor.into(),
            dependencies,
            may_change_dependencies,
        }))
    }
    pub(crate) async fn authorize_record_policy(
        &self,
        action: &Action,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        if matches!(action, Action::GrantVisibility { .. }) {
            return self.require_permission(Some(actor), "permission:assign").await;
        }
        let operation = if matches!(
            action,
            Action::CreateRecord { .. }
                | Action::CreateRecordDraft { .. }
                | Action::CreateRecordWithTags { .. }
        ) {
            Operation::Create
        } else if matches!(action, Action::DeleteRecord { .. }) {
            Operation::Delete
        } else {
            Operation::Update
        };
        let Some(mut policy) =
            protein::role_authority::policy_for(&self.store, actor, Some(operation)).await?
        else {
            if store::auth::person_access(&self.store.pool, actor)
                .await?
                .and_then(|access| access.read_filter)
                .is_some()
                && matches!(
                    Self::generic_write_permission(action),
                    Some("record:update" | "record:create" | "record:delete")
                )
                && !matches!(
                    action,
                    Action::EditRecordText { .. }
                        | Action::SetQuantity { .. }
                        | Action::SetQuantityExact { .. }
                        | Action::AddQuantity { .. }
                        | Action::AddQuantityExact { .. }
                        | Action::SetSlug { .. }
                        | Action::SetUnit { .. }
                        | Action::SetExtension { .. }
                        | Action::DeleteRecord { .. }
                        | Action::Activate { .. }
                        | Action::Deactivate { .. }
                        | Action::AddQuantityGroupExact { .. }
                        | Action::ChangeRecord { .. }
                        | Action::RecordExtensions { .. }
                        | Action::CreateRecordDraft { .. }
                        | Action::CreateRecord { .. }
                        | Action::CreateRecordWithTags { .. }
                        | Action::AssertRecord { .. }
                        | Action::RetractAssertion { .. }
                        | Action::RefineAssertion { .. }
                        | Action::RetractRecord { .. }
                        | Action::SetIdentity { .. }
                        | Action::SetAssertionOrder { .. }
                        | Action::SetPlace { .. }
                        | Action::PreviewAreaTransition { .. }
                        | Action::ApplyAreaTransition { .. }
                        | Action::TransitionRecord { .. }
                        | Action::Workspace { .. }
                )
            {
                return Err(denied(
                    "This command has no bounded policy-aware mutation. Use the Record editor or shared workspace action.",
                ));
            }
            return Ok(());
        };
        let mut properties = BTreeSet::new();
        let token = match action {
            Action::EditRecordText { target, head, body } if head.is_some() || body.is_some() => {
                if head.is_some() {
                    properties.insert(Property::Head);
                }
                if body.is_some() {
                    properties.insert(Property::Body);
                }
                target
            }
            Action::SetQuantity { target, .. }
            | Action::SetQuantityExact { target, .. }
            | Action::AddQuantity { target, .. }
            | Action::AddQuantityExact { target, .. } => {
                properties.insert(Property::Quantity);
                target
            }
            Action::SetSlug { target, .. } => {
                properties.insert(Property::Slug);
                target
            }
            Action::SetUnit { target, .. } => {
                properties.insert(Property::Unit);
                target
            }
            Action::DeleteRecord { target } => target,
            Action::SetExtension { target, .. } => target,
            Action::EditRecordText { .. }
            | Action::Activate { .. }
            | Action::Deactivate { .. }
            | Action::AddQuantityGroupExact { .. }
            | Action::ChangeRecord { .. }
            | Action::RecordExtensions { .. }
            | Action::CreateRecordDraft { .. }
            | Action::CreateRecord { .. }
            | Action::CreateRecordWithTags { .. }
            | Action::AssertRecord { .. }
            | Action::RetractAssertion { .. }
            | Action::RefineAssertion { .. }
            | Action::RetractRecord { .. }
            | Action::SetIdentity { .. }
            | Action::SetAssertionOrder { .. }
            | Action::SetPlace { .. }
            | Action::PreviewAreaTransition { .. }
            | Action::ApplyAreaTransition { .. }
            | Action::TransitionRecord { .. }
            | Action::Workspace { .. } => return Ok(()),
            _ => {
                if matches!(
                    Self::generic_write_permission(action),
                    Some("record:update" | "record:create" | "record:delete")
                ) {
                    return Err(denied(
                        "This command has no bounded policy-aware mutation. Use the Record editor or shared workspace action.",
                    ));
                }
                return Ok(());
            }
        };
        let uid = self.resolve(token).await?;
        let readable = protein::role_authority::visibility_ceiling(&self.store, actor).await?;
        if let Some(personal) = store::auth::person_access(&self.store.pool, actor)
            .await?
            .and_then(|person| person.read_filter)
        {
            let filter = serde_json::from_str(&personal)?;
            policy.read = protein::Predicate::All(vec![policy.read, filter]);
        }
        let mut tx = self.store.pool.begin().await?;
        let before = crate::access::policy_graph_on(&mut tx, std::slice::from_ref(&uid))
            .await
            .map_err(denied)?;
        tx.commit().await?;
        let mut after = before.clone();
        let record = after
            .records
            .iter_mut()
            .find(|record| record.uid == uid)
            .ok_or_else(|| denied("Record unavailable"))?;
        let content = record
            .content
            .as_mut()
            .ok_or_else(|| denied("Record content unavailable"))?;
        match action {
            Action::EditRecordText { head, body, .. } => {
                if let Some(head) = head {
                    content.head = head.clone();
                }
                if let Some(body) = body {
                    content.body = body.clone();
                }
            }
            Action::SetQuantity { value, .. } => content.quantity = store::exact::from_f64(*value),
            Action::SetQuantityExact { amount, .. } => {
                content.quantity = nucleus::DecimalValue::parse_inferred(amount).map_err(denied)?
            }
            Action::AddQuantity { delta, .. } => {
                content.quantity = content
                    .quantity
                    .aligned_add(store::exact::from_f64(*delta))
                    .ok_or_else(|| denied("Quantity overflow"))?
            }
            Action::AddQuantityExact { delta, .. } => {
                content.quantity = content
                    .quantity
                    .aligned_add(*delta)
                    .ok_or_else(|| denied("Quantity overflow"))?
            }
            Action::SetSlug { slug, .. } => content.slug = slug.clone(),
            Action::SetUnit { unit, .. } => {
                content.unit_uid = match unit {
                    Some(unit) => Some(
                        store::concepts::resolve(&self.store.pool, unit)
                            .await?
                            .ok_or_else(|| denied("Unit unavailable"))?,
                    ),
                    None => None,
                }
            }
            Action::DeleteRecord { .. } => record.deleted = true,
            Action::SetExtension { namespace, fds, .. } => {
                let fields = fds
                    .as_object()
                    .ok_or_else(|| denied("Extension must be an object"))?;
                for property in content
                    .extensions
                    .keys()
                    .filter(|property| &property.namespace == namespace)
                {
                    properties.insert(Property::Extension(property.clone()));
                }
                content
                    .extensions
                    .retain(|property, _| &property.namespace != namespace);
                for (field, value) in fields {
                    let property = authority::ExtensionProperty {
                        namespace: namespace.clone(),
                        field: field.clone(),
                    };
                    properties.insert(Property::Extension(property.clone()));
                    content.extensions.insert(property, value.clone());
                }
            }
            _ => unreachable!(),
        }
        check(
            &policy,
            &before,
            &after,
            &ceiling(&before, readable),
            &[MutationTarget {
                record_uid: uid,
                touched_properties: properties,
            }],
        )
    }
}
