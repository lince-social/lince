mod panel;
mod requests;
#[cfg(test)]
mod tests;

pub use panel::mount;

use crate::{
    controls::{clear, status, value},
    style::background,
    tokens::Token,
};
use bevy::{prelude::*, text::EditableText};
use lince_map::{
    Bounds, Camera, Coordinate, TileStore,
    navigation::{Mode, Place, Provider, Selection, Settings},
};
use panel::{Intent, toggle};
use requests::{Job, poll};
use std::{mem::take, path::PathBuf, sync::Arc};
use tokio::runtime::Handle;

const ROUTE_SAMPLE_TARGET: usize = 2048;
const CONFIGURATION_FIELDS: usize = 14;

#[derive(Component)]
struct Panel {
    owner: Entity,
    path: PathBuf,
    provider: Option<Arc<Provider>>,
    runtime: Option<Handle>,
    point: Coordinate,
    origin: Option<Coordinate>,
    destination: Option<Coordinate>,
    mode: Mode,
    selection: Selection,
    query: Entity,
    configuration_fields: [Entity; CONFIGURATION_FIELDS],
    results: Entity,
    status: Entity,
    configuration: Entity,
    job: Option<Job>,
    selected: Option<Place>,
    changed: bool,
    region: Option<(Bounds, Arc<TileStore>)>,
    scope: String,
}

impl Panel {
    fn get_mut(world: &mut World, entity: Entity) -> Result<Mut<'_, Self>, String> {
        world
            .get_mut(entity)
            .ok_or_else(|| "Map navigation closed".into())
    }

    fn for_owner(world: &mut World, owner: Entity) -> Option<Entity> {
        world
            .query::<(Entity, &Self)>()
            .iter(world)
            .find_map(|(entity, panel)| (panel.owner == owner).then_some(entity))
    }

    fn reset_selection(&mut self) {
        self.job = None;
        self.selection.invalidate();
        self.selected = None;
        self.changed = true;
    }

    fn configuration_values(
        &self,
        world: &World,
    ) -> Result<[String; CONFIGURATION_FIELDS], String> {
        // A composing editor blocks settings actions, including edits to the search query.
        value(world, self.query)?;
        let mut values = [const { String::new() }; CONFIGURATION_FIELDS];
        for (text, entity) in values.iter_mut().zip(self.configuration_fields) {
            *text = value(world, entity)?;
        }
        Ok(values)
    }
}

#[derive(Clone, Copy)]
enum RouteEdit {
    Origin,
    Destination,
    CycleMode,
    Cancel,
}

impl RouteEdit {
    fn apply(self, panel: &mut Panel) -> String {
        match self {
            Self::Origin => panel.origin = Some(panel.point),
            Self::Destination => panel.destination = Some(panel.point),
            Self::CycleMode => {
                panel.mode = match panel.mode {
                    Mode::Driving => Mode::Cycling,
                    Mode::Cycling => Mode::Walking,
                    Mode::Walking => Mode::Driving,
                };
            }
            Self::Cancel => {
                panel.origin = None;
                panel.destination = None;
            }
        }
        format!(
            "Mode: {:?}; start: {:?}; destination: {:?}",
            panel.mode, panel.origin, panel.destination
        )
    }
}

pub struct NavigationPlugin;
impl Plugin for NavigationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, poll);
    }
}

