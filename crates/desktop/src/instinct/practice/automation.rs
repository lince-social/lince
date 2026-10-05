use super::*;

pub(super) fn prepare(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.setup.is_some() || practice.records.is_empty() {
        return;
    }
    if crate::protein_area::auxiliary_sender(
        world,
        &crate::protein_area::Source::Organ(practice.source.clone()),
    )
    .is_none()
    {
        return;
    }
    match practice.runner.lesson.subject {
        "frequency" => {
            let _ = frequency(world, root);
        }
        "learn-karma" => {
            if !practice.records.contains(&practice.note) {
                if practice.exercise.is_none() {
                    let _ = lessons::begin_record(world, root, Operation::PreviewRule);
                }
            } else {
                let _ = rule(world, root);
            }
        }
        "habits" => {
            let _ = habit(world, root);
        }
        "commands" => {
            let source = practice.source.clone();
            let mut cells = world.resource_mut::<crate::practice_cells::PracticeCells>();
            if !cells.workers.contains_key(&source) {
                let worker = cells.cells[&source].engine.clone().start_effect_worker();
                cells
                    .workers
                    .insert(source, crate::practice_cells::Worker(worker));
            }
            let _ = command(world, root);
        }
        _ => {}
    }
    drive(world, root);
}

fn command(world: &mut World, root: Entity) -> Result<Entity, String> {
    if let Some(owner) = find(world, root, Role::Feature) {
        return Ok(owner);
    }
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    let owner =
        crate::karma_castle::spawn(world, root, workspace, DVec2::new(650.0, 0.0), default());
    own(world, root, owner, Role::Feature);
    crate::karma_castle::commands_ui::edit_shell(
        world,
        owner,
        "practice-number",
        "A fixed practice number",
        "printf '7\\n'",
    );
    Ok(owner)
}

fn rule(world: &mut World, root: Entity) -> Result<Entity, String> {
    if let Some(owner) = find(world, root, Role::Feature) {
        return Ok(owner);
    }
    let practice = world.get::<Practice>(root).unwrap();
    if !practice.records.contains(&practice.note) {
        return Err("Wait for the prepared note.".into());
    }
    let workspace = practice.workspace;
    let draft = lince_interface::karma::Draft {
        name: "Meet a practice Need".into(),
        slug: "practice-rule".into(),
        fields: ["@instinct-note", "<0", "@instinct-note = 0"].map(|text| {
            lince_interface::karma::FieldDraft {
                text: text.into(),
                linked: None,
            }
        }),
        ..default()
    };
    let preview = crate::karma_castle::preview_form(vec![engine::karma_preview::Input::Quantity {
        after_ms: 0,
        record: "instinct-note".into(),
        value: nucleus::DecimalValue::parse_inferred("-2").unwrap(),
    }]);
    let owner = crate::karma_castle::spawn(
        world,
        root,
        workspace,
        DVec2::new(800.0, 0.0),
        crate::karma_castle::KarmaCastle {
            draft: Some(draft),
            preview,
            ..default()
        },
    );
    own(world, root, owner, Role::Feature);
    Ok(owner)
}

fn habit(world: &mut World, root: Entity) -> Result<Entity, String> {
    if let Some(owner) = find(world, root, Role::Feature) {
        return Ok(owner);
    }
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    let owner = crate::sand_store::spawn_sand(
        world,
        root,
        workspace,
        crate::sand_store::SandKind::Square,
        "",
        DVec2::new(480.0, 0.0),
    );
    world.get_mut::<CanvasItem>(owner).unwrap().size = Vec2::new(650.0, 600.0);
    own(world, root, owner, Role::Feature);
    super::super::habit_ui::spawn(world, owner, owner);
    Ok(owner)
}

