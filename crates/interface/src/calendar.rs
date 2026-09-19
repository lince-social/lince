mod model;
mod persistence;
mod picker;
pub(crate) mod tests;
mod ui;

use crate::{actions::Action, area::InfluenceArea, workspace::WorkspaceMember};
use bevy::{math::DVec2, prelude::*};
pub use model::Calendar;
pub(crate) use persistence::{SavedCalendar, snapshot};
pub(crate) use picker::date_button;
pub(crate) use ui::store_entry;

pub const DATE_SELECTED: &str = "Date selected";

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::SYMBOLS,
    crate::credits::FONTIQUE,
    crate::credits::Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and Chrono contributors",
        license: crate::credits::CHRONO_LICENSE,
    },
    crate::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    crate::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
];

#[derive(Component, Clone)]
pub struct CalendarSand(pub Calendar);

#[derive(Component, Default)]
struct View {
    scroll: f32,
    rendered: Option<Calendar>,
    feed: String,
    body: Option<Entity>,
    selector: Option<Selector>,
    page: usize,
    error: Option<String>,
}

#[derive(Clone, Copy)]
enum Selector {
    Month,
    Year(i32),
    Source,
}

#[derive(Component)]
pub(crate) struct Picker {
    row: Entity,
    editor: Entity,
}

pub struct CalendarPlugin;
impl Plugin for CalendarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            update.after(crate::protein_area::UpdateProteinAreas),
        );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    calendar: Calendar,
) -> Entity {
    let entity = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(CREDITS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(840.0, 680.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            CalendarSand(if calendar.valid() {
                calendar
            } else {
                Calendar::default()
            }),
            View::default(),
        ))
        .observe(scroll)
        .id();
    ui::render(world, entity);
    entity
}

fn scroll(
    mut event: On<Pointer<bevy::picking::events::Scroll>>,
    mut views: Query<&mut View>,
    mut commands: Commands,
) {
    if !event.y.is_finite() || event.y.abs() < f32::EPSILON {
        return;
    }
    let entity = event.entity;
    let Ok(mut view) = views.get_mut(entity) else {
        return;
    };
    let delta = scroll_steps(&mut view.scroll, event.y, event.unit);
    if delta != 0 {
        commands.queue(move |world: &mut World| Command::Move(delta).apply(world, entity));
    }
    event.propagate(false);
}

fn scroll_steps(
    accumulated: &mut f32,
    amount: f32,
    unit: bevy::input::mouse::MouseScrollUnit,
) -> i32 {
    *accumulated += if unit == bevy::input::mouse::MouseScrollUnit::Line {
        amount
    } else {
        amount / 60.0
    };
    let steps = accumulated.trunc().clamp(-12.0, 12.0) as i32;
    *accumulated = accumulated.fract();
    -steps
}

fn area(world: &mut World, owner: Entity) -> Option<Entity> {
    let id = world.get::<CalendarSand>(owner)?.0.area.clone()?;
    let root = world.get::<ChildOf>(owner)?.parent();
    let workspace = world.get::<WorkspaceMember>(owner)?.0;
    world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .find(|(_, area, parent, member)| {
            area.id == id
                && parent.parent() == root
                && member.0 == workspace
                && area.protein.is_some()
        })
        .map(|(entity, _, _, _)| entity)
}