fn operate(world: &mut World, entity: Entity, intent: Intent) -> Result<(), String> {
    let panel = world.get::<Panel>(entity).ok_or("Map navigation closed")?;
    let status_entity = panel.status;
    let caption = match intent {
        Intent::Open => {
            if toggle(world, entity).ok_or("Map navigation closed")? == Display::None {
                Panel::get_mut(world, entity)?.job = None;
            }
            return Ok(());
        }
        Intent::ToggleSettings => {
            toggle(world, panel.configuration).ok_or("Settings panel closed")?;
            return Ok(());
        }
        Intent::Save => {
            let [places, geocoder, router, credits, online, .., interval] =
                panel.configuration_values(world)?;
            let settings = Settings {
                places: places.into(),
                geocoder,
                router,
                credits,
                minimum_interval_ms: interval
                    .parse()
                    .map_err(|_| "Enter a provider interval in milliseconds")?,
                online: online
                    .parse()
                    .map_err(|_| "Use true or false for provider requests")?,
            };
            let provider = Arc::new(Provider::new(settings.clone())?);
            settings.save_atomic(&panel.path)?;
            let mut panel = Panel::get_mut(world, entity)?;
            panel.reset_selection();
            panel.provider = Some(provider);
            "Providers saved; no request sent"
        }
        Intent::Choose(index) => {
            let mut panel = Panel::get_mut(world, entity)?;
            panel.selected = panel.selection.choose(index).cloned();
            panel.changed = true;
            "Place selected on map; use normal Save point controls to save it"
        }
        Intent::EditRoute(edit) => {
            let mut panel = Panel::get_mut(world, entity)?;
            panel.reset_selection();
            let caption = edit.apply(&mut panel);
            status(world, status_entity, caption);
            return Ok(());
        }
        Intent::PrepareRegion => {
            requests::prepare_region(world, entity)?;
            "Preparing selected region…"
        }
        Intent::Request(request) => {
            requests::lookup(world, entity, request)?;
            "Loading…"
        }
    };
    status(world, status_entity, caption);
    Ok(())
}

pub fn update(
    world: &mut World,
    owner: Entity,
    point: Coordinate,
    scope: &str,
) -> (bool, Option<Place>) {
    let Some(mut panel) =
        Panel::for_owner(world, owner).and_then(|entity| world.get_mut::<Panel>(entity))
    else {
        return (false, None);
    };
    panel.point = point;
    if panel.scope == scope {
        return (take(&mut panel.changed), panel.selected.take());
    }
    scope.clone_into(&mut panel.scope);
    panel.reset_selection();
    panel.origin = None;
    panel.destination = None;
    let Panel {
        results,
        status: status_entity,
        query,
        ..
    } = *panel;
    panel.changed = false;
    if let Some(mut editor) = world.get_mut::<EditableText>(query) {
        editor.editor.set_text("");
    }
    clear(world, results);
    status(world, status_entity, "");
    (true, None)
}

pub fn draw_route(
    world: &mut World,
    owner: Entity,
    parent: Entity,
    camera: Camera,
    size: [f64; 2],
) {
    let Some(route) = world
        .query::<&Panel>()
        .iter(world)
        .find(|panel| panel.owner == owner)
        .and_then(|panel| panel.selection.route.as_ref())
    else {
        return;
    };
    let stride = route.points.len().div_ceil(ROUTE_SAMPLE_TARGET).max(1);
    let mut sampled = route
        .points
        .iter()
        .step_by(stride)
        .copied()
        .collect::<Vec<_>>();
    if let Some(&last) = route.points.last()
        && sampled.last() != Some(&last)
    {
        sampled.push(last);
    }
    let max_length = size[0].max(size[1]) * 2.0;
    for &[start, end] in sampled.array_windows() {
        let (a, b) = (camera.screen(start, size), camera.screen(end, size));
        if [a, b].into_iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = (dx.powi(2) + dy.powi(2)).sqrt();
        if length > max_length {
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
            UiTransform::from_rotation(Rot2::radians(dy.atan2(dx) as f32)),
            background(Token::Accent),
            Pickable::IGNORE,
        ));
    }
}

pub fn region(
    world: &mut World,
    owner: Entity,
    bounds: Option<Bounds>,
    store: Option<Arc<TileStore>>,
) {
    if let Some(mut panel) =
        Panel::for_owner(world, owner).and_then(|entity| world.get_mut::<Panel>(entity))
    {
        panel.region = bounds.zip(store);
    }
}
