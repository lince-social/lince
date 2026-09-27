use super::{BehaviorRun, StressConfig, StressReport, stress::StressRun};
use crate::{actions::Action, container::BoxRoot, edit_mode::label, workspace::Workspaces};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    winit::WinitSettings,
};
use std::time::Instant;

#[derive(Component)]
pub struct LaboratoryRoot;

#[derive(Component)]
pub(crate) struct SuspendedRoot;

struct Suspended {
    root: Entity,
    display: Display,
    entities: Vec<Entity>,
}

#[derive(Resource, Default)]
pub struct Laboratory {
    pub root: Option<Entity>,
    pub behavior: Option<BehaviorRun>,
    pub reports: Vec<StressReport>,
    pub config: StressConfig,
    pub status: String,
    pub resources: super::ResourceSnapshot,
    resources_open: bool,
    resource_sort: super::resources::ResourceSort,
    resource_page: usize,
    stress: Option<StressRun>,
    suspended: Vec<Suspended>,
    focus: Option<Entity>,
    settings: Option<WinitSettings>,
    opened: Option<Instant>,
    frame: Option<Instant>,
    full: bool,
    closing: bool,
    rendered: bool,
    refresh: Option<Instant>,
}

pub fn active(world: &World) -> bool {
    world
        .get_resource::<Laboratory>()
        .is_some_and(|lab| lab.root.is_some())
}

pub fn normal(world: &World) -> bool {
    !active(world)
}

pub fn suspended(world: &World, mut entity: Entity) -> bool {
    loop {
        if world.get::<SuspendedRoot>(entity).is_some() {
            return true;
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = parent.parent();
    }
}

#[derive(Clone, Copy)]
pub enum LaboratoryAction {
    Open,
    RunAll,
    Behavior,
    Stress,
    Stop,
    Close,
    Budget,
    Limit,
    Export,
    Resources,
    SortResources,
    NextResources,
}

impl Action for LaboratoryAction {
    fn apply(&self, world: &mut World, _: Entity) {
        if !world.contains_resource::<Laboratory>() {
            return;
        }
        if matches!(self, Self::Open) {
            open(world);
            return;
        }
        if !active(world) {
            return;
        }
        match self {
            Self::Open => {}
            Self::Close => {
                stop(world);
                world.resource_mut::<Laboratory>().closing = true;
                if !busy(world) {
                    close(world);
                }
            }
            Self::Stop => stop(world),
            Self::RunAll | Self::Behavior if !busy(world) => {
                let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
                let run = BehaviorRun::start(wake);
                let mut lab = world.resource_mut::<Laboratory>();
                match run {
                    Ok(run) => {
                        lab.behavior = Some(run);
                        lab.full = matches!(self, Self::RunAll);
                        lab.status = "Running behavior checks".into();
                    }
                    Err(error) => lab.status = error.to_string(),
                }
            }
            Self::Stress if !busy(world) => start_stress(world),
            Self::Budget if !busy(world) => {
                let mut lab = world.resource_mut::<Laboratory>();
                lab.config.budget_ms = if lab.config.budget_ms == 20.0 {
                    40.0
                } else {
                    20.0
                };
            }
            Self::Limit if !busy(world) => {
                let mut lab = world.resource_mut::<Laboratory>();
                lab.config.max_sands = if lab.config.max_sands == 8192 {
                    32768
                } else {
                    8192
                };
            }
            Self::Export => export(world),
            Self::Resources => {
                let mut lab = world.resource_mut::<Laboratory>();
                lab.resources_open = !lab.resources_open;
            }
            Self::SortResources => {
                let mut lab = world.resource_mut::<Laboratory>();
                lab.resource_sort = lab.resource_sort.next();
                lab.resource_page = 0;
            }
            Self::NextResources => {
                let mut lab = world.resource_mut::<Laboratory>();
                lab.resource_page =
                    (lab.resource_page + 1) % lab.resources.sands.len().div_ceil(8).max(1);
            }
            _ => {}
        }
        if let Some(mut lab) = world.get_resource_mut::<Laboratory>() {
            lab.refresh = None;
        }
    }
}

pub struct LaboratoryPlugin;
impl Plugin for LaboratoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Laboratory>()
            .add_systems(First, tick)
            .add_systems(Update, render);
    }
}

