mod checks_ui;
mod limits_ui;
mod cycles_ui;
mod runner;
mod sharing_ui;
pub(crate) mod transfer_entry;
mod transfers_ui;

use crate::{actions::Action, workspace::WorkspaceMember};
use bevy::{math::DVec2, prelude::*, text::EditableText};
use runner::{Control, Job, Outcome, Progress};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SimulationCastle {
    pub scenario: String,
    pub current_database: bool,
    pub last_run: Option<String>,
    pub source_directory: String,
    pub output_directory: String,
    pub run_directory: String,
    pub selected_cell: String,
    pub search_count: String,
    pub through: String,
    check_form: checks_ui::Form,
    transfer_form: transfers_ui::Form,
    pending_transfers: Vec<transfers_ui::Form>,
    sharing: sharing_ui::Form,
}

impl Default for SimulationCastle {
    fn default() -> Self {
        Self {
            scenario: serde_json::to_string_pretty(&simulation::fixtures::daily()).unwrap(),
            current_database: false,
            last_run: None,
            source_directory: ".".into(),
            output_directory: "simulation-runs".into(),
            run_directory: String::new(),
            selected_cell: "a".into(),
            search_count: "1".into(),
            through: "2030-01-03".into(),
            check_form: Default::default(),
            transfer_form: Default::default(),
            pending_transfers: Vec::new(),
            sharing: Default::default(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Sources,
    Output,
    Run,
    Cell,
    Count,
    Through,
}

#[derive(Component)]
struct Input {
    owner: Entity,
    field: Field,
}

#[derive(Clone, Copy)]
enum Evidence {
    Result,
    Cycles,
    Timeline,
    Events,
    Findings,
    Scenario,
}

#[derive(Component)]
struct View {
    checks: Entity,
    limits: Entity,
    transfers: Entity,
    sharing: Entity,
    editor: Entity,
    status: Entity,
    evidence: Entity,
    task: Option<tokio::task::JoinHandle<()>>,
    completion: Option<tokio::sync::oneshot::Receiver<Result<Outcome, String>>>,
    controls: Option<tokio::sync::mpsc::UnboundedSender<Control>>,
    progress: Option<tokio::sync::watch::Receiver<Progress>>,
    bundle: Option<simulation::artifacts::Bundle>,
    showing: Evidence,
    page: usize,
}

impl Drop for View {
    fn drop(&mut self) {
        if let Some(execution) = self.progress.as_ref().and_then(|progress| progress.borrow().execution.clone()) {
            execution.request_cancel();
        }
        if let Some(controls) = &self.controls {
            let _ = controls.send(Control::Stop);
        } else if let Some(task) = &self.task {
            task.abort();
        }
    }
}

pub struct SimulationCastlePlugin;
impl Plugin for SimulationCastlePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, receive).add_systems(
            PostUpdate,
            capture
                .after(bevy::text::EditableTextSystems)
                .before(crate::actions::ApplyActions),
        );
    }
}

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    model: SimulationCastle,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(1000.0, 800.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    crate::scroll_sand::attach(world, owner);
    crate::edit_mode::label(world, owner, "Simulation", 22.0);
    let presets = row(world, owner);
    for (label, command) in [
        ("Daily Karma", Command::Daily),
        ("Four Organs", Command::Network(false)),
        ("Two Organs", Command::Network(true)),
        ("Transfer", Command::Transfer),
        ("Current database", Command::Current),
    ] {
        crate::castle_feed::button(world, presets, owner, label, command);
    }
    for (label, field, value) in [
        (
            "Seed and case folder",
            Field::Sources,
            &model.source_directory,
        ),
        ("Seed Cell", Field::Cell, &model.selected_cell),
        ("Save runs in", Field::Output, &model.output_directory),
        ("Saved run", Field::Run, &model.run_directory),
        ("Search cases", Field::Count, &model.search_count),
        (
            "Run through (UTC date, timestamp or milliseconds)",
            Field::Through,
            &model.through,
        ),
    ] {
        let line = row(world, owner);
        crate::edit_mode::label(world, line, label, 13.0);
        let editor = world
            .spawn(crate::sand::text_editor(
                value,
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .insert((
                ChildOf(line),
                Input { owner, field },
                Node {
                    width: px(420),
                    flex_shrink: 0.0,
                    ..default()
                },
            ))
            .id();
        let mut text = world.get_mut::<EditableText>(editor).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(4096);
    }
    let setup = row(world, owner);
    for (label, command) in [
        ("Use Lingua seed", Command::Lingua),
        ("Check setup", Command::Validate),
        ("Save case", Command::Save),
        ("Run case folder", Command::Cases),
        ("Resume search", Command::Search),
        ("Compare without Transfer assumptions", Command::Compare),
    ] {
        crate::castle_feed::button(world, setup, owner, label, command);
    }
    crate::edit_mode::label(
        world,
        owner,
        "Cells, seeds, events, checks and limits",
        14.0,
    );
    let editor = world
        .spawn(crate::sand::text_editor(
            &model.scenario,
            world.resource::<crate::theme::Typography>(),
            0,
        ))
        .insert((
            Node {
                width: percent(100),
                height: px(280),
                flex_shrink: 0.0,
                overflow: Overflow::clip(),
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    {
        let mut text = world.get_mut::<EditableText>(editor).unwrap();
        text.max_characters = Some(1024 * 1024);
        text.allow_newlines = true;
        text.visible_lines = Some(10.0);
    }
    let checks = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    let limits = row(world, owner);
    let controls = row(world, owner);
    let transfers = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    for (label, command) in [
        ("Run / Resume", Command::Run),
        ("Pause", Command::Pause),
        ("Next event", Command::Step),
        ("Run through", Command::Through),
        ("Stop", Command::Stop),
        ("Replay", Command::Replay),
        ("Inspect", Command::Inspect),
        ("Inspect other comparison run", Command::InspectOther),
        ("Reduce failure", Command::Reduce),
    ] {
        crate::castle_feed::button(world, controls, owner, label, command);
    }
    let status = crate::edit_mode::label(
        world,
        owner,
        "Saved setup. Press Run or Next event to start.",
        13.0,
    );
    let browsing = row(world, owner);
    for (label, command) in [
        ("Result", Command::Evidence(Evidence::Result)),
        ("Rule cycles", Command::Evidence(Evidence::Cycles)),
        ("Changes", Command::Evidence(Evidence::Events)),
        ("Quantity timeline", Command::Evidence(Evidence::Timeline)),
        ("Failures", Command::Evidence(Evidence::Findings)),
        ("Saved scenario", Command::Evidence(Evidence::Scenario)),
        ("Previous page", Command::Page(false)),
        ("Next page", Command::Page(true)),
    ] {
        crate::castle_feed::button(world, browsing, owner, label, command);
    }
    let evidence = crate::edit_mode::label(world, owner, "Run evidence will appear here.", 13.0);
    world.get_mut::<Node>(evidence).unwrap().flex_shrink = 0.0;
    let sharing = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(owner),
        ))
        .id();
    world.entity_mut(owner).insert((
        model,
        View {
            checks,
            limits,
            transfers,
            sharing,
            editor,
            status,
            evidence,
            task: None,
            completion: None,
            controls: None,
            progress: None,
            bundle: None,
            showing: Evidence::Result,
            page: 0,
        },
    ));
    checks_ui::render(world, owner);
    limits_ui::render(world, owner);
    transfers_ui::render(world, owner);
    sharing_ui::render(world, owner);
    owner
}

pub(crate) fn open_current(world: &mut World, root: Entity, record: Option<&str>) {
    let now = nucleus::execution::now().timestamp_millis();
    let scenario = simulation::fixtures::current_database(now);
    let model = SimulationCastle {
        scenario: serde_json::to_string_pretty(&scenario).unwrap(),
        current_database: true,
        selected_cell: "current".into(),
        check_form: checks_ui::Form::target("current", record.unwrap_or_default()),
        through: chrono::DateTime::from_timestamp_millis(scenario.end_ms)
            .unwrap()
            .to_rfc3339(),
        ..Default::default()
    };
    let workspace = world
        .get::<crate::workspace::Workspaces>(root)
        .map_or(1, |spaces| spaces.active);
    let position = world
        .get::<crate::canvas::CanvasView>(root)
        .map_or(DVec2::ZERO, |view| view.center);
    spawn(world, root, workspace, position, model);
}

fn capture(world: &mut World) {
    transfers_ui::capture(world);
    checks_ui::capture(world);
    let values: Vec<_> = world
        .query::<(Entity, &View)>()
        .iter(world)
        .filter_map(|(owner, view)| {
            Some((
                owner,
                world.get::<EditableText>(view.editor)?.value().to_string(),
            ))
        })
        .collect();
    for (owner, value) in values {
        if let Some(mut model) = world.get_mut::<SimulationCastle>(owner) {
            model.scenario = value;
        }
    }
    let values: Vec<_> = world
        .query::<(&Input, &EditableText)>()
        .iter(world)
        .map(|(input, text)| (input.owner, input.field, text.value().to_string()))
        .collect();
    for (owner, field, value) in values {
        if let Some(mut model) = world.get_mut::<SimulationCastle>(owner) {
            match field {
                Field::Sources => model.source_directory = value,
                Field::Output => model.output_directory = value,
                Field::Run => model.run_directory = value,
                Field::Cell => model.selected_cell = value,
                Field::Count => model.search_count = value,
                Field::Through => model.through = value,
            }
        }
    }
}

fn update_input(world: &mut World, owner: Entity, field: Field, value: &str) {
    let entity = world
        .query::<(Entity, &Input)>()
        .iter(world)
        .find(|(_, input)| input.owner == owner && input.field == field)
        .map(|(entity, _)| entity);
    if let Some(entity) = entity {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(value);
    }
}

fn receive(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .collect();
    for owner in owners {
        let mut view = world.get_mut::<View>(owner).unwrap();
        let progress = view.progress.as_mut().and_then(|progress| {
            progress
                .has_changed()
                .ok()
                .filter(|changed| *changed)
                .map(|_| progress.borrow_and_update().clone())
        });
        let completion = view
            .completion
            .as_mut()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => None,
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    Some(Err("Interrupted; unfinished runs remain incomplete.".into()))
                }
            });
        let status = view.status;
        let evidence = view.evidence;
        if let Some(completion) = completion {
            view.task.take();
            view.completion.take();
            view.controls.take();
            view.progress.take();
            match completion {
                Ok(outcome) => {
                    view.bundle = outcome.bundle;
                    view.page = 0;
                    view.showing = Evidence::Result;
                    world.get_mut::<Text>(status).unwrap().0 = outcome.status;
                    world.get_mut::<Text>(evidence).unwrap().0 = outcome.evidence;
                    if let Some(path) = outcome.path {
                        let mut model = world.get_mut::<SimulationCastle>(owner).unwrap();
                        model.last_run = Some(path.clone());
                        model.run_directory = path.clone();
                        update_input(world, owner, Field::Run, &path);
                    }
                    if let Some(set) = outcome.check_set {
                        let mut model = world.get::<SimulationCastle>(owner).unwrap().clone();
                        if let Ok(mut scenario) =
                            serde_json::from_str::<simulation::scenario::Scenario>(&model.scenario)
                        {
                            scenario.checks = set.checks.clone();
                            model.scenario = serde_json::to_string_pretty(&scenario).unwrap();
                            model.check_form.set_name = set.name.clone();
                            model.check_form.saved = Some(set);
                            let editor = world.get::<View>(owner).unwrap().editor;
                            world
                                .get_mut::<EditableText>(editor)
                                .unwrap()
                                .editor
                                .set_text(&model.scenario);
                            world.entity_mut(owner).insert(model);
                            checks_ui::render(world, owner);
                        }
                    }
                    show_evidence(world, owner);
                    world.get_mut::<SimulationCastle>(owner).unwrap().sharing = Default::default();
                    sharing_ui::render(world, owner);
                }
                Err(error) => world.get_mut::<Text>(status).unwrap().0 = error,
            }
        } else if let Some(progress) = progress {
            world.get_mut::<Text>(status).unwrap().0 = progress.status;
            world.get_mut::<Text>(evidence).unwrap().0 = progress.evidence;
        }
    }
}

