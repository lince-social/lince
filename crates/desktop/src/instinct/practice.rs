use crate::{
    actions::Action,
    area::{AreaForces, AreaShape, Direction, InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
};
use lince_interface::practice::{
    Effect, LESSONS, LearningProgress, Lesson, Mode, Observation, Operation, Phase, Runner,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, mpsc},
    time::Instant,
};

mod input;
mod lessons;
#[cfg(feature = "instinct")]
pub(crate) mod tests;
mod work;
pub(super) use input::refresh as refresh_input;

#[derive(Component)]
struct SemanticControl {
    session: u64,
    operation: Operation,
}

#[derive(Clone, Copy)]
struct Perform(Operation);

impl Action for Perform {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        crate::actions::PracticeIntent::Feature(self.0)
    }

    fn apply(&self, world: &mut World, root: Entity) {
        if world.get::<Practice>(root).is_none_or(|practice| {
            practice.pending.is_some()
                || practice.runner.current().and_then(|step| step.operation) != Some(self.0)
        }) {
            return;
        }
        if let Err(message) = execute(world, root, self.0) {
            world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Failed {
                ticket: None,
                message,
            };
            render(world, root);
        }
    }
}

pub(crate) fn visible(world: &World, root: Entity) -> bool {
    world.get::<Practice>(root).is_some_and(|practice| {
        !practice.summary
            && world
                .get::<Workspaces>(root)
                .is_some_and(|spaces| spaces.active == practice.workspace)
    })
}

pub(crate) fn permits_target(world: &World, entity: Entity) -> bool {
    let Some(root) = root(world, entity) else {
        return true;
    };
    let Some(practice) = world.get::<Practice>(root) else {
        return true;
    };
    if !practice.runner.restricted() || !visible(world, root) {
        return true;
    }
    let mut current = Some(entity);
    while let Some(entity) = current {
        if world.get::<Recovery>(entity).is_some() {
            return true;
        }
        if world
            .get::<Owned>(entity)
            .is_some_and(|owned| owned.session == practice.runner.session)
        {
            return true;
        }
        if world
            .get::<crate::protein_area::RecordBinding>(entity)
            .is_some_and(|binding| {
                binding.source == crate::protein_area::Source::Organ(practice.source.clone())
                    && practice.records.contains(&binding.uid)
            })
        {
            return true;
        }
        current = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
    false
}

pub(crate) fn permits_action(
    world: &World,
    target: Entity,
    intent: crate::actions::PracticeIntent,
) -> bool {
    use crate::actions::PracticeIntent;
    if matches!(
        intent,
        PracticeIntent::Navigation | PracticeIntent::Recovery
    ) {
        return true;
    }
    if let PracticeIntent::Object(entity) = intent {
        return permits_target(world, entity);
    }
    let Some(root) = root(world, target) else {
        return true;
    };
    let Some(practice) = world.get::<Practice>(root) else {
        return true;
    };
    if !practice.runner.restricted() || !visible(world, root) {
        return true;
    }
    if let PracticeIntent::Feature(operation) = intent {
        return practice.runner.current().and_then(|step| step.operation) == Some(operation);
    }
    permits_target(world, target)
}

#[derive(Resource, Default)]
pub(super) struct Learned(pub LearningProgress);

#[derive(Resource)]
struct Content(HashMap<String, engine::instinct::BundledRecord>);

impl FromWorld for Content {
    fn from_world(_: &mut World) -> Self {
        Self(
            engine::instinct::records()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|record| Some((record.slug.clone()?, record)))
                .collect(),
        )
    }
}

struct Prepared {
    runtime: cell::CellRuntime,
    records: Vec<String>,
}

#[derive(Component)]
pub(super) struct Practice {
    runner: Runner,
    shell: Entity,
    content: Entity,
    status: Entity,
    workspace: u64,
    previous: u64,
    source: String,
    records: Vec<String>,
    setup: Option<Mutex<mpsc::Receiver<Result<Prepared, String>>>>,
    started: Instant,
    pending: Option<lince_interface::practice::Ticket>,
    confirmed_entry: bool,
    confirmed_exit: bool,
    summary: bool,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    timer: Option<tokio::task::JoinHandle<()>>,
    extra_workspaces: Vec<u64>,
    note: String,
    exercise: Option<lessons::Pending>,
    results: HashMap<Operation, serde_json::Value>,
    previous_edit: bool,
    owned_edit_revision: Option<u64>,
}

