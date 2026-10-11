mod feed;
mod persistence;
mod render;
mod settings;
mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use lince_map::{Bounds, Camera, Coordinate, Progress, Tile, TileStore, scene::Scene};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) use persistence::{SavedMap, snapshot};
pub(crate) use settings::settings_button;
pub(crate) use ui::store_entry;

pub(crate) fn laboratory_cases(cases: &mut Vec<crate::laboratory::Case>) {
    tests::laboratory_cases(cases);
    feed::tests::laboratory_cases(cases);
    settings::tests::laboratory_cases(cases);
}

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "Galileo",
        author: "Maxim Gritsenko and contributors",
        license: include_str!("../../map/licenses/galileo-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Natural Earth",
        author: "Tom Patterson, Nathaniel Vaughn Kelso and contributors",
        license: include_str!("../../map/licenses/natural-earth.txt"),
    },
    crate::credits::Attribution {
        name: "PMTiles for Rust",
        author: "Luke Seelenbinder, Yuri Astrakhan and contributors",
        license: include_str!("../../map/licenses/pmtiles-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "GeoZero",
        author: "Pirmin Kalberer and contributors",
        license: include_str!("../../map/licenses/geozero-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "GeoJSON for Rust",
        author: "GeoRust contributors",
        license: include_str!("../../map/licenses/geojson-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "OpenStreetMap",
        author: "OpenStreetMap contributors",
        license: include_str!("../../map/licenses/ODbL.txt"),
    },
    crate::credits::Attribution {
        name: "ESA WorldCover",
        author: "ESA WorldCover consortium",
        license: include_str!("../../map/licenses/ESA-WorldCover.txt"),
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
    crate::credits::DEJAVU,
];

#[derive(Component)]
pub struct MapSand {
    pub camera: Camera,
    pub records: Vec<String>,
    pub person: String,
    pub area: Option<String>,
    pub authority_node_id: Option<String>,
    source_picker: Option<Entity>,
    auto_center: bool,
    viewport: Entity,
    status: Entity,
    overlay: Entity,
    selected_text: Entity,
    zoom_field: Entity,
    size: [u32; 2],
    image: Handle<Image>,
    scene: Option<Arc<Mutex<Scene>>>,
    source_revision: u64,
    rendered_camera: Option<Camera>,
    select_area: bool,
    selection: Option<Bounds>,
    drag_start: Option<[f64; 2]>,
    picked: Option<Coordinate>,
    markers: Vec<feed::Marker>,
    marker_stamp: String,
    job: Option<Job>,
    notice: String,
    last_status: String,
    next_wake: std::time::Instant,
}

struct Job {
    tiles: Vec<Tile>,
    cancelled: Arc<AtomicBool>,
    finished: Arc<Mutex<Option<Progress>>>,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Component)]
struct Viewport(Entity);

pub struct MapPlugin;

fn visible(world: &World, mut entity: Entity) -> bool {
    let mut workspace = None;
    loop {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
        {
            return false;
        }
        workspace = workspace.or_else(|| {
            world
                .get::<crate::workspace::WorkspaceMember>(entity)
                .map(|member| member.0)
        });
        if let Some(spaces) = world.get::<crate::workspace::Workspaces>(entity) {
            return workspace.is_none_or(|workspace| workspace == spaces.active);
        }
        let Some(parent) = world.get::<ChildOf>(entity) else {
            return true;
        };
        entity = parent.parent();
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<settings::Sources>()
            .init_resource::<render::Scenes>()
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                (
                    settings::initialize,
                    feed::receive,
                    feed::maintain,
                    ui::update,
                )
                    .chain(),
            );
        render::install(app);
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    camera: Camera,
    records: Vec<String>,
    person: String,
) -> Entity {
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            crate::workspace::WorkspaceMember(workspace),
            crate::sand_store::SandCredits(CREDITS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(900.0, 680.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
        ))
        .id();
    ui::mount(world, owner, camera, records, person);
    owner
}

pub fn mount(
    world: &mut World,
    parent: Entity,
    camera: Camera,
    records: Vec<String>,
    person: String,
) -> Entity {
    let owner = crate::sand_panel::column(world, parent);
    world
        .entity_mut(owner)
        .insert(crate::sand_store::SandCredits(CREDITS));
    ui::mount(world, owner, camera, records, person);
    owner
}

pub fn observe(
    world: &mut World,
    parent: Entity,
    invitation: nucleus::location::Invitation,
    person: String,
) -> Result<Entity, String> {
    invitation.reference().map_err(str::to_owned)?;
    let owner = mount(
        world,
        parent,
        Camera::default(),
        vec![invitation.record_uid],
        person,
    );
    world.get_mut::<MapSand>(owner).unwrap().authority_node_id = Some(invitation.authority_node_id);
    Ok(owner)
}

#[derive(Clone)]
struct Open {
    records: Vec<String>,
    person: String,
}

impl crate::actions::Action for Open {
    fn apply(&self, world: &mut World, target: Entity) {
        let existing = world.get::<Children>(target).and_then(|children| {
            children
                .iter()
                .find(|child| world.get::<MapSand>(*child).is_some())
        });
        if let Some(existing) = existing {
            world.despawn(existing);
            return;
        }
        mount(
            world,
            target,
            Camera::default(),
            self.records.clone(),
            self.person.clone(),
        );
    }
}

pub fn record_controls(world: &mut World, parent: Entity, record: &str, person: Option<&str>) {
    group_controls(world, parent, "Map", vec![record.into()], person);
}

pub fn group_controls(
    world: &mut World,
    parent: Entity,
    label: &str,
    mut records: Vec<String>,
    person: Option<&str>,
) {
    records.sort();
    records.dedup();
    records.truncate(32);
    crate::sand_panel::button(
        world,
        parent,
        parent,
        label,
        Open {
            records,
            person: person.unwrap_or_default().into(),
        },
    );
}

#[derive(Clone)]
enum Command {
    Create,
    Zoom(f64),
    Select,
    Download,
    Cancel,
    Retry,
    SavePoint,
    Center,
    SourcePicker,
    Source(Option<String>),
    RecordCenter,
    SharedPicker,
    SharedReference(Entity),
}

impl crate::actions::Action for Command {
    fn apply(&self, world: &mut World, target: Entity) {
        ui::command(world, target, self.clone());
    }
}
