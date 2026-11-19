use super::*;
use crate::native::cell_bridge::replies;
use cell::{ClientMessage, ServerMessage};
use std::sync::Arc;

#[derive(Component)]
pub(super) struct TimezoneInput(pub Entity);

struct Preparation {
    config: crate::native::protein_area::Config,
    year: i32,
    month: u32,
    timezone: String,
}

impl Preparation {
    fn matches(&self, config: &crate::native::protein_area::Config, calendar: &Calendar) -> bool {
        self.config.same_query_inputs(config)
            && self.year == calendar.year
            && self.month == calendar.month
            && self.timezone == calendar.timezone
    }
}

#[derive(Component)]
pub(super) struct Feed {
    id: String,
    query: String,
    preparation: Preparation,
    pub rows: Arc<Vec<serde_json::Value>>,
    pub revision: u64,
    pub status: String,
    ready: bool,
    subscribed: bool,
    outgoing: crate::native::cell_bridge::Sender,
}

impl Drop for Feed {
    fn drop(&mut self) {
        if !self.subscribed {
            return;
        }
        let message = ClientMessage::Unsubscribe {
            id: self.id.clone(),
        };
        let _ = self.outgoing.try_send(message);
    }
}

pub(super) fn receive(
    mut messages: MessageReader<crate::native::cell_bridge::CellMessage>,
    mut feeds: Query<&mut Feed>,
) {
    let _receive = nucleus::performance::Span::new("calendar-projection-receive");
    if feeds.is_empty() {
        messages.clear();
        return;
    }
    let incoming: Vec<_> = messages
        .read()
        .filter(|message| {
            matches!(
                message.0.as_ref(),
                ServerMessage::Snapshot { .. }
                    | ServerMessage::Update { .. }
                    | ServerMessage::Error { .. }
            )
        })
        .map(|message| message.0.clone())
        .collect();
    if incoming.is_empty() {
        return;
    }
    let batch = replies::Batch::new(&incoming, feeds.iter().nth(1).is_some());
    for mut feed in &mut feeds {
        let mut after = 0;
        while let Some(index) = batch.next(after, std::iter::once(feed.id.as_str())) {
            after = index + 1;
            match batch.message(index) {
                ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                    if id == &feed.id =>
                {
                    nucleus::performance::add("calendar-projection-replies", 1);
                    if feed.ready && (Arc::ptr_eq(&feed.rows, rows) || feed.rows == *rows) {
                        nucleus::performance::add("calendar-projection-unchanged-replies", 1);
                        continue;
                    }
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
                    nucleus::performance::add("calendar-projection-replies", 1);
                    if !feed.ready && feed.rows.is_empty() && feed.status == *message {
                        nucleus::performance::add("calendar-projection-unchanged-replies", 1);
                        continue;
                    }
                    feed.status.clone_from(message);
                    feed.rows = Arc::default();
                    feed.ready = false;
                    feed.revision += 1;
                }
                _ => {}
            }
        }
    }
}

fn retire_hidden(world: &mut World, owner: Entity) {
    let Some(mut feed) = world.get_mut::<Feed>(owner) else {
        return;
    };
    let retired = match feed.outgoing.try_reserve() {
        Ok(permit) => {
            permit.send(ClientMessage::Unsubscribe {
                id: feed.id.clone(),
            });
            true
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => true,
        Err(tokio::sync::mpsc::error::TrySendError::Full(())) => false,
    };
    if retired {
        feed.subscribed = false;
        drop(feed);
        world.entity_mut(owner).remove::<Feed>();
        if let Some(mut view) = world.get_mut::<View>(owner) {
            view.projection_revision = None;
            view.dirty = true;
        }
        nucleus::performance::add("calendar-projection-hidden-retirements", 1);
    }
}

pub(super) fn maintain(world: &mut World, owner: Entity) {
    let _maintain = nucleus::performance::Span::new("calendar-projection-update");
    if !crate::native::output_views::presents_entity(world, owner) {
        retire_hidden(world, owner);
        return;
    }
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
    if config.source != crate::native::protein_area::Source::Local {
        world.entity_mut(owner).remove::<Feed>();
        return;
    }
    let calendar = &world.get::<CalendarSand>(owner).unwrap().0;
    if world
        .get::<Feed>(owner)
        .is_some_and(|feed| feed.preparation.matches(config, calendar))
    {
        return;
    }
    let _prepare = nucleus::performance::Span::new("calendar-projection-preparation");
    let Ok(mut query) = config.query() else {
        world.entity_mut(owner).remove::<Feed>();
        return;
    };
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
    nucleus::performance::add("calendar-projection-queries-prepared", 1);
    let preparation = Preparation {
        config: config.clone(),
        year: calendar.year,
        month: calendar.month,
        timezone: calendar.timezone.clone(),
    };
    if world
        .get::<Feed>(owner)
        .is_some_and(|feed| feed.query == key)
    {
        world.get_mut::<Feed>(owner).unwrap().preparation = preparation;
        return;
    }
    let Some(bridge) = world.get_non_send::<crate::native::cell_bridge::CellBridge>() else {
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
        preparation,
        rows: Arc::default(),
        revision,
        status: "Projection updating".into(),
        ready: false,
        subscribed: true,
        outgoing,
    });
}

pub(super) fn data(world: &World, owner: Entity) -> Option<&[serde_json::Value]> {
    world
        .get::<Feed>(owner)
        .filter(|feed| feed.ready)
        .map(|feed| feed.rows.as_slice())
}
