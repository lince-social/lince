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
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .insert(operation, json!({"id":notice.id,"message":notice.message}));
            crate::notifications::NotificationAction::Toggle.apply(world, root);
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
            crate::notifications::NotificationAction::Close.apply(world, root);
        }
        Operation::CompleteTask
        | Operation::UndoTask
        | Operation::MoveTask
        | Operation::OpenDatedRecord
        | Operation::SetOperation => lessons::begin_record(world, root, operation)?,
        _ => return Err("The work example is unavailable.".into()),
    }
    Ok(())
}

pub(super) fn complete(world: &World, root: Entity, operation: Operation) -> bool {
    let practice = world.get::<Practice>(root).unwrap();
    match operation {
        Operation::ShowNotice => practice.results.contains_key(&operation),
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
        _ => practice.results.contains_key(&operation),
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
        Operation::CompleteTask
        | Operation::UndoTask
        | Operation::SetOperation
        | Operation::MoveTask => {
            let amount = match operation {
                Operation::UndoTask => "-1",
                Operation::MoveTask => "-2",
                _ => "0",
            };
            engine
                .act(
                    Action::SetQuantityExact {
                        target: uid.into(),
                        amount: amount.into(),
                    },
                    None,
                )
                .await?;
            if operation == Operation::MoveTask {
                let task = lessons::concept(engine, "task", Vec::new()).await?;
                let next = lessons::concept(engine, "next", Vec::new()).await?;
                for predicate in [task, next] {
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
                }
            }
            board
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
