mod annotations;
mod audio;
mod chrome;
mod events;
mod motion;
mod palette;
mod render;
mod scene;
pub use scene::SchedulePick;
mod ui;

#[cfg(test)]
mod integration;

use crate::protein_area::Source;
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use lince_interface::time_castle::{self as model, CursorMode, Entry, Mode, Settings};
use std::collections::HashMap;

#[derive(Component)]
pub(crate) struct AttachedCard;

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
    upcoming_stamp: Option<(u64, i64, i64, String)>,
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
    rendered: Option<(Settings, u64, i64, [u32; 2], u32, bool)>,
    scene: Option<Entity>,
    fallback: bool,
    projection: Option<nucleus::projection::Status>,
    palette: Option<palette::Palette>,
}

struct Feed {
    source: Source,
    query: protein::Protein,
    key: String,
    id: String,
    sent: bool,
    window: nucleus::projection::Window,
    sender: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
}

#[derive(Resource, Default)]
struct Feeds {
    active: HashMap<Entity, Feed>,
    closing: Vec<Feed>,
}

pub(crate) fn organs(world: &World) -> Vec<String> {
    world
        .get_resource::<Feeds>()
        .map(|feeds| {
            feeds
                .active
                .values()
                .chain(&feeds.closing)
                .filter_map(|feed| match &feed.source {
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
            .add_message::<crate::cell_bridge::CellMessage>()
            .add_observer(events::selected)
            .add_systems(
                Update,
                update
                    .after(crate::cell_bridge::ReceiveCell)
                    .after(crate::protein_area::UpdateProteinAreas),
            )
            .add_systems(
                PostUpdate,
                render::update
                    .after(bevy::ui::UiSystems::PostLayout)
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
        let timezone = crate::schedule_editor::local_timezone().unwrap_or_else(|| {
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
        .insert(crate::sand_store::SandCredits(CREDITS));
    let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
    let (viewport, status, present, details, fields) = ui::populate(world, owner, &settings);
    let upcoming = ui::upcoming_panel(world, owner);
    world.entity_mut(owner).insert(View {
        viewport,
        status,
        present,
        details,
        fields,
        entries: Vec::new(),
        upcoming,
        upcoming_rows: Vec::new(),
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

fn source_query(
    world: &mut World,
    owner: Entity,
) -> Result<(Source, Vec<protein::Predicate>), String> {
    let settings = &world
        .get::<TimeSettings>(owner)
        .ok_or("Time Castle closed")?
        .0;
    let Some(id) = settings.area.clone() else {
        return Ok((crate::practice_cells::source(world, owner).map_or(Source::Local, Source::Organ), Vec::new()));
    };
    let root = world
        .get::<ChildOf>(owner)
        .ok_or("Workspace missing")?
        .parent();
    let workspace = world
        .get::<crate::workspace::WorkspaceMember>(owner)
        .map(|member| member.0);
    let area = world
        .query::<(
            &crate::area::InfluenceArea,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .find(|(area, parent, member)| {
            area.id == id && parent.parent() == root && Some(member.0) == workspace
        })
        .and_then(|(area, _, _)| area.protein.clone())
        .ok_or("Selected Protein Area is unavailable")?;
    if crate::practice_cells::source(world, owner).is_some_and(|source| area.source != Source::Organ(source)) {
        return Err("Choose a Protein from this practice Cell.".into());
    }
    let query = area.query()?;
    Ok((area.source, query.filter))
}

pub(crate) fn receive(world: &mut World, source: &Source, message: &ServerMessage) {
    let Some(feeds) = world.get_resource::<Feeds>() else {
        return;
    };
    let owners: Vec<_> = feeds
        .active
        .iter()
        .filter(|(_, feed)| &feed.source == source)
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
                    .filter_map(|row| serde_json::from_value(row.clone()).ok())
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
                    view.entries = entries;
                    view.source = source.clone();
                    view.revision = view.revision.wrapping_add(1);
                    view.entries
                        .iter()
                        .find(|entry| view.selected.len() == 1 && view.selected.contains(&entry.id))
                        .filter(|entry| before.as_ref().is_some_and(|before| before != *entry))
                        .map(|entry| entry.id.clone())
                } else {
                    None
                };
                if let Some(id) = changed_selection {
                    crate::actions::Action::apply(&ui::Select(vec![id]), world, owner);
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
                if id == &subscription || id == crate::cell_bridge::CONNECTION =>
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
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    if crate::laboratory::active(world) {
        return;
    }
    chrome::dismiss(world);
    if let Some(messages) = world.get_resource::<Messages<crate::cell_bridge::CellMessage>>() {
        let messages: Vec<_> = cursor
            .read(messages)
            .map(|message| message.0.clone())
            .collect();
        for message in messages {
            receive(world, &Source::Local, &message);
        }
    }
    events::listeners(world);
    let now = chrono::Utc::now().timestamp_millis();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<View>>()
        .iter(world)
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
        world.resource_mut::<Feeds>().closing.push(feed);
    }
    for owner in owners {
        let settings = world.get::<TimeSettings>(owner).unwrap().0.clone();
        let (source, filter) = match source_query(world, owner) {
            Ok(value) => value,
            Err(error) => {
                status(world, owner, &error);
                if let Some(feed) = world.resource_mut::<Feeds>().active.remove(&owner) {
                    world.resource_mut::<Feeds>().closing.push(feed);
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
        let previous = world.resource::<Feeds>().active.get(&owner);
        let window = previous
            .filter(|feed| {
                feed.window.timezone == settings.timezone
                    && feed.window.until_ms >= now.saturating_add(settings.horizon_ms)
                    && feed.window.from_ms <= settings.history_from(now)
            })
            .map(|feed| feed.window.clone())
            .or_else(|| settings.window(now));
        let Some(window) = window else {
            status(world, owner, "Invalid clock settings");
            continue;
        };
        let query = protein::schedule::query(window.clone(), filter);
        let key = serde_json::to_string(&query).unwrap();
        let changed = previous.is_none_or(|feed| feed.source != source || feed.key != key);
        if changed {
            if let Some(feed) = world.resource_mut::<Feeds>().active.remove(&owner) {
                world.resource_mut::<Feeds>().closing.push(feed);
            }
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.entries.clear();
            view.projection = None;
            view.selected.clear();
            view.page = 0;
            view.source = source.clone();
            view.revision = view.revision.wrapping_add(1);
            world.resource_mut::<Feeds>().active.insert(
                owner,
                Feed {
                    source: source.clone(),
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
        }
        crate::protein_area::ensure_auxiliary(world, &source);
        let sender = crate::protein_area::auxiliary_sender(world, &source);
        let feed = world.resource::<Feeds>().active.get(&owner).unwrap();
        if !feed.sent {
            let message = ClientMessage::Subscribe {
                id: feed.id.clone(),
                protein: feed.query.clone(),
            };
            if let Some(sender) = sender
                && sender.try_send(message.clone()).is_ok()
            {
                crate::protein_area::auxiliary_subscribed(world, &source, &message);
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
                crate::protein_area::auxiliary_unsubscribe(world, organ, feed.id)
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

pub const CREDITS: &[crate::credits::Attribution] = &[
    crate::credits::Attribution {
        name: "tts",
        author: "Nolan Darilek and contributors",
        license: include_str!("../licenses/tts-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "speech-dispatcher Rust bindings",
        author: "Nolan Darilek and contributors",
        license: include_str!("../licenses/tts-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "ttf-parser",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/ttf-parser-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "cpal",
        author: "The CPAL contributors",
        license: include_str!("../licenses/cpal-Apache-2.0.txt"),
    },
    crate::credits::Attribution {
        name: "usvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/usvg-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "tiny-skia",
        author: "Yevhenii Reizner; Google; Skia contributors",
        license: include_str!("../licenses/tiny-skia-BSD.txt"),
    },
    crate::credits::Attribution {
        name: "fontdb",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/fontdb-MIT.txt"),
    },
    crate::credits::SYMBOLS,
    crate::credits::DEJAVU,
    crate::credits::Attribution {
        name: "Noto Sans Mono CJK",
        author: include_str!("../../../institute/assets/fonts/NotoSansMonoCJK/CREDITS.txt"),
        license: include_str!("../../../institute/assets/fonts/NotoSansMonoCJK/OFL.txt"),
    },
    crate::credits::FONTIQUE,
    crate::credits::Attribution {
        name: "Bevy",
        author: "Bevy contributors",
        license: crate::credits::BEVY_LICENSE,
    },
    crate::credits::Attribution {
        name: "Chrono",
        author: "Kang Seonghoon and contributors",
        license: crate::credits::CHRONO_LICENSE,
    },
    crate::credits::Attribution {
        name: "Chrono-TZ",
        author: "Chrono contributors; IANA timezone data",
        license: include_str!("../licenses/chrono-tz/LICENSE"),
    },
    crate::credits::Attribution {
        name: "iana-time-zone",
        author: "The iana-time-zone contributors",
        license: include_str!("../licenses/iana-time-zone-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "resvg",
        author: "Yevhenii Reizner and contributors",
        license: include_str!("../licenses/resvg-MIT.txt"),
    },
    crate::credits::Attribution {
        name: "Lato",
        author: "Łukasz Dziedzic",
        license: crate::credits::LATO_LICENSE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(sender: tokio::sync::mpsc::Sender<ClientMessage>) -> Feed {
        let window = Settings::default()
            .window(chrono::Utc::now().timestamp_millis())
            .unwrap();
        let query = protein::schedule::query(window.clone(), Vec::new());
        Feed {
            source: Source::Local,
            key: serde_json::to_string(&query).unwrap(),
            query,
            id: "closed-clock".into(),
            sent: true,
            window,
            sender: Some(sender),
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
