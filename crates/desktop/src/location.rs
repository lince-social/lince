use bevy::prelude::*;
use lince_interface::location::{self as ui, Request};

#[cfg(target_os = "linux")]
mod portal;

pub struct LocationPlugin;

#[derive(Resource)]
struct AuthenticationReplies {
    sender: tokio::sync::mpsc::UnboundedSender<(String, Result<String, String>)>,
    receiver: tokio::sync::mpsc::UnboundedReceiver<(String, Result<String, String>)>,
}

impl Default for AuthenticationReplies {
    fn default() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        Self { sender, receiver }
    }
}

#[derive(Resource)]
struct CaptureTask(tokio::task::JoinHandle<()>);

impl Drop for CaptureTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Plugin for LocationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ui::LocationUiPlugin)
            .init_resource::<ui::CopyReferencesAvailable>()
            .init_resource::<AuthenticationReplies>()
            .add_systems(
                Update,
                (receive, send, copy_reference, authenticate, capture).chain(),
            );
    }
}

fn copy_reference(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ui::CopyReferenceRequest>>,
) {
    let requests: Vec<_> = cursor
        .read(world.resource::<Messages<ui::CopyReferenceRequest>>())
        .cloned()
        .collect();
    for request in requests {
        let result = world
            .get_resource_mut::<bevy::clipboard::Clipboard>()
            .ok_or_else(|| "Clipboard is unavailable".to_string())
            .and_then(|mut clipboard| {
                clipboard
                    .set_text(&request.reference)
                    .map_err(|error| error.to_string())
            });
        ui::reference_copied(world, request.panel, result);
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
        let Some(handle) = world.get_resource::<crate::app::CellHandle>() else {
            ui::authentication_reply(
                world,
                &request.id,
                Err("The local Cell is unavailable".into()),
            );
            continue;
        };
        let cell = handle.0.clone();
        let sender = world.resource::<AuthenticationReplies>().sender.clone();
        let wake = world.get_resource::<crate::wake::WakeSignal>().cloned();
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            ui::authentication_reply(
                world,
                &request.id,
                Err("The device connection is unavailable".into()),
            );
            continue;
        };
        runtime.spawn(async move {
            let result = cell
                .authenticate_location_device(
                    &request.node,
                    &request.record,
                    &request.person,
                    request.expected_person.as_deref(),
                    request.username,
                    request.password,
                )
                .await;
            let _ = sender.send((request.id, result));
            if let Some(wake) = wake {
                wake.ring();
            }
        });
    }
    while let Ok((id, result)) = world
        .resource_mut::<AuthenticationReplies>()
        .receiver
        .try_recv()
    {
        ui::authentication_reply(world, &id, result);
    }
}

#[derive(Clone)]
struct Open {
    record: String,
    transfer: Option<String>,
    person: Option<String>,
    observer: bool,
}

impl crate::actions::Action for Open {
    fn apply(&self, world: &mut World, target: Entity) {
        let existing = world.get::<Children>(target).and_then(|children| {
            children
                .iter()
                .find(|child| world.get::<ui::Panel>(*child).is_some())
        });
        if let Some(existing) = existing {
            world.despawn(existing);
            return;
        }
        let panel = if self.observer {
            ui::mount_observer(world, target, &self.record, self.person.as_deref())
        } else {
            ui::mount(
                world,
                target,
                &self.record,
                self.person.as_deref(),
                self.transfer.as_deref(),
            )
        };
        #[cfg(target_os = "linux")]
        {
            ui::credits(
                world,
                panel,
                vec![
                    (
                        "Linux location portal · zbus contributors · MIT",
                        include_str!("../licenses/zbus-MIT.txt"),
                    ),
                    (
                        "Asynchronous streams · futures-rs contributors · MIT",
                        include_str!("../licenses/futures-MIT.txt"),
                    ),
                ],
            );
        }
        #[cfg(not(target_os = "linux"))]
        let _ = panel;
    }
}

pub fn record_controls(
    world: &mut World,
    parent: Entity,
    record: &str,
    transfer: Option<&str>,
    person: Option<&str>,
) {
    crate::visibility_castle::record_controls(world, parent, record);
    crate::sand_panel::button(
        world,
        parent,
        parent,
        "Location and sharing",
        Open {
            record: record.into(),
            transfer: transfer.map(str::to_string),
            person: person
                .filter(|person| !person.is_empty())
                .map(str::to_string),
            observer: false,
        },
    );
    crate::sand_panel::button(
        world,
        parent,
        parent,
        "View shared location",
        Open {
            record: record.into(),
            transfer: None,
            person: person
                .filter(|person| !person.is_empty())
                .map(str::to_string),
            observer: true,
        },
    );
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
        if let Err(error) = crate::sand_panel::send(
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
        if let Err(error) = crate::sand_panel::send(
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

fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| cursor.read(messages).cloned().collect())
        .unwrap_or_default();
    for message in messages {
        match message.0 {
            cell::ServerMessage::ActionOk { id, data, .. } => {
                ui::receive(world, &id, Ok(data.unwrap_or(serde_json::Value::Null)));
            }
            cell::ServerMessage::Error { id, message, .. } => {
                ui::receive(world, &id, Err(message));
            }
            _ => {}
        }
    }
}

fn capture(world: &mut World) {
    if world.contains_resource::<CaptureTask>() {
        return;
    }
    let Some(handle) = world.get_resource::<crate::app::CellHandle>() else {
        return;
    };
    let engine = std::sync::Arc::downgrade(&handle.0.engine);
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    world.insert_resource(CaptureTask(runtime.spawn(async move {
        let mut blocked = false;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let Some(engine) = engine.upgrade() else {
                break;
            };
            let needed =
                engine.location_sources().await.iter().any(|source| {
                    source.settings.source_kind == nucleus::location::SourceKind::Device
                });
            if !needed {
                blocked = false;
                continue;
            }
            if blocked {
                continue;
            }
            #[cfg(target_os = "linux")]
            let result = portal::capture(&engine).await;
            #[cfg(not(target_os = "linux"))]
            let result: Result<(), String> = Err("Use a manual source on this platform".into());
            if result.is_err() {
                engine.location_device_unavailable().await;
                blocked = true;
            }
        }
    })));
}
