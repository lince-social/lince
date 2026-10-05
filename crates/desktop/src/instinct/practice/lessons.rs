use super::*;
use engine::actions::Action as CellAction;
use serde_json::{Value, json};

pub(super) struct Pending {
    slug: &'static str,
    operation: Operation,
    receiver: Mutex<mpsc::Receiver<Result<Value, String>>>,
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    match operation {
        Operation::CreateWorkspace => {
            if world
                .get::<Practice>(root)
                .unwrap()
                .extra_workspaces
                .is_empty()
            {
                crate::workspace::create(world, root);
                let created = world.get::<Workspaces>(root).unwrap().active;
                if created == workspace {
                    return Err("The example workspace could not be created.".into());
                }
                crate::workspace::rename(world, root, "Instinct example");
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .extra_workspaces
                    .push(created);
            }
            crate::workspace::switch(world, root, workspace);
        }
        Operation::FrameCanvas => {
            pair(world, root);
            world
                .get_mut::<crate::canvas::CanvasView>(root)
                .unwrap()
                .set_zoom(0.8);
            crate::canvas_controls::CanvasAction::Recenter.apply(world, root);
        }
        Operation::MoveSand => {
            pair(world, root);
            crate::edit_mode::EditAction::Open.apply(world, root);
            let sample = find(world, root, Role::Square).unwrap();
            crate::topology::set_position(world, sample, DVec3::new(120.0, 0.0, 80.0));
            world.get_mut::<CanvasItem>(sample).unwrap().size = Vec2::new(280.0, 200.0);
            if world.get::<crate::sand_placement::Pinned>(sample).is_none() {
                crate::sand_placement::PlacementAction::Pin.apply(world, sample);
            }
        }
        Operation::EditText => {
            pair(world, root);
            let text = find(world, root, Role::Text).unwrap();
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::sand_text_editor::set_text(world, root, text, "My practice note")?;
            if find(world, root, Role::Note).is_none() {
                let note = crate::sand_store::spawn_sand(
                    world,
                    root,
                    workspace,
                    crate::sand_store::SandKind::EditableText,
                    "Today I tried Lince.",
                    DVec2::new(0.0, 240.0),
                );
                own(world, root, note, Role::Note);
            }
        }
        Operation::SetAppearance | Operation::ResetAppearance => {
            pair(world, root);
            let sample = find(world, root, Role::Square).unwrap();
            let mut values = crate::token_style::overrides(world, sample);
            if operation == Operation::SetAppearance {
                values.set(
                    crate::tokens::Token::Surface,
                    crate::tokens::TokenValue::Color([44, 106, 90, 255]),
                );
            } else {
                values.0.remove(&crate::tokens::Token::Surface);
            }
            crate::token_style::set_overrides(world, sample, values);
            world
                .entity_mut(sample)
                .insert(crate::token_style::background(
                    crate::tokens::Token::Surface,
                ));
        }
        Operation::InspectShortcuts => {
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Shortcuts.apply(world, root);
        }
        Operation::ScaleArea | Operation::DisableAreaEffect | Operation::InspectArea => {
            pair(world, root);
            let owner = area(world, root, Role::Area)?;
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Areas.apply(world, root);
            crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(owner))
                .apply(world, root);
            if operation == Operation::ScaleArea {
                crate::area_effects::ui::set_scale(world, owner, 1.5);
                let sample = find(world, root, Role::Square).unwrap();
                crate::topology::set_position(world, sample, DVec3::new(360.0, 0.0, 0.0));
            } else if operation == Operation::DisableAreaEffect {
                crate::area_effects::ui::set_scale(world, owner, 1.0);
                if world.get::<InfluenceArea>(owner).unwrap().enabled {
                    crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Enabled)
                        .apply(world, root);
                }
            } else {
                crate::edit_mode::EditAction::Open.apply(world, root);
                crate::edit_mode::EditAction::Areas.apply(world, root);
                crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(owner))
                    .apply(world, root);
            }
        }
        Operation::ArrangeProtein => {
            let owner = protein(world, root)?;
            let mut area = world.get_mut::<InfluenceArea>(owner).unwrap();
            let config = area.protein.as_mut().unwrap();
            config.columns = 1;
        }
        Operation::ReadRecord
        | Operation::ApplyAssertion
        | Operation::ReadVocabulary
        | Operation::OpenRecordViews
        | Operation::ReadFacts => begin_record(world, root, operation)?,
        other => work::execute(world, root, other)?,
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    match operation {
        Operation::CreateWorkspace => {
            let practice = world.get::<Practice>(root).unwrap();
            !practice.extra_workspaces.is_empty()
                && world.get::<Workspaces>(root).is_some_and(|spaces| {
                    spaces.active == practice.workspace
                        && practice.extra_workspaces.iter().all(|id| {
                            spaces.entries.iter().any(|workspace| {
                                workspace.id == *id && workspace.name == "Instinct example"
                            })
                        })
                })
        }
        Operation::FrameCanvas => {
            find(world, root, Role::Square).is_some()
                && world
                    .get::<crate::canvas::CanvasView>(root)
                    .is_some_and(|view| {
                        view.center == DVec2::ZERO && (view.zoom - 0.8).abs() < 0.0001
                    })
        }
        Operation::MoveSand => find(world, root, Role::Square).is_some_and(|entity| {
            world
                .get::<CanvasItem>(entity)
                .is_some_and(|item| item.size == Vec2::new(280.0, 200.0))
                && world.get::<crate::sand_placement::Pinned>(entity).is_some()
        }),
        Operation::EditText => {
            find(world, root, Role::Text).is_some_and(|entity| {
                crate::sand_text::snapshot(world, entity)
                    .iter()
                    .any(|text| text.text == "My practice note")
            }) && find(world, root, Role::Note).is_some()
        }
        Operation::SetAppearance => find(world, root, Role::Square).is_some_and(|entity| {
            crate::token_style::overrides(world, entity)
                .0
                .get(&crate::tokens::Token::Surface)
                == Some(&crate::tokens::TokenValue::Color([44, 106, 90, 255]))
        }),
        Operation::ResetAppearance => find(world, root, Role::Square).is_some_and(|entity| {
            !crate::token_style::overrides(world, entity)
                .0
                .contains_key(&crate::tokens::Token::Surface)
        }),
        Operation::InspectShortcuts => world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled && world.get::<Children>(mode.panel).is_some()),
        Operation::ScaleArea => find(world, root, Role::Square).is_some_and(|entity| {
            world
                .get::<crate::area_effects::AreaScale>(entity)
                .is_some_and(|scale| (scale.0 - 1.5).abs() < 0.001)
        }),
        Operation::DisableAreaEffect => find(world, root, Role::Area).is_some_and(|entity| {
            world
                .get::<InfluenceArea>(entity)
                .is_some_and(|area| !area.enabled && area.scale == 1.0)
        }),
        Operation::InspectArea => find(world, root, Role::Area).is_some_and(|entity| {
            world
                .get::<crate::area_panel::AreaEditor>(root)
                .is_some_and(|editor| editor.selected == Some(entity))
        }),
        Operation::ArrangeProtein => {
            sample_rows(world, root).len() == 2
                && find(world, root, Role::Spawn).is_some_and(|entity| {
                    world
                        .get::<InfluenceArea>(entity)
                        .and_then(|area| area.protein.as_ref())
                        .is_some_and(|config| config.columns == 1)
                })
        }
        Operation::ReadVocabulary => find(world, root, Role::Feature)
            .is_some_and(|owner| crate::ontology::sample_visible(world, owner)),
        Operation::ReadRecord
        | Operation::ApplyAssertion
        | Operation::OpenRecordViews
        | Operation::ReadFacts => {
            world
                .get::<Practice>(root)
                .unwrap()
                .results
                .contains_key(&operation)
                && find(world, root, Role::Record).is_some()
        }
        other => work::native_complete(world, root, other),
    }
}