pub fn open(world: &mut World) {
    if active(world) {
        return;
    }
    let resources = super::resources::capture(world);
    let mut roots: Vec<_> = world
        .query_filtered::<(Entity, &Node), With<BoxRoot>>()
        .iter(world)
        .map(|(root, node)| Suspended {
            root,
            display: node.display,
            entities: Vec::new(),
        })
        .collect();
    for entry in &mut roots {
        let mut pending = vec![entry.root];
        while let Some(entity) = pending.pop() {
            if let Some(children) = world.get::<Children>(entity) {
                pending.extend(children.iter());
            }
            if world
                .get::<bevy::ecs::entity_disabling::Disabled>(entity)
                .is_none()
            {
                entry.entities.push(entity);
                world
                    .entity_mut(entity)
                    .insert(bevy::ecs::entity_disabling::Disabled);
            }
        }
        world.get_mut::<Node>(entry.root).unwrap().display = Display::None;
        world.entity_mut(entry.root).insert(SuspendedRoot);
    }
    let focus = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.clear();
    }
    crate::time_limit::pause(world);
    let settings = world.get_resource::<WinitSettings>().cloned();
    let rendered = settings.is_some();
    let mut spaces = Workspaces::default();
    spaces.entries[0].name = "Performance & Behavior".into();
    let root = world
        .spawn((
            BoxRoot,
            spaces,
            LaboratoryRoot,
            crate::workspace_config::WorkspaceSettings::default(),
        ))
        .id();
    let mut lab = world.resource_mut::<Laboratory>();
    lab.root = Some(root);
    lab.resources = resources;
    lab.resources_open = false;
    lab.resource_page = 0;
    lab.suspended = roots;
    lab.focus = focus;
    lab.settings = settings;
    lab.rendered = rendered;
    lab.opened = Some(Instant::now());
    lab.frame = None;
    lab.refresh = None;
    lab.closing = false;
    lab.status = "Other workspaces are suspended. Tests use temporary data.".into();
}

pub fn close(world: &mut World) {
    if !active(world) {
        return;
    }
    stop(world);
    let mut lab = world.remove_resource::<Laboratory>().unwrap();
    if let Some(root) = lab.root.take() {
        world.despawn(root);
    }
    for entry in lab.suspended.drain(..) {
        for entity in entry.entities {
            if let Ok(mut entity) = world.get_entity_mut(entity) {
                entity.remove::<bevy::ecs::entity_disabling::Disabled>();
            }
        }
        if let Some(mut node) = world.get_mut::<Node>(entry.root) {
            node.display = entry.display;
        }
        if let Ok(mut entity) = world.get_entity_mut(entry.root) {
            entity.remove::<SuspendedRoot>();
        }
    }
    if let Some(settings) = lab.settings.take() {
        world.insert_resource(settings);
    }
    if let Some(opened) = lab.opened.take() {
        crate::time_limit::resume(world, opened.elapsed());
    }
    if let Some(entity) = lab.focus.take()
        && world.get_entity(entity).is_ok()
        && let Some(mut focus) = world.get_resource_mut::<InputFocus>()
    {
        focus.set(entity, FocusCause::Navigated);
    }
    lab.closing = false;
    world.insert_resource(lab);
}

fn busy(world: &World) -> bool {
    let lab = world.resource::<Laboratory>();
    lab.stress.is_some() || lab.behavior.as_ref().is_some_and(|run| !run.finished())
}

fn stop(world: &mut World) {
    let mut lab = world.resource_mut::<Laboratory>();
    lab.full = false;
    if let Some(run) = &mut lab.behavior
        && !run.finished()
    {
        run.cancel();
    }
    if let Some(run) = lab.stress.take() {
        if lab.reports.len() == 16 {
            lab.reports.remove(0);
        }
        lab.reports.push(run.report);
    }
    let root = lab.root;
    let settings = lab.settings.clone();
    lab.status = "Stopped. A behavior check already in progress finishes before returning.".into();
    if let Some(settings) = settings {
        world.insert_resource(settings);
    }
    if let Some(root) = root {
        StressRun::clear(world, root);
        crate::workspace_config::set_physics(world, root, 1, false);
    }
}

