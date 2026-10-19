use bevy::prelude::*;
use engine::location::clock::Instant;
use lince_interface::location::{self as ui, Request};
use nucleus::location::SourceKind;
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

pub struct LocationPlugin;

#[derive(Component)]
struct Overlay;

impl Plugin for LocationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ui::LocationUiPlugin)
            .insert_resource(ui::ElapsedClock(engine::location::clock::elapsed_millis))
            .add_systems(Update, (send, authenticate));
    }
}

fn authenticate(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ui::AuthenticationRequest>>,
) {
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<ui::AuthenticationRequest>>())
        .cloned()
        .collect();
    for request in requests {
        let result = world
            .get_non_send::<crate::connection::Connection>()
            .ok_or_else(|| "The device connection is unavailable".to_string())
            .and_then(|connection| connection.authenticate_location(request.clone()));
        if let Err(error) = result {
            ui::authentication_reply(world, &request.id, Err(error));
        }
    }
}

pub fn close(world: &mut World) -> bool {
    let entities: Vec<_> = world
        .query_filtered::<Entity, With<Overlay>>()
        .iter(world)
        .collect();
    let closed = !entities.is_empty();
    for entity in entities {
        world.despawn(entity);
    }
    closed
}

pub fn open(world: &mut World, record: &str, transfer: Option<&str>) {
    open_panel(world, record, transfer, false);
}

pub fn open_reference(world: &mut World, reference: &str) -> Result<(), String> {
    let invitation = nucleus::location::Invitation::parse(reference).map_err(str::to_owned)?;
    let panel = open_panel(world, "", None, true);
    ui::load_reference(world, panel, reference)?;
    let parent = world
        .get::<ChildOf>(panel)
        .map(ChildOf::parent)
        .unwrap_or(panel);
    crate::app::button(
        world,
        parent,
        "View this invitation on the map",
        crate::app::Intent::ViewLocationMap(invitation),
    );
    Ok(())
}

pub fn open_observer(world: &mut World) {
    open_panel(world, "", None, true);
}

fn open_panel(world: &mut World, record: &str, transfer: Option<&str>, observer: bool) -> Entity {
    crate::visibility::close(world);
    close(world);
    let person = match &world.resource::<crate::app::Mobile>().identity {
        crate::session::Identity::Person(person) => Some(person.clone()),
        _ => None,
    };
    let parent = world
        .spawn((
            Overlay,
            crate::app::Modal,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                padding: UiRect::all(px(18)),
                row_gap: px(12),
                ..default()
            },
            GlobalZIndex(100),
            BackgroundColor(lince_interface::theme::PAPER),
        ))
        .id();
    crate::app::button(
        world,
        parent,
        "Close location",
        crate::app::Intent::CloseLocation,
    );
    if observer {
        ui::mount_observer(world, parent, record, person.as_deref())
    } else {
        ui::mount(world, parent, record, person.as_deref(), transfer)
    }
}

fn send(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<Request>>,
    mut places: Local<bevy::ecs::message::MessageCursor<ui::SavePlaceRequest>>,
    mut saved: Local<bevy::ecs::message::MessageCursor<ui::SavedLocationRequest>>,
) {
    let saved: Vec<_> = saved
        .read(world.resource::<Messages<ui::SavedLocationRequest>>())
        .cloned()
        .collect();
    for request in saved {
        if let Err(error) = crate::app::send(
            world,
            cell::ClientMessage::Act {
                id: request.id.clone(),
                action: engine::actions::Action::SavedLocation {
                    request: request.command,
                },
            },
        ) {
            ui::receive(world, &request.id, Err(error));
        }
    }
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<Request>>())
        .cloned()
        .collect();
    for request in requests {
        if let Err(error) = crate::app::send(
            world,
            cell::ClientMessage::Act {
                id: request.id.clone(),
                action: engine::actions::Action::Location {
                    request: request.command,
                },
            },
        ) {
            ui::receive(world, &request.id, Err(error));
        }
    }
    let places: Vec<_> = places
        .read(world.resource::<Messages<ui::SavePlaceRequest>>())
        .cloned()
        .collect();
    for request in places {
        if let Err(error) = crate::app::send(
            world,
            cell::ClientMessage::Act {
                id: request.id.clone(),
                action: engine::actions::Action::SetPlace {
                    target: request.record,
                    lat: request.latitude,
                    lon: request.longitude,
                    address: None,
                },
            },
        ) {
            ui::receive(world, &request.id, Err(error));
        }
    }
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static ACTIVE_CAPTURE: AtomicU64 = AtomicU64::new(0);
static ACTIVE_ONCE: AtomicU64 = AtomicU64::new(0);
static SAMPLE: Mutex<Option<Sample>> = Mutex::new(None);
static ONCE: Mutex<Option<Sample>> = Mutex::new(None);