pub(super) fn begin_record(
    world: &mut World,
    root: Entity,
    operation: Operation,
) -> Result<(), String> {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.exercise.is_some() {
        return Ok(());
    }
    let runtime = world
        .resource::<crate::practice_cells::PracticeCells>()
        .cells
        .get(&practice.source)
        .ok_or("Wait for the isolated Cell, then Retry.")?
        .clone();
    let uid = practice.note.clone();
    let related = practice
        .records
        .first()
        .ok_or("The prepared Record is missing.")?
        .clone();
    let slug = practice.runner.current().unwrap().slug;
    let handle =
        tokio::runtime::Handle::try_current().map_err(|_| "The Cell runtime is unavailable.")?;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let (sender, receiver) = mpsc::channel();
    let task = handle.spawn(async move {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            record_example(&runtime.engine, operation, &uid, &related),
        )
        .await
        .map_err(|_| "The sample action timed out. Retry, Skip or Close.".to_owned())
        .and_then(|result| result.map_err(|error| error.to_string()));
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut practice = world.get_mut::<Practice>(root).unwrap();
    practice.tasks.push(task);
    practice.exercise = Some(Pending {
        slug,
        operation,
        receiver: Mutex::new(receiver),
    });
    Ok(())
}

async fn record_example(
    engine: &engine::Engine,
    operation: Operation,
    uid: &str,
    related: &str,
) -> Result<Value, engine::EngineError> {
    let draft = engine::record_creation::Draft {
        uid: uid.into(),
        head: "My practice note".into(),
        body: "Today I tried Lince.".into(),
        slug: Some("instinct-note".into()),
        quantity: "-1".into(),
        ..default()
    };
    if store::records::get(&engine.store.pool, uid)
        .await?
        .is_none()
    {
        engine
            .act(CellAction::CreateRecordDraft { draft }, None)
            .await?;
    }
    work::change(engine, operation, uid).await?;
    if operation == Operation::ReadRecord {
        let unit = concept(engine, "practice-session", Vec::new()).await?;
        engine
            .act(
                CellAction::SetUnit {
                    target: uid.into(),
                    unit: Some(unit),
                },
                None,
            )
            .await?;
    }
    if operation == Operation::ReadFacts {
        engine
            .act(
                CellAction::SetQuantityExact {
                    target: uid.into(),
                    amount: "0".into(),
                },
                None,
            )
            .await?;
        for (field, value) in [
            (engine::record_change::WorkField::Start, json!("2026-10-04")),
            (engine::record_change::WorkField::Due, json!("2026-10-05")),
            (engine::record_change::WorkField::Estimate, json!(30)),
        ] {
            engine
                .act(
                    CellAction::ChangeRecord {
                        request: engine::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: uid.into(),
                            mutation: engine::record_change::Mutation::Work { field, value },
                        },
                    },
                    None,
                )
                .await?;
        }
        for running in [true, false] {
            engine
                .act(
                    CellAction::ChangeRecord {
                        request: engine::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: uid.into(),
                            mutation: engine::record_change::Mutation::Timer { running },
                        },
                    },
                    None,
                )
                .await?;
        }
    }
    let threads = if operation == Operation::InspectFiote {
        match store::concepts::resolve(&engine.store.pool, "thread-of").await? {
            Some(predicate) => {
                store::assertions::subjects_pointing_to(&engine.store.pool, &predicate, uid).await?
            }
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    if operation == Operation::InspectFiote && threads.is_empty() {
        let thread = engine
            .act(
                CellAction::CreateThread {
                    target: uid.into(),
                    head: "Prepared suggestion".into(),
                },
                None,
            )
            .await?
            .created
            .ok_or_else(|| {
                engine::EngineError::Consequence("The example thread was not confirmed.".into())
            })?;
        engine.act(CellAction::CreateMessage {
            thread,
            body: "Prepared example: a Fiote could suggest organizing this task before you approve a change. This message was written for practice; no AI provider was called.".into(),
            content: Vec::new(), author: None, state: nucleus::MessageState::Finished,
            parent: None, references: Vec::new(),
        }, None).await?;
    }
    let mut concepts = Vec::new();
    if matches!(
        operation,
        Operation::ApplyAssertion | Operation::ReadVocabulary | Operation::OpenRecordViews
    ) {
        let parent = concept(engine, "note", Vec::new()).await?;
        let practice = concept(engine, "practice-note", vec![parent.clone()]).await?;
        if operation == Operation::ReadVocabulary {
            alias(engine, &practice, "pt", "nota-de-pratica").await?;
            alias(engine, &practice, "en", "sample-kind").await?;
            alias(engine, &parent, "en", "sample-kind").await?;
            if store::concepts::resolve(&engine.store.pool, "nota-de-pratica").await?
                != Some(practice.clone())
                || store::concepts::resolve(&engine.store.pool, "sample-kind")
                    .await
                    .is_ok()
            {
                return Err(engine::EngineError::Consequence(
                    "The Vocabulary example was not confirmed.".into(),
                ));
            }
        }
        engine
            .act(
                CellAction::AssertRecord {
                    subject: uid.into(),
                    predicate: practice.clone(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await?;
        engine
            .act(
                CellAction::SetIdentity {
                    subject: uid.into(),
                    predicate: Some(practice),
                },
                None,
            )
            .await?;
        let link = concept(engine, "related", Vec::new()).await?;
        engine
            .act(
                CellAction::AssertRecord {
                    subject: uid.into(),
                    predicate: link,
                    object: Some(related.into()),
                    quantity: None,
                    unit: None,
                },
                None,
            )
            .await?;
        concepts = store::concepts::list_all(&engine.store.pool).await?.into_iter().map(|row| json!({"name":row.canonical_name,"uid":row.uid,"parents":row.parents,"vocabulary":"Local"})).collect();
    }
    let query: protein::Protein = serde_json::from_value(
        json!({"source":"record","where":[{"uid_eq":uid}],"include":{"facts":{"limit":5}},"fields":["uid","slug","head","body","quantity","unit","assertions","start_date","due_date","estimate_min","work_logs"]}),
    )?;
    let records = protein::execute_for(&engine.store, &query, None).await?;
    if records.len() != 1 {
        return Err(engine::EngineError::Consequence(
            "The Cell did not project the sample.".into(),
        ));
    }
    Ok(json!({"record":records[0],"concepts":concepts}))
}

async fn alias(
    engine: &engine::Engine,
    concept: &str,
    language: &str,
    name: &str,
) -> Result<(), engine::EngineError> {
    let exists: bool = store::sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM concept_name WHERE concept_uid = ? AND lang = ? AND name = ?)",
    )
    .bind(concept)
    .bind(language)
    .bind(name)
    .fetch_one(&engine.store.pool)
    .await?;
    if !exists {
        store::concepts::add_name(&engine.store.pool, concept, language, name).await?;
    }
    Ok(())
}

pub(super) async fn concept(
    engine: &engine::Engine,
    name: &str,
    parents: Vec<String>,
) -> Result<String, engine::EngineError> {
    if let Some(uid) = store::concepts::resolve(&engine.store.pool, name).await? {
        return Ok(uid);
    }
    engine
        .act(
            CellAction::CreateConcept {
                lingua: store::linguas::LOCAL_UID.into(),
                name: name.into(),
                parents,
            },
            None,
        )
        .await?
        .created
        .ok_or_else(|| {
            engine::EngineError::Consequence("The Cell did not confirm the concept.".into())
        })
}

pub(super) fn receive(world: &mut World, root: Entity) {
    let result = world
        .get::<Practice>(root)
        .unwrap()
        .exercise
        .as_ref()
        .and_then(|pending| pending.receiver.lock().ok()?.try_recv().ok());
    let Some(result) = result else { return };
    let pending = world
        .get_mut::<Practice>(root)
        .unwrap()
        .exercise
        .take()
        .unwrap();
    if world
        .get::<Practice>(root)
        .unwrap()
        .runner
        .current()
        .map(|step| step.slug)
        != Some(pending.slug)
    {
        return;
    }
    match result {
        Ok(result) => {
            let practice = world.get::<Practice>(root).unwrap();
            let source = crate::protein_area::Source::Organ(practice.source.clone());
            let uid = practice.note.clone();
            let source_id = practice.source.clone();
            world
                .resource_mut::<crate::practice_cells::PracticeCells>()
                .records
                .entry(source_id)
                .or_default()
                .insert(uid.clone());
            let mut practice = world.get_mut::<Practice>(root).unwrap();
            if !practice.records.contains(&uid) {
                practice.records.push(uid.clone());
            }
            drop(practice);
            if !matches!(
                pending.operation,
                Operation::CompleteTask
                    | Operation::UndoTask
                    | Operation::MoveTask
                    | Operation::OpenDatedRecord
                    | Operation::SetOperation
                    | Operation::AddShaderExample
            ) && find(world, root, Role::Record).is_none()
                && let Some(entity) = crate::full_record::open(world, root, &uid, source)
            {
                own(world, root, entity, Role::Record);
            }
            if pending.operation == Operation::OpenRecordViews
                && find(world, root, Role::Assertions).is_none()
            {
                let workspace = world.get::<Practice>(root).unwrap().workspace;
                let source = crate::protein_area::Source::Organ(
                    world.get::<Practice>(root).unwrap().source.clone(),
                );
                let entity = crate::assertion_castle::spawn(
                    world,
                    root,
                    workspace,
                    DVec2::new(600.0, 0.0),
                    crate::assertion_castle::AssertionCastle::default(),
                );
                let area = world.get::<crate::castle_feed::Frame>(entity).unwrap().area;
                let config = &mut world.get_mut::<InfluenceArea>(area).unwrap().protein;
                if let Some(config) = config {
                    config.source = source;
                    config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
                }
                own(world, root, entity, Role::Assertions);
                if let Some(related) = world
                    .get::<Practice>(root)
                    .unwrap()
                    .records
                    .first()
                    .cloned()
                {
                    let source = crate::protein_area::Source::Organ(
                        world.get::<Practice>(root).unwrap().source.clone(),
                    );
                    if let Some(entity) = crate::relation_castle::spawn(world, root) {
                        if let Some(config) =
                            &mut world.get_mut::<InfluenceArea>(entity).unwrap().protein
                        {
                            config.source = source.clone();
                            config.draft.query["where"] =
                                json!([{"any":[{"uid_eq":uid},{"uid_eq":related}]}]);
                        }
                        own(world, root, entity, Role::Auxiliary);
                    }
                    if let Some(entity) = crate::full_record::open(world, root, &related, source) {
                        own(world, root, entity, Role::Auxiliary);
                    }
                }
            }
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .insert(pending.operation, result);
            work::present(world, root, pending.operation);
            render(world, root);
        }
        Err(message) => {
            world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Failed {
                ticket: None,
                message,
            };
            world.get_mut::<Practice>(root).unwrap().pending = None;
            render(world, root);
        }
    }
}