impl Drop for Practice {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        if let Some(timer) = self.timer.take() {
            timer.abort();
        }
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
struct Owned {
    session: u64,
    role: Role,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Square,
    Text,
    Area,
    Spawn,
    Changes,
    Note,
    Record,
    Assertions,
    Feature,
}

#[derive(Component)]
pub(super) struct Recovery;

#[derive(Clone)]
pub(crate) struct StartPage {
    pub slug: String,
    pub mode: Mode,
}

fn lesson(slug: &str) -> Option<(&'static Lesson, usize)> {
    let requested = match slug {
        "learn-areas-of-influence" | "areas-of-influence" => Some((1, 0)),
        "area-forces" => Some((1, 1)),
        "protein-presentation" => Some((2, 2)),
        _ => LESSONS
            .iter()
            .position(|lesson| lesson.subject == slug)
            .map(|index| (index, 0)),
    };
    requested
        .map(|(index, step)| (&LESSONS[index], step))
        .or_else(|| {
            lince_interface::handbook::READING
                .iter()
                .find(|lesson| lesson.subject == slug)
                .map(|lesson| (lesson, 0))
        })
}

fn root(world: &World, mut owner: Entity) -> Option<Entity> {
    loop {
        if world.get::<Workspaces>(owner).is_some() {
            return Some(owner);
        }
        owner = world.get::<ChildOf>(owner)?.parent();
    }
}

impl Action for StartPage {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        crate::actions::PracticeIntent::Recovery
    }
    fn apply(&self, world: &mut World, owner: Entity) {
        if !cfg!(feature = "instinct") || crate::laboratory::suspended(world, owner) {
            return;
        }
        let Some((lesson, step)) = lesson(&self.slug) else {
            return;
        };
        let Some(root) = root(world, owner) else {
            return;
        };
        if world.get::<Practice>(root).is_some() {
            finish(world, root, false);
        }
        world.init_resource::<Content>();
        world.init_resource::<Learned>();
        if let Some(directory) = world
            .get_resource::<crate::workspace::WorkspaceFile>()
            .map(|file| file.directory().to_path_buf())
            && let Ok(bytes) = std::fs::read(directory.join("instinct-progress.json"))
            && let Ok(progress) = serde_json::from_slice(&bytes)
        {
            world.resource_mut::<Learned>().0 = progress;
        }
        world.init_resource::<crate::practice_cells::PracticeCells>();
        let previous = world.get::<Workspaces>(root).unwrap().active;
        let previous_edit = world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled);
        crate::workspace::create(world, root);
        let workspace = world.get::<Workspaces>(root).unwrap().active;
        if workspace == previous {
            crate::notifications::report(
                world,
                "Instinct",
                "Could not create a practice workspace. Reading remains available.",
            );
            return;
        }
        crate::workspace::rename(world, root, "Instinct practice");
        let session = nucleus::execution::uuid().as_u128() as u64;
        let mut runner = Runner::start(
            session,
            lesson,
            self.mode,
            world.resource::<Learned>().0.clone(),
        );
        runner.select(lesson, step);
        let shell = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(16),
                    top: px(16),
                    width: px(360),
                    max_width: percent(48),
                    max_height: Val::Vh(90.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(12)),
                    row_gap: px(8),
                    ..default()
                },
                crate::token_style::background(crate::tokens::Token::Surface),
                GlobalZIndex(120),
                ChildOf(root),
                Recovery,
            ))
            .id();
        let content = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    ..default()
                },
                ChildOf(shell),
            ))
            .id();
        world.entity_mut(root).insert(Practice {
            runner,
            shell,
            content,
            status: Entity::PLACEHOLDER,
            workspace,
            previous,
            source: nucleus::new_uid("g"),
            records: Vec::new(),
            setup: None,
            started: Instant::now(),
            pending: None,
            confirmed_entry: false,
            confirmed_exit: false,
            summary: false,
            tasks: Vec::new(),
            timer: None,
            extra_workspaces: Vec::new(),
            note: nucleus::new_uid("r"),
            exercise: None,
            results: HashMap::new(),
            previous_edit,
            owned_edit_revision: None,
        });
        let data = lesson
            .steps
            .iter()
            .filter_map(|step| step.operation)
            .any(Operation::needs_cell);
        if data {
            prepare_cell(world, root);
        }
        if lesson.subject == "castles" {
            let _ = execute(world, root, Operation::PlaceSand);
        }
        if lesson.subject == "areas-of-influence" && step > 0 {
            let _ = execute(world, root, Operation::PlaceArea);
        }
        render(world, root);
    }
}

