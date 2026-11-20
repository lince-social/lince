pub(crate) mod allocation_tests;
mod annotations;
mod audio;
mod chrome;
mod events;
mod face;
mod keyboard;
pub(crate) use keyboard::complete_focused_row;
mod motion;
mod palette;
mod preparation;
mod render;
mod scene;
mod timing;
pub use scene::SchedulePick;
pub(crate) mod source;
mod ui;
#[cfg(any(test, feature = "behavior-checks"))]
pub(crate) mod wake_checks;

#[cfg(test)]
mod integration;

use crate::native::protein_area::Source;
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use lince_interface::time_castle::{self as model, CursorMode, Entry, Mode, Settings};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};

#[derive(Component)]
pub(crate) struct AttachedCard;

pub(crate) fn editable_record(
    world: &World,
    binding: &crate::native::protein_area::RecordBinding,
) -> bool {
    let matches = |owner| {
        world.get::<View>(owner).is_some_and(|view| {
            view.source == binding.source
                && view
                    .entries
                    .iter()
                    .any(|entry| entry.record_uid == binding.uid && !entry.preview)
        })
    };
    matches(binding.area)
        || (binding.area == Entity::PLACEHOLDER
            && world
                .get_resource::<Feeds>()
                .is_some_and(|feeds| feeds.active.keys().copied().any(matches)))
}

#[derive(Component, Clone)]
pub struct TimeSettings(pub Settings);

#[derive(Component, Clone)]
pub struct SelectedOccurrence(pub model::Selection);

#[derive(Component)]
struct View {
    viewport: Entity,
    status: Entity,
    present: Entity,
    details: Entity,
    fields: [Entity; 2],
    entries: Vec<Entry>,
    upcoming: Entity,
    upcoming_rows: Vec<(String, String)>,
    active_row: Option<String>,
    reveal_row: bool,
    upcoming_stamp: Option<(u64, i64, i64, String, u32)>,
    hovered: Option<String>,
    hover_point: Option<Vec3>,
    hover_at: std::time::Instant,
    source: Source,
    selected: Vec<String>,
    page: usize,
    revision: u64,
    detail_revision: u64,
    unwind: f32,
    animation_at: std::time::Instant,
    image: Option<Handle<Image>>,
    rendered: Option<(Arc<Settings>, u64, i64, [u32; 4], u32, bool, usize)>,
    scene: Option<Entity>,
    fallback: bool,
    projection: Option<nucleus::projection::Status>,
    palette: Option<palette::Palette>,
}

struct Feed {
    source: Source,
    selection: Option<Arc<preparation::Selection>>,
    query: protein::Protein,
    key: String,
    id: String,
    sent: bool,
    window: nucleus::projection::Window,
    sender: Option<crate::native::cell_bridge::Sender>,
}

struct ClosingFeed {
    source: Source,
    id: String,
    sender: Option<crate::native::cell_bridge::Sender>,
}

impl From<Feed> for ClosingFeed {
    fn from(feed: Feed) -> Self {
        Self {
            source: feed.source,
            id: feed.id,
            sender: feed.sender,
        }
    }
}

#[derive(Resource, Default)]
struct Feeds {
    active: HashMap<Entity, Feed>,
    closing: Vec<ClosingFeed>,
}

