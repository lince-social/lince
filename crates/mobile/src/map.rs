use crate::{
    app::{self, Mobile},
    navigation::Page,
};
use bevy::{
    picking::events::{Click, Drag, Press, Release},
    prelude::*,
};
use engine::location::clock::Instant;
use lince_interface::map_view::{
    self,
    data::{
        self, Bounds, Camera, Coordinate, Settings, TileStore,
        scene::{Palette, Scene},
    },
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Clone)]
pub enum Intent {
    Pan([f64; 2]),
    PickCenter,
    Corner,
    Locations,
    Focus(String),
    ListPage(isize),
    Zoom(f64),
    World,
    Layer(usize),
    More,
    SelectArea,
    SavePoint,
    Download,
    CancelDownload,
    Sources,
    SaveSources,
    #[cfg(target_os = "android")]
    ChoosePack,
    Online,
    Locate,
    Credits,
    Theme,
    PublicRoute(nucleus::social::transport::Announcement, i64),
}

#[derive(Clone)]
struct Marker {
    record: String,
    point: Coordinate,
    radius: Option<u32>,
    layer: usize,
    view: Option<nucleus::location::View>,
    received: Instant,
}

enum Kind {
    Saved,
    Context(String),
    Live(String),
    Save,
}
struct Pending {
    kind: Kind,
    epoch: u64,
    sent: Instant,
}

#[derive(Resource)]
pub struct State {
    scope: String,
    records: Vec<String>,
    invitation: Option<nucleus::location::Invitation>,
    auto_center: bool,
    next_wake: Instant,
    rows: Vec<(String, String)>,
    row_page: usize,
    list_open: bool,
    camera: Camera,
    surface: Option<Entity>,
    viewport_size: [u32; 2],
    settings: Settings,
    store: Option<Arc<TileStore>>,
    markers: Vec<Marker>,
    pending: HashMap<String, Pending>,
    generation: String,
    epoch: u64,
    after: Option<String>,
    window_start: Option<String>,
    refresh_at: Instant,
    seen: usize,
    done: bool,
    limited: bool,
    next: Instant,
    live_index: usize,
    layers: [bool; 3],
    selection: Option<Bounds>,
    picked: Option<Coordinate>,
    select_area: bool,
    anchor: Option<Coordinate>,
    touches: HashMap<bevy::picking::pointer::PointerId, [f64; 2]>,
    dragged: bool,
    notice: String,
    sources_open: bool,
    credits_open: bool,
    online: bool,
    public_deadline: Option<Instant>,
    locate_epoch: Option<u64>,
    locate_started: Option<Instant>,
    device_centered: bool,
    download: Option<Download>,
    import: Option<Import>,
    picked_pack: Option<PickedPack>,
    changed: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            scope: String::new(),
            records: Vec::new(),
            invitation: None,
            auto_center: false,
            next_wake: Instant::now(),
            rows: Vec::new(),
            row_page: 0,
            list_open: false,
            camera: Camera::default(),
            surface: None,
            viewport_size: [400, 400],
            settings: Settings::default(),
            store: None,
            markers: Vec::new(),
            pending: HashMap::new(),
            generation: nucleus::new_uid("map-generation"),
            epoch: 0,
            after: None,
            window_start: None,
            refresh_at: Instant::now() + Duration::from_secs(5),
            seen: 0,
            done: false,
            limited: false,
            next: Instant::now(),
            live_index: 0,
            layers: [true; 3],
            selection: None,
            picked: None,
            select_area: false,
            anchor: None,
            touches: HashMap::new(),
            dragged: false,
            notice: String::new(),
            sources_open: false,
            credits_open: false,
            online: false,
            public_deadline: None,
            locate_epoch: None,
            locate_started: None,
            device_centered: false,
            download: None,
            import: None,
            picked_pack: None,
            changed: true,
        }
    }
}

struct Download {
    cancelled: Arc<AtomicBool>,
    result: Arc<Mutex<Option<data::Progress>>>,
    tiles: Vec<data::Tile>,
}
impl Drop for Download {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
struct Import {
    task: tokio::task::JoinHandle<()>,
    receiver: tokio::sync::oneshot::Receiver<Result<(Settings, Arc<TileStore>), String>>,
    cancelled: Arc<AtomicBool>,
    original: Settings,
    progress: Arc<data::pack::InstallProgress>,
}

struct PickedPack(u64);
impl Drop for PickedPack {
    fn drop(&mut self) {
        #[cfg(target_os = "android")]
        crate::android::cancel_map_pack(self.0);
        #[cfg(not(target_os = "android"))]
        let _ = self.0;
    }
}
impl Drop for Import {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.task.abort();
    }
}

#[derive(Component)]
struct Surface {
    viewport: Entity,
    overlay: Entity,
    status: Entity,
    image: Handle<Image>,
    size: [u32; 2],
    scene: Option<Arc<Mutex<Scene>>>,
    rendered_camera: Option<Camera>,
}
#[derive(Component)]
struct Canvas;

pub struct MapPlugin;
impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(lince_interface::map_navigation::NavigationPlugin);
        app.init_resource::<State>()
            .init_resource::<lince_interface::tokens::ThemeSettings>();
    }
}

fn runtime(world: &World) -> Option<tokio::runtime::Handle> {
    world
        .get_non_send::<crate::connection::Connection>()?
        .runtime()
}

fn reset_geometry(state: &mut State) {
    if let Some(epoch) = state.locate_epoch.take() {
        crate::location::cancel_locate(epoch);
    }
    state.locate_started = None;
    state.epoch = state.epoch.wrapping_add(1);
    state.generation = nucleus::new_uid("map-generation");
    state.after = None;
    state.window_start = None;
    state.refresh_at = Instant::now() + Duration::from_secs(5);
    state.seen = 0;
    state.done = false;
    state.limited = false;
    state.live_index = 0;
    state.markers.retain(|marker| marker.layer == 0);
    state.next = Instant::now() + Duration::from_millis(150);
    state.changed = true;
    state.rows.clear();
    state.row_page = 0;
}

fn close_surface(world: &mut World) {
    let Some(owner) = world.resource_mut::<State>().surface.take() else {
        return;
    };
    world.init_resource::<map_view::Scenes>();
    world.resource_mut::<map_view::Scenes>().0.remove(&owner);
    if let Some(surface) = world.get::<Surface>(owner) {
        let image = surface.image.id();
        if let Some(mut images) = world.get_resource_mut::<Assets<Image>>() {
            images.remove(image);
        }
    }
    world.despawn(owner);
}

