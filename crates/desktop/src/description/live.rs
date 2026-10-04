use super::*;
use cell::{ClientMessage, ServerMessage};
use std::collections::HashMap;

struct Feed {
    entity: Entity,
    reference: String,
    source: Source,
    sender: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
}

#[derive(Resource, Default)]
struct Feeds(HashMap<String, Feed>);

pub(super) fn watch(world: &mut World, entity: Entity, reference: &str, source: Source) {
    world.init_resource::<Feeds>();
    if world.resource::<Feeds>().0.len() >= 128 {
        crate::edit_mode::label(world, entity, "Description view limit reached.", 14.0);
        return;
    }
    let id = nucleus::new_uid("description");
    world.resource_mut::<Feeds>().0.insert(
        id,
        Feed {
            entity,
            reference: reference.into(),
            source,
            sender: None,
        },
    );
}

pub(super) fn sender(
    world: &World,
    source: &Source,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    crate::protein_area::editor_sender(
        world,
        &crate::protein_area::RecordBinding {
            area: Entity::PLACEHOLDER,
            uid: String::new(),
            source: source.clone(),
        },
    )
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    if matches!(message, ServerMessage::Error { id, code, .. } if id == "connection" || id == crate::cell_bridge::CONNECTION || code.as_deref() == Some("session_expired"))
    {
        let targets = world
            .get_resource::<Feeds>()
            .map(|feeds| {
                feeds
                    .0
                    .values()
                    .filter(|feed| &feed.source == source)
                    .map(|feed| feed.entity)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for entity in targets {
            if world.entities().contains(entity) {
                super::transclusion::received(world, entity, None);
                super::assets::permission(world, entity, None);
            }
        }
    }
    let (id, rows) = match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, rows.as_slice())
        }
        ServerMessage::Error { id, .. } => (id, &[][..]),
        _ => {
            super::assets::receive(world, source, message);
            return;
        }
    };
    let entity = world
        .get_resource::<Feeds>()
        .and_then(|feeds| feeds.0.get(id))
        .filter(|feed| &feed.source == source)
        .map(|feed| feed.entity);
    if let Some(entity) = entity
        && world.entities().contains(entity)
    {
        super::transclusion::received(world, entity, rows.first());
        super::assets::permission(world, entity, rows.first());
    }
    super::assets::receive(world, source, message);
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &Source::Local, &message);
    }
    world.init_resource::<Feeds>();
    let mut feeds = world.remove_resource::<Feeds>().unwrap();
    feeds.0.retain(|id, feed| {
        if !world.entities().contains(feed.entity) {
            return feed.sender.as_ref().is_some_and(|sender| {
                sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_err()
                    && !sender.is_closed()
            });
        }
        if feed.sender.as_ref().is_none_or(|sender| sender.is_closed()) {
            feed.sender = None;
            if let Some(sender) = sender(world, &feed.source) {
                let predicate = if nucleus::valid_uid(&feed.reference, "r") {
                    protein::Predicate::UidEq(feed.reference.clone())
                } else {
                    protein::Predicate::SlugEq(feed.reference.trim_start_matches(['@', '#']).into())
                };
                let query = protein::Protein {
                    source: protein::Source::Record,
                    filter: vec![predicate],
                    fields: Some(vec!["uid".into(), "head".into(), "body".into()]),
                    include: Default::default(),
                    aggregate: None,
                    order: Vec::new(),
                    limit: Some(1),
                };
                if sender
                    .try_send(ClientMessage::Subscribe {
                        id: id.clone(),
                        protein: query,
                    })
                    .is_ok()
                {
                    feed.sender = Some(sender);
                }
            }
        }
        true
    });
    world.insert_resource(feeds);
}