pub(crate) struct Sample {
    pub epoch: u64,
    pub received: Instant,
    pub result: Result<(f64, f64, Option<f64>, i64), String>,
    pub stopped: bool,
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn sample(sample: Sample) {
    if ACTIVE_CAPTURE.load(Ordering::Acquire) != sample.epoch {
        return;
    }
    if let Ok(mut latest) = SAMPLE.lock() {
        if !latest
            .as_ref()
            .is_some_and(|latest| latest.stopped && latest.epoch == sample.epoch)
        {
            *latest = Some(sample);
        }
    }
}

#[cfg(target_os = "android")]
pub(crate) fn sample_once(sample: Sample) {
    if ACTIVE_ONCE.load(Ordering::Acquire) == sample.epoch
        && let Ok(mut latest) = ONCE.lock()
    {
        *latest = Some(sample);
    }
}

pub fn locate_once(epoch: u64) -> Result<(), String> {
    let previous = ACTIVE_ONCE.swap(epoch, Ordering::AcqRel);
    if previous != 0 {
        cancel_native_once(previous);
    }
    if let Ok(mut latest) = ONCE.lock() {
        *latest = None;
    }
    #[cfg(target_os = "android")]
    let result = crate::android::locate_once(true, epoch);
    #[cfg(not(target_os = "android"))]
    let result = Err(
        "Device positioning is available on Android; choose a point manually in this preview"
            .into(),
    );
    if result.is_err() {
        cancel_locate(epoch);
    }
    result
}

pub fn take_locate(epoch: u64) -> Option<Sample> {
    let mut latest = ONCE.lock().ok()?;
    if latest.as_ref().is_some_and(|sample| sample.epoch == epoch) {
        latest.take()
    } else {
        None
    }
}

pub fn cancel_locate(epoch: u64) {
    if ACTIVE_ONCE
        .compare_exchange(epoch, 0, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        cancel_native_once(epoch);
        if let Ok(mut latest) = ONCE.lock() {
            *latest = None;
        }
    }
}

fn cancel_native_once(epoch: u64) {
    #[cfg(target_os = "android")]
    let _ = crate::android::locate_once(false, epoch);
    #[cfg(not(target_os = "android"))]
    let _ = epoch;
}

#[derive(Default)]
pub(crate) struct Capture {
    epoch: u64,
    until: Option<Instant>,
    sources: Vec<nucleus::location::SourceLease>,
    sequence: u64,
}

fn same_sources(
    left: &[nucleus::location::SourceLease],
    right: &[nucleus::location::SourceLease],
) -> bool {
    left.len() == right.len()
        && left.iter().all(|source| {
            right.iter().any(|other| {
                source.settings.record_uid == other.settings.record_uid
                    && source.session_uid == other.session_uid
            })
        })
}

impl Capture {
    fn stop_device(&mut self) {
        let epoch = self.epoch;
        ACTIVE_CAPTURE
            .compare_exchange(epoch, 0, Ordering::AcqRel, Ordering::Acquire)
            .ok();
        if epoch != 0 {
            stop_native(epoch);
        }
        self.epoch = 0;
        self.until = None;
        if let Ok(mut latest) = SAMPLE.lock() {
            *latest = None;
        }
    }

    pub async fn stop(&mut self, engine: &engine::Engine) {
        self.stop_device();
        let sources = std::mem::take(&mut self.sources);
        engine.location_device_unavailable_for(&sources).await;
    }

    pub async fn tick(&mut self, engine: &engine::Engine) {
        let sources = engine
            .location_sources()
            .await
            .into_iter()
            .filter(|source| source.settings.source_kind == SourceKind::Device)
            .collect::<Vec<_>>();
        if !same_sources(&sources, &self.sources) {
            self.stop_device();
            self.sources = sources;
            self.sequence = 0;
        }
        if self.sources.is_empty() {
            self.stop_device();
            return;
        }
        let sample = SAMPLE.lock().ok().and_then(|mut latest| latest.take());
        if let Some(sample) = sample.filter(|sample| sample.epoch == self.epoch && self.epoch != 0)
        {
            if sample.stopped || sample.result.is_err() {
                self.stop(engine).await;
                return;
            }
            if let Ok((latitude, longitude, accuracy_metres, age_ms)) = sample.result {
                let age_ms = age_ms.saturating_add(
                    sample.received.elapsed().as_millis().min(i64::MAX as u128) as i64,
                );
                if (0..60_000).contains(&age_ms) {
                    self.sequence = self.sequence.saturating_add(1);
                    let fix = nucleus::location::Fix {
                        latitude,
                        longitude,
                        accuracy_metres,
                        captured_at_ms: chrono::Utc::now()
                            .timestamp_millis()
                            .saturating_sub(age_ms),
                        capture_age_ms: age_ms as u64,
                        sequence: self.sequence,
                    };
                    if engine
                        .publish_device_fix_for(&self.sources, fix)
                        .await
                        .is_err()
                    {
                        self.stop(engine).await;
                        return;
                    }
                }
            }
        }
        let duration = engine
            .location_sources()
            .await
            .into_iter()
            .filter(|source| {
                source.settings.source_kind == SourceKind::Device
                    && self.sources.iter().any(|bound| {
                        bound.settings.record_uid == source.settings.record_uid
                            && bound.session_uid == source.session_uid
                    })
            })
            .map(|source| source.lease_millis)
            .max()
            .unwrap_or(0);
        if duration == 0 {
            self.stop_device();
            self.sources.clear();
            return;
        }
        let until = Instant::now() + std::time::Duration::from_millis(u64::from(duration));
        if self.epoch == 0
            || self
                .until
                .is_none_or(|current| until > current + std::time::Duration::from_secs(1))
        {
            if self.epoch == 0 {
                self.epoch = NEXT.fetch_add(1, Ordering::Relaxed);
                ACTIVE_CAPTURE.store(self.epoch, Ordering::Release);
            }
            self.until = Some(until);
            #[cfg(target_os = "android")]
            let result = crate::android::location(true, i64::from(duration), self.epoch);
            #[cfg(not(target_os = "android"))]
            let result: Result<(), String> =
                Err("Choose a manual source in the mobile preview".into());
            if result.is_err() {
                self.stop(engine).await;
            }
        }
    }
}

fn stop_native(epoch: u64) {
    #[cfg(target_os = "android")]
    let _ = crate::android::location(false, 0, epoch);
    #[cfg(not(target_os = "android"))]
    let _ = epoch;
}

#[cfg(test)]
mod tests;