fn scope(world: &mut World) {
    world.init_resource::<State>();
    let current = world.resource::<Mobile>().scope_key();
    if world.resource::<State>().scope != current {
        if let Some(epoch) = world.resource_mut::<State>().locate_epoch.take() {
            crate::location::cancel_locate(epoch);
        }
        close_surface(world);
        let base = world.resource::<Mobile>().data_directory().join("maps");
        let loaded = match std::fs::read(base.join("settings.json")) {
            Ok(bytes) if bytes.len() <= 65_536 => {
                serde_json::from_slice::<Settings>(&bytes).map_err(|error| error.to_string())
            }
            Ok(_) => Err("Map Settings exceed their size limit".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Settings::browsing()),
            Err(error) => Err(error.to_string()),
        };
        let (loaded, store, notice) = match loaded.and_then(|settings| {
            TileStore::new(settings.clone(), &base).map(|store| (settings, store))
        }) {
            Ok((settings, store)) => (settings, Some(store), String::new()),
            Err(error) => {
                let settings = Settings::default();
                let store = TileStore::new(settings.clone(), &base).ok();
                (
                    settings,
                    store,
                    format!("Using the offline overview. Check map sources: {error}"),
                )
            }
        };
        world.insert_resource(State {
            scope: current,
            online: loaded.online,
            settings: loaded,
            store,
            notice,
            ..Default::default()
        });
    }
}

pub fn open(world: &mut World, records: Vec<String>) {
    scope(world);
    close_surface(world);
    let state = &mut *world.resource_mut::<State>();
    state.records = records
        .into_iter()
        .filter(|uid| nucleus::valid_uid(uid, "r"))
        .take(10_000)
        .collect();
    state.auto_center = !state.records.is_empty();
    state.invitation = None;
    state.camera = Camera::default();
    reset_geometry(state);
}

pub fn observe(world: &mut World, invitation: nucleus::location::Invitation) -> Result<(), String> {
    invitation.reference().map_err(str::to_owned)?;
    open(world, vec![invitation.record_uid.clone()]);
    world.resource_mut::<State>().invitation = Some(invitation);
    world.resource_mut::<Mobile>().navigation.open(Page::Map);
    Ok(())
}

fn request(world: &mut World, kind: Kind, action: engine::actions::Action) -> Result<(), String> {
    if world.resource::<State>().pending.len() >= 4 {
        return Err("Waiting for the current map requests".into());
    }
    let id = nucleus::new_uid("mobile-map");
    app::send(
        world,
        cell::ClientMessage::Act {
            id: id.clone(),
            action,
        },
    )?;
    let epoch = world.resource::<State>().epoch;
    world.resource_mut::<State>().pending.insert(
        id,
        Pending {
            kind,
            epoch,
            sent: Instant::now(),
        },
    );
    Ok(())
}

fn bounds(state: &State, size: [u32; 2]) -> [f64; 4] {
    let size = size.map(f64::from);
    let a = state.camera.coordinate([0.0, size[1]], size);
    let b = state.camera.coordinate([size[0], 0.0], size);
    let world = size[0] >= 256.0 * 2.0_f64.powf(state.camera.zoom);
    [
        if world { -180.0 } else { a.longitude },
        a.latitude,
        if world { 180.0 } else { b.longitude },
        b.latitude,
    ]
}

