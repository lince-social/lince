use super::*;
use bevy::ecs::message::MessageCursor;
use std::collections::HashMap;

#[derive(Component, Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub uid: String,
}

impl Attachment {
    pub fn valid(&self) -> bool {
        nucleus::valid_uid(&self.uid, "r")
    }
}

struct Stream {
    id: String,
    uid: String,
    sender: tokio::sync::mpsc::Sender<cell::ClientMessage>,
    baseline: Option<crate::native::area::RecordProperties>,
    message: String,
}

#[derive(Resource, Default)]
struct Streams {
    active: HashMap<Entity, Stream>,
    retired: Vec<Stream>,
    cursor: MessageCursor<crate::native::cell_bridge::CellMessage>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Streams>().add_systems(
        PostUpdate,
        synchronize
            .after(crate::native::actions::ApplyActions)
            .before(crate::native::area::ComputeAreaForces)
            .before(bevy::ui::UiSystems::Layout),
    );
}

pub fn attach(
    world: &mut World,
    root: Entity,
    item: Entity,
    uid: &str,
) -> Result<(), &'static str> {
    if !eligible(world, root, item) {
        return Err("The original canvas item is unavailable.");
    }
    let attachment = Attachment {
        uid: uid.trim().to_owned(),
    };
    if !attachment.valid() {
        return Err("Choose an existing Record UID.");
    }
    if world.get::<Attachment>(item).is_none() && world.get::<RecordBinding>(item).is_some() {
        return Err("This item already presents its own Record.");
    }
    world.entity_mut(item).insert(attachment);
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub fn detach(world: &mut World, root: Entity, item: Entity) -> Result<(), &'static str> {
    if !eligible(world, root, item) {
        return Err("The original canvas item is unavailable.");
    }
    world.entity_mut(item).remove::<Attachment>();
    if let Some(mut streams) = world.remove_resource::<Streams>() {
        if let Some(stream) = streams.active.remove(&item) {
            restore(world, item, &stream);
            streams.retired.push(stream);
        }
        world.insert_resource(streams);
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

fn restore(world: &mut World, item: Entity, stream: &Stream) {
    let metadata = window_metadata(world, item);
    if let Ok(mut entity) = world.get_entity_mut(item) {
        entity.remove::<RecordBinding>();
        if let Some(baseline) = &stream.baseline {
            let mut baseline = baseline.clone();
            if let Some((title, app_id)) = metadata {
                baseline.0["head"] = title;
                baseline.0["app_id"] = app_id;
            }
            entity.insert(baseline);
        } else {
            entity.remove::<crate::native::area::RecordProperties>();
        }
    }
}

pub(super) fn status(world: &World, item: Entity) -> &str {
    world
        .get_resource::<Streams>()
        .and_then(|streams| streams.active.get(&item))
        .map_or("", |stream| stream.message.as_str())
}

fn apply(world: &mut World, item: Entity, stream: &mut Stream, row: Option<serde_json::Value>) {
    restore(world, item, stream);
    let Some(mut row) = row.filter(|row| row.is_object() && row["uid"] == stream.uid) else {
        stream.message = "Record unavailable.".into();
        return;
    };
    if let Some((title, app_id)) = window_metadata(world, item) {
        row["window_title"] = title;
        row["app_id"] = app_id;
    }
    world.entity_mut(item).insert((
        crate::native::area::RecordProperties(row),
        RecordBinding {
            area: item,
            uid: stream.uid.clone(),
            source: Source::Local,
        },
    ));
    stream.message.clear();
}

fn synchronize(world: &mut World) {
    let mut streams = world.remove_resource::<Streams>().unwrap_or_default();
    let sender = world
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
        .map(|bridge| bridge.outgoing.clone())
        .filter(|sender| !sender.is_closed());
    let items: HashMap<_, _> = world
        .query_filtered::<(Entity, &Attachment), (
            With<crate::native::canvas::CanvasItem>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        )>()
        .iter(world)
        .filter(|(item, attachment)| attachment.valid() && source_allowed(world, *item))
        .map(|(item, attachment)| (item, attachment.uid.clone()))
        .collect();
    let stale: Vec<_> = streams
        .active
        .iter()
        .filter(|(item, stream)| {
            items.get(*item) != Some(&stream.uid)
                || sender
                    .as_ref()
                    .is_none_or(|sender| !sender.same_channel(&stream.sender))
        })
        .map(|(item, _)| *item)
        .collect();
    for item in stale {
        let stream = streams.active.remove(&item).unwrap();
        restore(world, item, &stream);
        streams.retired.push(stream);
    }
    streams.retired.retain(|stream| {
        match stream.sender.try_send(cell::ClientMessage::Unsubscribe {
            id: stream.id.clone(),
        }) {
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => true,
            _ => false,
        }
    });
    let mut retry = !streams.retired.is_empty();
    if let Some(sender) = sender {
        for (item, uid) in items {
            if streams.active.contains_key(&item) {
                continue;
            }
            let mut query = crate::native::full_record::config(&uid, Source::Local)
                .draft
                .compile()
                .unwrap();
            query.fields = None;
            query.include.links = Some(protein::LinksInclude {
                kinds: vec!["*".into()],
                direction: protein::LinkDirection::Both,
                depth: 1,
            });
            let id = format!("item-properties-{}", nucleus::new_uid("op"));
            if sender
                .try_send(cell::ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query,
                })
                .is_err()
            {
                retry = true;
                continue;
            }
            streams.active.insert(
                item,
                Stream {
                    id,
                    uid,
                    sender: sender.clone(),
                    baseline: world
                        .get::<crate::native::area::RecordProperties>(item)
                        .cloned(),
                    message: "Loading Record…".into(),
                },
            );
        }
    }
    let messages: Vec<_> = world
        .get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
        .map(|messages| streams.cursor.read(messages).cloned().collect())
        .unwrap_or_default();
    for message in messages {
        match message.0 {
            cell::ServerMessage::Snapshot { id, rows, .. }
            | cell::ServerMessage::Update { id, rows, .. } => {
                for (item, stream) in &mut streams.active {
                    if stream.id == id {
                        let row = rows.iter().find(|row| row["uid"] == stream.uid).cloned();
                        apply(world, *item, stream, row);
                    }
                }
            }
            cell::ServerMessage::Error { id, message, .. } => {
                for (item, stream) in &mut streams.active {
                    if stream.id == id {
                        restore(world, *item, stream);
                        stream.message = message.chars().take(1024).collect();
                    }
                }
            }
            _ => {}
        }
    }
    world.insert_resource(streams);
    if retry && let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.after(std::time::Duration::from_millis(50));
    }
}

pub(super) fn source_allowed(world: &World, mut item: Entity) -> bool {
    for _ in 0..crate::native::layout::MAX_DEPTH {
        if world
            .get::<crate::native::practice_cells::PracticeSource>(item)
            .is_some()
        {
            return false;
        }
        if world.get::<Workspaces>(item).is_some() {
            return !crate::native::laboratory::blocked(world, item);
        }
        let Some(parent) = world.get::<ChildOf>(item) else {
            return false;
        };
        item = parent.parent();
    }
    false
}

fn window_metadata(world: &World, item: Entity) -> Option<(serde_json::Value, serde_json::Value)> {
    world.get::<crate::native::layout::desktop::ApplicationWindow>(item)?;
    let metadata = &world.get::<crate::native::area::RecordProperties>(item)?.0;
    Some((
        metadata
            .get("window_title")
            .or_else(|| metadata.get("head"))
            .cloned()
            .unwrap_or_default(),
        metadata.get("app_id").cloned().unwrap_or_default(),
    ))
}
