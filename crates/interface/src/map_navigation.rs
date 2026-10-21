use crate::{controls, theme::Typography};
use bevy::prelude::*;
use lince_map::{
    Camera, Coordinate,
    navigation::{Geocoder, Mode, Place, Provider, Route, Router, Selection, Settings},
};
use std::{path::PathBuf, sync::Arc};

#[cfg(test)]
mod tests;

#[derive(Component)]
struct Panel {
    owner: Entity,
    path: PathBuf,
    settings: Settings,
    provider: Option<Arc<Provider>>,
    runtime: Option<tokio::runtime::Handle>,
    point: Coordinate,
    origin: Option<Coordinate>,
    destination: Option<Coordinate>,
    mode: Mode,
    selection: Selection,
    fields: Vec<Entity>,
    results: Entity,
    status: Entity,
    configuration: Entity,
    job: Option<Job>,
    selected: Option<Place>,
    changed: bool,
    region: Option<(lince_map::Bounds, Arc<lince_map::TileStore>)>,
    scope: String,
}
struct Job {
    task: tokio::task::JoinHandle<()>,
    response: tokio::sync::oneshot::Receiver<Result<Response, String>>,
    generation: u64,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    progress: Option<Arc<lince_map::region::Progress>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.task.abort();
    }
}
enum Response {
    Places(Vec<Place>),
    Route(Route),
    Region(lince_map::pack::Manifest),
}
#[derive(Clone)]
enum Intent {
    Open,
    Settings,
    Save,
    Search,
    Reverse,
    Choose(usize),
    Origin,
    Destination,
    Mode,
    Route,
    Region,
    Cancel,
}
#[derive(Component, Clone)]
struct Button {
    panel: Entity,
    intent: Intent,
}

pub struct NavigationPlugin;
impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, poll);
    }
}