fn update(world: &mut World) {
    if crate::laboratory::active(world) {
        return;
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<CalendarSand>>()
        .iter(world)
        .collect();
    for owner in owners {
        if let Some(picker) = world.get::<Picker>(owner)
            && (world.get_entity(picker.row).is_err()
                || world
                    .get::<bevy::text::EditableText>(picker.editor)
                    .is_none()
                || world.get::<crate::canvas_selection::SandGroup>(picker.row)
                    != world.get::<crate::canvas_selection::SandGroup>(owner))
        {
            world.despawn(owner);
            continue;
        }
        let picker_key = picker::sync(world, owner);
        let feed = area(world, owner)
            .and_then(|a| crate::protein_area::calendar_feed(world, a).map(|(_, key)| key))
            .unwrap_or(picker_key);
        let current = &world.get::<CalendarSand>(owner).unwrap().0;
        let view = world.get::<View>(owner).unwrap();
        if view.rendered.as_ref() != Some(current) || view.feed != feed {
            world.get_mut::<View>(owner).unwrap().feed = feed;
            ui::render(world, owner);
        }
    }
}

#[derive(Clone)]
enum Command {
    Create,
    Move(i32),
    Select(String),
    Selector(Option<Selector>),
    Month(u32),
    Year(i32),
    Endpoint(bool),
    Clear,
    Source(Option<String>),
    Page(usize),
    Close,
    Record(crate::protein_area::RecordBinding),
}

impl Action for Command {
    fn apply(&self, world: &mut World, owner: Entity) {
        if crate::laboratory::suspended(world, owner) {
            return;
        }
        if matches!(self, Self::Create) {
            let Some(workspace) = world
                .get::<crate::workspace::Workspaces>(owner)
                .map(|w| w.active)
            else {
                return;
            };
            let position = world
                .get::<crate::canvas::CanvasView>(owner)
                .map_or(DVec2::ZERO, |v| v.center);
            spawn(world, owner, workspace, position, Calendar::default());
            return;
        }
        if matches!(self, Self::Close) {
            if world.get::<Picker>(owner).is_some() {
                world.despawn(owner);
            } else if let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) {
                crate::deletion::request(world, root, vec![owner]);
            }
            return;
        }
        let Some(mut model) = world.get::<CalendarSand>(owner).map(|s| s.0.clone()) else {
            return;
        };
        match self {
            Self::Move(delta) => {
                model.move_months(*delta);
                world.get_mut::<View>(owner).unwrap().page = 0;
                world.get_mut::<View>(owner).unwrap().selector = None;
            }
            Self::Month(month) => {
                model.month = *month;
                world.get_mut::<View>(owner).unwrap().selector = None;
                world.get_mut::<View>(owner).unwrap().page = 0;
            }
            Self::Year(year) => {
                model.year = *year;
                world.get_mut::<View>(owner).unwrap().selector = None;
                world.get_mut::<View>(owner).unwrap().page = 0;
            }
            Self::Selector(selector) => world.get_mut::<View>(owner).unwrap().selector = *selector,
            Self::Endpoint(end) => model.selecting_end = *end,
            Self::Clear => {
                model.start = None;
                model.end = None;
            }
            Self::Page(page) => world.get_mut::<View>(owner).unwrap().page = *page,
            Self::Source(id) => {
                model.area = id.clone();
                world.get_mut::<CalendarSand>(owner).unwrap().0 = model.clone();
                if let Some(entity) = area(world, owner) {
                    let mut influence = world.get_mut::<InfluenceArea>(entity).unwrap();
                    if let Some(config) = influence.protein.as_mut() {
                        config.calendar_dates = true;
                    }
                }
                world.get_mut::<View>(owner).unwrap().selector = None;
            }
            Self::Select(date) => {
                if let Err(error) = model.select(date) {
                    world.get_mut::<View>(owner).unwrap().error = Some(error.into());
                    ui::render(world, owner);
                    return;
                }
                world.get_mut::<CalendarSand>(owner).unwrap().0 = model.clone();
                crate::scoped_events::emit(
                    world,
                    owner,
                    DATE_SELECTED,
                    serde_json::Value::String(date.clone()),
                );
            }
            Self::Record(binding) => {
                if let Some(root) = world.get::<ChildOf>(owner).map(ChildOf::parent) {
                    world.trigger(crate::protein_area::RecordClicked {
                        entity: root,
                        sand: owner,
                        uid: binding.uid.clone(),
                        source: binding.source.clone(),
                    });
                }
                return;
            }
            _ => {}
        }
        if !model.valid() || world.get_entity(owner).is_err() {
            return;
        }
        world.get_mut::<CalendarSand>(owner).unwrap().0 = model;
        world.get_mut::<View>(owner).unwrap().error = None;
        ui::render(world, owner);
    }
}
