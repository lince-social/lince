use super::*;

pub(super) fn prepare(world: &mut World, root: Entity) {
    community::prepare(world, root);
    disk::prepare(world, root);
    local::prepare(world, root);
    media_practice::prepare(world, root);
    visual::prepare(world, root);
    let practice = world.get::<Practice>(root).unwrap();
    if practice.setup.is_some() || practice.records.is_empty() {
        return;
    }
    let subject = practice.runner.lesson.subject;
    let prepared_note = practice.records.contains(&practice.note);
    let preparing_note = practice.exercise.is_some();
    if subject == "simulation" {
        let _ = simulation(world, root);
    }
    if subject == "learn-fiote" {
        if !prepared_note {
            if !preparing_note {
                let _ = lessons::begin_record(world, root, Operation::InspectFiote);
            }
        } else if let Some(record) = find(world, root, Role::Record) {
            own(world, root, record, Role::Feature);
            world
                .get_mut::<InfluenceArea>(record)
                .unwrap()
                .protein
                .as_mut()
                .unwrap()
                .fiote = true;
        }
    }
    let practice = world.get::<Practice>(root).unwrap();
    if practice.pending.is_some()
        && practice.runner.current().and_then(|step| step.operation)
            == Some(Operation::InspectFiote)
    {
        let _ = execute(world, root, Operation::InspectFiote);
    }
}

fn simulation(world: &mut World, root: Entity) -> Result<Entity, String> {
    if let Some(owner) = find(world, root, Role::Feature) {
        return Ok(owner);
    }
    let practice = world.get::<Practice>(root).unwrap();
    let directory = world
        .resource::<crate::practice_cells::PracticeCells>()
        .directories
        .get(&practice.source)
        .ok_or("Wait for the practice directory.")?;
    let mut castle = crate::simulation_castle::SimulationCastle::default();
    castle.output_directory = directory
        .join("simulation-runs")
        .to_string_lossy()
        .into_owned();
    castle.source_directory = directory.to_string_lossy().into_owned();
    let owner = crate::simulation_castle::spawn(
        world,
        root,
        practice.workspace,
        DVec2::new(580.0, 0.0),
        castle,
    );
    own(world, root, owner, Role::Feature);
    Ok(owner)
}

pub(super) fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    match operation {
        Operation::InspectFiote => {
            let owner = find(world, root, Role::Feature)
                .ok_or("Wait for the prepared Fiote view, then Retry.")?;
            if !crate::fiote::session::connections_visible(world, owner) {
                crate::fiote::session::inspect_connections(world, owner);
            }
        }
        Operation::StepSimulation => {
            let owner = simulation(world, root)?;
            crate::simulation_castle::next_event(world, owner);
        }
        Operation::StopSimulation => {
            let owner = simulation(world, root)?;
            crate::simulation_castle::stop(world, owner);
        }
        other
            if matches!(
                other,
                Operation::OpenPracticeFile
                    | Operation::EditSavePracticeFile
                    | Operation::InspectLanguageTools
                    | Operation::NextDocumentPage
                    | Operation::InspectTerminal
                    | Operation::InspectFileChoices
                    | Operation::CancelFileChoices
            ) =>
        {
            local::execute(world, root, other)?
        }
        other if media_practice::handles(other) => media_practice::execute(world, root, other)?,
        other if visual::handles(other) => visual::execute(world, root, other)?,
        other => community::execute(world, root, other)?,
    }
    Ok(())
}

pub(super) fn complete(world: &mut World, root: Entity, operation: Operation) -> bool {
    let owner = find(world, root, Role::Feature);
    match operation {
        Operation::InspectFiote => {
            owner.is_some_and(|owner| crate::fiote::session::connections_visible(world, owner))
        }
        Operation::StepSimulation => {
            owner.is_some_and(|owner| crate::simulation_castle::stepped(world, owner))
        }
        Operation::StopSimulation => {
            owner.is_some_and(|owner| crate::simulation_castle::stopped(world, owner))
        }
        other
            if matches!(
                other,
                Operation::OpenPracticeFile
                    | Operation::EditSavePracticeFile
                    | Operation::InspectLanguageTools
                    | Operation::NextDocumentPage
                    | Operation::InspectTerminal
                    | Operation::InspectFileChoices
                    | Operation::CancelFileChoices
            ) =>
        {
            local::complete(world, root, other)
        }
        other if media_practice::handles(other) => media_practice::complete(world, root, other),
        other if visual::handles(other) => visual::complete(world, root, other),
        other => community::complete(world, root, other),
    }
}
