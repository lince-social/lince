use super::*;
use nucleus::simulation::sharing::{Assumption, Kind, SelectedResult, Shared};
use simulation::scenario::{Event, Scenario};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Form {
    assumptions: Vec<usize>,
    coverage: Vec<usize>,
    preview: Option<String>,
}

impl Form {
    pub(super) fn valid(&self) -> bool {
        self.assumptions.len() <= 64
            && self.coverage.len() <= 64
            && self
                .preview
                .as_deref()
                .is_none_or(|body| Shared::parse(body).is_some())
    }
}

#[derive(Clone)]
pub(super) enum Command {
    Assumption(usize),
    Coverage(usize),
    Preview,
    Discuss,
}

fn button(world: &mut World, parent: Entity, owner: Entity, label: &str, command: Command) {
    crate::castle_feed::button(
        world,
        parent,
        owner,
        label,
        super::Command::Sharing(command),
    );
}

pub(super) fn render(world: &mut World, owner: Entity) {
    let model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let view = world.get::<View>(owner).unwrap();
    let parent = view.sharing;
    let saved = view.bundle.is_some();
    let scenario = view
        .bundle
        .as_ref()
        .map(|bundle| bundle.scenario.clone())
        .or_else(|| serde_json::from_str::<Scenario>(&model.scenario).ok());
    let coverage = view
        .bundle
        .as_ref()
        .map(|bundle| bundle.result.coverage.clone())
        .unwrap_or_default();
    world.entity_mut(parent).despawn_children();
    crate::edit_mode::label(
        world,
        parent,
        if saved {
            "Share selected information from the saved run"
        } else {
            "Share selected assumptions"
        },
        14.0,
    );
    if let Some(scenario) = scenario {
        for (index, input) in scenario.inputs.iter().enumerate() {
            if let Event::AssumeTransfer { assumption } = &input.event
                && assumption.source.is_some()
            {
                button(
                    world,
                    parent,
                    owner,
                    &format!(
                        "{} {} · {}",
                        if model.sharing.assumptions.contains(&index) {
                            "☑"
                        } else {
                            "☐"
                        },
                        assumption.title,
                        assumption.quantity
                    ),
                    Command::Assumption(index),
                );
            }
        }
    }
    for (index, result) in coverage.iter().enumerate() {
        button(
            world,
            parent,
            owner,
            &format!(
                "{} {} · {:?} · {:?}",
                if model.sharing.coverage.contains(&index) {
                    "☑"
                } else {
                    "☐"
                },
                result.check,
                result.status,
                result.kind
            ),
            Command::Coverage(index),
        );
    }
    button(
        world,
        parent,
        owner,
        "Preview exactly what will be shared",
        Command::Preview,
    );
    if let Some(preview) = &model.sharing.preview {
        crate::edit_mode::label(world, parent, preview, 12.0);
        button(
            world,
            parent,
            owner,
            "Open Transfer discussion",
            Command::Discuss,
        );
    }
}

pub(super) fn apply(command: &Command, world: &mut World, owner: Entity) -> simulation::Result<()> {
    let mut model = world.get::<SimulationCastle>(owner).unwrap().clone();
    match command {
        Command::Assumption(index) | Command::Coverage(index) => {
            let selected = if matches!(command, Command::Assumption(_)) {
                &mut model.sharing.assumptions
            } else {
                &mut model.sharing.coverage
            };
            if selected.contains(index) {
                selected.retain(|value| value != index);
            } else {
                selected.push(*index);
            }
            model.sharing.preview = None;
        }
        Command::Preview => {
            let view = world.get::<View>(owner).unwrap();
            let scenario = view
                .bundle
                .as_ref()
                .map(|bundle| bundle.scenario.clone())
                .map_or_else(|| serde_json::from_str::<Scenario>(&model.scenario), Ok)?;
            let mut assumptions = Vec::new();
            let mut binding = None;
            for index in &model.sharing.assumptions {
                let input = scenario
                    .inputs
                    .get(*index)
                    .ok_or("Select an existing assumption")?;
                let Event::AssumeTransfer { assumption } = &input.event else {
                    return Err("Select a Transfer assumption".into());
                };
                let owner = (&input.cell, &assumption.person);
                if binding.is_some_and(|binding| binding != owner) {
                    return Err("Choose assumptions for one owner at a time".into());
                }
                binding = Some(owner);
                assumptions.push(Assumption {
                    source: assumption
                        .source
                        .clone()
                        .ok_or("Share public terms after the draft exists")?,
                    after_ms: input.at_ms - scenario.start_ms,
                    quantity: assumption.quantity,
                });
            }
            let source = assumptions
                .first()
                .map(|assumption| &assumption.source)
                .or(model.transfer_form.source.as_ref())
                .ok_or("Choose a Transfer assumption to identify the discussion")?;
            let result = if model.sharing.coverage.is_empty() {
                None
            } else {
                let bundle = view
                    .bundle
                    .as_ref()
                    .ok_or("Inspect a saved run before sharing results")?;
                let mut coverage = Vec::new();
                for index in &model.sharing.coverage {
                    let mut result = bundle
                        .result
                        .coverage
                        .get(*index)
                        .ok_or("Choose an existing check result")?
                        .clone();
                    result.check = format!("Selected check {}", coverage.len() + 1);
                    coverage.push(result);
                }
                Some(SelectedResult {
                    verdict: bundle.result.verdict.clone(),
                    finished: bundle.result.stop == (nucleus::simulation::Stop::HorizonReached {}),
                    stopped_at_ms: bundle.result.stopped_at_ms,
                    selected_coverage: coverage,
                })
            };
            let shared = Shared {
                kind: Kind::TransferSimulation,
                id: nucleus::new_uid("shared-simulation"),
                transfer: source.transfer.clone(),
                revision: source.revision,
                source_at_ms: scenario.start_ms,
                duration_ms: scenario.end_ms - scenario.start_ms,
                assumptions,
                result,
            };
            if !shared.valid() {
                return Err("Select information for one Transfer revision at a time".into());
            }
            model.sharing.preview = Some(serde_json::to_string_pretty(&shared)?);
        }
        Command::Discuss => {
            let body = model
                .sharing
                .preview
                .as_ref()
                .ok_or("Preview the selected fields first")?;
            let shared = Shared::parse(body).ok_or("Shared information is invalid")?;
            let root = world
                .get::<ChildOf>(owner)
                .ok_or("Workspace unavailable")?
                .parent();
            let workspace = world
                .get::<WorkspaceMember>(owner)
                .ok_or("Workspace unavailable")?
                .0;
            let position = world
                .get::<crate::canvas::CanvasItem>(owner)
                .ok_or("Canvas unavailable")?
                .position
                + DVec2::new(1040.0, 0.0);
            crate::transfer_castle::spawn(
                world,
                root,
                workspace,
                position,
                crate::transfer_castle::TransferCastle {
                    selected: shared.transfer,
                    person: model.transfer_form.person.clone(),
                    shared_simulation: Some(body.clone()),
                    ..Default::default()
                },
            );
        }
    }
    world.entity_mut(owner).insert(model);
    render(world, owner);
    Ok(())
}