fn prepare_cell(world: &mut World, root: Entity) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(
            "A live Cell runtime is unavailable. Skip or Close to continue reading.".into(),
        );
        return;
    };
    let (sender, receiver) = mpsc::channel();
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let task = handle.spawn(async move {
        let result = async {
            let engine = Arc::new(
                engine::Engine::open_memory()
                    .await
                    .map_err(|error| error.to_string())?,
            );
            let mut records = Vec::new();
            for index in 1..=2 {
                let draft = engine::record_creation::Draft {
                    head: format!("Instinct sample {index}"),
                    body: "Disposable practice data.".into(),
                    ..default()
                };
                let result = engine
                    .act(engine::actions::Action::CreateRecordDraft { draft }, None)
                    .await
                    .map_err(|error| error.to_string())?;
                records.push(
                    result
                        .created
                        .ok_or("The Cell did not confirm the sample Record.")?,
                );
            }
            Ok::<_, String>(Prepared {
                runtime: cell::CellRuntime {
                    commands: Default::default(),
                    store: engine.store.clone(),
                    engine,
                    lanes: Arc::new(cell::LaneHub::new()),
                    wire: Default::default(),
                    fiote: None,
                    speech: None,
                    information: None,
                },
                records,
            })
        }
        .await;
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.get_mut::<Practice>(root).unwrap().tasks.push(task);
    world.get_mut::<Practice>(root).unwrap().setup = Some(Mutex::new(receiver));
}

fn find(world: &mut World, root: Entity, role: Role) -> Option<Entity> {
    let practice = world.get::<Practice>(root)?;
    let session = practice.runner.session;
    let workspace = practice.workspace;
    let mut query = world.query::<(Entity, &Owned)>();
    let mut matches = query
        .iter(world)
        .filter(|(entity, owned)| {
            owned.session == session
                && owned.role == role
                && self::root(world, *entity) == Some(root)
                && world
                    .get::<WorkspaceMember>(*entity)
                    .is_some_and(|member| member.0 == workspace)
        })
        .map(|(entity, _)| entity);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn own(world: &mut World, root: Entity, entity: Entity, role: Role) {
    let session = world.get::<Practice>(root).unwrap().runner.session;
    world.entity_mut(entity).insert(Owned { session, role });
    if role == Role::Changes {
        let source = world.get::<Practice>(root).unwrap().source.clone();
        world
            .entity_mut(entity)
            .insert(crate::practice_cells::PracticeArea(source));
    }
}

fn pair(world: &mut World, root: Entity) {
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    for (kind, role, position) in [
        (
            crate::sand_store::SandKind::Square,
            Role::Square,
            DVec2::ZERO,
        ),
        (
            crate::sand_store::SandKind::Text,
            Role::Text,
            DVec2::new(240.0, 0.0),
        ),
    ] {
        if find(world, root, role).is_none() {
            let entity = crate::sand_store::spawn_sand(
                world,
                root,
                workspace,
                kind,
                "Instinct sample",
                position,
            );
            own(world, root, entity, role);
        }
    }
}

fn area(world: &mut World, root: Entity, role: Role) -> Result<Entity, String> {
    if let Some(entity) = find(world, root, role) {
        return Ok(entity);
    }
    let workspace = world.get::<Practice>(root).unwrap().workspace;
    let area = InfluenceArea::new(
        AreaShape::Square,
        DVec2::new(360.0, 0.0),
        DVec2::splat(320.0),
    );
    let entity = crate::area::spawn_area(world, root, workspace, area)
        .ok_or("The sample Area could not be placed.")?;
    own(world, root, entity, role);
    Ok(entity)
}

fn sample_rows(world: &mut World, root: Entity) -> Vec<(Entity, String)> {
    let practice = world.get::<Practice>(root).unwrap();
    let (workspace, source, ids) = (
        practice.workspace,
        crate::protein_area::Source::Organ(practice.source.clone()),
        practice.records.clone(),
    );
    world
        .query::<(
            Entity,
            &crate::protein_area::RecordBinding,
            &WorkspaceMember,
            &ChildOf,
        )>()
        .iter(world)
        .filter(|(_, binding, member, parent)| {
            member.0 == workspace
                && parent.parent() == root
                && binding.source == source
                && ids.contains(&binding.uid)
        })
        .map(|(entity, binding, _, _)| (entity, binding.uid.clone()))
        .collect()
}

fn protein(world: &mut World, root: Entity) -> Result<Entity, String> {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.records.len() < 2 {
        return Err("Waiting for the isolated practice Cell.".into());
    }
    let (source, ids) = (practice.source.clone(), practice.records[..2].to_vec());
    let entity = area(world, root, Role::Spawn)?;
    if world
        .get::<InfluenceArea>(entity)
        .unwrap()
        .protein
        .is_none()
    {
        let mut config = crate::protein_area::Config {
            source: crate::protein_area::Source::Organ(source),
            ..default()
        };
        config.draft.query["where"] = serde_json::json!([{"any":ids.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
        config.draft.compile()?;
        world.get_mut::<InfluenceArea>(entity).unwrap().protein = Some(config);
    }
    Ok(entity)
}

fn execute(world: &mut World, root: Entity, operation: Operation) -> Result<(), String> {
    let previous_revision = world
        .get::<crate::edit_mode::EditMode>(root)
        .map(|mode| mode.revision);
    let workspace = world
        .get::<Practice>(root)
        .ok_or("This practice session closed.")?
        .workspace;
    if world
        .get::<Workspaces>(root)
        .is_none_or(|spaces| spaces.active != workspace)
    {
        return Err("Return to the practice workspace, switch to Free or Close.".into());
    }
    match operation {
        Operation::OpenEdit => crate::edit_mode::EditAction::Open.apply(world, root),
        Operation::PlaceSand => pair(world, root),
        Operation::ComposeCastle => {
            crate::edit_mode::EditAction::Open.apply(world, root);
            pair(world, root);
            let square = find(world, root, Role::Square).unwrap();
            let text = find(world, root, Role::Text).unwrap();
            world
                .entity_mut(root)
                .insert(crate::canvas_selection::SandSelection(vec![square, text]));
            crate::canvas_selection::GroupAction::Group.apply(world, square);
            let mut rules =
                crate::layout::Rules::fixed(world.get::<CanvasItem>(square).unwrap().size);
            rules.arrangement = crate::layout::Arrangement::Row;
            crate::layout::configure(world, square, rules).map_err(str::to_owned)?;
            crate::layout::attach(world, text, square).map_err(str::to_owned)?;
        }
        Operation::UngroupCastle => {
            let square = find(world, root, Role::Square)
                .ok_or("The sample composition is missing. Retry or Skip.")?;
            let text = find(world, root, Role::Text)
                .ok_or("The sample text is missing. Retry or Skip.")?;
            crate::canvas_selection::GroupAction::Ungroup.apply(world, square);
            crate::layout::detach(world, text);
        }
        Operation::PlaceArea => {
            pair(world, root);
            area(world, root, Role::Area)?;
        }
        Operation::Attract | Operation::Repel => {
            pair(world, root);
            let entity = area(world, root, Role::Area)?;
            crate::workspace_config::set_physics(world, root, workspace, true);
            let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
            area.attraction_enabled = true;
            area.strength = 100.0;
            area.direction = if operation == Operation::Attract {
                Direction::Attract
            } else {
                Direction::Repel
            };
            drop(area);
            let sample = find(world, root, Role::Square).unwrap();
            crate::topology::set_position(world, sample, DVec3::new(420.0, 0.0, 0.0));
        }
        Operation::CreateProteinArea | Operation::PreviewProtein | Operation::PresentProperties => {
            let entity = protein(world, root)?;
            if operation == Operation::PresentProperties {
                world
                    .get_mut::<InfluenceArea>(entity)
                    .unwrap()
                    .protein
                    .as_mut()
                    .unwrap()
                    .bindings = vec![
                    crate::protein_area::Binding::new("head"),
                    crate::protein_area::Binding::new("quantity"),
                ];
            }
        }
        Operation::MatchRecord => {
            protein(world, root)?;
            let entity = area(world, root, Role::Changes)?;
            let mut value = world.get_mut::<InfluenceArea>(entity).unwrap();
            value.center = [900.0, 0.0];
            value.rules = vec![crate::area::PropertyRule {
                property: crate::area::Property::Title,
                value: "Instinct sample 1".into(),
            }];
            value.changes.enter.quantity = Some("1".into());
            value.changes.leave.quantity = Some("0".into());
            value.changes_enabled = true;
            drop(value);
            crate::topology::set_position(world, entity, DVec3::new(900.0, 0.0, 0.0));
        }
        Operation::EnterArea | Operation::LeaveArea => {
            let entity = find(world, root, Role::Changes)
                .ok_or("Prepare the sample matching Area first.")?;
            if !crate::area_mutation::armed(world, entity) {
                return Err("Wait for the property Area to be ready, then Retry.".into());
            }
            let first = world.get::<Practice>(root).unwrap().records[0].clone();
            let sample = sample_rows(world, root)
                .into_iter()
                .find(|(_, uid)| *uid == first)
                .ok_or("The sample Sand is missing. Retry its Protein setup.")?
                .0;
            let area = world.get::<InfluenceArea>(entity).unwrap();
            let position = DVec3::new(
                area.center[0]
                    + if operation == Operation::EnterArea {
                        0.0
                    } else {
                        area.size[0] + 400.0
                    },
                0.0,
                area.center[1],
            );
            crate::topology::set_position(world, sample, position);
        }
        other => lessons::execute(world, root, other)?,
    }
    if let Some(mode) = world.get::<crate::edit_mode::EditMode>(root)
        && Some(mode.revision) != previous_revision
    {
        let revision = mode.revision;
        world.get_mut::<Practice>(root).unwrap().owned_edit_revision = Some(revision);
    }
    Ok(())
}

fn observe(world: &mut World, root: Entity) -> Observation {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.setup.is_some() {
        return Observation::Waiting;
    }
    let Some(step) = practice.runner.current() else {
        return Observation::Waiting;
    };
    let Some(operation) = step.operation else {
        return Observation::Complete;
    };
    let complete = match operation {
        Operation::OpenEdit => world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled),
        Operation::PlaceSand => {
            find(world, root, Role::Square).is_some() && find(world, root, Role::Text).is_some()
        }
        Operation::ComposeCastle => {
            let square = find(world, root, Role::Square);
            let text = find(world, root, Role::Text);
            square.zip(text).is_some_and(|(square, text)| {
                world
                    .get::<crate::layout::LayoutBox>(square)
                    .zip(world.get::<crate::layout::LayoutBox>(text))
                    .is_some_and(|(parent, child)| {
                        child.parent == Some(parent.id)
                            && parent.rules.arrangement == crate::layout::Arrangement::Row
                    })
                    && world
                        .get::<crate::canvas_selection::SandGroup>(square)
                        .is_some_and(|group| {
                            world.get::<crate::canvas_selection::SandGroup>(text) == Some(group)
                        })
            })
        }
        Operation::PlaceArea => find(world, root, Role::Area).is_some(),
        Operation::UngroupCastle => find(world, root, Role::Square)
            .zip(find(world, root, Role::Text))
            .is_some_and(|(square, text)| {
                world
                    .get::<crate::canvas_selection::SandGroup>(square)
                    .is_none()
                    && world
                        .get::<crate::canvas_selection::SandGroup>(text)
                        .is_none()
                    && world
                        .get::<crate::layout::LayoutBox>(text)
                        .is_none_or(|layout| layout.parent.is_none())
            }),
        Operation::Attract | Operation::Repel => {
            let area = find(world, root, Role::Area);
            let sample = find(world, root, Role::Square);
            area.zip(sample).is_some_and(|(area, sample)| {
                world.get::<AreaForces>(sample).is_some_and(|forces| {
                    forces.0.iter().any(|force| {
                        force.area == area
                            && force.force.length_squared() > 0.0001
                            && world.get::<InfluenceArea>(area).is_some_and(|area| {
                                area.direction
                                    == if operation == Operation::Attract {
                                        Direction::Attract
                                    } else {
                                        Direction::Repel
                                    }
                            })
                    })
                })
            })
        }
        Operation::CreateProteinArea => find(world, root, Role::Spawn).is_some_and(|entity| {
            world
                .get::<InfluenceArea>(entity)
                .is_some_and(|area| area.protein.is_some())
        }),
        Operation::PreviewProtein | Operation::PresentProperties => {
            let rows = sample_rows(world, root);
            rows.len() == 2
                && (operation != Operation::PresentProperties
                    || find(world, root, Role::Spawn).is_some_and(|entity| {
                        world
                            .get::<InfluenceArea>(entity)
                            .and_then(|area| area.protein.as_ref())
                            .is_some_and(|config| {
                                ["head", "quantity"].iter().all(|property| {
                                    config
                                        .bindings
                                        .iter()
                                        .any(|binding| binding.property == *property)
                                })
                            })
                    }))
        }
        Operation::MatchRecord => find(world, root, Role::Changes)
            .is_some_and(|entity| crate::area_mutation::armed(world, entity)),
        Operation::EnterArea | Operation::LeaveArea => {
            let practice = world.get::<Practice>(root).unwrap();
            let confirmed = if operation == Operation::EnterArea {
                practice.confirmed_entry
            } else {
                practice.confirmed_exit
            };
            let first = practice.records.first().cloned();
            confirmed
                && sample_rows(world, root).iter().any(|(entity, uid)| {
                    Some(uid) == first.as_ref()
                        && world
                            .get::<RecordProperties>(*entity)
                            .is_some_and(|record| {
                                record.0["quantity"]
                                    .as_str()
                                    .and_then(|quantity| quantity.parse::<f64>().ok())
                                    .or_else(|| record.0["quantity"].as_f64())
                                    == Some(if operation == Operation::EnterArea {
                                        1.0
                                    } else {
                                        0.0
                                    })
                            })
                })
        }
        other => lessons::complete(world, root, other),
    };
    if complete {
        Observation::Complete
    } else {
        Observation::Waiting
    }
}

#[derive(Clone, Copy)]
enum Command {
    Next,
    Skip,
    Free,
    Assisted,
    Close,
    Keep,
    Discard,
    Retry,
}

impl Action for Command {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        crate::actions::PracticeIntent::Recovery
    }
    fn apply(&self, world: &mut World, root: Entity) {
        if world.get::<Practice>(root).is_none() {
            return;
        }
        match self {
            Self::Close => {
                let mut practice = world.get_mut::<Practice>(root).unwrap();
                practice.runner.close();
                practice.pending = None;
                practice.summary = true;
                if let Some(timer) = practice.timer.take() {
                    timer.abort();
                }
                for task in practice.tasks.drain(..) {
                    task.abort();
                }
                drop(practice);
                input::release(world, root);
                crate::tutorial::highlight::clear(world, root);
            }
            Self::Keep | Self::Discard => {
                finish(world, root, matches!(self, Self::Keep));
                return;
            }
            Self::Free => world
                .get_mut::<Practice>(root)
                .unwrap()
                .runner
                .switch_mode(Mode::Free),
            Self::Assisted => world
                .get_mut::<Practice>(root)
                .unwrap()
                .runner
                .switch_mode(Mode::Assisted),
            Self::Skip => {
                if let Some(timer) = world.get_mut::<Practice>(root).unwrap().timer.take() {
                    timer.abort();
                }
                world.get_mut::<Practice>(root).unwrap().pending = None;
                world.get_mut::<Practice>(root).unwrap().exercise = None;
                world.get_mut::<Practice>(root).unwrap().runner.skip();
            }
            Self::Next | Self::Retry => {
                if matches!(self, Self::Retry)
                    && matches!(
                        world.get::<Practice>(root).unwrap().runner.phase,
                        Phase::Unavailable(_)
                    )
                {
                    world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Ready;
                    if world.get::<Practice>(root).unwrap().records.is_empty()
                        && world
                            .get::<Practice>(root)
                            .unwrap()
                            .runner
                            .lesson
                            .steps
                            .iter()
                            .filter_map(|step| step.operation)
                            .any(Operation::needs_cell)
                    {
                        prepare_cell(world, root);
                    }
                }
                let observation = observe(world, root);
                let mut practice = world.get_mut::<Practice>(root).unwrap();
                let now = practice.started.elapsed().as_millis() as u64;
                let effect = practice.runner.next(observation, now, 30_000);
                if matches!(effect, Effect::Advanced | Effect::Finished) {
                    practice.pending = None;
                    practice.exercise = None;
                    if let Some(timer) = practice.timer.take() {
                        timer.abort();
                    }
                }
                if let Effect::Execute { ticket, operation } = effect {
                    practice.pending = Some(ticket);
                    drop(practice);
                    if let Ok(handle) = tokio::runtime::Handle::try_current()
                        && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned()
                    {
                        let task = handle.spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            wake.ring();
                        });
                        if let Some(timer) =
                            world.get_mut::<Practice>(root).unwrap().timer.replace(task)
                        {
                            timer.abort();
                        }
                    }
                    if let Err(message) = execute(world, root, operation) {
                        world
                            .get_mut::<Practice>(root)
                            .unwrap()
                            .runner
                            .response(ticket, Observation::Failed(message));
                    }
                }
            }
        }
        save_progress(world, root);
        render(world, root);
        input::refresh(world);
    }
}

fn save_progress(world: &mut World, root: Entity) {
    let Some(practice) = world.get::<Practice>(root) else {
        return;
    };
    let mut progress = practice.runner.progress.clone();
    let state = if practice.runner.lesson.steps.iter().all(|step| {
        progress.0.get(step.slug) == Some(&lince_interface::practice::Progress::Practiced)
    }) {
        lince_interface::practice::Progress::Practiced
    } else if practice.runner.lesson.steps.iter().any(|step| {
        progress.0.get(step.slug) == Some(&lince_interface::practice::Progress::Unavailable)
    }) {
        lince_interface::practice::Progress::Unavailable
    } else if practice.runner.lesson.steps.iter().any(|step| {
        progress.0.get(step.slug) == Some(&lince_interface::practice::Progress::Skipped)
    }) {
        lince_interface::practice::Progress::Skipped
    } else {
        lince_interface::practice::Progress::Visited
    };
    progress
        .0
        .insert(practice.runner.lesson.subject.into(), state);
    world.resource_mut::<Learned>().0 = progress.clone();
    let Some(directory) = world
        .get_resource::<crate::workspace::WorkspaceFile>()
        .map(|file| file.directory().to_path_buf())
    else {
        return;
    };
    let path = directory.join("instinct-progress.json");
    let temporary = directory.join("instinct-progress.json.tmp");
    if let Ok(bytes) = serde_json::to_vec(&progress)
        && std::fs::write(&temporary, bytes).is_ok()
    {
        let _ = std::fs::rename(temporary, path);
    }
}

fn finish(world: &mut World, root: Entity, keep: bool) {
    save_progress(world, root);
    let Some(mut practice) = world.entity_mut(root).take::<Practice>() else {
        return;
    };
    practice.runner.close();
    for task in practice.tasks.drain(..) {
        task.abort();
    }
    input::release(world, root);
    crate::tutorial::highlight::clear(world, root);
    if !practice.previous_edit
        && practice.owned_edit_revision.is_some_and(|revision| {
            world
                .get::<crate::edit_mode::EditMode>(root)
                .is_some_and(|mode| mode.enabled && mode.revision == revision)
        })
    {
        crate::edit_mode::EditAction::Close.apply(world, root);
    }
    world.despawn(practice.shell);
    let owned: Vec<_> = world
        .query::<(Entity, &Owned)>()
        .iter(world)
        .filter(|(_, owned)| owned.session == practice.runner.session)
        .map(|(entity, _)| entity)
        .collect();
    for entity in &owned {
        if world.get::<InfluenceArea>(*entity).is_some() {
            crate::area_mutation::disarm(world, *entity, "Practice ended");
        }
        if let Some(mut area) = world.get_mut::<InfluenceArea>(*entity) {
            area.changes_enabled = false;
        }
    }
    crate::workspace::switch(world, root, practice.previous);
    if !keep {
        crate::protein_area::release_practice_source(world, &practice.source);
        crate::workspace::remove(world, root, practice.workspace);
        for workspace in &practice.extra_workspaces {
            crate::workspace::remove(world, root, *workspace);
        }
        let mut cells = world.resource_mut::<crate::practice_cells::PracticeCells>();
        cells.cells.remove(&practice.source);
        let uids: Vec<_> = cells
            .records
            .iter()
            .filter(|(_, source)| **source == practice.source)
            .map(|(uid, _)| uid.clone())
            .collect();
        for uid in uids {
            cells.records.remove(&uid);
            cells.retired.insert(uid);
        }
    }
    crate::notifications::report(
        world,
        "Instinct",
        if keep {
            "Kept the practice workspace. Property actions are disabled. Its sample Cell stays separate from personal data for this run."
        } else {
            "Discarded the practice workspace and its isolated Cell. Personal Records were untouched."
        },
    );
}

fn render(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    let (content, summary, phase, mode, current, subject, count) = (
        practice.content,
        practice.summary,
        practice.runner.phase.clone(),
        practice.runner.mode,
        practice.runner.current().map(|step| step.slug),
        practice.runner.lesson.subject,
        practice.records.len(),
    );
    if let Some(children) = world.get::<Children>(content) {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    let controls = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            },
            ChildOf(content),
            Recovery,
        ))
        .id();
    for (label, action) in [
        ("Free", Command::Free),
        ("Assisted", Command::Assisted),
        ("Close", Command::Close),
    ] {
        crate::description::button(world, controls, root, label, action);
    }
    let title = world
        .resource::<Content>()
        .0
        .get(subject)
        .map_or(subject, |record| record.head.as_str())
        .to_owned();
    crate::description::heading(world, content, &title, 20.0);
    crate::edit_mode::label(
        world,
        content,
        &format!("{mode:?} · {count} isolated sample Records"),
        14.0,
    );
    if summary {
        crate::edit_mode::label(
            world,
            content,
            "Practice stopped. Restrictions are released. Keep retains this separate workspace for this run; Discard removes its samples. Submitted changes may have finished in the isolated Cell.",
            14.0,
        );
        crate::description::button(world, content, root, "Keep practice", Command::Keep);
        crate::description::button(world, content, root, "Discard practice", Command::Discard);
        return;
    }
    if let Some(slug) = current {
        let body = world
            .resource::<Content>()
            .0
            .get(slug)
            .map(|record| record.body.clone())
            .unwrap_or_else(|| {
                "This instruction is unavailable. Skip, choose another page or Close.".into()
            });
        let instructions = super::reader::scrolling(world, content, "Practice instructions");
        let mut node = world.get_mut::<Node>(instructions).unwrap();
        node.height = Val::Auto;
        node.max_height = Val::Vh(35.0);
        crate::description::spawn(
            world,
            instructions,
            &body,
            crate::description::Context {
                owner: root,
                source: crate::protein_area::Source::Organ(
                    world.get::<Practice>(root).unwrap().source.clone(),
                ),
            },
        );
    }
    let status_text = match &phase {
        Phase::Ready => {
            "Try the highlighted action, or use Next to perform it. Skip leaves it unchanged."
                .to_owned()
        }
        Phase::Waiting { .. } => {
            "Waiting for the feature to confirm the result. Skip, Free and Close remain available."
                .to_owned()
        }
        Phase::Failed { message, .. } | Phase::Unavailable(message) => message.clone(),
        Phase::Complete => {
            "This page is finished. Choose another page or Close to review the practice samples."
                .to_owned()
        }
        Phase::Closed => "Practice stopped.".to_owned(),
    };
    let status = crate::edit_mode::label(world, content, &status_text, 14.0);
    world.get_mut::<Practice>(root).unwrap().status = status;
    let footer = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                ..default()
            },
            ChildOf(content),
            Recovery,
        ))
        .id();
    if let Some(operation) = world
        .get::<Practice>(root)
        .unwrap()
        .runner
        .current()
        .and_then(|step| step.operation)
    {
        let control = crate::description::button(
            world,
            footer,
            root,
            "Perform this sample action",
            Perform(operation),
        );
        let session = world.get::<Practice>(root).unwrap().runner.session;
        world
            .entity_mut(control)
            .insert(SemanticControl { session, operation });
    }
    crate::description::button(world, footer, root, "Next", Command::Next);
    crate::description::button(world, footer, root, "Skip", Command::Skip);
    if matches!(phase, Phase::Failed { .. } | Phase::Unavailable(_)) {
        crate::description::button(world, footer, root, "Retry", Command::Retry);
    }
    let pages = super::reader::scrolling(world, content, "Start another page");
    world.entity_mut(pages).insert(Recovery);
    let mut node = world.get_mut::<Node>(pages).unwrap();
    node.height = Val::Auto;
    node.max_height = px(120);
    for page in lince_interface::handbook::PAGES {
        let title = world
            .resource::<Content>()
            .0
            .get(page.slug)
            .map_or(page.slug, |record| record.head.as_str())
            .to_owned();
        crate::description::button(
            world,
            pages,
            root,
            &title,
            StartPage {
                slug: page.slug.into(),
                mode,
            },
        );
    }
}

