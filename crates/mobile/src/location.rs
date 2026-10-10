use bevy::prelude::*;
use lince_interface::location::{self as ui, Request};
use nucleus::location::{Command, SourceKind};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

pub struct LocationPlugin;

#[derive(Component)]
struct Overlay;

impl Plugin for LocationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ui::LocationUiPlugin)
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

pub fn open_observer(world: &mut World) {
    open_panel(world, "", None, true);
}

fn open_panel(world: &mut World, record: &str, transfer: Option<&str>, observer: bool) {
    close(world);
    let person = match &world.resource::<crate::app::Mobile>().identity {
        crate::session::Identity::Person(person) => Some(person.clone()),
        _ => None,
    };
    let parent = world
        .spawn((
            Overlay,
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
        ui::mount_observer(world, parent, record, person.as_deref());
    } else {
        ui::mount(world, parent, record, person.as_deref(), transfer);
    }
}

fn send(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<Request>>,
    mut places: Local<bevy::ecs::message::MessageCursor<ui::SavePlaceRequest>>,
) {
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
static SAMPLE: Mutex<Option<Sample>> = Mutex::new(None);

pub(crate) struct Sample {
    pub epoch: u64,
    pub received: Instant,
    pub result: Result<(f64, f64, Option<f64>, i64), String>,
    pub stopped: bool,
}

#[cfg(target_os = "android")]
pub(crate) fn sample(sample: Sample) {
    if let Ok(mut latest) = SAMPLE.lock() {
        if !latest
            .as_ref()
            .is_some_and(|latest| latest.stopped && latest.epoch == sample.epoch)
        {
            *latest = Some(sample);
        }
    }
}

#[derive(Default)]
pub(crate) struct Capture {
    epoch: u64,
    until_ms: i64,
    blocked: bool,
    sequence: u64,
}

impl Capture {
    pub async fn stop(&mut self, engine: &engine::Engine) {
        stop_native();
        for source in engine.location_sources().await {
            let _ = engine
                .location_request(
                    Command::StopAll {
                        person: source.settings.controller_uid,
                    },
                    None,
                )
                .await;
        }
        self.until_ms = 0;
        self.epoch = 0;
        if let Ok(mut latest) = SAMPLE.lock() {
            *latest = None;
        }
    }

    pub async fn tick(&mut self, engine: &engine::Engine) {
        let sources = engine.location_sources().await;
        let until = sources
            .iter()
            .filter(|source| source.settings.source_kind == SourceKind::Device)
            .map(|source| source.expires_at_ms)
            .max()
            .unwrap_or_default();
        if until == 0 {
            if self.until_ms != 0 {
                stop_native();
            }
            self.until_ms = 0;
            self.epoch = 0;
            self.blocked = false;
            if let Ok(mut latest) = SAMPLE.lock() {
                *latest = None;
            }
            return;
        }
        let sample = SAMPLE.lock().ok().and_then(|mut latest| latest.take());
        if let Some(sample) = sample.filter(|sample| sample.epoch == self.epoch) {
            if sample.stopped {
                self.stop(engine).await;
                return;
            }
            match sample.result {
                Ok((latitude, longitude, accuracy, age_ms)) if !self.blocked => {
                    let age_ms =
                        age_ms.saturating_add(sample.received.elapsed().as_millis() as i64);
                    self.sequence = self.sequence.saturating_add(1);
                    let _ = engine
                        .publish_device_location(
                            latitude,
                            longitude,
                            accuracy,
                            chrono::Utc::now().timestamp_millis().saturating_sub(age_ms),
                            self.sequence,
                        )
                        .await;
                }
                Err(_) => {
                    self.blocked = true;
                    engine.location_device_unavailable().await;
                    stop_native();
                }
                _ => {}
            }
        }
        if self.blocked {
            return;
        }
        if self.until_ms != until {
            if self.epoch == 0 {
                self.epoch = NEXT.fetch_add(1, Ordering::Relaxed);
            }
            self.until_ms = until;
            #[cfg(target_os = "android")]
            let result = crate::android::location(
                true,
                until.saturating_sub(chrono::Utc::now().timestamp_millis()),
                self.epoch,
            );
            #[cfg(not(target_os = "android"))]
            let result: Result<(), String> =
                Err("Choose a manual source in the mobile preview".into());
            if result.is_err() {
                self.blocked = true;
                engine.location_device_unavailable().await;
            }
        }
    }
}

fn stop_native() {
    #[cfg(target_os = "android")]
    let _ = crate::android::location(false, 0, 0);
}
