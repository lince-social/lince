use super::*;
use cell::{ClientMessage, ServerMessage};

#[derive(Component)]
pub(super) struct TimezoneInput(pub Entity);

#[derive(Component)]
pub(super) struct Feed {
    id: String,
    query: String,
    pub rows: Vec<serde_json::Value>,
    pub revision: u64,
    pub status: String,
    ready: bool,
    outgoing: tokio::sync::mpsc::Sender<ClientMessage>,
}

impl Drop for Feed {
    fn drop(&mut self) {
        let message = ClientMessage::Unsubscribe {
            id: self.id.clone(),
        };
        let _ = self.outgoing.try_send(message);
    }
}

pub(super) fn receive(
    mut messages: MessageReader<crate::cell_bridge::CellMessage>,
    mut feeds: Query<&mut Feed>,
) {
    for message in messages.read() {
        for mut feed in &mut feeds {
            match &message.0 {
                ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                    if id == &feed.id =>
                {
                    feed.rows = rows.clone();
                    feed.status = rows
                        .iter()
                        .find(|row| row["kind"] == "projection-status")
                        .map_or(
                            "Projection updating".into(),
                            |row| match row["status"]["kind"].as_str() {
                                Some("ready") => "Projection ready".into(),
                                Some("incomplete") => format!(
                                    "Projection incomplete: {}",
                                    row["status"]["reason"]["kind"]
                                        .as_str()
                                        .unwrap_or("unavailable")
                                ),
                                _ => "Projection updating".into(),
                            },
                        );
                    feed.ready = true;
                    feed.revision += 1;
                }
                ServerMessage::Error { id, message, .. } if id == &feed.id => {
                    feed.status = message.clone();
                    feed.rows.clear();
                    feed.ready = false;
                    feed.revision += 1;
                }
                _ => {}
            }
        }
    }
}

pub(super) fn maintain(world: &mut World, owner: Entity) {
    let Some(source) = area(world, owner) else {
        world.entity_mut(owner).remove::<Feed>();
        return;
    };
    let Some(config) = world
        .get::<InfluenceArea>(source)
        .and_then(|area| area.protein.as_ref())
    else {
        world.entity_mut(owner).remove::<Feed>();
        return;
    };
    if config.source != crate::protein_area::Source::Local {
        world.entity_mut(owner).remove::<Feed>();
        return;
    }
    let Ok(mut query) = config.query() else {
        world.entity_mut(owner).remove::<Feed>();
        return;
    };
    let calendar = &world.get::<CalendarSand>(owner).unwrap().0;
    let Ok(window) = nucleus::projection::Window::month(
        calendar.year,
        calendar.month,
        calendar.timezone.clone(),
    ) else {
        world.entity_mut(owner).remove::<Feed>();
        return;
    };
    query.source = protein::Source::Calendar;
    query.include.projection = None;
    query
        .filter
        .retain(|predicate| !matches!(predicate, protein::Predicate::ProjectionWindow(_)));
    query
        .filter
        .push(protein::Predicate::ProjectionWindow(window));
    query.fields = None;
    let Ok(key) = serde_json::to_string(&query) else {
        return;
    };
    if world
        .get::<Feed>(owner)
        .is_some_and(|feed| feed.query == key)
    {
        return;
    }
    let Some(bridge) = world.get_non_send::<crate::cell_bridge::CellBridge>() else {
        return;
    };
    let id = nucleus::new_uid("calendar");
    let outgoing = bridge.outgoing.clone();
    if bridge
        .outgoing
        .try_send(ClientMessage::Subscribe {
            id: id.clone(),
            protein: query,
        })
        .is_err()
    {
        return;
    }
    let revision = world.get::<Feed>(owner).map_or(1, |feed| feed.revision + 1);
    world.entity_mut(owner).insert(Feed {
        id,
        query: key,
        rows: Vec::new(),
        revision,
        status: "Projection updating".into(),
        ready: false,
        outgoing,
    });
}

pub(super) fn data(world: &World, owner: Entity) -> Option<&[serde_json::Value]> {
    world
        .get::<Feed>(owner)
        .filter(|feed| feed.ready)
        .map(|feed| feed.rows.as_slice())
}