fn frequency(world: &mut World, root: Entity) -> Result<Entity, String> {
    if let Some(owner) = find(world, root, Role::Feature) {
        return Ok(owner);
    }
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    let mut draft = lince_interface::frequency::model::Draft::default();
    draft.fields[0] = "practice-clock".into();
    draft.fields[1] = "A one-minute practice clock".into();
    draft.fields[2] = "1 minute".into();
    draft.fields[3] = (chrono::Utc::now() + chrono::Duration::minutes(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let owner = crate::frequency_castle::spawn(
        world,
        root,
        workspace,
        DVec2::new(460.0, 0.0),
        crate::frequency_castle::FrequencyCastle {
            draft: Some(draft),
            ..default()
        },
    );
    own(world, root, owner, Role::Feature);
    Ok(owner)
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    match operation {
        Operation::SaveCommand => {
            let owner = command(world, root)?;
            crate::karma_castle::commands_ui::save(world, owner);
        }
        Operation::RunCommand => {
            let owner = command(world, root)?;
            let uid = crate::karma_castle::commands_ui::saved(world, owner, "practice-number")
                .ok_or("Save the prepared Command first, or Skip.")?;
            crate::karma_castle::commands_ui::run(world, owner, uid);
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .entry(operation)
                .or_default()["refresh_at"] = serde_json::json!(0);
        }
        Operation::CreateFrequency => {
            let owner = frequency(world, root)?;
            crate::frequency_castle::FrequencyAction::Save.apply(world, owner);
        }
        Operation::PreviewRule => {
            if let Ok(owner) = rule(world, root) {
                if !crate::karma_castle::ready(world, owner) {
                    return Ok(());
                }
                crate::karma_castle::preview(world, owner);
                world
                    .get_mut::<Practice>(root)
                    .unwrap()
                    .results
                    .entry(operation)
                    .or_default()["started"] = serde_json::json!(true);
            } else {
                lessons::begin_record(world, root, operation)?;
            }
        }
        Operation::RunRule => {
            let owner = rule(world, root)?;
            if crate::karma_castle::saved_rule(world, owner, "practice-rule").is_none() {
                crate::karma_castle::RuleAction::Save.apply(world, owner);
            }
            drive(world, root);
        }
        Operation::PauseRule => {
            let owner = rule(world, root)?;
            let (uid, revision, _) = crate::karma_castle::saved_rule(world, owner, "practice-rule")
                .ok_or("Save the sample Rule first, or Skip.")?;
            crate::karma_castle::RuleAction::Pause(uid.clone(), revision, true).apply(world, owner);
            crate::karma_castle::history(world, owner, &uid);
        }
        Operation::PreviewHabit | Operation::ImportHabit | Operation::CompleteHabit => {
            let owner = habit(world, root)?;
            let command = match operation {
                Operation::PreviewHabit => super::super::habit_ui::Command::Preview,
                Operation::ImportHabit => super::super::habit_ui::Command::Import,
                _ => super::super::habit_ui::Command::Complete,
            };
            command.apply(world, owner);
        }
        other => tools::execute(world, root, other)?,
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    match operation {
        Operation::SaveCommand => find(world, root, Role::Feature).is_some_and(|owner| {
            crate::karma_castle::commands_ui::saved(world, owner, "practice-number").is_some()
        }),
        Operation::RunCommand => find(world, root, Role::Feature).is_some_and(|owner| {
            crate::karma_castle::commands_ui::sampled(world, owner, "practice-number", "7")
        }),
        Operation::CreateFrequency => find(world, root, Role::Feature).is_some_and(|owner| {
            crate::frequency_castle::next_occurrence(world, owner, "practice-clock").is_some()
        }),
        Operation::PreviewRule => find(world, root, Role::Feature)
            .is_some_and(|owner| crate::karma_castle::previewed(world, owner)),
        Operation::RunRule => {
            let uid = world.get::<Practice>(root).unwrap().note.clone();
            world
                .query::<&RecordProperties>()
                .iter(world)
                .any(|properties| {
                    properties.0["uid"] == uid && properties.0["quantity"].as_str() == Some("0")
                })
        }
        Operation::PauseRule => find(world, root, Role::Feature).is_some_and(|owner| {
            crate::karma_castle::saved_rule(world, owner, "practice-rule")
                .is_some_and(|(_, _, state)| state == "paused")
        }),
        Operation::PreviewHabit => find(world, root, Role::Feature)
            .is_some_and(|owner| super::super::habit_ui::previewed(world, owner)),
        Operation::ImportHabit => find(world, root, Role::Feature)
            .is_some_and(|owner| super::super::habit_ui::imported(world, owner).is_some()),
        Operation::CompleteHabit => find(world, root, Role::Feature)
            .is_some_and(|owner| super::super::habit_ui::completed(world, owner)),
        other => tools::complete(world, root, other),
    }
}

fn drive(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.pending.is_none() {
        return;
    }
    let Some(operation) = practice.runner.current().and_then(|step| step.operation) else {
        return;
    };
    let Some(owner) = find(world, root, Role::Feature) else {
        return;
    };
    if operation == Operation::RunCommand {
        let elapsed = world
            .get::<Practice>(root)
            .unwrap()
            .started
            .elapsed()
            .as_millis() as u64;
        let refresh_at = world
            .get::<Practice>(root)
            .unwrap()
            .results
            .get(&operation)
            .and_then(|result| result["refresh_at"].as_u64());
        if refresh_at.is_some_and(|at| elapsed >= at) {
            crate::karma_castle::commands_ui::refresh(world, owner);
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .entry(operation)
                .or_default()["refresh_at"] = serde_json::json!(elapsed + 250);
        }
    }
    if operation == Operation::PreviewRule
        && (crate::karma_castle::preview_needs_refresh(world, owner)
            || !world
                .get::<Practice>(root)
                .unwrap()
                .results
                .get(&operation)
                .is_some_and(|result| result["started"] == true))
    {
        let _ = execute(world, root, operation);
    }
    if operation == Operation::RunRule
        && !world
            .get::<Practice>(root)
            .unwrap()
            .results
            .get(&operation)
            .is_some_and(|result| result["triggered"] == true)
        && crate::karma_castle::saved_rule(world, owner, "practice-rule")
            .is_some_and(|(_, _, state)| state == "active")
    {
        let uid = world.get::<Practice>(root).unwrap().note.clone();
        let source =
            crate::protein_area::Source::Organ(world.get::<Practice>(root).unwrap().source.clone());
        let binding = world
            .query::<&crate::protein_area::RecordBinding>()
            .iter(world)
            .find(|binding| binding.uid == uid && binding.source == source)
            .cloned();
        if let Some(binding) = binding
            && crate::record_binding::submit(
                world,
                &binding,
                engine::record_change::Request {
                    id: nucleus::new_uid("op"),
                    record_uid: uid,
                    mutation: engine::record_change::Mutation::Quantity { value: "-2".into() },
                },
            )
            .is_ok()
        {
            world
                .get_mut::<Practice>(root)
                .unwrap()
                .results
                .entry(operation)
                .or_default()["triggered"] = serde_json::json!(true);
        }
    }
}