pub fn receive(world: &mut World, message: &cell::ServerMessage) -> bool {
    let (id, data, error) = match message {
        cell::ServerMessage::ActionOk { id, data, .. } if id.starts_with("mobile-map_") => {
            (id, data.clone(), None)
        }
        cell::ServerMessage::Error { id, message, .. } if id.starts_with("mobile-map_") => {
            (id, None, Some(message.clone()))
        }
        _ => return false,
    };
    scope(world);
    let Some(pending) = world.resource_mut::<State>().pending.remove(id) else {
        return true;
    };
    if pending.epoch != world.resource::<State>().epoch
        || world.resource::<Mobile>().navigation.current != Page::Map
        || world.resource::<State>().sources_open
        || world.resource::<State>().credits_open
    {
        return true;
    }
    if let Some(error) = error {
        let state = &mut *world.resource_mut::<State>();
        state.notice = error;
        match pending.kind {
            Kind::Context(record) | Kind::Live(record) => state
                .markers
                .retain(|marker| marker.record != record || marker.layer != 2),
            Kind::Saved => {
                state.markers.retain(|marker| marker.layer != 1);
                state.rows.clear();
            }
            Kind::Save => {}
        }
        state.changed = true;
        return true;
    }
    let data = data.unwrap_or_default();
    match pending.kind {
        Kind::Saved => {
            let state = &mut *world.resource_mut::<State>();
            if data["generation"] != state.generation {
                return true;
            }
            let features = data["features"].as_array().cloned().unwrap_or_default();
            if features.len() > 128 {
                return true;
            }
            state.seen += features.len();
            state.after = data["next_after"].as_str().map(str::to_owned);
            state.done = state.after.is_none();
            state.limited = state.seen >= 2048 && !state.done;
            if state.done || state.limited {
                state.refresh_at = Instant::now() + Duration::from_secs(5);
            }
            for feature in features {
                let Some(record) = feature["record"]
                    .as_str()
                    .filter(|uid| nucleus::valid_uid(uid, "r"))
                else {
                    continue;
                };
                if !state.rows.iter().any(|(uid, _)| uid == record) {
                    state.rows.push((
                        record.into(),
                        feature["head"].as_str().unwrap_or("Record").into(),
                    ));
                }
                state
                    .markers
                    .retain(|marker| marker.record != record || marker.layer != 1);
                if let Ok(location) = serde_json::from_value::<nucleus::vocabulary::LocationCandidate>(
                    feature["location"].clone(),
                ) {
                    let point = Coordinate {
                        latitude: location.latitude,
                        longitude: location.longitude,
                    };
                    if point.valid() {
                        state.markers.push(Marker {
                            record: record.into(),
                            point,
                            radius: location.radius_metres,
                            layer: 1,
                            view: None,
                            received: Instant::now(),
                        });
                    }
                }
            }
            if state.limited {
                state.notice =
                    "2048 permitted Records loaded. Next locations continues the list.".into();
            }
            if state.auto_center
                && let Some(point) = state.markers.first().map(|marker| marker.point)
            {
                state.auto_center = false;
                state.camera.center = point;
                state.camera.zoom = 14.0;
                reset_geometry(state);
            }
            state.changed = true;
        }
        Kind::Context(record) => {
            if let Ok(mut context) = serde_json::from_value::<nucleus::location::Context>(data) {
                context
                    .view
                    .elapse(pending.sent.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                add_live(world, record, context.view);
            }
        }
        Kind::Live(record) => {
            if let Ok(mut view) = serde_json::from_value::<nucleus::location::View>(data) {
                view.elapse(pending.sent.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
                add_live(world, record, view);
            }
        }
        Kind::Save => {
            let state = &mut *world.resource_mut::<State>();
            state.notice = "Reference point saved on the Record".into();
            reset_geometry(state);
        }
    }
    if world.resource::<State>().list_open {
        close_surface(world);
        world.resource_mut::<Mobile>().dirty = true;
    }
    true
}

fn add_live(world: &mut World, record: String, view: nucleus::location::View) {
    let state = &mut *world.resource_mut::<State>();
    state
        .markers
        .retain(|marker| marker.record != record || marker.layer != 2);
    if view.record_uid == record
        && view.fix.is_some()
        && matches!(
            view.status,
            nucleus::location::Status::Live | nucleus::location::Status::Stale
        )
    {
        let fix = view.fix.as_ref().unwrap();
        let point = Coordinate {
            latitude: fix.latitude,
            longitude: fix.longitude,
        };
        if point.valid() {
            state.markers.push(Marker {
                record,
                point,
                radius: view.radius_metres,
                layer: 2,
                view: Some(view),
                received: Instant::now(),
            });
        }
    }
    state.changed = true;
    if state.auto_center
        && let Some(point) = state.markers.first().map(|marker| marker.point)
    {
        state.auto_center = false;
        state.camera.center = point;
        state.camera.zoom = 14.0;
        reset_geometry(state);
    }
}

pub fn apply(world: &mut World, intent: Intent) -> Result<(), String> {
    scope(world);
    app::capture(world);
    match intent {
        Intent::SavePoint => {
            let state = world.resource::<State>();
            if state.records.len() != 1 || state.invitation.is_some() {
                return Err("Open one writable Record to save a point".into());
            }
            let point = state.picked.ok_or("Tap a reference point first")?;
            request(
                world,
                Kind::Save,
                engine::actions::Action::SetPlace {
                    target: state.records[0].clone(),
                    lat: point.latitude,
                    lon: point.longitude,
                    address: None,
                },
            )?;
        }
        Intent::Sources => {
            world.resource_mut::<State>().sources_open = !world.resource::<State>().sources_open;
            close_surface(world);
            reset_geometry(&mut world.resource_mut::<State>());
        }
        Intent::Locations => {
            world.resource_mut::<State>().list_open = !world.resource::<State>().list_open;
            close_surface(world);
        }
        Intent::ListPage(delta) => {
            let mut state = world.resource_mut::<State>();
            state.row_page = state
                .row_page
                .saturating_add_signed(delta)
                .min(state.rows.len().saturating_sub(1) / 128);
            close_surface(world);
        }
        Intent::Focus(uid) => {
            let mut state = world.resource_mut::<State>();
            if let Some(point) = state
                .markers
                .iter()
                .find(|marker| marker.record == uid)
                .map(|marker| marker.point)
            {
                state.camera.center = point;
                state.camera.zoom = 14.0;
                state.list_open = false;
                reset_geometry(&mut state);
            }
            close_surface(world);
        }
        Intent::SaveSources => start_import(world)?,
        #[cfg(target_os = "android")]
        Intent::ChoosePack => {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let epoch = NEXT.fetch_add(1, Ordering::Relaxed);
            let quota = world.resource::<State>().settings.storage_megabytes * 1024 * 1024;
            world.resource_mut::<State>().import = None;
            world.resource_mut::<State>().picked_pack = None;
            crate::android::choose_map_pack(epoch, quota)?;
            world.resource_mut::<State>().picked_pack = Some(PickedPack(epoch));
            world.resource_mut::<State>().notice = "Choose a folder containing manifest.json and its map files. The pack will be verified before use.".into();
        }
        Intent::Locate => {
            static NEXT: AtomicU64 = AtomicU64::new(1);
            let epoch = NEXT.fetch_add(1, Ordering::Relaxed);
            crate::location::locate_once(epoch)?;
            world.resource_mut::<State>().locate_epoch = Some(epoch);
            world.resource_mut::<State>().locate_started = Some(Instant::now());
            world.resource_mut::<State>().notice =
                "Waiting for one device position. No place is saved and sharing stays separate."
                    .into();
        }
        Intent::Download => {
            let state = world.resource::<State>();
            let area = state.selection.ok_or("Select an area first")?;
            let zoom = state.camera.data_zoom(state.settings.max_zoom);
            let tiles = area
                .pyramid(zoom)?
                .into_iter()
                .filter(|tile| tile.z >= 3)
                .collect::<Vec<_>>();
            let store = state.store.clone().ok_or("Map storage is unavailable")?;
            let cancelled = Arc::new(AtomicBool::new(false));
            let result = Arc::new(Mutex::new(None));
            let output = result.clone();
            let cancellation = cancelled.clone();
            let request_tiles = tiles.clone();
            runtime(world)
                .ok_or("Map runtime is unavailable")?
                .spawn(async move {
                    let progress = store.download(request_tiles, cancellation).await;
                    *output.lock().unwrap_or_else(|error| error.into_inner()) = Some(progress);
                });
            world.resource_mut::<State>().download = Some(Download {
                cancelled,
                result,
                tiles,
            });
        }
        Intent::PublicRoute(route, expires) => {
            route.validate()?;
            open(world, Vec::new());
            let state = &mut *world.resource_mut::<State>();
            for area in [&route.origin, &route.destination] {
                state.markers.push(Marker {
                    record: String::new(),
                    point: Coordinate {
                        latitude: f64::from(area.latitude_e5) / 100_000.0,
                        longitude: f64::from(area.longitude_e5) / 100_000.0,
                    },
                    radius: Some(area.radius_metres),
                    layer: 0,
                    view: None,
                    received: Instant::now(),
                });
            }
            state.camera.center = state.markers[0].point;
            state.camera.zoom = 10.0;
            state.public_deadline = Some(
                Instant::now()
                    + Duration::from_secs(
                        expires
                            .saturating_sub(chrono::Utc::now().timestamp())
                            .clamp(0, nucleus::social::MAX_LIFETIME) as u64,
                    ),
            );
            world.resource_mut::<Mobile>().navigation.open(Page::Map);
        }
        Intent::Theme => {
            let mut theme = world.resource_mut::<lince_interface::tokens::ThemeSettings>();
            let index = lince_interface::tokens::ColorScheme::ALL
                .iter()
                .position(|scheme| *scheme == theme.scheme)
                .unwrap_or_default();
            theme.scheme = lince_interface::tokens::ColorScheme::ALL[(index + 1) % 4];
        }
        other => {
            let state = &mut *world.resource_mut::<State>();
            match other {
                Intent::Pan(delta) => {
                    state.auto_center = false;
                    state.camera.pan(delta);
                    reset_geometry(state);
                }
                Intent::PickCenter => {
                    state.picked = Some(state.camera.center);
                    state.notice =
                        "Map center selected as a reference point; saving is separate.".into();
                }
                Intent::Corner => {
                    if let Some(anchor) = state.anchor.take() {
                        state.selection = Some(Bounds::between(anchor, state.camera.center));
                    } else {
                        state.anchor = Some(state.camera.center);
                        state.notice =
                            "First corner selected. Pan to the other corner and set it.".into();
                    }
                }
                Intent::Zoom(amount) => {
                    state.camera.zoom = (state.camera.zoom + amount).clamp(0.0, 20.0);
                    reset_geometry(state);
                }
                Intent::World => {
                    state.camera = Camera::default();
                    state.device_centered = false;
                    reset_geometry(state);
                }
                Intent::Layer(layer) if layer < 3 => {
                    state.layers[layer] = !state.layers[layer];
                    state.changed = true;
                }
                Intent::More => {
                    state.epoch = state.epoch.wrapping_add(1);
                    state.window_start = state.after.clone();
                    state.seen = 0;
                    state.limited = false;
                    state.markers.retain(|marker| marker.layer == 0);
                    state.rows.clear();
                    state.row_page = 0;
                    state.changed = true;
                }
                Intent::SelectArea => {
                    state.select_area = !state.select_area;
                    state.anchor = None;
                    state.notice =
                        "Drag to choose an area, then save it offline at the current map detail."
                            .into();
                }
                Intent::Online => state.online = !state.online,
                Intent::CancelDownload => {
                    state.download = None;
                    state.import = None;
                    state.notice = "Cancelled; complete local data was kept".into();
                }
                Intent::Credits => {
                    state.credits_open = !state.credits_open;
                    close_surface(world);
                }
                _ => {}
            }
        }
    }
    world.resource_mut::<State>().changed = true;
    Ok(())
}

fn source_draft(world: &World) -> Result<Settings, String> {
    let state = world.resource::<State>();
    let mobile = world.resource::<Mobile>();
    let settings = Settings {
        local_directory: mobile
            .draft(
                "map-source",
                "local",
                &state.settings.local_directory.to_string_lossy(),
            )
            .into(),
        archives: mobile
            .draft(
                "map-source",
                "archives",
                &state
                    .settings
                    .archives
                    .iter()
                    .map(|path| path.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(";"),
            )
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(Into::into)
            .collect(),
        server: mobile.draft("map-source", "server", &state.settings.server),
        dataset_version: mobile.draft("map-source", "version", &state.settings.dataset_version),
        online: state.online,
        max_zoom: 14,
        storage_megabytes: mobile
            .draft(
                "map-source",
                "storage",
                &state.settings.storage_megabytes.to_string(),
            )
            .parse()
            .map_err(|_| "Enter a storage quota in MB")?,
    };
    settings.validate()?;
    Ok(settings)
}

#[cfg(target_os = "android")]
pub fn receive_pack(world: &mut World, epoch: u64, result: Result<String, String>) {
    scope(world);
    if !world
        .resource::<State>()
        .picked_pack
        .as_ref()
        .is_some_and(|pick| pick.0 == epoch)
        || world.resource::<Mobile>().navigation.current != Page::Map
    {
        crate::android::cancel_map_pack(epoch);
        return;
    }
    let result = result.and_then(|path| {
        let root = std::path::PathBuf::from(path);
        let manifest = data::pack::manifest(&root)?;
        let mobile = &mut *world.resource_mut::<Mobile>();
        match manifest.format {
            data::pack::Format::Directory => {
                mobile
                    .drafts
                    .insert("map-source/local".into(), root.to_string_lossy().into());
                mobile
                    .drafts
                    .insert("map-source/archives".into(), String::new());
            }
            data::pack::Format::Pmtiles => {
                mobile
                    .drafts
                    .insert("map-source/local".into(), String::new());
                mobile.drafts.insert(
                    "map-source/archives".into(),
                    root.join(&manifest.files[0].path).to_string_lossy().into(),
                );
            }
        }
        start_import(world)
    });
    if let Err(error) = result {
        world.resource_mut::<State>().picked_pack = None;
        world.resource_mut::<State>().notice = error;
    }
    close_surface(world);
    world.resource_mut::<Mobile>().dirty = true;
}

fn start_import(world: &mut World) -> Result<(), String> {
    let original = source_draft(world)?;
    let mut settings = original.clone();
    let base = world.resource::<Mobile>().data_directory().join("maps");
    let managed = base.join("packs");
    let root = base.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancellation = cancelled.clone();
    let progress = Arc::new(data::pack::InstallProgress::default());
    let report = progress.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let task = runtime(world)
        .ok_or("Map runtime is unavailable")?
        .spawn(async move {
            let result = async {
                let quota = settings.storage_megabytes * 1024 * 1024;
                if !settings.local_directory.as_os_str().is_empty() {
                    settings.local_directory = data::pack::install(
                        settings.local_directory.clone(),
                        managed.clone(),
                        quota,
                        cancellation.clone(),
                        progress.clone(),
                    )
                    .await?;
                }
                for archive in &mut settings.archives {
                    let parent = archive
                        .parent()
                        .ok_or("Choose a PMTiles pack directory")?
                        .to_path_buf();
                    let name = archive
                        .file_name()
                        .ok_or("Invalid archive name")?
                        .to_os_string();
                    *archive = data::pack::install(
                        parent,
                        managed.clone(),
                        quota,
                        cancellation.clone(),
                        progress.clone(),
                    )
                    .await?
                    .join(name);
                }
                let store = TileStore::new(settings.clone(), root)?;
                store.validate_sources().await?;
                Ok((settings, store))
            }
            .await;
            let _ = sender.send(result);
        });
    world.resource_mut::<State>().import = Some(Import {
        task,
        receiver,
        cancelled,
        original,
        progress: report,
    });
    world.resource_mut::<State>().notice =
        "Checking and staging map sources; current data remains usable.".into();
    Ok(())
}

fn palette(world: &World) -> Palette {
    use lince_interface::tokens::{ThemeSettings, Token, TokenOverrides};
    let default = ThemeSettings::default();
    let theme = world.get_resource::<ThemeSettings>().unwrap_or(&default);
    let color = |token| {
        theme
            .resolve(token, None, &TokenOverrides::default())
            .0
            .color()
    };
    map_view::palette(
        color(Token::Surface),
        color(Token::Ink),
        color(Token::Accent),
    )
}

fn apply_theme(world: &mut World, root: Entity) {
    use lince_interface::tokens::{ThemeSettings, Token, TokenOverrides};
    let theme = world
        .get_resource::<ThemeSettings>()
        .cloned()
        .unwrap_or_default();
    let resolve = |token| {
        theme
            .resolve(token, None, &TokenOverrides::default())
            .0
            .color()
    };
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
        if world.get::<Text>(entity).is_some() {
            world
                .entity_mut(entity)
                .insert(TextColor(resolve(Token::Ink)));
        }
        if let Some(token) = world
            .get::<lince_interface::style::BackgroundToken>(entity)
            .map(|token| token.0)
        {
            world
                .entity_mut(entity)
                .insert(BackgroundColor(resolve(token)));
        } else if world.get::<crate::app::ButtonIntent>(entity).is_some() {
            world.entity_mut(entity).insert((
                BackgroundColor(resolve(Token::Surface)),
                BorderColor::all(resolve(Token::Accent)),
            ));
        }
    }
}

fn map_button(world: &mut World, parent: Entity, label: &str, intent: Intent) {
    app::button(world, parent, label, app::Intent::Map(intent));
}

pub fn render(world: &mut World, _parent: Entity) {
    scope(world);
    if world.resource::<State>().surface.is_some() {
        return;
    }
    world.init_resource::<Assets<Image>>();
    world.init_resource::<map_view::Scenes>();
    let modal = world.resource::<State>().sources_open
        || world.resource::<State>().credits_open
        || world.resource::<State>().list_open;
    let owner = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(8)),
                row_gap: px(6),
                overflow: if modal {
                    Overflow::scroll_y()
                } else {
                    Overflow::clip()
                },
                ..default()
            },
            GlobalZIndex(50),
            lince_interface::style::background(lince_interface::tokens::Token::Surface),
        ))
        .id();
    let controls = lince_interface::controls::row(world, owner);
    world.get_mut::<Node>(controls).unwrap().flex_wrap = FlexWrap::Wrap;
    world.get_mut::<Node>(controls).unwrap().max_height = percent(35);
    world.get_mut::<Node>(controls).unwrap().overflow = Overflow::scroll_y();
    app::button(world, controls, "Back", app::Intent::Back);
    for (label, intent) in [
        ("−", Intent::Zoom(-1.0)),
        ("+", Intent::Zoom(1.0)),
        ("World", Intent::World),
        ("Locate me", Intent::Locate),
        ("Public areas", Intent::Layer(0)),
        ("Saved places", Intent::Layer(1)),
        ("Live", Intent::Layer(2)),
        ("Locations list", Intent::Locations),
        ("Next locations", Intent::More),
        ("Draw area", Intent::SelectArea),
        ("Set area corner", Intent::Corner),
        ("Pick center", Intent::PickCenter),
        ("North", Intent::Pan([0.0, 128.0])),
        ("South", Intent::Pan([0.0, -128.0])),
        ("West", Intent::Pan([128.0, 0.0])),
        ("East", Intent::Pan([-128.0, 0.0])),
        ("Sources", Intent::Sources),
        ("Theme", Intent::Theme),
        ("Credits", Intent::Credits),
    ] {
        map_button(world, controls, label, intent);
    }
    lince_interface::map_navigation::mount(
        world,
        controls,
        owner,
        world
            .resource::<Mobile>()
            .data_directory()
            .join("maps/navigation.json"),
        world.resource::<State>().camera.center,
        runtime(world),
    );
    if world.resource::<State>().list_open {
        let state = world.resource::<State>();
        let page = state.row_page;
        let rows = state.rows.clone();
        if page > 0 {
            map_button(world, owner, "Previous Records", Intent::ListPage(-1));
        }
        if (page + 1) * 128 < rows.len() {
            map_button(world, owner, "More Records", Intent::ListPage(1));
        }
        for (uid, head) in rows.into_iter().skip(page * 128).take(128) {
            app::button(
                world,
                owner,
                &head,
                app::Intent::Open(Page::Record(uid.clone())),
            );
            if world
                .resource::<State>()
                .markers
                .iter()
                .any(|marker| marker.record == uid)
            {
                map_button(world, owner, "Focus on map", Intent::Focus(uid));
            } else {
                app::label(world, owner, "No permitted location in this viewport", 14.0);
            }
        }
    }
    if world.resource::<State>().sources_open {
        let settings = world.resource::<State>().settings.clone();
        for (key, label, value) in [
            (
                "local",
                "Local pack directory (with manifest)",
                settings.local_directory.to_string_lossy().into_owned(),
            ),
            (
                "archives",
                "PMTiles paths, separated by semicolons",
                settings
                    .archives
                    .iter()
                    .map(|path| path.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(";"),
            ),
            (
                "server",
                "Optional HTTPS server with {z}/{x}/{y}",
                settings.server,
            ),
            (
                "version",
                "External dataset version",
                settings.dataset_version,
            ),
            (
                "storage",
                "Map storage quota (MB)",
                settings.storage_megabytes.to_string(),
            ),
        ] {
            app::input(world, owner, "map-source", key, label, &value, false);
        }
        map_button(
            world,
            owner,
            if world.resource::<State>().online {
                "External downloads: on"
            } else {
                "External downloads: off"
            },
            Intent::Online,
        );
        map_button(world, owner, "Verify and save sources", Intent::SaveSources);
        #[cfg(target_os = "android")]
        map_button(
            world,
            owner,
            "Choose and verify a local map pack folder",
            Intent::ChoosePack,
        );
    }
    let image = world
        .resource_mut::<Assets<Image>>()
        .add(Image::new_target_texture(
            400,
            400,
            bevy::render::render_resource::TextureFormat::Rgba8Unorm,
            None,
        ));
    let viewport = world
        .spawn((
            ChildOf(owner),
            ImageNode::new(image.clone()),
            Node {
                display: if modal { Display::None } else { Display::Flex },
                width: percent(100),
                min_height: px(220),
                flex_grow: 1.0,
                overflow: Overflow::clip(),
                ..default()
            },
            Canvas,
        ))
        .observe(press)
        .observe(drag)
        .observe(release)
        .observe(click)
        .id();
    let overlay = lince_interface::controls::column(world, viewport);
    world.entity_mut(overlay).insert((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Pickable::IGNORE,
    ));
    let controls = lince_interface::controls::row(world, owner);
    world.get_mut::<Node>(controls).unwrap().flex_wrap = FlexWrap::Wrap;
    world.get_mut::<Node>(controls).unwrap().max_height = percent(25);
    world.get_mut::<Node>(controls).unwrap().overflow = Overflow::scroll_y();
    map_button(
        world,
        controls,
        "Save selected area offline",
        Intent::Download,
    );
    map_button(world, controls, "Cancel", Intent::CancelDownload);
    if world.resource::<State>().records.len() == 1
        && world.resource::<State>().invitation.is_none()
    {
        map_button(world, controls, "Save point on Record", Intent::SavePoint);
    }
    let status = app::label(world, owner, "Offline world overview", 14.0);
    app::label(
        world,
        owner,
        "Natural Earth · © OpenStreetMap contributors · ESA WorldCover. Maps grant no live sharing permission.",
        11.0,
    );
    if world.resource::<State>().credits_open {
        for (name, license) in [
            (
                "Galileo",
                include_str!("../../map/licenses/galileo-MIT.txt"),
            ),
            ("wgpu", include_str!("../../map/licenses/wgpu-MIT.txt")),
            (
                "PMTiles",
                include_str!("../../map/licenses/pmtiles-MIT.txt"),
            ),
            (
                "GeoZero",
                include_str!("../../map/licenses/geozero-MIT.txt"),
            ),
            (
                "GeoJSON",
                include_str!("../../map/licenses/geojson-MIT.txt"),
            ),
            (
                "Natural Earth",
                include_str!("../../map/licenses/natural-earth.txt"),
            ),
            ("OpenStreetMap", include_str!("../../map/licenses/ODbL.txt")),
            (
                "ESA WorldCover",
                include_str!("../../map/licenses/ESA-WorldCover.txt"),
            ),
        ] {
            app::label(world, owner, name, 18.0);
            app::label(world, owner, license, 12.0);
        }
    }
    world.entity_mut(owner).insert(Surface {
        viewport,
        overlay,
        status,
        image,
        size: [400, 400],
        scene: None,
        rendered_camera: None,
    });
    world.resource_mut::<State>().surface = Some(owner);
    world.resource_mut::<State>().changed = true;
}

