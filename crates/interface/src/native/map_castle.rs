mod diagnostics;
mod feed;
mod input;
mod locating;
mod paging;
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

pub(crate) fn laboratory_cases(cases: &mut Vec<crate::native::laboratory::Case>) {
    tests::laboratory_cases(cases);
    feed::tests::laboratory_cases(cases);
    settings::tests::laboratory_cases(cases);
}

pub const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "Galileo",
        author: "Maxim Gritsenko and contributors",
        license: include_str!("../../../map/licenses/galileo-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Natural Earth",
        author: "Tom Patterson, Nathaniel Vaughn Kelso and contributors",
        license: include_str!("../../../map/licenses/natural-earth.txt"),
    },
    crate::native::credits::Attribution {
        name: "PMTiles for Rust",
        author: "Luke Seelenbinder, Yuri Astrakhan and contributors",
        license: include_str!("../../../map/licenses/pmtiles-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "GeoZero",
        author: "Pirmin Kalberer and contributors",
        license: include_str!("../../../map/licenses/geozero-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "GeoJSON for Rust",
        author: "GeoRust contributors",
        license: include_str!("../../../map/licenses/geojson-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "OpenStreetMap",
        author: "OpenStreetMap contributors",
        license: include_str!("../../../map/licenses/ODbL.txt"),
    },
    crate::native::credits::Attribution {
        name: "ESA WorldCover",
        author: "ESA WorldCover consortium",
        license: include_str!("../../../map/licenses/ESA-WorldCover.txt"),
    },
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
    },
    crate::native::credits::DEJAVU,
    crate::native::credits::Attribution {
        name: "wgpu",
        author: "gfx-rs developers",
        license: include_str!("../../../map/licenses/wgpu-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Linux location portal · zbus",
        author: "zbus contributors",
        license: include_str!("../../licenses/native/zbus-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Asynchronous streams",
        author: "futures-rs contributors",
        license: include_str!("../../licenses/native/futures-MIT.txt"),
    },
];

#[derive(Component)]
pub struct MapSand {
    pub camera: Camera,
    pub records: Vec<String>,
    pub person: String,
    pub area: Option<String>,
    pub authority_node_id: Option<String>,
    invitation: Option<nucleus::location::Invitation>,
    live_offset: usize,
    record_list: Entity,
    show_saved: bool,
    show_live: bool,
    show_public: bool,
    public_markers: Vec<feed::Marker>,
    public_deadline: Option<std::time::Instant>,
    device_centered: bool,
    keyboard_anchor: Option<Coordinate>,
    diagnostics: Option<Entity>,
    pending_input: Option<std::time::Instant>,
    source_picker: Option<Entity>,
    auto_center: bool,
    viewport: Entity,
    status: Entity,
    overlay: Entity,
    selected_text: Entity,
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
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .map(|member| member.0)
        });
        if let Some(spaces) = world.get::<crate::native::workspace::Workspaces>(entity) {
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
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_systems(
                Update,
                (
                    settings::initialize,
                    paging::receive,
                    settings::poll,
                    paging::maintain,
                    locating::maintain,
                    input::maintain,
                    feed::receive,
                    feed::maintain,
                    ui::update,
                    diagnostics::maintain,
                )
                    .chain(),
            );
        app.add_plugins(crate::map_navigation::NavigationPlugin);
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
            crate::native::castle::Castle,
            crate::native::sand::Square,
            crate::native::sand::InBox(root),
            ChildOf(root),
            crate::native::workspace::WorkspaceMember(workspace),
            crate::native::sand_store::SandCredits(CREDITS),
            crate::native::canvas::CanvasItem {
                position,
                size: Vec2::new(900.0, 680.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
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
    let owner = crate::native::sand_panel::column(world, parent);
    world
        .entity_mut(owner)
        .insert(crate::native::sand_store::SandCredits(CREDITS));
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
        vec![invitation.record_uid.clone()],
        person,
    );
    world.get_mut::<MapSand>(owner).unwrap().authority_node_id =
        Some(invitation.authority_node_id.clone());
    if invitation.grant.is_some() {
        world.get_mut::<MapSand>(owner).unwrap().invitation = Some(invitation);
    }
    Ok(owner)
}

#[derive(Clone)]
pub(crate) struct PublicTransport(pub nucleus::social::transport::Announcement, pub i64);

impl crate::native::actions::Action for PublicTransport {
    fn apply(&self, world: &mut World, target: Entity) {
        if self.0.validate().is_err() {
            return;
        }
        let remaining = self
            .1
            .saturating_sub(chrono::Utc::now().timestamp())
            .clamp(0, nucleus::social::MAX_LIFETIME);
        if remaining == 0 {
            return;
        }
        let Some((root, workspace)) = ui::scope(world, target) else {
            return;
        };
        let owner = spawn(
            world,
            root,
            workspace,
            DVec2::ZERO,
            Camera::default(),
            Vec::new(),
            String::new(),
        );
        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
        for (label, area) in [
            ("Public origin area", &self.0.origin),
            ("Public destination area", &self.0.destination),
        ] {
            sand.public_markers.push(feed::Marker {
                record: label.into(),
                coordinate: Coordinate {
                    latitude: f64::from(area.latitude_e5) / 100_000.0,
                    longitude: f64::from(area.longitude_e5) / 100_000.0,
                },
                live: false,
                view: None,
                observed: engine::location::clock::Instant::now(),
                radius_metres: Some(area.radius_metres),
            });
        }
        sand.camera.center = sand.public_markers[0].coordinate;
        sand.camera.zoom = 10.0;
        sand.public_deadline =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(remaining as u64));
        sand.notice =
            "Public approximate route. Exact addresses and live sharing are separate.".into();
    }
}

#[derive(Clone)]
struct Open {
    records: Vec<String>,
    person: String,
}

impl crate::native::actions::Action for Open {
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
    crate::native::sand_panel::button(
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
    CopyDiagnostics,
    ResetDiagnostics,
    Diagnostics,
    FocusRecord(String),
    RecordListPage(isize),
    Pan([f64; 2]),
    PickCenter,
    ClearSelection,
    PublicLayer,
    Locate,
    SavedLayer,
    LiveLayer,
    More,
    OpenRecord(String),
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

impl crate::native::actions::Action for Command {
    fn apply(&self, world: &mut World, target: Entity) {
        ui::command(world, target, self.clone());
    }
}