pub(crate) fn organs(world: &World) -> Vec<String> {
    world
        .get_resource::<Feeds>()
        .map(|feeds| {
            feeds
                .active
                .values()
                .map(|feed| &feed.source)
                .chain(feeds.closing.iter().map(|feed| &feed.source))
                .filter_map(|source| match source {
                    Source::Organ(organ) => Some(organ.clone()),
                    Source::Local => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub struct TimeCastlePlugin;

impl Plugin for TimeCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Feeds>()
            .add_message::<crate::native::cell_bridge::CellMessage>()
            .add_observer(events::selected)
            .add_systems(
                Update,
                update
                    .after(crate::native::cell_bridge::ReceiveCell)
                    .after(crate::native::protein_area::UpdateProteinAreas),
            )
            .add_systems(
                PostUpdate,
                render::update
                    .after(bevy::ui::UiSystems::Layout)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(Update, audio::update.after(update));
    }
}

pub fn populate(world: &mut World, owner: Entity) {
    if world.get::<View>(owner).is_some() {
        return;
    }
    let mut fallback = false;
    if world.get::<TimeSettings>(owner).is_none() {
        let timezone = crate::native::schedule_editor::local_timezone().unwrap_or_else(|| {
            fallback = true;
            "UTC".into()
        });
        world.entity_mut(owner).insert(TimeSettings(Settings {
            timezone,
            ..Settings::default()
        }));
    }
    {
        let mut settings = world.get_mut::<TimeSettings>(owner).unwrap();
        settings.0.horizon_ms = settings.0.aperture_ms;
    }
    world
        .entity_mut(owner)
        .insert(crate::native::sand_store::SandCredits(CREDITS));
    let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
    let (viewport, status, present, details, fields) = ui::populate(world, owner, &settings);
    let upcoming = ui::upcoming_panel(world, owner);
    keyboard::attach(world, owner);
    world.entity_mut(owner).insert(View {
        viewport,
        status,
        present,
        details,
        fields,
        entries: Vec::new(),
        upcoming,
        upcoming_rows: Vec::new(),
        active_row: None,
        reveal_row: false,
        upcoming_stamp: None,
        hovered: None,
        hover_point: None,
        hover_at: std::time::Instant::now(),
        source: Source::Local,
        selected: Vec::new(),
        page: 0,
        revision: 0,
        detail_revision: u64::MAX,
        unwind: if settings.mode == Mode::Straight {
            1.0
        } else {
            0.0
        },
        animation_at: std::time::Instant::now(),
        image: None,
        rendered: None,
        scene: None,
        fallback,
        projection: None,
        palette: None,
    });
}

pub(crate) fn stopwatch_panel(world: &mut World, owner: Entity, input: Entity) -> Entity {
    chrome::stopwatch(world, owner, input)
}

pub(crate) fn preview(world: &World, owner: Entity) -> Option<(Entity, Image)> {
    render::preview(world, owner)
}

fn status(world: &mut World, owner: Entity, message: &str) {
    if let Some(label) = world.get::<View>(owner).map(|view| view.status)
        && let Some(mut text) = world.get_mut::<Text>(label)
        && text.0 != message
    {
        text.0 = message.into();
    }
}

fn default_schedule_order(order: Vec<protein::Order>) -> Vec<protein::Order> {
    if order.iter().any(|key| matches!(key, protein::Order::Asc(field) | protein::Order::Desc(field) if field.starts_with('#'))) {
        order
    } else {
        vec![protein::Order::Desc("#task".into())]
    }
}

fn source_query(
    world: &mut World,
    owner: Entity,
) -> Result<(Source, Vec<protein::Predicate>, Vec<protein::Order>), String> {
    let (settings, _) = preparation::settings(world, owner)?;
    let selection = preparation::source(world, owner, &settings)?;
    Ok((
        selection.source.clone(),
        selection.filter.clone(),
        selection.order.clone(),
    ))
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    let Some(feeds) = world.get_resource::<Feeds>() else {
        return;
    };
    let owners: Vec<_> = feeds
        .active
        .iter()
        .filter(|(_, feed)| &feed.source == source)
        .filter(|(owner, _)| !crate::native::laboratory::blocked(world, **owner))
        .map(|(owner, feed)| (*owner, feed.id.clone()))
        .collect();
    for (owner, subscription) in owners {
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                if id == &subscription =>
            {
                let entries: Vec<model::Entry> = rows
                    .iter()
                    .filter(|row| row["kind"] == "schedule-entry")
                    .filter_map(|row| model::Entry::deserialize(row).ok())
                    .collect();
                let changed_selection = if let Some(mut view) = world.get_mut::<View>(owner) {
                    let before = (view.selected.len() == 1)
                        .then(|| {
                            view.entries
                                .iter()
                                .find(|entry| view.selected.contains(&entry.id))
                                .cloned()
                        })
                        .flatten();
                    let selected: std::collections::HashSet<_> = view
                        .entries
                        .iter()
                        .filter(|entry| view.selected.contains(&entry.id))
                        .filter_map(|entry| entry.cue_key(""))
                        .collect();
                    view.selected = entries
                        .iter()
                        .filter(|entry| {
                            view.selected.contains(&entry.id)
                                || entry.cue_key("").is_some_and(|key| selected.contains(&key))
                        })
                        .map(|entry| entry.id.clone())
                        .collect();
                    if view.entries.len() != entries.len()
                        || !view
                            .entries
                            .iter()
                            .zip(&entries)
                            .all(|(a, b)| a.same_schedule(b))
                    {
                        view.entries = entries;
                        view.revision = view.revision.wrapping_add(1);
                    }
                    view.source = source.clone();
                    view.entries
                        .iter()
                        .find(|entry| view.selected.len() == 1 && view.selected.contains(&entry.id))
                        .filter(|entry| before.as_ref().is_some_and(|before| before != *entry))
                        .map(|entry| entry.id.clone())
                } else {
                    None
                };
                if let Some(id) = changed_selection {
                    crate::native::actions::Action::apply(&ui::Select(vec![id]), world, owner);
                }
                let state = rows
                    .iter()
                    .find(|row| row["kind"] == "projection-status")
                    .and_then(|row| {
                        serde_json::from_value::<nucleus::projection::Status>(row["status"].clone())
                            .ok()
                    });
                let forecast = matches!(&state, Some(nucleus::projection::Status::Ready { .. }));
                world.get_mut::<View>(owner).unwrap().projection = state.clone();
                chrome::availability(world, owner, forecast);
                let message = match state {
                    Some(nucleus::projection::Status::Ready { .. }) => {
                        "Future simulation ready".to_owned()
                    }
                    Some(nucleus::projection::Status::Updating {}) | None => {
                        "Updating future simulation".to_owned()
                    }
                    Some(nucleus::projection::Status::Incomplete { reason, .. }) => {
                        use nucleus::projection::Incomplete;
                        match reason {
                            Incomplete::Budget {} => "Future simulation reached its limit; shorten the window or narrow the source",
                            Incomplete::UnavailableRuntime {} => "Future simulation unavailable; showing scheduled Records",
                            Incomplete::ExternalEffects {} => "Future simulation stops at work that needs external services",
                            Incomplete::RuleFailure {} => "Future simulation stops at a rule error",
                            Incomplete::PastWindow {} => "Only current and future work can be projected",
                            Incomplete::UnsupportedFilter {} => "This source filter cannot be projected",
                            Incomplete::UnsupportedUnit {} => "Future simulation cannot project this unit change",
                        }.into()
                    }
                };
                status(world, owner, &message);
            }
            ServerMessage::Error { id, message, .. }
                if id == &subscription || id == crate::native::cell_bridge::CONNECTION =>
            {
                status(world, owner, &format!("Schedule unavailable: {message}"));
                if let Some(feed) = world.resource_mut::<Feeds>().active.get_mut(&owner) {
                    feed.sent = false;
                }
                if let Some(mut view) = world.get_mut::<View>(owner) {
                    view.entries.clear();
                    view.projection = None;
                    view.revision = view.revision.wrapping_add(1);
                }
                chrome::availability(world, owner, false);
            }
            _ => {}
        }
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let _update = nucleus::performance::Span::new("time-feed-update");
    if !crate::native::laboratory::normal(world) {
        return;
    }
    chrome::dismiss(world);
    if let Some(messages) =
        world.get_resource::<Messages<crate::native::cell_bridge::CellMessage>>()
    {
        let messages: Vec<_> = cursor
            .read(messages)
            .map(|message| message.0.clone())
            .collect();
        for message in messages.iter().map(|message| message.as_ref()) {
            receive(world, &Source::Local, &message);
        }
    }
    events::listeners(world);
    let now = chrono::Utc::now().timestamp_millis();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
        .filter(|owner| !crate::native::laboratory::blocked(world, *owner))
        .collect();
    let closed: Vec<_> = world
        .resource::<Feeds>()
        .active
        .keys()
        .copied()
        .filter(|owner| !owners.contains(owner))
        .collect();
    for owner in closed {
        let feed = world.resource_mut::<Feeds>().active.remove(&owner).unwrap();
        world.resource_mut::<Feeds>().closing.push(feed.into());
    }
    for owner in owners {
        let (settings, settings_valid) = preparation::settings(world, owner).unwrap();
        let selection = match preparation::source(world, owner, &settings) {
            Ok(value) => value,
            Err(error) => {
                status(world, owner, &error);
                if let Some(feed) = world.resource_mut::<Feeds>().active.remove(&owner) {
                    world.resource_mut::<Feeds>().closing.push(feed.into());
                }
                if let Some(mut view) = world.get_mut::<View>(owner)
                    && (!view.entries.is_empty() || view.projection.is_some())
                {
                    view.entries.clear();
                    view.projection = None;
                    view.revision = view.revision.wrapping_add(1);
                }
                chrome::availability(world, owner, false);
                continue;
            }
        };
        let source = &selection.source;
        let previous = world.resource::<Feeds>().active.get(&owner);
        let window = previous
            .filter(|feed| {
                feed.window.timezone == settings.timezone
                    && feed.window.until_ms >= now.saturating_add(settings.horizon_ms)
                    && feed.window.from_ms <= settings.history_from(now)
            })
            .map(|feed| std::borrow::Cow::Borrowed(&feed.window))
            .or_else(|| {
                settings_valid
                    .then(|| settings.window(now))
                    .flatten()
                    .map(std::borrow::Cow::Owned)
            });
        let Some(window) = window else {
            status(world, owner, "Invalid clock settings");
            continue;
        };
        let retained = previous.is_some_and(|feed| {
            &feed.source == source
                && feed.window == *window
                && feed
                    .selection
                    .as_ref()
                    .is_some_and(|previous| Arc::ptr_eq(previous, &selection))
        });
        let query = (!retained).then(|| {
            let _prepare = nucleus::performance::Span::new("time-subscription-preparation");
            let mut query =
                protein::schedule::query(window.clone().into_owned(), selection.filter.clone());
            query.order = selection.order.clone();
            let key = serde_json::to_string(&query).unwrap();
            nucleus::performance::add("time-subscription-queries-prepared", 1);
            (query, key)
        });
        let changed = query.as_ref().is_some_and(|(_, key)| {
            previous.is_none_or(|feed| &feed.source != source || &feed.key != key)
        });
        let filter_key = |query: &protein::Protein| {
            serde_json::to_value(
                query
                    .filter
                    .iter()
                    .filter(|filter| !matches!(filter, protein::Predicate::ProjectionWindow(_)))
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        };
        let same_source = changed
            && previous.is_some_and(|feed| {
                &feed.source == source
                    && feed.window.timezone == window.timezone
                    && filter_key(&feed.query) == filter_key(&query.as_ref().unwrap().0)
            });
        if changed {
            let (query, key) = query.unwrap();
            let window = window.into_owned();
            if let Some(feed) = world.resource_mut::<Feeds>().active.remove(&owner) {
                world.resource_mut::<Feeds>().closing.push(feed.into());
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            if !same_source {
                view.entries.clear();
                view.selected.clear();
                view.page = 0;
                view.revision = view.revision.wrapping_add(1);
            }
            view.projection = None;
            view.source = source.clone();
            world.resource_mut::<Feeds>().active.insert(
                owner,
                Feed {
                    source: source.clone(),
                    selection: Some(selection.clone()),
                    query,
                    key,
                    id: nucleus::new_uid("clock"),
                    sent: false,
                    window,
                    sender: None,
                },
            );
            status(world, owner, "Loading schedule");
            chrome::availability(world, owner, false);
        } else if !retained {
            drop(window);
            world
                .resource_mut::<Feeds>()
                .active
                .get_mut(&owner)
                .unwrap()
                .selection = Some(selection.clone());
        }
        crate::native::protein_area::ensure_auxiliary(world, source);
        let sender = crate::native::protein_area::auxiliary_sender(world, source);
        let feed = world.resource::<Feeds>().active.get(&owner).unwrap();
        if !feed.sent {
            let message = ClientMessage::Subscribe {
                id: feed.id.clone(),
                protein: feed.query.clone(),
            };
            if let Some(sender) = sender
                && sender.try_send(message.clone()).is_ok()
            {
                crate::native::protein_area::auxiliary_subscribed(world, source, &message);
                let mut feeds = world.resource_mut::<Feeds>();
                let feed = feeds.active.get_mut(&owner).unwrap();
                feed.sent = true;
                feed.sender = Some(sender);
            } else {
                status(world, owner, "Connect or sign in to the schedule source");
            }
        } else if sender.as_ref().is_none_or(|sender| sender.is_closed()) {
            world
                .resource_mut::<Feeds>()
                .active
                .get_mut(&owner)
                .unwrap()
                .sent = false;
            let mut view = world.get_mut::<View>(owner).unwrap();
            if !view.entries.is_empty() || view.projection.is_some() {
                view.entries.clear();
                view.projection = None;
                view.revision = view.revision.wrapping_add(1);
            }
            status(world, owner, "Schedule connection closed");
            chrome::availability(world, owner, false);
        }
        ui::details(world, owner, now);
    }
    let closing = std::mem::take(&mut world.resource_mut::<Feeds>().closing);
    for feed in closing {
        match &feed.source {
            Source::Organ(organ) => {
                crate::native::protein_area::auxiliary_unsubscribe(world, organ, feed.id)
            }
            Source::Local => {
                if let Some(sender) = feed.sender.as_ref().filter(|sender| !sender.is_closed())
                    && sender
                        .try_send(ClientMessage::Unsubscribe {
                            id: feed.id.clone(),
                        })
                        .is_err()
                {
                    world.resource_mut::<Feeds>().closing.push(feed);
                }
            }
        }
    }
}

pub const CREDITS: &[crate::native::credits::Attribution] = &[
    crate::native::credits::Attribution {
        name: "tts",
        author: "Nolan Darilek and contributors",
        license: include_str!("../../licenses/native/tts-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "speech-dispatcher Rust bindings",
        author: "Nolan Darilek and contributors",
        license: include_str!("../../licenses/native/tts-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "ttf-parser",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/ttf-parser-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../../licenses/native/cpal-Apache-2.0.txt"),
    },
    crate::native::credits::Attribution {
        name: "usvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/usvg-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "tiny-skia",
        author: "Yevhenii Reizner; Google; Skia contributors",
        license: include_str!("../../licenses/native/tiny-skia-BSD.txt"),
    },
    crate::native::credits::Attribution {
        name: "fontdb",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/fontdb-MIT.txt"),
    },
    crate::native::credits::SYMBOLS,
    crate::native::credits::DEJAVU,
    crate::native::credits::Attribution {
        name: "Noto Sans Mono CJK",
        author: include_str!("../../../../institute/assets/fonts/NotoSansMonoCJK/CREDITS.txt"),
        license: include_str!("../../../../institute/assets/fonts/NotoSansMonoCJK/OFL.txt"),
    },
    crate::native::credits::FONTIQUE,
    crate::native::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::native::credits::BEVY_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and contributors",
        license: crate::native::credits::CHRONO_LICENSE,
    },
    crate::native::credits::Attribution {
        name: "Chrono-TZ",
        author: "Chrono contributors; IANA timezone data",
        license: include_str!("../../licenses/native/chrono-tz/LICENSE"),
    },
    crate::native::credits::Attribution {
        name: "iana-time-zone",
        author: "The iana-time-zone contributors",
        license: include_str!("../../licenses/native/iana-time-zone-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../../licenses/native/resvg-MIT.txt"),
    },
    crate::native::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::native::credits::LATO_LICENSE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_schedule_refreshes_do_not_invalidate_the_clock_layout() {
        let mut world = World::new();
        world.init_resource::<Feeds>();
        world.insert_resource(crate::native::theme::Typography(Handle::default()));
        world.init_resource::<bevy::input_focus::InputFocus>();
        let owner = world.spawn(Node::default()).id();
        populate(&mut world, owner);
        let (sender, _receiver) = tokio::sync::mpsc::channel(1);
        world
            .resource_mut::<Feeds>()
            .active
            .insert(owner, feed(sender));
        let rows = vec![serde_json::json!({
            "uid":"need", "kind":"schedule-entry", "record_uid":"r:need",
            "head":"Floss", "quantity":"-1", "category":"overdue",
            "time":null, "origin":{"kind":"need"}, "preview":false
        })];
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Snapshot {
                id: "closed-clock".into(),
                rows: rows.clone().into(),
            },
        );
        let revision = world.get::<View>(owner).unwrap().revision;
        assert_eq!(world.get::<View>(owner).unwrap().entries.len(), 1);
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Update {
                id: "closed-clock".into(),
                rows: rows.into(),
            },
        );
        assert_eq!(world.get::<View>(owner).unwrap().revision, revision);
        receive(
            &mut world,
            &Source::Local,
            &ServerMessage::Update {
                id: "closed-clock".into(),
                rows: Vec::new().into(),
            },
        );
        assert!(world.get::<View>(owner).unwrap().entries.is_empty());
        assert_eq!(world.get::<View>(owner).unwrap().revision, revision + 1);
    }

    fn feed(sender: impl Into<crate::native::cell_bridge::Sender>) -> Feed {
        let window = Settings::default()
            .window(chrono::Utc::now().timestamp_millis())
            .unwrap();
        let query = protein::schedule::query(window.clone(), Vec::new());
        Feed {
            source: Source::Local,
            selection: None,
            key: serde_json::to_string(&query).unwrap(),
            query,
            id: "closed-clock".into(),
            sent: true,
            window,
            sender: Some(sender.into()),
        }
    }

    #[test]
    fn time_castle_close_retries_unsubscribe_after_channel_backpressure() {
        let mut app = App::new();
        app.init_resource::<Feeds>().add_systems(Update, update);
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        sender
            .try_send(ClientMessage::Unsubscribe { id: "other".into() })
            .unwrap();
        let owner = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Feeds>()
            .active
            .insert(owner, feed(sender));
        app.world_mut().despawn(owner);
        app.update();
        assert!(app.world().resource::<Feeds>().active.is_empty());
        assert_eq!(app.world().resource::<Feeds>().closing.len(), 1);
        assert!(
            matches!(receiver.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == "other")
        );
        app.update();
        assert!(app.world().resource::<Feeds>().closing.is_empty());
        assert!(
            matches!(receiver.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == "closed-clock")
        );
        app.update();
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn time_castle_close_discards_a_disconnected_feed() {
        let mut app = App::new();
        app.init_resource::<Feeds>().add_systems(Update, update);
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        drop(receiver);
        let owner = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<Feeds>()
            .active
            .insert(owner, feed(sender));
        app.world_mut().despawn(owner);
        app.update();
        assert!(app.world().resource::<Feeds>().active.is_empty());
        assert!(app.world().resource::<Feeds>().closing.is_empty());
    }
}