pub fn maintain(world: &mut World) {
    scope(world);
    if world.resource::<Mobile>().navigation.current != Page::Map
        || !world.resource::<Mobile>().ready
    {
        close_surface(world);
        let state = &mut *world.resource_mut::<State>();
        state.pending.clear();
        state.markers.clear();
        state.rows.clear();
        state.records.clear();
        state.invitation = None;
        state.sources_open = false;
        state.credits_open = false;
        state.list_open = false;
        reset_geometry(state);
        state.download = None;
        state.import = None;
        state.picked_pack = None;
        if let Some(epoch) = state.locate_epoch.take() {
            crate::location::cancel_locate(epoch);
        }
        state.locate_started = None;
        state.touches.clear();
        return;
    }
    poll_locate(world);
    let Some(owner) = world.resource::<State>().surface else {
        return;
    };
    let point = world
        .resource::<State>()
        .picked
        .unwrap_or(world.resource::<State>().camera.center);
    let scope = world.resource::<State>().scope.clone();
    let bounds = world.resource::<State>().selection;
    let store = world.resource::<State>().store.clone();
    lince_interface::map_navigation::region(world, owner, bounds, store);
    let (changed, place) = lince_interface::map_navigation::update(world, owner, point, &scope);
    if changed {
        world.resource_mut::<State>().changed = true;
    }
    if let Some(place) = place {
        let state = &mut *world.resource_mut::<State>();
        state.picked = Some(place.coordinate);
        state.camera.center = place.coordinate;
        state.camera.zoom = 14.0;
        state.changed = true;
    }
    if let Some(mut import) = world.resource_mut::<State>().import.take() {
        if source_draft(world).ok().as_ref() == Some(&import.original) {
            match import.receiver.try_recv() {
                Ok(result) => match result.and_then(|(settings, store)| {
                    settings.save_atomic(
                        &world
                            .resource::<Mobile>()
                            .data_directory()
                            .join("maps/settings.json"),
                    )?;
                    let mut state = world.resource_mut::<State>();
                    state.settings = settings;
                    state.store = Some(store);
                    state.notice = "Map sources verified and saved".into();
                    Ok(())
                }) {
                    Ok(()) => {
                        world.resource_mut::<State>().picked_pack = None;
                        world
                            .resource_mut::<Mobile>()
                            .drafts
                            .retain(|key, _| !key.starts_with("map-source/"));
                        close_surface(world);
                        world.resource_mut::<Mobile>().dirty = true;
                    }
                    Err(error) => {
                        world.resource_mut::<State>().picked_pack = None;
                        world.resource_mut::<State>().notice = error;
                    }
                },
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    world.resource_mut::<State>().import = Some(import)
                }
                Err(_) => {
                    world.resource_mut::<State>().notice =
                        "Map import ended; previous sources were kept".into()
                }
            }
        }
    }
    expire_and_wake(world);
    let Some(surface) = world.get::<Surface>(owner) else {
        return;
    };
    if world.resource::<State>().sources_open
        || world.resource::<State>().credits_open
        || world.resource::<State>().list_open
    {
        let status_entity = surface.status;
        let size = world.resource::<State>().viewport_size;
        if world.resource::<State>().list_open {
            query(world, size);
        }
        apply_theme(world, owner);
        let message = status(world);
        if let Some(mut label) = world.get_mut::<Text>(status_entity) {
            label.0 = message;
        }
        return;
    }
    let viewport = surface.viewport;
    let size = world
        .get::<ComputedNode>(viewport)
        .map(|node| {
            (node.size() * node.inverse_scale_factor())
                .to_array()
                .map(|value| value.clamp(1.0, 2048.0) as u32)
        })
        .filter(|size| size[0] > 1 && size[1] > 1)
        .unwrap_or([400, 400]);
    if size != surface.size {
        let old = surface.image.id();
        let image = world
            .resource_mut::<Assets<Image>>()
            .add(Image::new_target_texture(
                size[0],
                size[1],
                bevy::render::render_resource::TextureFormat::Rgba8Unorm,
                None,
            ));
        world.resource_mut::<Assets<Image>>().remove(old);
        world.get_mut::<ImageNode>(viewport).unwrap().image = image.clone();
        let mut surface = world.get_mut::<Surface>(owner).unwrap();
        surface.image = image;
        surface.size = size;
        surface.rendered_camera = None;
        world.resource_mut::<State>().viewport_size = size;
        reset_geometry(&mut world.resource_mut::<State>());
    }
    query(world, size);
    let palette = palette(world);
    let (store, camera) = {
        let state = world.resource::<State>();
        (state.store.clone(), state.camera)
    };
    let changed = world
        .get::<Surface>(owner)
        .unwrap()
        .scene
        .as_ref()
        .is_none_or(|scene| {
            scene
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .palette
                != palette
        });
    if changed && let Some(store) = store {
        let wake = world
            .get_resource::<lince_interface::wake::WakeSignal>()
            .cloned()
            .map(|wake| Arc::new(move || wake.ring()) as Arc<dyn Fn() + Send + Sync>);
        match Scene::new(store, camera, size, palette, wake) {
            Ok(scene) => {
                let mut surface = world.get_mut::<Surface>(owner).unwrap();
                surface.scene = Some(Arc::new(Mutex::new(scene)));
                surface.rendered_camera = Some(camera);
            }
            Err(error) => world.resource_mut::<State>().notice = error,
        }
    }
    if let Some(scene) = world.get::<Surface>(owner).unwrap().scene.clone() {
        if world.get::<Surface>(owner).unwrap().rendered_camera != Some(camera) {
            scene
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .set_camera(camera, size);
            world.get_mut::<Surface>(owner).unwrap().rendered_camera = Some(camera);
        }
        if let Some(runtime) = runtime(world) {
            let _entered = runtime.enter();
            scene
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .load();
        }
        let image = world.get::<Surface>(owner).unwrap().image.id();
        world.resource_mut::<map_view::Scenes>().0.insert(
            owner,
            map_view::Target {
                scene,
                image,
                size,
                input_sent: None,
                on_render: None,
            },
        );
    }
    let text = status(world);
    let status = world.get::<Surface>(owner).unwrap().status;
    if let Some(mut label) = world.get_mut::<Text>(status) {
        label.0 = text;
    }
    overlays(world, owner);
    apply_theme(world, owner);
}

