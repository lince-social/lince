use super::*;
use serde_json::json;

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    match operation {
        Operation::StartTimer | Operation::StopTimer => {
            let workspace = world.get::<Practice>(root).unwrap().workspace;
            let owner = match find(world, root, Role::Feature) {
                Some(owner) => owner,
                None => {
                    let owner = crate::sand_store::spawn_sand(
                        world,
                        root,
                        workspace,
                        crate::sand_store::SandKind::WorkTimer,
                        "",
                        DVec2::ZERO,
                    );
                    own(world, root, owner, Role::Feature);
                    owner
                }
            };
            let expected = operation == Operation::StartTimer;
            if crate::work_timer::running(world, owner) != Some(expected) {
                crate::work_timer::Toggle.apply(world, owner);
            }
        }
        Operation::ShowNotice => {
            world.init_resource::<crate::notifications::Notifications>();
            let previous_open = crate::notifications::open_state(world, root)
                .map(|(open, _)| open)
                .unwrap_or(false);
            let source = format!(
                "Instinct practice {}",
                world.get::<Practice>(root).unwrap().runner.session
            );
            let log = world
                .resource::<crate::notifications::Notifications>()
                .log
                .clone();
            log.report(&source, "The prepared sample action was confirmed.");
            let notice = log
                .snapshot()
                .1
                .into_iter()
                .find(|notice| notice.source == source)
                .ok_or("The sample feedback is unavailable.")?;
            crate::notifications::NotificationAction::Open.apply(world, root);
            let revision =
                crate::notifications::open_state(world, root).map(|(_, revision)| revision);
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .insert(operation, json!({"id":notice.id,"message":notice.message,"previous_open":previous_open,"revision":revision}));
        }
        Operation::DismissNotice => {
            let id = world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&Operation::ShowNotice)
                .and_then(|result| result["id"].as_u64())
                .ok_or("Open the sample feedback first, or Skip.")?;
            crate::notifications::NotificationAction::Delete(id).apply(world, root);
            clean_notice(world, root);
        }
        Operation::CompleteTask
        | Operation::UndoTask
        | Operation::MoveTask
        | Operation::OpenDatedRecord
        | Operation::SetOperation => {
            if find(world, root, Role::Feature).is_none() {
                lessons::begin_record(world, root, operation)?;
            }
            drive(world, root);
        }
        other => automation::execute(world, root, other)?,
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    let owner = find(world, root, Role::Feature);
    let practice = world.get::<Practice>(root).unwrap();
    match operation {
        Operation::ShowNotice => {
            practice.results.contains_key(&operation)
                && crate::notifications::open_state(world, root).is_some_and(|(open, _)| open)
        }
        Operation::DismissNotice => practice
            .results
            .get(&Operation::ShowNotice)
            .and_then(|result| result["id"].as_u64())
            .is_some_and(|id| {
                world
                    .get_resource::<crate::notifications::Notifications>()
                    .is_some_and(|notices| {
                        !notices
                            .log
                            .snapshot()
                            .1
                            .iter()
                            .any(|notice| notice.id == id)
                    })
            }),
        Operation::CompleteTask | Operation::UndoTask => owner.is_some_and(|owner| {
            crate::todo::saved(
                world,
                owner,
                &practice.note,
                if operation == Operation::CompleteTask {
                    "0"
                } else {
                    "-1"
                },
            )
        }),
        Operation::SetOperation => {
            owner.is_some_and(|owner| crate::operation::saved(world, owner, &practice.note))
        }
        Operation::MoveTask => practice
            .results
            .get(&operation)
            .is_some_and(|result| result["confirmed"] == true),
        _ => automation::complete(world, root, operation),
    }
}

pub(super) fn clean_notice(world: &mut World, root: Entity) {
    let Some(result) = world
        .get::<Practice>(root)
        .and_then(|practice| practice.results.get(&Operation::ShowNotice))
        .cloned()
    else {
        return;
    };
    if let Some(id) = result["id"].as_u64() {
        crate::notifications::NotificationAction::Delete(id).apply(world, root);
    }
    if result["previous_open"] == false
        && crate::notifications::open_state(world, root)
            .is_some_and(|(open, revision)| open && Some(revision) == result["revision"].as_u64())
    {
        crate::notifications::NotificationAction::Close.apply(world, root);
    }
}