fn start_stress(world: &mut World) {
    let lab = world.resource::<Laboratory>();
    if !lab.config.validate() {
        world.resource_mut::<Laboratory>().status = "Invalid stress limits".into();
        return;
    }
    let root = lab.root.unwrap();
    let viewport = world
        .get::<ComputedUiRenderTargetInfo>(root)
        .map(|target| target.logical_size())
        .filter(|size| size.min_element() > 0.0)
        .unwrap_or(Vec2::new(1000.0, 800.0));
    let run = StressRun::new(lab.config, lab.rendered, viewport);
    let rendered = lab.rendered;
    let mut lab = world.resource_mut::<Laboratory>();
    lab.stress = Some(run);
    lab.frame = None;
    lab.status = "Adding Sands; measuring each load after warmup".into();
    if rendered {
        world.insert_resource(WinitSettings::continuous());
    }
}

fn tick(world: &mut World) {
    let mut lab = world.remove_resource::<Laboratory>().unwrap();
    if let Some(run) = &mut lab.behavior {
        let previous = (run.results.len(), run.current.clone(), run.finished());
        run.poll();
        if !previous.2 && run.finished() {
            lab.status = if run.cancelled {
                "Behavior cancelled".into()
            } else {
                format!(
                    "Behavior complete: {} / {} checks, {} failed",
                    run.results.len(),
                    run.total,
                    run.results.iter().filter(|row| row.error.is_some()).count()
                )
            };
        }
        if previous != (run.results.len(), run.current.clone(), run.finished()) {
            lab.refresh = None;
        }
    }
    let Some(root) = lab.root else {
        world.insert_resource(lab);
        return;
    };
    let now = Instant::now();
    let elapsed = lab.frame.replace(now).map_or(0.0, |previous| {
        now.duration_since(previous).as_secs_f64() * 1000.0
    });
    let stress = lab.stress.take();
    world.insert_resource(lab);
    if let Some(mut run) = stress {
        run.advance(world, root, elapsed);
        let mut lab = world.resource_mut::<Laboratory>();
        if run.report.complete {
            lab.refresh = None;
            lab.status = "Stress run complete".into();
            if lab.reports.len() == 16 {
                lab.reports.remove(0);
            }
            lab.reports.push(run.report);
            if let Some(settings) = lab.settings.clone() {
                world.insert_resource(settings);
            }
        } else {
            lab.stress = Some(run);
        }
    }
    if !busy(world) {
        if world.resource::<Laboratory>().closing {
            close(world);
            return;
        }
        if world.resource::<Laboratory>().full {
            world.resource_mut::<Laboratory>().full = false;
            start_stress(world);
        }
    }
}

#[derive(Component)]
struct LaboratoryPanel;