fn expire_and_wake(world: &mut World) {
    let before = (
        world.resource::<State>().markers.len(),
        world.resource::<State>().rows.len(),
    );
    {
        let state = &mut *world.resource_mut::<State>();
        state
            .pending
            .retain(|_, pending| pending.sent.elapsed() < Duration::from_secs(10));
        if (state.done || state.limited) && state.refresh_at <= Instant::now() {
            state.after = state.window_start.clone();
            state.seen = 0;
            state.done = false;
            state.limited = false;
            state.markers.retain(|marker| marker.layer != 1);
            state.rows.clear();
            state.row_page = 0;
            state.next = Instant::now();
            state.changed = true;
        }
        let count = state.markers.len();
        state.markers.retain(|marker| {
            marker.view.as_ref().is_none_or(|view| {
                let mut view = view.clone();
                view.elapse(
                    marker
                        .received
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                );
                view.fix.is_some()
            })
        });
        state.changed |= count != state.markers.len();
        if state
            .public_deadline
            .is_some_and(|deadline| deadline <= Instant::now())
        {
            state.markers.retain(|marker| marker.layer != 0);
            state.public_deadline = None;
            state.changed = true;
        }
    }
    if world.resource::<State>().list_open
        && before
            != (
                world.resource::<State>().markers.len(),
                world.resource::<State>().rows.len(),
            )
    {
        close_surface(world);
        world.resource_mut::<Mobile>().dirty = true;
    }
    if world.resource::<State>().next_wake <= Instant::now()
        && let Some(runtime) = runtime(world)
        && let Some(wake) = world
            .get_resource::<lince_interface::wake::WakeSignal>()
            .cloned()
    {
        world.resource_mut::<State>().next_wake = Instant::now() + Duration::from_secs(1);
        runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            wake.ring();
        });
    }
}