fn show_evidence(world: &mut World, owner: Entity) {
    let mut view = world.get_mut::<View>(owner).unwrap();
    let Some(bundle) = &view.bundle else {
        return;
    };
    let count = match view.showing {
        Evidence::Events => bundle.events.len(),
        Evidence::Timeline => bundle
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event.observation,
                    nucleus::simulation::Observation::CommittedQuantity { .. } | nucleus::simulation::Observation::LoanQuantity {..}
                )
            })
            .count(),
        Evidence::Findings => bundle.findings.len(),
        Evidence::Cycles => bundle.result.cycles.len(),
        Evidence::Result | Evidence::Scenario => 1,
    };
    let pages = count.max(1).div_ceil(20);
    view.page = view.page.min(pages - 1);
    let page = view.page;
    let entity = view.evidence;
    let range = (page * 20).min(count)..((page + 1) * 20).min(count);
    let bundle = view.bundle.as_ref().unwrap();
    let value = match view.showing {
        Evidence::Events => serde_json::to_string_pretty(&bundle.events[range]),
        Evidence::Timeline => Ok(bundle.events.iter().filter_map(|event| {
            if let nucleus::simulation::Observation::CommittedQuantity { record, before, after, at_ms, .. } | nucleus::simulation::Observation::LoanQuantity {record,before,after,at_ms,..} = &event.observation {
                let measure = if matches!(&event.observation,nucleus::simulation::Observation::LoanQuantity {..}) {"available"} else {"stored"};
                Some(format!("{} · {} · {} · {measure}: {} → {}{}", chrono::DateTime::from_timestamp_millis(*at_ms).map_or_else(|| at_ms.to_string(), |at| at.to_rfc3339()), event.cell, record.as_str(), before.value, after.value, after.unit.as_ref().map_or(String::new(), |unit| format!(" {}", unit.as_str()))))
            } else { None }
        }).skip(range.start).take(range.len()).collect::<Vec<_>>().join("\n")),
        Evidence::Findings => serde_json::to_string_pretty(&bundle.findings[range]),
        Evidence::Cycles => Ok(if bundle.result.cycles.is_empty() {
            "No Rule cycles were observed during the covered period.".into()
        } else {
            bundle.result.cycles[range].iter().map(cycles_ui::describe).collect::<Vec<_>>().join("\n\n")
        }),
        Evidence::Result => serde_json::to_string_pretty(&serde_json::json!({"result":bundle.result,"checks":bundle.scenario.checks,"cost":bundle.cost,"build":bundle.manifest.build,"sources":bundle.manifest.sources})),
        Evidence::Scenario => serde_json::to_string_pretty(&bundle.scenario),
    }.unwrap();
    let text = format!("Page {} of {pages}\n{value}", page + 1,);
    world.get_mut::<Text>(entity).unwrap().0 = text;
}