fn label(world: &mut World, parent: Entity, caption: impl Into<String>) -> Entity {
    let font = world.resource::<Typography>().text(14.0);
    world
        .spawn((
            ChildOf(parent),
            Text::new(caption),
            font,
            crate::style::text(crate::tokens::Token::Ink),
        ))
        .id()
}
fn button(world: &mut World, parent: Entity, panel: Entity, caption: &str, intent: Intent) {
    let entity = world
        .spawn((
            ChildOf(parent),
            controls::button(0),
            Node {
                min_height: px(44),
                padding: UiRect::axes(px(8), px(6)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::style::border(crate::tokens::Token::Accent),
            Button { panel, intent },
        ))
        .id();
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(caption);
    }
    label(world, entity, caption);
    world.entity_mut(entity).observe(
        |activate: On<bevy::ui_widgets::Activate>,
         buttons: Query<&Button>,
         mut commands: Commands| {
            if let Ok(button) = buttons.get(activate.entity) {
                let button = button.clone();
                commands.queue(move |world: &mut World| apply(world, button.panel, button.intent));
            }
        },
    );
}
fn field(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    label(world, parent, caption);
    let editor = controls::single_line_editor(value, world.resource::<Typography>(), 0, 4096);
    let mut accessibility = accesskit::Node::new(accesskit::Role::TextInput);
    accessibility.set_label(caption);
    accessibility.set_value(value);
    world
        .spawn((
            ChildOf(parent),
            editor,
            controls::Editor {
                title: caption.into(),
                secret: false,
            },
            bevy::a11y::AccessibilityNode::from(accessibility),
        ))
        .id()
}

pub fn mount(
    world: &mut World,
    controls_parent: Entity,
    owner: Entity,
    path: PathBuf,
    point: Coordinate,
    runtime: Option<tokio::runtime::Handle>,
) {
    let settings = Settings::load(&path).unwrap_or_default();
    let panel = world
        .spawn((
            ChildOf(owner),
            Node {
                position_type: PositionType::Absolute,
                top: px(42),
                width: percent(100),
                max_height: percent(85),
                padding: UiRect::all(px(8)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            ZIndex(30),
            ScrollPosition::default(),
            crate::style::background(crate::tokens::Token::Surface),
        ))
        .id();
    button(
        world,
        controls_parent,
        panel,
        "Search / directions",
        Intent::Open,
    );
    button(
        world,
        panel,
        panel,
        "Close search / directions",
        Intent::Open,
    );
    label(
        world,
        panel,
        "Search changes nothing until you choose and save a place. Route endpoints use the visible map point.",
    );
    let query = field(world, panel, "Place search", "");
    let configuration = world
        .spawn((
            ChildOf(panel),
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
        ))
        .id();
    let fields = vec![
        query,
        field(
            world,
            configuration,
            "Local place index JSON",
            &settings.places.to_string_lossy(),
        ),
        field(
            world,
            configuration,
            "Geocoding base URL (Nominatim)",
            &settings.geocoder,
        ),
        field(
            world,
            configuration,
            "Routing base URL (OSRM)",
            &settings.router,
        ),
        field(
            world,
            configuration,
            "Provider/data credits",
            &settings.credits,
        ),
        field(
            world,
            configuration,
            "Allow provider requests (true/false)",
            if settings.online { "true" } else { "false" },
        ),
        field(world, configuration, "Regional dataset name", "my-region"),
        field(world, configuration, "Regional data date", ""),
        field(world, configuration, "Regional original source", ""),
        field(world, configuration, "Regional data license", ""),
        field(world, configuration, "Regional credits", ""),
        field(
            world,
            configuration,
            "New regional output directory",
            &path
                .parent()
                .unwrap_or(std::path::Path::new("maps"))
                .join("my-region")
                .to_string_lossy(),
        ),
        field(world, configuration, "Regional minimum zoom", "0"),
        field(world, configuration, "Regional maximum zoom", "14"),
        field(
            world,
            configuration,
            "Minimum provider request interval (milliseconds)",
            &settings.minimum_interval_ms.to_string(),
        ),
    ];
    let row = controls::row(world, panel);
    for (caption, intent) in [
        ("Providers / regional packs", Intent::Settings),
        ("Save providers", Intent::Save),
        ("Search", Intent::Search),
        ("Name map point", Intent::Reverse),
        ("Use point as start", Intent::Origin),
        ("Use point as destination", Intent::Destination),
        ("Travel mode", Intent::Mode),
        ("Find route", Intent::Route),
        ("Prepare selected region", Intent::Region),
        ("Cancel / clear", Intent::Cancel),
    ] {
        button(world, row, panel, caption, intent);
    }
    let status = label(world, panel, "");
    let results = world
        .spawn((
            ChildOf(panel),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
        ))
        .id();
    let provider = Provider::new(settings.clone()).ok().map(Arc::new);
    world.entity_mut(panel).insert(Panel {
        owner,
        path,
        settings,
        provider,
        runtime,
        point,
        origin: None,
        destination: None,
        mode: Mode::Driving,
        selection: Selection::default(),
        fields,
        results,
        status,
        configuration,
        job: None,
        selected: None,
        changed: false,
        region: None,
        scope: String::new(),
    });
}

fn apply(world: &mut World, panel: Entity, intent: Intent) {
    let result = operate(world, panel, intent);
    if let Err(error) = result
        && let Some(state) = world.get::<Panel>(panel)
    {
        controls::status(world, state.status, error);
    }
}
fn operate(world: &mut World, panel: Entity, intent: Intent) -> Result<(), String> {
    let state = world.get::<Panel>(panel).ok_or("Map navigation closed")?;
    let status = state.status;
    let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
    if matches!(intent, Intent::Open) {
        let node = &mut *world
            .get_mut::<Node>(panel)
            .ok_or("Map navigation closed")?;
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
        if node.display == Display::None {
            world.get_mut::<Panel>(panel).unwrap().job = None;
        }
        return Ok(());
    }
    if matches!(intent, Intent::Settings) {
        let configuration = state.configuration;
        let node = &mut *world
            .get_mut::<Node>(configuration)
            .ok_or("Settings panel closed")?;
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
        return Ok(());
    }
    if matches!(intent, Intent::Save) {
        let fields = state
            .fields
            .iter()
            .map(|entity| controls::value(world, *entity))
            .collect::<Result<Vec<_>, _>>()?;
        let settings = Settings {
            places: fields[1].clone().into(),
            geocoder: fields[2].clone(),
            router: fields[3].clone(),
            credits: fields[4].clone(),
            minimum_interval_ms: fields[14]
                .parse()
                .map_err(|_| "Enter a provider interval in milliseconds")?,
            online: fields[5]
                .parse()
                .map_err(|_| "Use true or false for provider requests")?,
        };
        settings.validate()?;
        let provider = Arc::new(Provider::new(settings.clone())?);
        let path = state.path.clone();
        settings.save_atomic(&path)?;
        let state = &mut *world.get_mut::<Panel>(panel).unwrap();
        state.job = None;
        state.selection.invalidate();
        state.selected = None;
        state.settings = settings;
        state.provider = Some(provider);
        state.changed = true;
        controls::status(world, status, "Providers saved; no request sent");
        return Ok(());
    }
    if let Intent::Choose(index) = intent {
        let state = &mut *world.get_mut::<Panel>(panel).unwrap();
        state.selected = state.selection.choose(index).cloned();
        state.changed = true;
        controls::status(
            world,
            status,
            "Place selected on map; use normal Save point controls to save it",
        );
        return Ok(());
    }
    if matches!(
        intent,
        Intent::Origin | Intent::Destination | Intent::Mode | Intent::Cancel
    ) {
        let state = &mut *world.get_mut::<Panel>(panel).unwrap();
        state.job = None;
        state.selection.invalidate();
        state.selected = None;
        state.changed = true;
        match intent {
            Intent::Origin => state.origin = Some(state.point),
            Intent::Destination => state.destination = Some(state.point),
            Intent::Mode => {
                state.mode = match state.mode {
                    Mode::Driving => Mode::Cycling,
                    Mode::Cycling => Mode::Walking,
                    Mode::Walking => Mode::Driving,
                }
            }
            Intent::Cancel => {
                state.origin = None;
                state.destination = None;
            }
            _ => {}
        }
        let caption = format!(
            "Mode: {:?}; start: {:?}; destination: {:?}",
            state.mode, state.origin, state.destination
        );
        controls::status(world, status, caption);
        return Ok(());
    }
    if matches!(intent, Intent::Region) {
        let (coverage, store) = state
            .region
            .clone()
            .ok_or("Draw a region and configure a local or optional download source first")?;
        let values = state
            .fields
            .iter()
            .map(|entity| controls::value(world, *entity))
            .collect::<Result<Vec<_>, _>>()?;
        let definition = lince_map::region::Definition {
            dataset: values[6].clone(),
            date: values[7].clone(),
            source: values[8].clone(),
            license: values[9].clone(),
            credits: values[10].clone(),
            coverage,
            min_zoom: values[12].parse().map_err(|_| "Choose minimum zoom 0–14")?,
            max_zoom: values[13].parse().map_err(|_| "Choose maximum zoom 0–14")?,
        };
        definition.tiles()?;
        let output = PathBuf::from(&values[11]);
        let runtime = state.runtime.clone().ok_or("Map runtime unavailable")?;
        let (cancelled, progress) = (
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Arc::new(lince_map::region::Progress {
                wake: wake
                    .clone()
                    .map(|wake| Arc::new(move || wake.ring()) as Arc<dyn Fn() + Send + Sync>),
                ..Default::default()
            }),
        );
        let (cancel, counts) = (cancelled.clone(), progress.clone());
        let (sender, response) = tokio::sync::oneshot::channel();
        let state = &mut *world.get_mut::<Panel>(panel).unwrap();
        state.job = None;
        let generation = state.selection.invalidate();
        let task = runtime.spawn(async move {
            let _ = sender.send(
                lince_map::region::prepare(store, definition, output, cancel, counts)
                    .await
                    .map(Response::Region),
            );
            if let Some(wake) = wake {
                wake.ring();
            }
        });
        state.job = Some(Job {
            task,
            response,
            generation,
            cancelled,
            progress: Some(progress),
        });
        controls::status(world, status, "Preparing selected region…");
        return Ok(());
    }
    let provider = state.provider.clone().ok_or("Configure providers first")?;
    let query = controls::value(world, state.fields[0])?;
    let (point, origin, destination, mode) =
        (state.point, state.origin, state.destination, state.mode);
    let runtime = state
        .runtime
        .clone()
        .ok_or("Map provider runtime unavailable")?;
    let (sender, response) = tokio::sync::oneshot::channel();
    let state = &mut *world.get_mut::<Panel>(panel).unwrap();
    state.job = None;
    let generation = state.selection.invalidate();
    state.changed = true;
    let task = runtime.spawn(async move {
        let result = match intent {
            Intent::Search => provider.search(&query).await.map(Response::Places),
            Intent::Reverse => provider
                .reverse(point)
                .await
                .map(|place| Response::Places(vec![place])),
            Intent::Route => match (origin, destination) {
                (Some(a), Some(b)) => provider
                    .route(&[a, b], mode, nucleus::execution::now().timestamp())
                    .await
                    .map(Response::Route),
                _ => Err("Select start and destination from visible map points".into()),
            },
            _ => Err("Unknown navigation operation".into()),
        };
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    state.job = Some(Job {
        task,
        response,
        generation,
        cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        progress: None,
    });
    controls::status(world, status, "Loading…");
    Ok(())
}

fn poll(world: &mut World) {
    let panels = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .map(|(entity, _)| entity)
        .collect::<Vec<_>>();
    for entity in panels {
        let mut job = match world.get_mut::<Panel>(entity).unwrap().job.take() {
            Some(job) => job,
            None => continue,
        };
        if let Some(progress) = &job.progress {
            let caption = format!(
                "Region: {}/{} tiles · {} bytes",
                progress
                    .completed
                    .load(std::sync::atomic::Ordering::Relaxed),
                progress.total.load(std::sync::atomic::Ordering::Relaxed),
                progress.bytes.load(std::sync::atomic::Ordering::Relaxed)
            );
            let status = world.get::<Panel>(entity).unwrap().status;
            controls::status(world, status, caption);
        }
        let result = match job.response.try_recv() {
            Ok(result) => result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                world.get_mut::<Panel>(entity).unwrap().job = Some(job);
                continue;
            }
            Err(_) => Err("Provider request cancelled".into()),
        };
        let (status, results) = {
            let panel = world.get::<Panel>(entity).unwrap();
            (panel.status, panel.results)
        };
        controls::clear(world, results);
        match result {
            Ok(Response::Places(places)) => {
                let state = &mut *world.get_mut::<Panel>(entity).unwrap();
                if state.selection.accept_places(job.generation, places) {
                    let places = state.selection.places.clone();
                    for (index, place) in places.iter().enumerate() {
                        button(
                            world,
                            results,
                            entity,
                            &format!("Choose {} · {}", place.name, place.credits),
                            Intent::Choose(index),
                        );
                    }
                    controls::status(
                        world,
                        status,
                        if places.is_empty() {
                            "No places in provider coverage"
                        } else {
                            "Choose a result; nothing has been saved"
                        },
                    );
                }
            }
            Ok(Response::Route(route)) => {
                let arrival = route
                    .calculated_at
                    .checked_add(route.duration_seconds.ceil() as i64)
                    .and_then(|time| chrono::DateTime::from_timestamp(time, 0))
                    .map(|time| time.to_rfc3339())
                    .unwrap_or_else(|| "unavailable".into());
                let caption = format!(
                    "{:?}: {:.1} km · {:.0} min estimate · calculated {} · {}",
                    route.mode,
                    route.distance_metres / 1000.0,
                    route.duration_seconds / 60.0,
                    chrono::DateTime::from_timestamp(route.calculated_at, 0)
                        .map(|date| date.to_rfc3339())
                        .unwrap_or_else(|| "unknown time".into()),
                    route.credits
                );
                let caption = format!("{caption} · estimated arrival {arrival}");
                let state = &mut *world.get_mut::<Panel>(entity).unwrap();
                if state.selection.accept_route(job.generation, route) {
                    state.changed = true;
                    controls::status(world, status, caption);
                }
            }
            Ok(Response::Region(manifest)) => controls::status(
                world,
                status,
                format!(
                    "Regional pack ready: {} · {} files · {}",
                    manifest.dataset,
                    manifest.files.len(),
                    manifest.credits
                ),
            ),
            Err(error) => controls::status(world, status, error),
        }
    }
}

pub fn update(
    world: &mut World,
    owner: Entity,
    point: Coordinate,
    scope: &str,
) -> (bool, Option<Place>) {
    let panel = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| panel.owner == owner)
        .map(|(entity, _)| entity);
    let Some(panel) = panel else {
        return (false, None);
    };
    let state = &mut *world.get_mut::<Panel>(panel).unwrap();
    state.point = point;
    if state.scope != scope {
        state.scope = scope.into();
        state.job = None;
        state.selection.invalidate();
        state.selected = None;
        state.origin = None;
        state.destination = None;
        state.changed = true;
        let results = state.results;
        let status = state.status;
        let query = state.fields[0];
        if let Some(mut editor) = world.get_mut::<bevy::text::EditableText>(query) {
            editor.editor.set_text("");
        }
        controls::clear(world, results);
        controls::status(world, status, "");
    }
    let state = &mut *world.get_mut::<Panel>(panel).unwrap();
    (std::mem::take(&mut state.changed), state.selected.take())
}
pub fn draw_route(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    camera: Camera,
    size: [f64; 2],
) {
    let points = world
        .query::<&Panel>()
        .iter(world)
        .find(|panel| panel.owner == owner)
        .and_then(|panel| panel.selection.route.as_ref())
        .map(|route| route.points.clone())
        .unwrap_or_default();
    let stride = points.len().div_ceil(2048).max(1);
    let mut sampled = points.iter().step_by(stride).copied().collect::<Vec<_>>();
    if let Some(last) = points.last()
        && sampled.last() != Some(last)
    {
        sampled.push(*last);
    }
    for line in sampled.windows(2) {
        let a = camera.screen(line[0], size);
        let b = camera.screen(line[1], size);
        if [a, b].iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if length > size[0].max(size[1]) * 2.0 {
            continue;
        }
        world.spawn((
            ChildOf(parent),
            Node {
                position_type: PositionType::Absolute,
                left: px(((a[0] + b[0]) / 2.0 - length / 2.0) as f32),
                top: px(((a[1] + b[1]) / 2.0) as f32),
                width: px(length as f32),
                height: px(3),
                ..default()
            },
            UiTransform::from_rotation(Rot2::radians((b[1] - a[1]).atan2(b[0] - a[0]) as f32)),
            crate::style::background(crate::tokens::Token::Accent),
            Pickable::IGNORE,
        ));
    }
}

pub fn region(
    world: &mut World,
    owner: Entity,
    bounds: Option<lince_map::Bounds>,
    store: Option<Arc<lince_map::TileStore>>,
) {
    let panel = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, panel)| panel.owner == owner)
        .map(|(entity, _)| entity);
    if let Some(panel) = panel {
        world.get_mut::<Panel>(panel).unwrap().region = bounds.zip(store);
    }
}