fn poll_locate(world: &mut World) {
    let Some(epoch) = world.resource::<State>().locate_epoch else {
        return;
    };
    let result = crate::location::take_locate(epoch)
        .map(|sample| {
            let (latitude, longitude, accuracy_metres, age) = sample.result?;
            let age = age
                .saturating_add(sample.received.elapsed().as_millis().min(i64::MAX as u128) as i64);
            if !(0..60_000).contains(&age) {
                return Err("The device position expired; locate again".into());
            }
            nucleus::location::Fix {
                latitude,
                longitude,
                accuracy_metres,
                captured_at_ms: chrono::Utc::now().timestamp_millis().saturating_sub(age),
                capture_age_ms: age as u64,
                sequence: 1,
            }
            .validate()?;
            Ok(Coordinate {
                latitude,
                longitude,
            })
        })
        .or_else(|| {
            world
                .resource::<State>()
                .locate_started
                .filter(|started| started.elapsed() >= Duration::from_secs(60))
                .map(|_| Err("Device positioning timed out; choose a point manually".into()))
        });
    if let Some(result) = result {
        let state = &mut *world.resource_mut::<State>();
        state.locate_epoch = None;
        state.locate_started = None;
        crate::location::cancel_locate(epoch);
        match result {
            Ok(point) => {
                state.camera.center = point;
                state.camera.zoom = 14.0;
                state.device_centered = true;
                state.auto_center = false;
                reset_geometry(state);
                state.notice = "Centered on one device position. Saving and live sharing are separate actions.".into();
            }
            Err(error) => state.notice = error,
        }
    }
}