#[derive(Clone)]
enum Command {
    Checks(checks_ui::Command),
    Transfers(transfers_ui::Command),
    Sharing(sharing_ui::Command),
    Create,
    Run,
    Pause,
    Step,
    Through,
    Stop,
    Replay,
    Inspect,
    InspectOther,
    Reduce,
    Search,
    Cases,
    Compare,
    Daily,
    Network(bool),
    Transfer,
    Current,
    Lingua,
    Validate,
    Save,
    Evidence(Evidence),
    Page(bool),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            let Some(workspace) = world
                .get::<crate::workspace::Workspaces>(owner)
                .map(|spaces| spaces.active)
            else {
                return;
            };
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |view| view.center);
            spawn(
                world,
                owner,
                workspace,
                position,
                SimulationCastle::default(),
            );
            return;
        }
        capture(world);
        let Some(view) = world.get::<View>(owner) else {
            return;
        };
        let status = view.status;
        if let Err(error) = apply(self, world, owner) {
            world.get_mut::<Text>(status).unwrap().0 = error.to_string();
        }
    }
}

fn apply(command: &Command, world: &mut World, owner: Entity) -> simulation::Result<()> {
    let model = world.get::<SimulationCastle>(owner).unwrap().clone();
    let control = match command {
        Command::Run => Some(Control::Run),
        Command::Pause => Some(Control::Pause),
        Command::Step => Some(Control::Step),
        Command::Stop => Some(Control::Stop),
        Command::Through => Some(Control::Through(runner::through(&model.through)?)),
        _ => None,
    };
    let view = world.get::<View>(owner).unwrap();
    let status = view.status;
    let editor = view.editor;
    if let Command::Evidence(evidence) = command {
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.showing = *evidence;
        view.page = 0;
        show_evidence(world, owner);
        return Ok(());
    }
    if let Command::Page(forward) = command {
        let mut view = world.get_mut::<View>(owner).unwrap();
        view.page = if *forward {
            view.page.saturating_add(1)
        } else {
            view.page.saturating_sub(1)
        };
        show_evidence(world, owner);
        return Ok(());
    }
    if let Some(task) = &view.task {
        if let Some(control) = control {
            if let Some(execution) = view.progress.as_ref().and_then(|progress| progress.borrow().execution.clone()) {
                match control {
                    Control::Pause => execution.pause(),
                    Control::Stop => execution.request_cancel(),
                    _ => execution.resume(),
                }
            }
            if let Some(sender) = &view.controls {
                sender.send(control)?;
            } else if control == Control::Stop {
                task.abort();
            } else {
                return Err("Pause and stepping are available for a single scenario run".into());
            }
        }
        return Ok(());
    }
    if let Command::Checks(command) = command
        && checks_ui::apply(command, world, owner)?
    {
        return Ok(());
    }
    if matches!(command, Command::Pause | Command::Stop) {
        return Ok(());
    }
    if let Command::Transfers(command) = command {
        return transfers_ui::apply(*command, world, owner);
    }
    if let Command::Sharing(command) = command {
        return sharing_ui::apply(command, world, owner);
    }
    if matches!(
        command,
        Command::Daily | Command::Network(_) | Command::Transfer | Command::Current
    ) {
        let scenario = match command {
            Command::Network(two) => simulation::fixtures::network(*two),
            Command::Transfer => simulation::fixtures::transfer::sale(),
            Command::Current => {
                simulation::fixtures::current_database(nucleus::execution::now().timestamp_millis())
            }
            _ => simulation::fixtures::daily(),
        };
        let selected = scenario.cells[0].name.clone();
        let through = chrono::DateTime::from_timestamp_millis(
            (scenario.start_ms + 2 * 86_400_000).min(scenario.end_ms),
        )
        .ok_or("invalid scenario end")?
        .to_rfc3339();
        let value = serde_json::to_string_pretty(&scenario)?;
        world
            .get_mut::<EditableText>(editor)
            .unwrap()
            .editor
            .set_text(&value);
        let mut model = world.get_mut::<SimulationCastle>(owner).unwrap();
        model.scenario = value;
        model.current_database = matches!(command, Command::Current);
        model.selected_cell = selected.clone();
        model.check_form = checks_ui::Form::target(&selected, "");
        model.through = through.clone();
        model.transfer_form = Default::default();
        model.pending_transfers.clear();
        model.sharing = Default::default();
        update_input(world, owner, Field::Cell, &selected);
        update_input(world, owner, Field::Through, &through);
        limits_ui::render(world, owner);
        checks_ui::render(world, owner);
        transfers_ui::render(world, owner);
        sharing_ui::render(world, owner);
        world.get_mut::<Text>(status).unwrap().0 =
            "Setup prepared. Nothing runs until Run or Next event is pressed.".into();
        return Ok(());
    }
    if matches!(command, Command::Lingua | Command::Validate | Command::Save) {
        let mut scenario: simulation::scenario::Scenario = serde_json::from_str(&model.scenario)?;
        if matches!(command, Command::Lingua) {
            let files = simulation::lingua::package(std::path::Path::new(&model.source_directory))?;
            let cell = scenario
                .cells
                .iter_mut()
                .find(|cell| cell.name == model.selected_cell)
                .ok_or("Choose a Cell present in the scenario")?;
            cell.lingua = files;
            cell.database = None;
            cell.seed.clear();
            scenario.validate()?;
            let value = serde_json::to_string_pretty(&scenario)?;
            world
                .get_mut::<EditableText>(editor)
                .unwrap()
                .editor
                .set_text(&value);
            let mut model = world.get_mut::<SimulationCastle>(owner).unwrap();
            model.scenario = value;
            model.current_database = false;
            world.get_mut::<Text>(status).unwrap().0 =
                "Lingua seed prepared. Review the events and checks, then Run.".into();
        } else {
            scenario.validate()?;
            if matches!(command, Command::Save) {
                if model.current_database {
                    return Err(
                        "Run the current database setup first to save its snapshot and scenario"
                            .into(),
                    );
                }
                let path = std::path::Path::new(&model.source_directory)
                    .join(format!("{}.json", scenario.name));
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)?;
                serde_json::to_writer_pretty(&file, &scenario)?;
                file.sync_all()?;
                world.get_mut::<Text>(status).unwrap().0 =
                    format!("Case saved to {}", path.display());
            } else {
                world.get_mut::<Text>(status).unwrap().0 =
                    "Scenario structure checked. Run to check its behavior.".into();
            }
        }
        return Ok(());
    }
    let job = match command {
        Command::Checks(checks_ui::Command::Save) => Job::SaveChecks,
        Command::Checks(checks_ui::Command::Load) => Job::LoadChecks,
        Command::Checks(checks_ui::Command::List) => Job::ListChecks,
        Command::Checks(checks_ui::Command::Search) => Job::SearchSelected,
        Command::Replay => Job::Replay,
        Command::Inspect => Job::Inspect,
        Command::InspectOther => Job::InspectOther,
        Command::Reduce => Job::Reduce,
        Command::Search => Job::Search,
        Command::Cases => Job::Cases,
        Command::Compare => Job::Compare,
        _ => Job::Scenario(control.unwrap_or(Control::Run)),
    };
    let runtime = world
        .get_resource::<crate::app::CellHandle>()
        .map(|cell| cell.0.clone());
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let handle =
        tokio::runtime::Handle::try_current().map_err(|_| "Simulation runtime unavailable")?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let (controls, requests) = tokio::sync::mpsc::unbounded_channel();
    let (progress, updates) = tokio::sync::watch::channel(Progress::default());
    let mut notifications = updates.clone();
    let task = handle.spawn(async move {
        let operation = runner::run(model, runtime, job, requests, progress);
        tokio::pin!(operation);
        let mut notifying = true;
        let completion = loop {
            tokio::select! {
                result = &mut operation => break result.map_err(|error| error.to_string()),
                changed = notifications.changed(), if notifying => {
                    if changed.is_err() { notifying = false; }
                    if let Some(wake) = &wake { wake.ring(); }
                }
            }
        };
        let _ = sender.send(completion);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    let mut view = world.get_mut::<View>(owner).unwrap();
    view.task = Some(task);
    view.completion = Some(receiver);
    view.progress = Some(updates);
    view.controls = matches!(job, Job::Scenario(_) | Job::Compare).then_some(controls);
    view.bundle = None;
    world.get_mut::<Text>(status).unwrap().0 = "Preparing…".into();
    Ok(())
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Simulation Castle",
        "Prepare a seed and events, run headless Cells, and inspect or replay the result.",
        Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, SimulationCastle::default()),
    );
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedSimulation {
    pub workspace: u64,
    castle: SimulationCastle,
    position: [f64; 2],
    size: [f32; 2],
    placement: crate::sand_placement::Placement,
    tokens: crate::tokens::TokenOverrides,
}