pub(super) fn native_complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    match operation {
        Operation::StartTimer => find(world, root, Role::Feature)
            .is_some_and(|owner| crate::work_timer::running(world, owner) == Some(true)),
        Operation::StopTimer => find(world, root, Role::Feature).is_some_and(|owner| {
            crate::work_timer::running(world, owner) == Some(false)
                && crate::work_timer::stopped_log(world, owner)
        }),
        Operation::OpenDatedRecord => {
            let record = find(world, root, Role::Record);
            let uid = world.get::<Practice>(root).unwrap().note.clone();
            record.is_some_and(|record| {
                world
                    .query::<&crate::protein_area::RecordBinding>()
                    .iter(world)
                    .any(|binding| binding.area == record && binding.uid == uid)
            })
        }
        _ => complete(world, root, operation),
    }
}

pub(super) async fn change(
    engine: &engine::Engine,
    operation: Operation,
    uid: &str,
) -> Result<(), engine::EngineError> {
    use engine::actions::Action;
    match operation {
        Operation::MoveTask => {
            let task = lessons::concept(engine, "task", Vec::new()).await?;
            let todo = lessons::concept(engine, "todo", Vec::new()).await?;
            for predicate in [task, todo] {
                engine
                    .act(
                        Action::AssertRecord {
                            subject: uid.into(),
                            predicate,
                            object: None,
                            quantity: None,
                            unit: None,
                        },
                        None,
                    )
                    .await?;
            }
        }
        Operation::OpenDatedRecord => {
            for (field, value) in [
                (engine::record_change::WorkField::Start, "2026-10-04"),
                (engine::record_change::WorkField::Due, "2026-10-05"),
            ] {
                engine
                    .act(
                        Action::ChangeRecord {
                            request: engine::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: uid.into(),
                                mutation: engine::record_change::Mutation::Work {
                                    field,
                                    value: json!(value),
                                },
                            },
                        },
                        None,
                    )
                    .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn present(world: &mut World, root: Entity, operation: Operation) {
    if find(world, root, Role::Feature).is_some() {
        return;
    }
    let practice = world.get::<Practice>(root).unwrap();
    let workspace = practice.workspace;
    let source = practice.source.clone();
    let uid = practice.note.clone();
    let owner = match operation {
        Operation::CompleteTask | Operation::UndoTask => Some(crate::sand_store::spawn_sand(
            world,
            root,
            workspace,
            crate::sand_store::SandKind::Todo,
            "",
            DVec2::new(900.0, 0.0),
        )),
        Operation::OpenDatedRecord => {
            let spawn = protein(world, root).ok();
            if let Some(spawn) = spawn {
                let mut area = world.get_mut::<InfluenceArea>(spawn).unwrap();
                if let Some(config) = &mut area.protein {
                    config.draft.query["where"] = json!([{"uid_eq":uid}]);
                }
            }
            let area = spawn
                .and_then(|entity| world.get::<InfluenceArea>(entity))
                .map(|area| area.id.clone());
            Some(crate::calendar::spawn(
                world,
                root,
                workspace,
                DVec2::new(900.0, 0.0),
                crate::calendar::Calendar {
                    year: 2026,
                    month: 10,
                    area,
                    ..default()
                },
            ))
        }
        Operation::MoveTask => {
            let board = crate::kanban::spawn(world, root, workspace, DVec2::new(900.0, 0.0));
            if board.is_some() {
                let areas: Vec<_> = world
                    .query::<(Entity, &InfluenceArea, &WorkspaceMember)>()
                    .iter(world)
                    .filter(|(_, _, member)| member.0 == workspace)
                    .map(|(entity, _, _)| entity)
                    .collect();
                for entity in areas {
                    let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
                    let area = &mut *area;
                    for config in [&mut area.protein, &mut area.filter, &mut area.change_filter]
                        .into_iter()
                        .flatten()
                    {
                        config.source = crate::protein_area::Source::Organ(source.clone());
                        if let Some(filters) = config.draft.query["where"].as_array_mut() {
                            filters.push(json!({"uid_eq":uid}));
                        }
                    }
                    world
                        .entity_mut(entity)
                        .insert(crate::practice_cells::PracticeArea(source.clone()));
                    own(world, root, entity, Role::Auxiliary);
                }
            }
            board
        }
        Operation::SetOperation => {
            let owner = crate::sand_store::spawn_sand(
                world,
                root,
                workspace,
                crate::sand_store::SandKind::Operation,
                "",
                DVec2::new(900.0, 0.0),
            );
            crate::operation::input(world, owner, "@instinct-note");
            Some(owner)
        }
        Operation::ReadVocabulary => {
            let owner = crate::sand_store::spawn_sand(
                world,
                root,
                workspace,
                crate::sand_store::SandKind::Ontology,
                "",
                DVec2::new(700.0, 0.0),
            );
            crate::ontology::inspect_sample(world, owner);
            crate::edit_mode::label(
                world,
                owner,
                "nota-de-pratica → practice-note\nsample-kind → choose note or practice-note",
                14.0,
            );
            Some(owner)
        }
        _ => None,
    };
    if let Some(owner) = owner {
        own(world, root, owner, Role::Feature);
        world
            .entity_mut(owner)
            .insert(crate::practice_cells::PracticeSource(source));
    }
}

pub(super) fn drive(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.pending.is_none() {
        return;
    }
    let Some(operation) = practice.runner.current().and_then(|step| step.operation) else {
        return;
    };
    let uid = practice.note.clone();
    let source = practice.source.clone();
    let Some(owner) = find(world, root, Role::Feature) else {
        return;
    };
    match operation {
        Operation::CompleteTask
            if crate::todo::contains(world, owner, &uid)
                && !crate::todo::busy(world, owner)
                && !crate::todo::saved(world, owner, &uid, "0") =>
        {
            crate::todo::Command::Complete(uid).apply(world, owner)
        }
        Operation::UndoTask
            if !crate::todo::busy(world, owner)
                && !crate::todo::saved(world, owner, &uid, "-1") =>
        {
            crate::todo::Command::Undo.apply(world, owner)
        }
        Operation::SetOperation
            if crate::operation::ready(world, owner, "instinct-note")
                && !crate::operation::saved(world, owner, &uid) =>
        {
            crate::operation::input(world, owner, "@instinct-note");
            crate::operation::OperationAction::Submit.apply(world, owner);
        }
        Operation::MoveTask => {
            if !world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["moving"] == true)
                && crate::kanban::move_card(world, owner, &uid, 2).is_ok()
            {
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .entry(operation)
                    .or_default()["moving"] = json!(true);
            }
        }
        Operation::OpenDatedRecord => {
            let rows = sample_rows(world, root);
            if find(world, root, Role::Record).is_none()
                && rows.iter().any(|(_, record)| record == &uid)
            {
                let binding = crate::protein_area::RecordBinding {
                    uid: uid.clone(),
                    source: crate::protein_area::Source::Organ(source),
                    area: owner,
                };
                crate::calendar::Command::Record(binding).apply(world, owner);
            }
        }
        _ => {}
    }
}

pub(super) fn transition(
    world: &mut World,
    root: Entity,
    event: &crate::area_mutation::TransitionApplied,
) {
    let practice = world.get::<Practice>(root).unwrap();
    if event.record != practice.note || !event.inside {
        return;
    }
    if let Some(owner) = find(world, root, Role::Feature)
        && crate::kanban::column_entity(world, owner, 2) == Some(event.area)
    {
        world
            .get_mut::<Practice>(root)
            .unwrap()
            .results
            .entry(Operation::MoveTask)
            .or_default()["confirmed"] = json!(true);
    }
}