fn status(world: &World) -> String {
    let state = world.resource::<State>();
    if let Some(import) = &state.import {
        return format!(
            "Checking map sources · {} / {} bytes",
            import.progress.copied.load(Ordering::Relaxed),
            import.progress.total.load(Ordering::Relaxed)
        );
    }
    if let Some(download) = &state.download {
        let progress = download
            .result
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .or_else(|| {
                state
                    .store
                    .as_ref()
                    .map(|store| store.progress(&download.tiles))
            })
            .unwrap_or_default();
        return format!(
            "Selected area · {}{}",
            data::source::progress_caption(
                &progress,
                chrono::Utc::now().timestamp_millis().div_euclid(250) as usize
            ),
            if progress.cancelled {
                " · cancelled"
            } else {
                ""
            }
        );
    }
    let detail = state
        .store
        .as_ref()
        .map(|store| {
            let tiles = state
                .surface
                .and_then(|owner| world.get::<Surface>(owner))
                .map(|surface| {
                    if state.camera.data_zoom(store.settings().max_zoom) < 3 {
                        Vec::new()
                    } else {
                        state
                            .camera
                            .visible_tiles(surface.size.map(f64::from), store.settings().max_zoom)
                    }
                })
                .unwrap_or_default();
            data::source::progress_caption(
                &store.progress(&tiles),
                chrono::Utc::now().timestamp_millis().div_euclid(250) as usize,
            )
        })
        .unwrap_or_else(|| "Preparing map".into());
    format!(
        "{detail} · {}{}{}",
        if state.settings.online {
            "Zoom to see streets; detail downloads automatically"
        } else {
            "Offline mode"
        },
        if state.pending.is_empty() {
            ""
        } else {
            " · Loading permitted locations…"
        },
        if state.notice.is_empty() {
            String::new()
        } else {
            format!(" · {}", state.notice)
        }
    )
}

fn query(world: &mut World, size: [u32; 2]) {
    if world.resource::<State>().pending.len() >= 4
        || world.resource::<State>().next > Instant::now()
    {
        return;
    }
    let person = crate::workflows::person(world);
    let state = world.resource::<State>();
    if state
        .invitation
        .as_ref()
        .is_none_or(|invitation| invitation.grant.is_none())
        && !state.done
        && !state.limited
        && !state
            .pending
            .values()
            .any(|pending| matches!(pending.kind, Kind::Saved))
    {
        let location_request = nucleus::vocabulary::SavedLocationCommand::Page {
            query: state.records.is_empty().then(|| json!({"source":"record"})),
            records: state.records.clone(),
            after: state.after.clone(),
            generation: state.generation.clone(),
            bounds: bounds(state, size),
        };
        let _ = request(
            world,
            Kind::Saved,
            engine::actions::Action::SavedLocation {
                request: location_request,
            },
        );
        return;
    }
    let state = world.resource::<State>();
    if !state.layers[2] || state.records.is_empty() {
        return;
    }
    let index = state.live_index % state.records.len();
    let record = state.records[index].clone();
    let invitation = state.invitation.clone();
    let command = if let Some(invitation) = invitation {
        if invitation.grant.is_some() {
            nucleus::location::Command::ObserveInvitation { person, invitation }
        } else {
            nucleus::location::Command::Observe {
                person,
                record_uid: record.clone(),
                node_id: invitation.authority_node_id,
            }
        }
    } else {
        nucleus::location::Command::Context {
            person,
            record_uid: record.clone(),
        }
    };
    let kind = if matches!(command, nucleus::location::Command::Context { .. }) {
        Kind::Context(record)
    } else {
        Kind::Live(record)
    };
    if request(
        world,
        kind,
        engine::actions::Action::Location { request: command },
    )
    .is_ok()
    {
        let state = &mut *world.resource_mut::<State>();
        state.live_index += 1;
        state.next = Instant::now()
            + if state.live_index % state.records.len() == 0 {
                Duration::from_secs(5)
            } else {
                Duration::from_millis(16)
            };
    }
}