impl SavedSimulation {
    pub(crate) fn valid(&self) -> bool {
        self.castle.check_form.valid()
            && self.castle.transfer_form.valid()
            && self.castle.pending_transfers.len() <= 64
            && self
                .castle
                .pending_transfers
                .iter()
                .all(transfers_ui::Form::valid)
            && self.castle.sharing.valid()
            && self.castle.scenario.len() <= 1024 * 1024
            && [
                &self.castle.source_directory,
                &self.castle.output_directory,
                &self.castle.run_directory,
                &self.castle.selected_cell,
                &self.castle.search_count,
                &self.castle.through,
            ]
            .iter()
            .all(|value| value.len() <= 4096)
            && self
                .castle
                .last_run
                .as_ref()
                .is_none_or(|path| path.len() <= 4096)
            && DVec2::from_array(self.position).is_finite()
            && Vec2::from_array(self.size).is_finite()
            && Vec2::from_array(self.size).min_element() > 0.0
            && self.placement.valid()
            && self.tokens.validate()
    }
    pub(crate) fn restore(self, world: &mut World, root: Entity) {
        let entity = spawn(
            world,
            root,
            self.workspace,
            DVec2::from_array(self.position),
            self.castle,
        );
        let size = Vec2::from_array(self.size);
        world
            .get_mut::<crate::canvas::CanvasItem>(entity)
            .unwrap()
            .size = size;
        self.placement.restore(world, entity);
        world
            .entity_mut(entity)
            .insert((self.tokens, crate::token_style::AppliedSize(size)));
    }
}

