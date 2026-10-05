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

mod automation;
mod cleanup;
mod commitments;
mod community;
mod disk;
mod input;
mod laboratory_practice;
mod lessons;
mod local;
mod media_practice;
#[cfg(feature = "instinct")]
pub(crate) mod tests;
mod tools;
mod visual;
mod work;
pub(super) use input::refresh as refresh_input;
pub(super) use input::{changed as input_changed, guard_focus};

#[derive(Component)]
pub(in crate::instinct) struct SemanticControl {
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
        if input::requires_control(self.0) {
            Command::Next.apply(world, root);
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
        if world
            .get::<crate::actions::TutorialField>(entity)
            .is_some_and(|field| {
                self::root(world, field.owner) == Some(root)
                    && practice.runner.current().and_then(|step| step.operation)
                        == Some(field.operation)
            })
        {
            return true;
        }
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
            .get::<crate::actions::ControlOwner>(entity)
            .is_some_and(|context| {
                world
                    .get::<Owned>(context.0)
                    .is_some_and(|owned| owned.session == practice.runner.session)
                    && self::root(world, context.0) == Some(root)
            })
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
    if intent == PracticeIntent::SelectedArea {
        return world
            .get::<crate::area_panel::AreaEditor>(root)
            .and_then(|editor| editor.selected)
            .is_some_and(|entity| permits_target(world, entity));
    }
    if intent == PracticeIntent::SelectedObjects {
        return world
            .get::<crate::canvas_selection::SandSelection>(root)
            .is_some_and(|selection| {
                !selection.0.is_empty()
                    && selection
                        .0
                        .iter()
                        .all(|entity| permits_target(world, *entity))
            });
    }
    if intent == PracticeIntent::EditedSand {
        return world
            .get::<crate::edit_mode::EditMode>(root)
            .and_then(|mode| world.get::<crate::sand_text_editor::TextPanel>(mode.panel))
            .is_some_and(|panel| permits_target(world, panel.sand));
    }
    permits_target(world, target)
}

pub(crate) fn permits_control(world: &World, target: Entity, action: &dyn Action) -> bool {
    if !permits_action(world, target, action.practice_intent()) {
        return false;
    }
    let Some(root) = self::root(world, target) else {
        return true;
    };
    let Some(practice) = world.get::<Practice>(root) else {
        return true;
    };
    let Some(operation) = practice.runner.current().and_then(|step| step.operation) else {
        return true;
    };
    if practice.runner.restricted()
        && visible(world, root)
        && input::requires_control(operation)
        && action.practice_intent() == crate::actions::PracticeIntent::Target
    {
        if !action.teaches_tutorial(operation) && !action.supports_tutorial(operation) {
            return false;
        }
        if target == root {
            return true;
        }
        let mut cursor = Some(target);
        while let Some(entity) = cursor {
            if world.get::<Owned>(entity).is_some_and(|owned| {
                owned.session == practice.runner.session && owned.role == Role::Feature
            }) {
                return true;
            }
            if let Some(context) = world.get::<crate::actions::ControlOwner>(entity)
                && world.get::<Owned>(context.0).is_some_and(|owned| {
                    owned.session == practice.runner.session && owned.role == Role::Feature
                })
            {
                return true;
            }
            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        return false;
    }
    true
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
    spatial_sample: Option<serde_json::Value>,
}

#[derive(Component)]
#[component(on_remove = removed)]
pub(super) struct Practice {
    runner: Runner,
    shell: Entity,
    content: Entity,
    status: Entity,
    workspace: u64,
    previous: u64,
    source: String,
    extra_sources: Vec<String>,
    community: community::State,
    local: local::State,
    media: media_practice::State,
    visual: visual::State,
    reference: Option<String>,
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
    cleanup_handled: bool,
    cleanup: cleanup::Quiescence,
}

fn removed(mut world: bevy::ecs::world::DeferredWorld, context: bevy::ecs::lifecycle::HookContext) {
    let Some(practice) = world.get::<Practice>(context.entity) else {
        return;
    };
    if practice.cleanup_handled {
        return;
    }
    let root = context.entity;
    let shell = practice.shell;
    let source = practice.source.clone();
    let workspace = practice.workspace;
    let extra_sources = practice.extra_sources.clone();
    let previous = practice.previous;
    let extra = practice.extra_workspaces.clone();
    let session = practice.runner.session;
    let previous_edit = practice.previous_edit;
    let edit_revision = practice.owned_edit_revision;
    let restoration = practice.visual.restoration.clone();
    let directory = world
        .get_resource::<crate::practice_cells::PracticeCells>()
        .and_then(|cells| cells.directories.get(&source))
        .cloned();
    world.commands().queue(move |world: &mut World| {
        input::release(world, root);
        if let Some(directory) = directory {
            crate::external_drop::cancel_owned_choice(world, root, &directory);
        }
        visual::restore_value(world, root, restoration);
        if world.get_entity(root).is_ok() {
            restore_edit_value(world, root, previous_edit, edit_revision);
            crate::tutorial::highlight::clear(world, root);
            crate::workspace::switch(world, root, previous);
            crate::workspace::remove(world, root, workspace);
            for workspace in extra {
                crate::workspace::remove(world, root, workspace);
            }
        }
        if world.get_entity(shell).is_ok() {
            world.despawn(shell);
        }
        let owned: Vec<_> = world
            .query::<(Entity, &Owned)>()
            .iter(world)
            .filter(|(_, owned)| owned.session == session)
            .map(|(entity, _)| entity)
            .collect();
        for entity in owned {
            world.despawn(entity);
        }
        crate::protein_area::release_practice_source(world, &source);
        retire_source(world, &source);
        for source in extra_sources {
            crate::protein_area::release_practice_source(world, &source);
            retire_source(world, &source);
        }
    });
}

fn retire_source(world: &mut World, source: &str) {
    let Some(mut cells) = world.get_resource_mut::<crate::practice_cells::PracticeCells>() else {
        return;
    };
    let runtime = cells.cells.remove(source);
    cells.workers.remove(source);
    cells.servers.remove(source);
    cells.audio.remove(source);
    cells.retired.insert(source.into());
    if let Some(path) = cells.directories.remove(source) {
        drop(cells);
        crate::ide::forget_directory(world, &path);
        crate::practice_cells::persistence::discard(path, runtime);
        if let Some(mut cells) = world.get_resource_mut::<crate::practice_cells::PracticeCells>() {
            cells.records.remove(source);
        }
        return;
    }
    cells.records.remove(source);
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
pub(in crate::instinct) struct Owned {
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
    Auxiliary,
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

pub(super) fn root(world: &World, mut owner: Entity) -> Option<Entity> {
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
            extra_sources: Vec::new(),
            community: default(),
            local: default(),
            media: default(),
            visual: default(),
            reference: None,
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
            cleanup_handled: false,
            cleanup: default(),
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
    let source = world.get::<Practice>(root).unwrap().source.clone();
    let existing = world
        .resource::<crate::practice_cells::PracticeCells>()
        .directories
        .get(&source)
        .cloned();
    let fiote = world.get::<Practice>(root).unwrap().runner.lesson.subject == "learn-fiote";
    let spatial =
        world.get::<Practice>(root).unwrap().runner.lesson.subject == "areas-of-influence";
    let directory = existing.map(Ok).or_else(|| {
        let base = world
            .get_resource::<crate::workspace::WorkspaceFile>()
            .map(|file| file.directory().to_path_buf())
            .unwrap_or_else(|| std::env::temp_dir().join("lince-instinct"));
        Some(crate::practice_cells::persistence::directory(
            &base, &source,
        ))
    });
    let directory = match directory.transpose() {
        Ok(directory) => directory,
        Err(message) => {
            world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable(message);
            return;
        }
    };
    if let Some(path) = &directory {
        world
            .resource_mut::<crate::practice_cells::PracticeCells>()
            .directories
            .insert(source, path.clone());
    }
    let (sender, receiver) = mpsc::channel();
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    let task = handle.spawn(async move {
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let engine = Arc::new(
                match &directory {
                    Some(path) => {
                        engine::Engine::open(&format!(
                            "sqlite://{}",
                            path.join("cell.db").display()
                        ))
                        .await
                    }
                    None => engine::Engine::open_memory().await,
                }
                .map_err(|error| error.to_string())?,
            );
            engine
                .install_karma_runtime_config(
                    engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(
                        "instinct-practice".into(),
                    )
                    .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            let mut records = Vec::new();
            if let Some(directory) = &directory {
                engine
                    .set_command_directory(directory)
                    .map_err(|error| error.to_string())?;
            }
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
            let spatial_sample = if spatial {
                let query: protein::Protein = serde_json::from_value(serde_json::json!({"source":"record","where":[{"uid_eq":records[0]}],"fields":["uid","head","kind","quantity"]})).map_err(|error| error.to_string())?;
                protein::execute_for(&engine.store, &query, None).await.map_err(|error| error.to_string())?.into_iter().next()
            } else { None };
            let fiote = if fiote {
                Some(Arc::new(
                    cell::fiote::Host::open(
                        engine.clone(),
                        directory.as_ref().unwrap().join("fiote"),
                    )
                    .await?,
                ))
            } else {
                None
            };
            Ok::<_, String>(Prepared {
                runtime: cell::CellRuntime {
                    commands: cell::terminal::commands::CommandHost::new(
                        directory.as_ref().unwrap().join("commands"),
                    ),
                    store: engine.store.clone(),
                    engine,
                    lanes: Arc::new(cell::LaneHub::new()),
                    wire: Default::default(),
                    fiote,
                    speech: None,
                    information: None,
                },
                records,
                spatial_sample,
            })
        })
        .await
        .unwrap_or_else(|_| Err("Practice setup timed out. Retry, Skip or Close.".into()));
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
    if matches!(
        role,
        Role::Spawn
            | Role::Changes
            | Role::Record
            | Role::Assertions
            | Role::Feature
            | Role::Auxiliary
    ) && world
        .get::<crate::practice_cells::PracticeSource>(entity)
        .is_none()
    {
        let source = world.get::<Practice>(root).unwrap().source.clone();
        world
            .entity_mut(entity)
            .insert(crate::practice_cells::PracticeSource(source));
    }
    if role == Role::Changes {
        let source = world.get::<Practice>(root).unwrap().source.clone();
        world
            .entity_mut(entity)
            .insert(crate::practice_cells::PracticeArea(source));
    }
}

pub(crate) fn active_source(world: &World, root: Entity) -> Option<&str> {
    if !visible(world, root) {
        return None;
    }
    Some(world.get::<Practice>(root)?.source.as_str())
}

pub(crate) fn sample_directory(world: &World, root: Entity) -> Option<std::path::PathBuf> {
    let source = active_source(world, root)?;
    world
        .get_resource::<crate::practice_cells::PracticeCells>()?
        .directories
        .get(source)
        .cloned()
}

pub(crate) fn window_root(world: &World, entity: Entity) -> Option<Entity> {
    self::root(world, entity)
}

pub(crate) use laboratory_practice::closed as laboratory_closed;

pub(crate) fn track_custom(world: &mut World, root: Entity, entities: &[Entity]) {
    if !visible(world, root) {
        return;
    }
    for entity in entities {
        own(world, root, *entity, Role::Auxiliary);
    }
}

fn pair(world: &mut World, root: Entity) {
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
            if let Some(entity) = crate::edit_mode::place_sand(world, root, kind) {
                crate::topology::set_position(
                    world,
                    entity,
                    DVec3::new(position.x, 0.0, position.y),
                );
                own(world, root, entity, role);
            }
        }
    }
}

pub(crate) fn track_record_view(
    world: &mut World,
    root: Entity,
    entity: Entity,
    binding: &crate::protein_area::RecordBinding,
) {
    if let Some(practice) = world.get::<Practice>(root)
        && binding.source == crate::protein_area::Source::Organ(practice.source.clone())
        && practice.records.contains(&binding.uid)
        && visible(world, root)
    {
        own(world, root, entity, Role::Record);
    }
}

pub(crate) fn track_sand(
    world: &mut World,
    root: Entity,
    entity: Entity,
    kind: crate::sand_store::SandKind,
) {
    let Some(practice) = world.get::<Practice>(root) else {
        return;
    };
    if practice.runner.current().and_then(|step| step.operation) != Some(Operation::PlaceSand)
        || !visible(world, root)
    {
        return;
    }
    let role = match kind {
        crate::sand_store::SandKind::Square => Role::Square,
        crate::sand_store::SandKind::Text => Role::Text,
        _ => return,
    };
    if find(world, root, role).is_none() {
        own(world, root, entity, role);
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
        )>()
        .iter(world)
        .filter(|(entity, binding, member)| {
            member.0 == workspace
                && self::root(world, *entity) == Some(root)
                && binding.source == source
                && ids.contains(&binding.uid)
        })
        .map(|(entity, binding, _)| (entity, binding.uid.clone()))
        .collect()
}

fn protein(world: &mut World, root: Entity) -> Result<Entity, String> {
    let practice = world.get::<Practice>(root).unwrap();
    if practice.records.len() < 2 {
        return Err("Waiting for the isolated practice Cell.".into());
    }
    let (source, ids) = (practice.source.clone(), practice.records[..2].to_vec());
    let learning_protein = practice.runner.lesson.subject == "protein";
    let entity = area(world, root, Role::Spawn)?;
    if world
        .get::<InfluenceArea>(entity)
        .unwrap()
        .protein
        .is_none()
    {
        let mut config = crate::protein_area::Config {
            source: crate::protein_area::Source::Organ(source),
            bindings: if learning_protein {
                vec![
                    crate::protein_area::Binding::new("head"),
                    crate::protein_area::Binding::new("body"),
                ]
            } else {
                vec![
                    crate::protein_area::Binding::new("head"),
                    crate::protein_area::Binding::new("body"),
                    crate::protein_area::Binding::new("quantity"),
                ]
            },
            columns: 2,
            ..default()
        };
        config.draft.query["where"] = serde_json::json!([{"any":ids.iter().take(if learning_protein { 1 } else { 2 }).map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
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
            for entity in [square, text] {
                if world.get::<crate::sand_placement::Pinned>(entity).is_some() {
                    crate::sand_placement::PlacementAction::Pin.apply(world, entity);
                }
            }
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
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Areas.apply(world, root);
            crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(entity))
                .apply(world, root);
            if !world
                .get::<InfluenceArea>(entity)
                .unwrap()
                .attraction_enabled
            {
                crate::edit_mode::EditAction::Area(
                    crate::area_panel::AreaAction::AttractionEnabled,
                )
                .apply(world, root);
            }
            let direction = if operation == Operation::Attract {
                Direction::Attract
            } else {
                Direction::Repel
            };
            crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Direction(direction))
                .apply(world, root);
            crate::area_panel::set_force_strength(world, root, entity, 100.0)?;
            let sample = find(world, root, Role::Square).unwrap();
            crate::topology::set_position(world, sample, DVec3::new(420.0, 0.0, 0.0));
        }
        Operation::CreateProteinArea | Operation::PreviewProtein | Operation::PresentProperties => {
            let entity = protein(world, root)?;
            crate::edit_mode::EditAction::Open.apply(world, root);
            crate::edit_mode::EditAction::Areas.apply(world, root);
            crate::edit_mode::EditAction::Area(crate::area_panel::AreaAction::Select(entity))
                .apply(world, root);
            if operation != Operation::CreateProteinArea {
                let ids = world.get::<Practice>(root).unwrap().records[..2].to_vec();
                crate::protein_area::ProteinAction::Query.apply(world, entity);
                let editor = world
                    .query::<(Entity, &crate::protein_area::QueryEditor)>()
                    .iter(world)
                    .find(|(_, link)| link.0 == entity)
                    .map(|(entity, _)| entity)
                    .ok_or("The query editor is unavailable. Skip or Close.")?;
                world
                    .get_mut::<crate::protein_castle::ProteinCastle>(editor)
                    .unwrap()
                    .draft
                    .query["where"] = serde_json::json!([{"any":ids.iter().map(|uid| serde_json::json!({"uid_eq":uid})).collect::<Vec<_>>()}]);
                crate::protein_castle::ProteinAction::Run.apply(world, editor);
            }
            if operation == Operation::PresentProperties {
                let bindings = world
                    .get::<InfluenceArea>(entity)
                    .unwrap()
                    .protein
                    .as_ref()
                    .unwrap()
                    .bindings
                    .clone();
                for (index, binding) in bindings.iter().enumerate().rev() {
                    if !["head", "quantity"].contains(&binding.property.as_str()) {
                        crate::protein_area::ProteinAction::RemoveField(index).apply(world, entity);
                    }
                }
                for property in ["head", "quantity"] {
                    if !bindings.iter().any(|binding| binding.property == property) {
                        crate::protein_area::ProteinAction::Add(property.into())
                            .apply(world, entity);
                    }
                }
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
    if operation == Operation::InspectCalls && community::complete(world, root, operation)
        || media_practice::unavailable_playback(world, root, operation)
        || visual::source_only(world, root, operation)
    {
        return Observation::Viewed;
    }
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
                                config.bindings.len() == 2
                                    && ["head", "quantity"].iter().all(|property| {
                                        config
                                            .bindings
                                            .iter()
                                            .any(|binding| binding.property == *property)
                                    })
                            })
                    }))
        }
        Operation::MatchRecord => {
            find(world, root, Role::Changes)
                .is_some_and(|entity| crate::area_mutation::armed(world, entity))
                && sample_rows(world, root).iter().any(|(entity, uid)| {
                    world
                        .get::<crate::protein_area::placement::Pending>(*entity)
                        .is_none()
                        && world.get::<Practice>(root).unwrap().records.first() == Some(uid)
                })
        }
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

#[derive(Clone, Copy)]
struct Recall(Option<&'static str>);

pub(crate) fn follow(world: &mut World, root: Entity, reference: &str) -> bool {
    if !visible(world, root) {
        return false;
    }
    let slug = world.get_resource::<Content>().and_then(|content| {
        content.0.iter().find_map(|(slug, record)| {
            (slug == reference || record.projection.uid == reference).then(|| slug.clone())
        })
    });
    let Some(slug) = slug else { return false };
    world.get_mut::<Practice>(root).unwrap().reference = Some(slug);
    render(world, root);
    true
}

impl Action for Recall {
    fn practice_intent(&self) -> crate::actions::PracticeIntent {
        crate::actions::PracticeIntent::Recovery
    }
    fn apply(&self, world: &mut World, root: Entity) {
        if let Some(mut practice) = world.get_mut::<Practice>(root) {
            if self.0.is_none_or(|slug| {
                lince_interface::handbook::foundations(practice.runner.lesson.subject)
                    .contains(&slug)
            }) {
                practice.reference = self.0.map(str::to_owned);
                render(world, root);
            }
        }
    }
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
                if let Some(directory) = sample_directory(world, root) {
                    crate::external_drop::cancel_owned_choice(world, root, &directory);
                }
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
                restore_edit(world, root);
                disarm_samples(world, root);
                cleanup::start(world, root);
            }
            Self::Keep | Self::Discard => {
                if matches!(self, Self::Keep) {
                    cleanup::start(world, root);
                    cleanup::receive(world, root);
                    if !world.get::<Practice>(root).unwrap().cleanup.ready() {
                        crate::notifications::report(
                            world,
                            "Instinct practice",
                            "The sample automation is still stopping or could not be stopped. Keep is available after confirmation; Discard and Close remain available.",
                        );
                        render(world, root);
                        return;
                    }
                    let practice = world.get::<Practice>(root).unwrap();
                    if practice
                        .runner
                        .lesson
                        .steps
                        .iter()
                        .filter_map(|step| step.operation)
                        .any(Operation::needs_cell)
                        && !world
                            .resource::<crate::practice_cells::PracticeCells>()
                            .cells
                            .contains_key(&practice.source)
                    {
                        crate::notifications::report(
                            world,
                            "Instinct practice",
                            "The sample Cell did not finish setup. Discard this attempt and restart the page to create usable examples.",
                        );
                        return;
                    }
                    let sources: Vec<_> = std::iter::once(practice.source.clone())
                        .chain(practice.extra_sources.iter().cloned())
                        .collect();
                    let cells = world.resource::<crate::practice_cells::PracticeCells>();
                    let result = sources.iter().try_for_each(|source| {
                        let path = cells
                            .directories
                            .get(source)
                            .ok_or("The practice directory is unavailable.".to_string())?;
                        if !cells.cells.contains_key(source) {
                            return Err(
                                "A sample Cell is not ready. Discard or restart this page.".into(),
                            );
                        }
                        let records: Vec<_> = cells
                            .records
                            .get(source)
                            .into_iter()
                            .flatten()
                            .cloned()
                            .collect();
                        crate::practice_cells::persistence::keep(path, source, &records)
                    });
                    if let Err(error) = result {
                        crate::notifications::report(
                            world,
                            "Instinct practice",
                            &format!(
                                "Keep failed: {error}. The examples are still available; retry Keep or Discard."
                            ),
                        );
                        return;
                    }
                }
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
                let operation = world
                    .get::<Practice>(root)
                    .unwrap()
                    .runner
                    .current()
                    .and_then(|step| step.operation);
                let loading_control = operation == Some(Operation::CompleteTask)
                    && find(world, root, Role::Feature).is_some_and(|owner| {
                        !crate::todo::contains(
                            world,
                            owner,
                            &world.get::<Practice>(root).unwrap().note,
                        )
                    });
                if matches!(observation, Observation::Waiting)
                    && operation.is_some_and(input::requires_control)
                    && find(world, root, Role::Feature).is_some()
                    && !loading_control
                    && input::semantic(world, root).is_err()
                {
                    world.get_mut::<Practice>(root).unwrap().runner.phase = Phase::Unavailable("The intended native control is missing or ambiguous. Interaction is released; reveal the control and Retry, choose another page, Skip or Close.".into());
                    input::release(world, root);
                    render(world, root);
                    input::refresh(world);
                    return;
                }
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
    restore_edit(world, root);
    disarm_samples(world, root);
    if let Some(mut practice) = world.get_mut::<Practice>(root) {
        practice.cleanup_handled = true;
    }
    let Some(mut practice) = world.entity_mut(root).take::<Practice>() else {
        return;
    };
    practice.runner.close();
    for task in practice.tasks.drain(..) {
        task.abort();
    }
    input::release(world, root);
    crate::tutorial::highlight::clear(world, root);
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
    if keep {
        crate::practice_cells::resume_audio(world, &practice.source);
    } else {
        crate::protein_area::release_practice_source(world, &practice.source);
        crate::workspace::remove(world, root, practice.workspace);
        for workspace in &practice.extra_workspaces {
            crate::workspace::remove(world, root, *workspace);
        }
        for entity in owned {
            if world.get_entity(entity).is_ok() {
                world.despawn(entity);
            }
        }
        retire_source(world, &practice.source);
        for source in &practice.extra_sources {
            crate::protein_area::release_practice_source(world, source);
            retire_source(world, source);
        }
    }
    crate::notifications::report(
        world,
        "Instinct",
        if keep {
            "Kept the practice workspace. Property actions and sample automation are disabled. Its sample Cell remains separate from personal data; when workspace storage is available it will reopen next time."
        } else {
            "Discarded the practice workspace and its isolated Cell. Personal Records were untouched."
        },
    );
}

fn restore_edit(world: &mut World, root: Entity) {
    visual::restore(world, root);
    let Some(practice) = world.get::<Practice>(root) else {
        return;
    };
    restore_edit_value(
        world,
        root,
        practice.previous_edit,
        practice.owned_edit_revision,
    );
}

fn restore_edit_value(world: &mut World, root: Entity, previous: bool, revision: Option<u64>) {
    if revision.is_some_and(|revision| {
        world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled != previous && mode.revision == revision)
    }) {
        if previous {
            crate::edit_mode::EditAction::Open.apply(world, root);
        } else {
            crate::edit_mode::EditAction::Close.apply(world, root);
        }
    }
}

fn disarm_samples(world: &mut World, root: Entity) {
    let Some(practice) = world.get::<Practice>(root) else {
        return;
    };
    let source = practice.source.clone();
    let workspace = practice.workspace;
    let session = practice.runner.session;
    let timers: Vec<_> = world
        .query::<(Entity, &Owned)>()
        .iter(world)
        .filter(|(entity, owned)| {
            owned.session == session && crate::work_timer::running(world, *entity) == Some(true)
        })
        .map(|(entity, _)| entity)
        .collect();
    for timer in timers {
        crate::work_timer::Toggle.apply(world, timer);
    }
    let simulations: Vec<_> = world
        .query::<(Entity, &Owned)>()
        .iter(world)
        .filter(|(entity, owned)| {
            owned.session == session
                && world
                    .get::<crate::simulation_castle::SimulationCastle>(*entity)
                    .is_some()
        })
        .map(|(entity, _)| entity)
        .collect();
    for owner in simulations {
        crate::simulation_castle::stop(world, owner);
    }
    work::clean_notice(world, root);
    let areas: Vec<_> = world
        .query::<(
            Entity,
            &crate::practice_cells::PracticeArea,
            &WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(entity, area, member)| {
            area.0 == source && member.0 == workspace && self::root(world, *entity) == Some(root)
        })
        .map(|(entity, _, _)| entity)
        .collect();
    for entity in areas {
        crate::area_mutation::disarm(world, entity, "Practice ended");
        if let Some(mut area) = world.get_mut::<InfluenceArea>(entity) {
            area.changes_enabled = false;
        }
    }
}

fn render(world: &mut World, root: Entity) {
    let practice = world.get::<Practice>(root).unwrap();
    let (content, summary, phase, mode, current, subject) = (
        practice.content,
        practice.summary,
        practice.runner.phase.clone(),
        practice.runner.mode,
        practice.runner.current().map(|step| step.slug),
        practice.runner.lesson.subject,
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
        &format!("{mode:?} · disposable practice. Close offers Keep or Discard."),
        14.0,
    );
    if summary {
        let practice = world.get::<Practice>(root).unwrap();
        let session = practice.runner.session;
        let workspaces = 1 + practice.extra_workspaces.len();
        let sources: Vec<_> = std::iter::once(practice.source.clone())
            .chain(practice.extra_sources.iter().cloned())
            .collect();
        let cells = world.resource::<crate::practice_cells::PracticeCells>();
        let records = sources
            .iter()
            .filter_map(|source| cells.records.get(source))
            .map(std::collections::HashSet::len)
            .sum::<usize>();
        let directories: Vec<_> = sources
            .iter()
            .filter_map(|source| cells.directories.get(source))
            .cloned()
            .collect();
        let objects = world
            .query::<&Owned>()
            .iter(world)
            .filter(|owned| owned.session == session)
            .count();
        crate::edit_mode::label(
            world,
            content,
            &format!(
                "This example created {workspaces} workspace(s), {objects} canvas object(s) and {records} Record(s) in separate sample Cells."
            ),
            14.0,
        );
        for directory in directories {
            crate::edit_mode::label(
                world,
                content,
                &format!(
                    "Example files: {}. Keep retains these; Discard removes them.",
                    directory.display()
                ),
                12.0,
            );
        }
        let cleanup = world.get::<Practice>(root).unwrap().cleanup.message();
        crate::edit_mode::label(world, content, &cleanup, 14.0);
        crate::edit_mode::label(
            world,
            content,
            "Practice stopped. Restrictions are released. Keep saves this separate workspace and its sample Cell when workspace storage is available; Discard removes its samples. Submitted changes may have finished in the isolated Cell.",
            14.0,
        );
        crate::description::button(world, content, root, "Keep practice", Command::Keep);
        crate::description::button(world, content, root, "Discard practice", Command::Discard);
        return;
    }
    let subject_record = world.resource::<Content>().0.get(subject).cloned();
    if current != Some(subject)
        && let Some(record) = subject_record
    {
        crate::description::spawn(
            world,
            content,
            super::teaching_text(&record.body),
            crate::description::Context {
                owner: root,
                source: crate::protein_area::Source::Organ(
                    world.get::<Practice>(root).unwrap().source.clone(),
                ),
            },
        );
    }
    let foundations = lince_interface::handbook::foundations(subject);
    if !foundations.is_empty() {
        let earlier = crate::sand_panel::row(world, content);
        for slug in foundations {
            let title = world
                .resource::<Content>()
                .0
                .get(*slug)
                .map_or(*slug, |record| record.head.as_str())
                .to_owned();
            crate::description::button(
                world,
                earlier,
                root,
                &format!("Recall {title}"),
                Recall(Some(slug)),
            );
        }
    }
    if let Some(reference) = world.get::<Practice>(root).unwrap().reference.clone()
        && let Some(record) = world.resource::<Content>().0.get(&reference).cloned()
    {
        let pane = super::reader::scrolling(world, content, "Earlier explanation");
        world.get_mut::<Node>(pane).unwrap().max_height = Val::Vh(30.0);
        crate::description::heading(world, pane, &record.head, 18.0);
        crate::description::spawn(
            world,
            pane,
            super::teaching_text(&record.body),
            crate::description::Context {
                owner: root,
                source: crate::protein_area::Source::Organ(
                    world.get::<Practice>(root).unwrap().source.clone(),
                ),
            },
        );
        crate::description::button(world, content, root, "Back to this step", Recall(None));
    }
    if let Some(slug) = current {
        let body = world
            .resource::<Content>()
            .0
            .get(slug)
            .map(|record| super::teaching_text(&record.body).to_owned())
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
    if phase == Phase::Complete
        && let Some(index) = lince_interface::handbook::PAGES
            .iter()
            .position(|page| page.slug == subject || page.reference == Some(subject))
        && let Some(page) = lince_interface::handbook::PAGES.get(index + 1)
    {
        let title = world
            .resource::<Content>()
            .0
            .get(page.slug)
            .map_or(page.slug, |record| record.head.as_str())
            .to_owned();
        crate::description::button(
            world,
            footer,
            root,
            &format!("Continue: {title}"),
            StartPage {
                slug: page.slug.into(),
                mode,
            },
        );
    }
    if matches!(phase, Phase::Failed { .. } | Phase::Unavailable(_)) {
        crate::description::button(world, footer, root, "Retry", Command::Retry);
    }
    let pages = super::reader::scrolling(world, content, "Start another page");
    crate::edit_mode::label(
        world,
        pages,
        "Starting another page discards this disposable example. To retain it, Close and choose Keep first.",
        12.0,
    );
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

pub(super) fn active(
    practices: Query<
        (),
        (
            With<Practice>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
) -> bool {
    !practices.is_empty()
}

pub(super) fn advancing(practices: Query<&Practice>) -> bool {
    practices.iter().any(|practice| !practice.summary)
}

pub(super) fn emergency(
    keys: Res<ButtonInput<KeyCode>>,
    practices: Query<
        Entity,
        (
            With<Practice>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Escape)
        && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    {
        for root in &practices {
            commands.queue(move |world: &mut World| {
                laboratory_practice::return_to_workspace(world, root);
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
        if practice.summary {
            if cleanup::receive(world, root) {
                render(world, root);
            }
            continue;
        }
        if practice.setup.is_none()
            && practice.exercise.is_none()
            && practice.pending.is_none()
            && practice.records.is_empty()
            && !practice
                .runner
                .lesson
                .steps
                .iter()
                .filter_map(|step| step.operation)
                .any(visual::handles)
        {
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
                    cells
                        .records
                        .entry(source.clone())
                        .or_default()
                        .extend(prepared.records.iter().cloned());
                    cells.cells.insert(source.clone(), prepared.runtime);
                    world.get_mut::<Practice>(root).unwrap().records = prepared.records;
                    if let Some(properties) = prepared.spatial_sample {
                        pair(world, root);
                        let sample = find(world, root, Role::Square).unwrap();
                        world
                            .entity_mut(sample)
                            .insert(RecordProperties(properties));
                        let entity = area(world, root, Role::Area).unwrap();
                        world.get_mut::<InfluenceArea>(entity).unwrap().rules =
                            vec![crate::area::PropertyRule {
                                property: crate::area::Property::Title,
                                value: "Instinct sample 1".into(),
                            }];
                        if let Some(operation) =
                            world.get::<Practice>(root).unwrap().pending.and_then(|_| {
                                world
                                    .get::<Practice>(root)
                                    .unwrap()
                                    .runner
                                    .current()
                                    .and_then(|step| step.operation)
                            })
                        {
                            let _ = execute(world, root, operation);
                        }
                    }
                    crate::protein_area::ensure_auxiliary(
                        world,
                        &crate::protein_area::Source::Organ(source),
                    );
                    let first = world
                        .get::<Practice>(root)
                        .unwrap()
                        .runner
                        .current()
                        .and_then(|step| step.operation);
                    if let Some(
                        operation @ (Operation::CompleteTask
                        | Operation::MoveTask
                        | Operation::OpenDatedRecord
                        | Operation::SetOperation),
                    ) = first
                    {
                        let _ = lessons::begin_record(world, root, operation);
                    }
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
        automation::prepare(world, root);
        tools::prepare(world, root);
        if !world.get::<Practice>(root).unwrap().records.is_empty() {
            let source = crate::protein_area::Source::Organ(
                world.get::<Practice>(root).unwrap().source.clone(),
            );
            let rows: Vec<_> = world.query_filtered::<(Entity, &crate::protein_area::RecordBinding), Without<crate::practice_cells::PracticeRecord>>().iter(world).filter(|(_, binding)| binding.source == source).map(|(entity, _)| entity).collect();
            for entity in rows {
                world
                    .entity_mut(entity)
                    .insert(crate::practice_cells::PracticeRecord);
            }
        }
        work::drive(world, root);
        let change_area = (!changes.is_empty())
            .then(|| find(world, root, Role::Changes))
            .flatten();
        for event in &changes {
            work::transition(world, root, event);
            let mut practice = world.get_mut::<Practice>(root).unwrap();
            if Some(event.area) == change_area && practice.records.first() == Some(&event.record) {
                if event.inside {
                    practice.confirmed_entry = true;
                } else if practice.confirmed_entry {
                    practice.confirmed_exit = true;
                }
            }
        }
        let observation = if world.get::<Practice>(root).unwrap().pending.is_some() {
            observe(world, root)
        } else {
            Observation::Waiting
        };
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
}