fn render(world: &mut World) {
    let lab = world.resource::<Laboratory>();
    let Some(root) = lab.root else {
        return;
    };
    if lab.refresh.is_some_and(|at| at.elapsed().as_millis() < 250) {
        return;
    }
    let status = lab.status.clone();
    let config = lab.config;
    let resources_open = lab.resources_open;
    let mut lines = vec![super::GraphicsDevice::read(world).label()];
    if lab.resources_open {
        lines.extend(lab.resources.lines(lab.resource_sort, lab.resource_page));
    }
    if let Some(run) = &lab.behavior {
        let failures = run
            .results
            .iter()
            .filter(|result| result.error.is_some())
            .count();
        lines.push(format!(
            "Behavior: {} / {} · {} failed{}",
            run.results.len(),
            run.total,
            failures,
            if run.cancelled { " · cancelled" } else { "" }
        ));
        if let Some(name) = &run.current {
            lines.push(
                name.split("::tests::")
                    .last()
                    .unwrap_or(name)
                    .replace('_', " "),
            );
        }
        for result in run
            .results
            .iter()
            .filter(|result| result.error.is_some())
            .take(3)
        {
            lines.push(format!(
                "Failed: {}: {}",
                result.name,
                result.error.as_deref().unwrap()
            ));
        }
    }
    if let Some(run) = &lab.stress {
        lines.push(format!(
            "{} · physics {} · {} / {} Sands",
            run.kind().name(),
            if run.physics() { "on" } else { "off" },
            run.count,
            run.target
        ));
    }
    let report = lab
        .stress
        .as_ref()
        .map(|run| &run.report)
        .or_else(|| lab.reports.last());
    if let Some(report) = report {
        lines.push(report.clock.clone());
        if let Some(row) = report.measurements.last() {
            lines.push(format!(
                "{} Sands · {} visible · mean {:.2} ms · p95 {:.2} · p99 {:.2} · max {:.2}",
                row.sands,
                row.visible_sands
                    .map_or_else(|| "unmeasured".into(), |count| count.to_string()),
                row.mean_ms,
                row.p95_ms,
                row.p99_ms,
                row.worst_ms
            ));
        }
        lines.extend(report.stops.iter().cloned());
    }
    let existing = world
        .query_filtered::<Entity, With<LaboratoryPanel>>()
        .iter(world)
        .next();
    let panel = existing.unwrap_or_else(|| {
        let panel = world
            .spawn((
                LaboratoryPanel,
                ChildOf(root),
                GlobalZIndex(50),
                crate::token_style::background(crate::tokens::Token::Surface),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    right: px(12),
                    top: px(12),
                    max_height: percent(45),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    padding: UiRect::all(px(12)),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
            ))
            .observe(
                |mut event: On<Pointer<bevy::picking::events::Scroll>>,
                 mut scrolls: Query<&mut ScrollPosition, With<LaboratoryPanel>>| {
                    if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
                        let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                            24.0
                        } else {
                            1.0
                        };
                        scroll.0.y -= event.y * scale;
                        event.propagate(false);
                    }
                },
            )
            .id();
        label(world, panel, "Performance & Behavior", 22.0);
        let buttons = world
            .spawn((
                ChildOf(panel),
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(6),
                    row_gap: px(6),
                    ..default()
                },
            ))
            .id();
        for (title, action) in [
            ("Run all", LaboratoryAction::RunAll),
            ("Behavior", LaboratoryAction::Behavior),
            ("Stress", LaboratoryAction::Stress),
            ("Stop", LaboratoryAction::Stop),
            ("Frame budget", LaboratoryAction::Budget),
            ("Sand limit", LaboratoryAction::Limit),
            ("Export results", LaboratoryAction::Export),
            ("Sand resources", LaboratoryAction::Resources),
            ("Sort resources", LaboratoryAction::SortResources),
            ("Next Sands", LaboratoryAction::NextResources),
            ("Return", LaboratoryAction::Close),
        ] {
            let button = crate::information::action_button(
                world,
                buttons,
                root,
                title,
                crate::actions![action],
            );
            if matches!(
                action,
                LaboratoryAction::SortResources | LaboratoryAction::NextResources
            ) {
                world.entity_mut(button).insert(ResourceControl);
            }
        }
        world.spawn((
            StatusText,
            Text::new(""),
            crate::theme::Typography::text(world.resource::<crate::theme::Typography>(), 14.0),
            crate::token_style::text(crate::tokens::Token::Ink),
            ChildOf(panel),
        ));
        panel
    });
    let text = format!(
        "{status}\nFrame budget: {:.0} ms · Sand limit: {}\n{}",
        config.budget_ms,
        config.max_sands,
        lines.join("\n")
    );
    world.get_mut::<Node>(panel).unwrap().max_height =
        percent(if resources_open { 85 } else { 45 });
    for mut node in world
        .query_filtered::<&mut Node, With<ResourceControl>>()
        .iter_mut(world)
    {
        node.display = if resources_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut current in world
        .query_filtered::<&mut Text, With<StatusText>>()
        .iter_mut(world)
    {
        if current.0 != text {
            current.0.clone_from(&text);
        }
    }
    world.resource_mut::<Laboratory>().refresh = Some(Instant::now());
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ResourceControl;

fn export(world: &mut World) {
    let lab = world.resource::<Laboratory>();
    let report = serde_json::json!({
        "schema": 2,
        "graphics": super::GraphicsDevice::read(world),
        "sand_resources_before_suspension": lab.resources,
        "behavior": lab.behavior.as_ref().map(|run| &run.results),
        "behavior_total": lab.behavior.as_ref().map(|run| run.total),
        "behavior_cancelled": lab.behavior.as_ref().is_some_and(|run| run.cancelled),
        "stress": lab.reports,
        "running_stress": lab.stress.as_ref().map(|run| &run.report),
    });
    let directory = world
        .get_resource::<crate::workspace::WorkspaceFile>()
        .map(|file| file.directory().to_path_buf())
        .unwrap_or_else(std::env::temp_dir);
    let result = (|| -> std::io::Result<_> {
        let mut file = tempfile::Builder::new()
            .prefix("laboratory-")
            .suffix(".json")
            .tempfile_in(directory)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec_pretty(&report)?)?;
        let (_, path) = file.keep().map_err(|error| error.error)?;
        Ok(path)
    })();
    world.resource_mut::<Laboratory>().status = match result {
        Ok(path) => format!("Results saved to {}", path.display()),
        Err(error) => format!("Could not export results: {error}"),
    };
}
