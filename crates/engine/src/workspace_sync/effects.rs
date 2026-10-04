use super::*;
use crate::area_transition::{QuantityOperation, RecordChanges};
use chrono::{DateTime, Utc};
use protein::authority::{MutationTarget, Operation, Property};

fn contains(area: &Geometry, position: [f64; 2]) -> bool {
    (0..2).all(|axis| (position[axis] - area.position[axis]).abs() <= area.size[axis] / 2.0)
}

fn placements(layout: &Layout) -> Vec<(String, Vec<String>, String, Geometry)> {
    fn visit(
        component: &Component,
        root: &str,
        path: &mut Vec<String>,
        geometry: &Geometry,
        records: &mut Vec<(String, Vec<String>, String, Geometry)>,
    ) {
        match component {
            Component::Builtin {
                state: ComponentState::Record { record, .. },
            } => records.push((root.into(), path.clone(), record.clone(), geometry.clone())),
            Component::Composition { composition } => {
                for part in &composition.parts {
                    path.push(part.id.clone());
                    visit(
                        &part.component,
                        root,
                        path,
                        &Geometry {
                            position: [
                                geometry.position[0] + part.geometry.position[0],
                                geometry.position[1] + part.geometry.position[1],
                            ],
                            size: part.geometry.size,
                        },
                        records,
                    );
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut records = vec![];
    for element in &layout.elements {
        visit(
            &element.component,
            &element.id,
            &mut vec![],
            &element.geometry,
            &mut records,
        );
    }
    records
}

impl Engine {
    pub(super) async fn workspace_recipe_ceiling(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        policy: &Policy,
        changes: &RecordChanges,
    ) -> Result<(), EngineError> {
        use protein::authority::{AssertionRole, AssertionTarget};
        for person in &changes.assign {
            refuse_automation_assignment(tx, "assigned-to", person).await?;
        }
        let grants = policy
            .ceiling
            .grants
            .iter()
            .filter(|grant| grant.operation == Operation::Update)
            .collect::<Vec<_>>();
        if changes.quantity.is_some()
            && !grants
                .iter()
                .any(|grant| grant.properties.contains(&Property::Quantity))
        {
            return Err(crate::record_policy::denied(
                "This workspace has no quantity lever",
            ));
        }
        for (concepts, add) in [(&changes.assert, true), (&changes.retract, false)] {
            for concept in concepts {
                if !grants
                    .iter()
                    .flat_map(|grant| {
                        if add {
                            &grant.assertions_add
                        } else {
                            &grant.assertions_remove
                        }
                    })
                    .any(|rule| {
                        rule.predicate_uid == *concept
                            && rule.role == AssertionRole::Ordinary
                            && rule.target == AssertionTarget::Unary
                    })
                {
                    return Err(crate::record_policy::denied(
                        "This assertion lever is outside the workspace ceiling",
                    ));
                }
            }
        }
        if !changes.assign.is_empty() || !changes.unassign.is_empty() {
            let predicate: String = store::sqlx::query_scalar(
                "SELECT uid FROM concept WHERE canonical_name='assigned-to'",
            )
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| invalid("Assignment predicate unavailable"))?;
            for (people, add) in [(&changes.assign, true), (&changes.unassign, false)] {
                for person in people {
                    if !grants
                        .iter()
                        .flat_map(|grant| {
                            if add {
                                &grant.assertions_add
                            } else {
                                &grant.assertions_remove
                            }
                        })
                        .any(|rule| {
                            rule.predicate_uid == predicate
                                && rule.role == AssertionRole::Ordinary
                                && (rule.target == AssertionTarget::AnyReadableRecord
                                    || rule.target == AssertionTarget::Record(person.clone()))
                        })
                    {
                        return Err(crate::record_policy::denied(
                            "This assignment lever is outside the workspace ceiling",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) async fn workspace_effects(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        before_layout: &Hosted,
        after_layout: &Hosted,
        change: &Change,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<nucleus::Fact>, EngineError> {
        let changed = match change {
            Change::Move { element, .. }
            | Change::Configure { element, .. }
            | Change::Resize { element, .. } => element.as_str(),
            Change::Add { element } => &element.id,
            Change::CreateRecord { placement, .. } | Change::RestoreRecord { placement, .. } => {
                placement
            }
            _ => return Ok(vec![]),
        };
        let mut crossed: BTreeMap<String, BTreeMap<String, RecordChanges>> = BTreeMap::new();
        let before_placements = placements(&before_layout.layout);
        for (root, path, record, geometry) in placements(&after_layout.layout) {
            geometry.validate().map_err(invalid)?;
            let previous = before_placements
                .iter()
                .find(|(old_root, old_path, old_record, _)| {
                    old_root == &root && old_path == &path && old_record == &record
                });
            for (id, recipe) in &after_layout.layout.areas {
                if after_layout.layout.disabled_areas.contains(id)
                    || root != changed && id != changed
                {
                    continue;
                }
                let area = after_layout
                    .layout
                    .elements
                    .iter()
                    .find(|area| &area.id == id)
                    .ok_or_else(|| invalid("Area missing"))?;
                let old_area = before_layout
                    .layout
                    .elements
                    .iter()
                    .find(|area| &area.id == id);
                let was_inside =
                    previous
                        .zip(old_area)
                        .is_some_and(|((_, _, _, previous), area)| {
                            contains(&area.geometry, previous.position)
                        });
                if contains(&area.geometry, geometry.position) && !was_inside {
                    crossed
                        .entry(record.clone())
                        .or_default()
                        .insert(id.clone(), recipe.clone());
                }
            }
        }
        let mut changes = BTreeMap::new();
        for (record, recipes) in crossed {
            let mut combined = RecordChanges::default();
            for recipe in recipes.values() {
                if !combined.merge(recipe) {
                    return Err(invalid("Overlapping Area effects conflict"));
                }
            }
            if !combined.is_empty() {
                changes.insert(record, combined);
            }
        }
        if changes.is_empty() {
            return Ok(vec![]);
        }
        let targets = changes.keys().cloned().collect::<Vec<_>>();
        for record in &targets {
            self.require_permission_on(tx, actor, "record:update")
                .await?;
            let kind: String = store::sqlx::query_scalar(
                "SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(record)
            .fetch_one(&mut **tx)
            .await?;
            let owned: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND replica_root IS NULL AND organ_uid=(SELECT uid FROM record WHERE slug='local-organ' AND kind='organ' AND deleted_at IS NULL))").bind(record).fetch_one(&mut **tx).await?;
            if kind != "plain" || !owned {
                return Err(crate::record_policy::denied(
                    "Shared Areas execute only on plain Records owned by this host",
                ));
            }
        }
        let before = crate::access::policy_graph_on(tx, &targets)
            .await
            .map_err(crate::record_policy::denied)?;
        let checkpoint = self
            .record_checkpoint_on(tx, actor, targets.clone(), Operation::Update)
            .await?
            .map(crate::record_policy::Checkpoint::bounded);
        let readable = self.readable_on(tx, actor, &before).await?;
        if targets.iter().any(|record| !readable.contains(record)) {
            return Err(crate::record_policy::denied(
                "An Area target is unavailable",
            ));
        }
        let mut footprints = Vec::new();
        let mut facts = Vec::new();
        for (record, changes) in changes {
            let canonical = changes;
            for person in canonical.assign.iter().chain(&canonical.unassign) {
                if !readable.contains(person) {
                    return Err(crate::record_policy::denied(
                        "Assignment target unavailable",
                    ));
                }
            }
            let (properties, committed) = self
                .stage_workspace_changes(tx, &record, &canonical, actor, now)
                .await?;
            footprints.push(MutationTarget {
                record_uid: record,
                touched_properties: properties,
            });
            facts.extend(committed);
        }
        let after = crate::access::policy_graph_on(tx, &targets)
            .await
            .map_err(crate::record_policy::denied)?;
        let ceiling = crate::record_policy::ceiling(
            &before,
            before
                .records
                .iter()
                .filter(|record| !record.deleted)
                .map(|record| record.uid.clone())
                .collect(),
        );
        crate::record_policy::check(
            &after_layout.policy.ceiling,
            &before,
            &after,
            &ceiling,
            &footprints,
        )?;
        crate::record_policy::check_workspace_dependencies_on(tx, &before, &after, &targets)
            .await?;
        if let Some(checkpoint) = checkpoint {
            checkpoint.finish(tx, Default::default()).await?;
        }
        Ok(facts)
    }

    pub(super) async fn stage_workspace_changes(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        record: &str,
        canonical: &RecordChanges,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<(BTreeSet<Property>, Vec<nucleus::Fact>), EngineError> {
        let mut properties = BTreeSet::new();
        store::assertions::transition_unary(
            tx,
            record,
            &canonical.retract,
            &canonical.assert,
            actor,
        )
        .await?;
        if !canonical.assign.is_empty() || !canonical.unassign.is_empty() {
            let predicate: String = store::sqlx::query_scalar(
                "SELECT uid FROM concept WHERE canonical_name='assigned-to'",
            )
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| invalid("Create the assigned-to concept before using assignments"))?;
            for person in &canonical.unassign {
                let assertions: Vec<String> = store::sqlx::query_scalar("SELECT uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid=? AND role='ordinary' AND retracted_at IS NULL LIMIT 129").bind(record).bind(&predicate).bind(person).fetch_all(&mut **tx).await?;
                if assertions.len() > 128 {
                    return Err(invalid("Too many assignments"));
                }
                for uid in assertions {
                    store::assertions::retract_tx(tx, &uid, actor).await?;
                }
            }
            for person in &canonical.assign {
                refuse_automation_assignment(tx, "assigned-to", person).await?;
                let kind: String = store::sqlx::query_scalar(
                    "SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL",
                )
                .bind(person)
                .fetch_one(&mut **tx)
                .await?;
                if kind != "person" {
                    return Err(invalid("Assign an existing Person"));
                }
                let exists: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_assertion WHERE subject_uid=? AND predicate_uid=? AND object_uid=? AND retracted_at IS NULL)").bind(record).bind(&predicate).bind(person).fetch_one(&mut **tx).await?;
                if !exists {
                    store::assertions::insert_tx(
                        tx,
                        &nucleus::new_uid("a"),
                        store::assertions::NewAssertion {
                            subject_uid: record,
                            predicate_uid: &predicate,
                            object_uid: Some(person),
                            role: store::assertions::AssertionRole::Ordinary,
                            quantity: None,
                            unit_uid: None,
                            asserted_by: actor,
                        },
                    )
                    .await?;
                }
            }
        }
        let mut facts = Vec::new();
        if let Some(quantity) = &canonical.quantity {
            properties.insert(Property::Quantity);
            let current = store::records::quantity_in_transaction(tx, record)
                .await?
                .ok_or_else(|| invalid("Record unavailable"))?;
            let (operation, operand) = QuantityOperation::parse(quantity)
                .ok_or_else(|| invalid("Invalid quantity operation"))?;
            let next = operation
                .evaluate(current, operand)
                .ok_or_else(|| invalid("Quantity overflow"))?;
            let signer = self.signer.lock().await.clone();
            if let Some(fact) = crate::append::append_one_in_transaction(
                tx,
                nucleus::NewFact {
                    actor_uid: actor.map(str::to_owned),
                    ..nucleus::NewFact::quantity(
                        record.to_owned(),
                        store::exact::difference(next, current)?,
                        nucleus::Cause::user_edit(),
                    )
                },
                now,
                signer.as_ref(),
            )
            .await?
            {
                facts.push(fact);
            }
        }
        Ok((properties, facts))
    }
}

pub(super) async fn refuse_automation_assignment(tx: &mut Transaction<'_, Sqlite>, predicate: &str, object: &str) -> Result<(), EngineError> {
    let automated: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension e WHERE e.record_uid=? AND e.namespace='lince.fiote' AND (?='assigned-to' OR EXISTS(SELECT 1 FROM concept WHERE uid=? AND canonical_name='assigned-to')))")
        .bind(object).bind(predicate).bind(predicate).fetch_one(&mut **tx).await?;
    if automated { return Err(crate::record_policy::denied("Shared controls cannot delegate the host's Fiote execution authority")); }
    Ok(())
}