fn overlays(world: &mut World, owner: Entity) {
    let overlay = world.get::<Surface>(owner).unwrap().overlay;
    let size = world.get::<Surface>(owner).unwrap().size.map(f64::from);
    let state = world.resource::<State>();
    if !state.changed {
        return;
    }
    let markers = state
        .markers
        .iter()
        .filter(|marker| state.layers[marker.layer])
        .cloned()
        .collect::<Vec<_>>();
    let camera = state.camera;
    let selection = state.selection;
    let picked = state.picked;
    lince_interface::controls::clear(world, overlay);
    lince_interface::map_navigation::draw_route(world, owner, overlay, camera, size);
    for marker in markers.into_iter().chain(picked.map(|point| Marker {
        record: String::new(),
        point,
        radius: None,
        layer: 1,
        view: None,
        received: Instant::now(),
    })) {
        let point = camera.screen(marker.point, size);
        let radius = marker.radius.map_or(6.0, |radius| {
            (f64::from(radius)
                / (camera.resolution() * marker.point.latitude.to_radians().cos().abs().max(0.001)))
            .clamp(4.0, 8192.0)
        });
        if point[0] + radius < 0.0
            || point[1] + radius < 0.0
            || point[0] - radius > size[0]
            || point[1] - radius > size[1]
        {
            continue;
        }
        let dot = world
            .spawn((
                ChildOf(overlay),
                Node {
                    position_type: PositionType::Absolute,
                    left: px((point[0] - radius) as f32),
                    top: px((point[1] - radius) as f32),
                    width: px((radius * 2.0) as f32),
                    height: px((radius * 2.0) as f32),
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                lince_interface::style::background(if marker.layer == 2 {
                    lince_interface::tokens::Token::Accent
                } else {
                    lince_interface::tokens::Token::Ink
                }),
                Pickable::IGNORE,
            ))
            .id();
        let _ = dot;
    }
    if let Some(selection) = selection {
        let rect = camera.area_screen(selection, size);
        world.spawn((
            ChildOf(overlay),
            Node {
                position_type: PositionType::Absolute,
                left: px(rect[0][0] as f32),
                top: px(rect[0][1] as f32),
                width: px((rect[1][0] - rect[0][0]).abs() as f32),
                height: px((rect[1][1] - rect[0][1]).abs() as f32),
                border: UiRect::all(px(2)),
                ..default()
            },
            lince_interface::style::border(lince_interface::tokens::Token::Accent),
            Pickable::IGNORE,
        ));
    }
    world.resource_mut::<State>().changed = false;
}

fn point(world: &World, viewport: Entity, window: Vec2) -> Option<[f64; 2]> {
    let node = world.get::<ComputedNode>(viewport)?;
    let transform = world.get::<UiGlobalTransform>(viewport)?;
    let normalized =
        node.normalize_point(*transform, window / node.inverse_scale_factor())? + Vec2::splat(0.5);
    Some(
        (normalized * node.size() * node.inverse_scale_factor())
            .to_array()
            .map(f64::from),
    )
}

fn press(mut event: On<Pointer<Press>>, mut commands: Commands) {
    let entity = event.entity;
    let id = event.pointer_id;
    let position = event.pointer_location.position;
    commands.queue(move |world: &mut World| {
        let Some(point) = point(world, entity, position) else {
            return;
        };
        let size = world
            .get::<ComputedNode>(entity)
            .map(|node| {
                (node.size() * node.inverse_scale_factor())
                    .to_array()
                    .map(f64::from)
            })
            .unwrap_or([400.0; 2]);
        let state = &mut *world.resource_mut::<State>();
        state.touches.insert(id, point);
        state.dragged = false;
        if state.select_area {
            state.anchor = Some(state.camera.coordinate(point, size));
        }
    });
    event.propagate(false);
}

fn drag(mut event: On<Pointer<Drag>>, mut commands: Commands) {
    let entity = event.entity;
    let id = event.pointer_id;
    let position = event.pointer_location.position;
    commands.queue(move |world: &mut World| {
        let Some(point) = point(world, entity, position) else {
            return;
        };
        let size = world
            .get::<ComputedNode>(entity)
            .map(|node| {
                (node.size() * node.inverse_scale_factor())
                    .to_array()
                    .map(f64::from)
            })
            .unwrap_or([400.0; 2]);
        let state = &mut *world.resource_mut::<State>();
        let Some(previous) = state.touches.insert(id, point) else {
            return;
        };
        state.dragged = true;
        if state.touches.len() == 2 {
            let other = *state
                .touches
                .iter()
                .find(|(other, _)| **other != id)
                .unwrap()
                .1;
            let distance =
                |a: [f64; 2]| ((a[0] - other[0]).powi(2) + (a[1] - other[1]).powi(2)).sqrt();
            let old = distance(previous);
            let new = distance(point);
            if old > 1.0 && new > 1.0 {
                state.camera.zoom_at(
                    (new / old).log2(),
                    [(point[0] + other[0]) / 2.0, (point[1] + other[1]) / 2.0],
                    size,
                );
            }
        } else if state.select_area {
            if let Some(anchor) = state.anchor {
                state.selection = Some(Bounds::between(
                    anchor,
                    state.camera.coordinate(point, size),
                ));
            }
        } else {
            state
                .camera
                .pan([point[0] - previous[0], point[1] - previous[1]]);
        }
        reset_geometry(state);
    });
    event.propagate(false);
}

fn release(mut event: On<Pointer<Release>>, mut state: ResMut<State>) {
    state.touches.remove(&event.pointer_id);
    state.anchor = None;
    event.propagate(false);
}
fn click(mut event: On<Pointer<Click>>, mut commands: Commands) {
    let entity = event.entity;
    let position = event.pointer_location.position;
    commands.queue(move |world: &mut World| {
        let Some(point) = point(world, entity, position) else {
            return;
        };
        let size = world
            .get::<ComputedNode>(entity)
            .map(|node| {
                (node.size() * node.inverse_scale_factor())
                    .to_array()
                    .map(f64::from)
            })
            .unwrap_or([400.0; 2]);
        let state = &mut *world.resource_mut::<State>();
        if !state.dragged && !state.select_area {
            state.picked = Some(state.camera.coordinate(point, size));
            state.changed = true;
        }
    });
    event.propagate(false);
}

#[cfg(test)]
mod tests;