pub(crate) fn snapshot(world: &mut World, root: Entity) -> Vec<SavedSimulation> {
    world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            &crate::canvas::CanvasItem,
            &SimulationCastle,
        )>()
        .iter(world)
        .filter(|(_, parent, _, _, _)| parent.parent() == root)
        .map(|(entity, _, member, item, castle)| SavedSimulation {
            workspace: member.0,
            castle: castle.clone(),
            position: item.position.to_array(),
            size: item.size.to_array(),
            placement: crate::sand_placement::Placement::capture(world, entity),
            tokens: crate::token_style::overrides(world, entity),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fixture(model: SimulationCastle) -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(SimulationCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let owner = spawn(app.world_mut(), root, 1, DVec2::ZERO, model);
        (app, owner)
    }

    async fn until(app: &mut App, owner: Entity, predicate: impl Fn(&View, &str) -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(30), async {
            loop {
                app.update();
                let view = app.world().get::<View>(owner).unwrap();
                let status = &app.world().get::<Text>(view.status).unwrap().0;
                if predicate(view, status) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn check_controls_change_only_the_prepared_scenario() {
        let (mut app, owner) = fixture(SimulationCastle::default());
        let before: simulation::scenario::Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        Command::Checks(checks_ui::Command::Toggle(0)).apply(app.world_mut(), owner);
        Command::Checks(checks_ui::Command::Failure).apply(app.world_mut(), owner);
        Command::Checks(checks_ui::Command::Builtin(0)).apply(app.world_mut(), owner);
        let after: simulation::scenario::Scenario =
            serde_json::from_str(&app.world().get::<SimulationCastle>(owner).unwrap().scenario)
                .unwrap();
        assert!(!after.checks[0].options.enabled);
        assert_eq!(after.checks.len(), before.checks.len() + 1);
        assert_eq!(
            after.checking.on_failure,
            nucleus::simulation::FailureMode::Continue
        );
        assert_eq!(
            serde_json::to_value(after.inputs).unwrap(),
            serde_json::to_value(before.inputs).unwrap()
        );
        assert!(app.world().get::<View>(owner).unwrap().task.is_none());
    }

    #[test]
    fn lingua_setup_is_inert_and_exports_a_headless_case_without_changing_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("stock.lingua");
        let source = "Stock (@stock: 10) {\n}\n";
        std::fs::write(&path, source).unwrap();
        let (mut app, owner) = fixture(SimulationCastle {
            source_directory: directory.path().to_string_lossy().into(),
            ..Default::default()
        });
        Command::Lingua.apply(app.world_mut(), owner);
        let model = app.world().get::<SimulationCastle>(owner).unwrap();
        let scenario: simulation::scenario::Scenario =
            serde_json::from_str(&model.scenario).unwrap();
        assert_eq!(scenario.cells[0].lingua.len(), 1);
        assert!(scenario.cells[0].seed.is_empty());
        assert!(app.world().get::<View>(owner).unwrap().task.is_none());
        Command::Save.apply(app.world_mut(), owner);
        let saved: simulation::scenario::Scenario = serde_json::from_slice(
            &std::fs::read(directory.path().join("daily-negative.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            saved.cells[0].lingua[0].hash,
            simulation::artifacts::file_hash(&path).unwrap()
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), source);
    }

    #[tokio::test]
    async fn stepping_advancing_stopping_and_replay_work_through_castle_commands() {
        let directory = tempfile::tempdir().unwrap();
        let (mut app, owner) = fixture(SimulationCastle {
            output_directory: directory.path().to_string_lossy().into(),
            ..Default::default()
        });
        Command::Step.apply(app.world_mut(), owner);
        until(&mut app, owner, |view, status| {
            view.task.is_some() && status.starts_with("Paused") && status.contains("1 steps")
        })
        .await;
        assert!(!directory.path().join("run0001/result.json").exists());
        app.update();
        let view = app.world().get::<View>(owner).unwrap();
        assert!(
            app.world()
                .get::<Text>(view.status)
                .unwrap()
                .0
                .contains("1 steps")
        );
        Command::Through.apply(app.world_mut(), owner);
        until(&mut app, owner, |_, status| {
            status.starts_with("Paused") && status.contains("2030-01-03")
        })
        .await;
        Command::Stop.apply(app.world_mut(), owner);
        until(&mut app, owner, |view, _| view.task.is_none()).await;
        let view = app.world().get::<View>(owner).unwrap();
        let bundle = view.bundle.as_ref().unwrap();
        assert_eq!(bundle.result.stop, nucleus::simulation::Stop::Cancelled {});
        assert_eq!(
            bundle.result.verdict,
            nucleus::simulation::Verdict::Inconclusive
        );
        Command::Evidence(Evidence::Events).apply(app.world_mut(), owner);
        let view = app.world().get::<View>(owner).unwrap();
        assert!(
            app.world()
                .get::<Text>(view.evidence)
                .unwrap()
                .0
                .contains("sequence")
        );
        Command::Replay.apply(app.world_mut(), owner);
        until(&mut app, owner, |view, _| view.task.is_none()).await;
        let view = app.world().get::<View>(owner).unwrap();
        assert!(
            app.world()
                .get::<Text>(view.status)
                .unwrap()
                .0
                .contains("Verified")
        );
    }

    #[test]
    fn setup_and_restore_are_inert_and_keep_the_authored_scenario() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins(SimulationCastlePlugin);
        let root = app
            .world_mut()
            .spawn(crate::workspace::Workspaces::default())
            .id();
        let model = SimulationCastle::default();
        let expected = model.scenario.clone();
        let owner = spawn(app.world_mut(), root, 1, DVec2::ZERO, model);
        app.update();
        assert!(app.world().get::<View>(owner).unwrap().task.is_none());
        let saved = snapshot(app.world_mut(), root).pop().unwrap();
        assert_eq!(saved.castle.scenario, expected);
        app.world_mut().despawn(owner);
        saved.restore(app.world_mut(), root);
        app.update();
        let (castle, view) = app
            .world_mut()
            .query::<(&SimulationCastle, &View)>()
            .single(app.world())
            .unwrap();
        assert_eq!(castle.scenario, expected);
        assert!(view.task.is_none());
        assert!(view.completion.is_none());
    }
}