pub(super) fn active(practices: Query<(), With<Practice>>) -> bool {
    !practices.is_empty()
}

pub(super) fn advancing(practices: Query<&Practice>) -> bool {
    practices.iter().any(|practice| !practice.summary)
}

pub(super) fn emergency(
    keys: Res<ButtonInput<KeyCode>>,
    practices: Query<Entity, With<Practice>>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Escape)
        && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    {
        for root in &practices {
            commands.queue(move |world: &mut World| {
                finish(world, root, false);
            });
        }
    }
}

pub(super) fn update(
    world: &mut World,
    mut transitions: Local<
        bevy::ecs::message::MessageCursor<crate::area_mutation::TransitionApplied>,
    >,
) {
    let changes: Vec<_> = world
        .get_resource::<Messages<crate::area_mutation::TransitionApplied>>()
        .map(|messages| transitions.read(messages).cloned().collect())
        .unwrap_or_default();
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Practice>>()
        .iter(world)
        .collect();
    for root in roots {
        let practice = world.get::<Practice>(root).unwrap();
        if practice.summary {
            continue;
        }
        if world.get_entity(practice.shell).is_err()
            || world.get::<Workspaces>(root).is_none_or(|spaces| {
                !spaces
                    .entries
                    .iter()
                    .any(|entry| entry.id == practice.workspace)
            })
        {
            finish(world, root, false);
            continue;
        }
        let prepared = practice
            .setup
            .as_ref()
            .and_then(|receiver| receiver.lock().ok()?.try_recv().ok());
        if let Some(prepared) = prepared {
            world.get_mut::<Practice>(root).unwrap().setup = None;
            match prepared {
                Ok(prepared) => {
                    let source = world.get::<Practice>(root).unwrap().source.clone();
                    let mut cells = world.resource_mut::<crate::practice_cells::PracticeCells>();
                    for uid in &prepared.records {
                        cells.records.insert(uid.clone(), source.clone());
                    }
                    cells.cells.insert(source.clone(), prepared.runtime);
                    world.get_mut::<Practice>(root).unwrap().records = prepared.records;
                    crate::protein_area::ensure_auxiliary(
                        world,
                        &crate::protein_area::Source::Organ(source),
                    );
                    if world.get::<Practice>(root).unwrap().runner.lesson.subject
                        == "area-record-actions"
                    {
                        let _ = protein(world, root);
                    }
                    render(world, root);
                }
                Err(message) => {
                    world.get_mut::<Practice>(root).unwrap().runner.phase =
                        Phase::Unavailable(message);
                    render(world, root);
                }
            }
        }
        lessons::receive(world, root);
        for (entity, _) in sample_rows(world, root) {
            world
                .entity_mut(entity)
                .insert(crate::practice_cells::PracticeRecord);
        }
        let change_area = find(world, root, Role::Changes);
        for event in &changes {
            let mut practice = world.get_mut::<Practice>(root).unwrap();
            if Some(event.area) == change_area && practice.records.first() == Some(&event.record) {
                if event.inside {
                    practice.confirmed_entry = true;
                } else if practice.confirmed_entry {
                    practice.confirmed_exit = true;
                }
            }
        }
        let observation = observe(world, root);
        let mut practice = world.get_mut::<Practice>(root).unwrap();
        let now = practice.started.elapsed().as_millis() as u64;
        let timed_out = practice.runner.tick(now);
        let effect = if let Some(ticket) = practice.pending {
            practice.runner.response(ticket, observation)
        } else {
            Effect::None
        };
        if matches!(effect, Effect::Advanced | Effect::Finished) {
            practice.pending = None;
            if let Some(timer) = practice.timer.take() {
                timer.abort();
            }
        }
        let redraw = timed_out || effect != Effect::None;
        drop(practice);
        if redraw {
            save_progress(world, root);
            render(world, root);
        }
    }
    input::refresh(world);
}
