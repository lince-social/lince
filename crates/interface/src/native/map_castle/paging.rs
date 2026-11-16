use super::*;
mod identity;
pub(super) use identity::Key;
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Component)]
pub(super) struct Pages {
    key: Arc<Key>,
    generation: String,
    pending: Option<(String, Instant)>,
    after: Option<String>,
    window_start: Option<String>,
    seen: usize,
    done: bool,
    limited: bool,
    rows: Vec<(String, String)>,
    list_page: usize,
    next: Instant,
}

pub(super) fn key(world: &mut World, owner: Entity) -> Arc<Key> {
    identity::prepare(world, owner).key.clone()
}

pub(super) fn records(world: &World, owner: Entity) -> impl Iterator<Item = &String> + Clone {
    let pages = world.get::<Pages>(owner);
    pages
        .into_iter()
        .flat_map(|pages| pages.rows.iter().map(|(uid, _)| uid))
        .chain(
            pages
                .is_none()
                .then(|| world.get::<MapSand>(owner).unwrap())
                .into_iter()
                .flat_map(|sand| sand.records.iter()),
        )
}

pub(super) fn more(world: &mut World, owner: Entity) {
    if let Some(mut pages) = world.get_mut::<Pages>(owner)
        && pages.limited
    {
        pages.limited = false;
        pages.window_start = pages.after.clone();
        pages.next = Instant::now();
        pages.seen = 0;
        pages.rows.clear();
        pages.list_page = 0;
        world.get_mut::<MapSand>(owner).unwrap().markers.clear();
    }
}

pub(super) fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<MapSand>>()
        .iter(world)
        .collect();
    for owner in owners {
        if !visible(world, owner)
            || world
                .get::<MapSand>(owner)
                .unwrap()
                .authority_node_id
                .is_some()
        {
            world.entity_mut(owner).remove::<Pages>();
            continue;
        }
        if world
            .get::<MapSand>(owner)
            .is_some_and(|sand| sand.records.is_empty() && sand.area.is_none())
        {
            world.entity_mut(owner).remove::<Pages>();
            feed::reset(world, owner);
            let list = world.get::<MapSand>(owner).unwrap().record_list;
            crate::native::sand_panel::clear(world, list);
            continue;
        }
        let prepared = identity::prepare(world, owner);
        let current = prepared.key.clone();
        let changed = world
            .get::<Pages>(owner)
            .is_none_or(|pages| pages.key != current);
        if changed {
            world.entity_mut(owner).insert(Pages {
                key: current,
                generation: nucleus::new_uid("map-page"),
                pending: None,
                after: None,
                window_start: None,
                seen: 0,
                done: false,
                limited: false,
                rows: Vec::new(),
                list_page: 0,
                next: Instant::now() + Duration::from_millis(150),
            });
            feed::reset(world, owner);
            let list = world.get::<MapSand>(owner).unwrap().record_list;
            crate::native::sand_panel::clear(world, list);
        }
        let now = Instant::now();
        let pages = world.get::<Pages>(owner).unwrap();
        if pages.next > now {
            continue;
        }
        if let Some((_, sent)) = &pages.pending {
            if sent.elapsed() < Duration::from_secs(10) {
                continue;
            }
            world.entity_mut(owner).remove::<Pages>();
            world.get_mut::<MapSand>(owner).unwrap().markers.clear();
            continue;
        }
        if pages.done || pages.limited {
            let mut pages = world.get_mut::<Pages>(owner).unwrap();
            pages.after = pages.window_start.clone();
            pages.seen = 0;
            pages.rows.clear();
            pages.done = false;
            pages.limited = false;
            let mut sand = world.get_mut::<MapSand>(owner).unwrap();
            sand.markers.retain(|marker| marker.live);
            let list = sand.record_list;
            crate::native::sand_panel::clear(world, list);
        }
        let pages = world.get::<Pages>(owner).unwrap();
        let sand = world.get::<MapSand>(owner).unwrap();
        let query = prepared.query.as_deref().cloned();
        let action = engine::actions::Action::SavedLocation {
            request: nucleus::vocabulary::SavedLocationCommand::Page {
                query,
                records: sand.records.clone(),
                after: pages.after.clone(),
                generation: pages.generation.clone(),
                bounds: prepared.bounds,
            },
        };
        let id = nucleus::new_uid("map-page");
        match crate::native::sand_panel::send(
            world,
            owner,
            cell::ClientMessage::Act {
                id: id.clone(),
                action,
            },
        ) {
            Ok(()) => world.get_mut::<Pages>(owner).unwrap().pending = Some((id, now)),
            Err(error) => {
                let mut pages = world.get_mut::<Pages>(owner).unwrap();
                pages.done = true;
                pages.next = now + Duration::from_secs(5);
                world.get_mut::<MapSand>(owner).unwrap().notice = error;
            }
        }
    }
}

pub(super) fn receive(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<crate::native::cell_bridge::CellMessage>>())
        .cloned()
        .collect();
    for message in messages {
        if matches!(
            message.0.as_ref(),
            cell::ServerMessage::SessionAuthenticated { .. }
                | cell::ServerMessage::LiveHello {
                    login_required: true
                }
        ) || matches!(message.0.as_ref(), cell::ServerMessage::Error { id, .. } if id == "connection")
        {
            settings::cancel_pending(world);
            let owners: Vec<_> = world
                .query_filtered::<Entity, With<MapSand>>()
                .iter(world)
                .collect();
            for owner in owners {
                world.entity_mut(owner).remove::<Pages>();
                world.entity_mut(owner).remove::<locating::Locate>();
                let mut sand = world.get_mut::<MapSand>(owner).unwrap();
                sand.camera = Camera::default();
                sand.picked = None;
                sand.selection = None;
                sand.keyboard_anchor = None;
                sand.device_centered = false;
                let list = sand.record_list;
                crate::native::sand_panel::clear(world, list);
                feed::reset(world, owner);
            }
            continue;
        }
        let (id, data) = match message.0.as_ref() {
            cell::ServerMessage::ActionOk { id, data, .. } => (id, data.as_ref()),
            cell::ServerMessage::Error { id, .. } => (id, None),
            _ => continue,
        };
        let owner = world
            .query::<(Entity, &Pages)>()
            .iter(world)
            .find(|(_, pages)| {
                pages
                    .pending
                    .as_ref()
                    .is_some_and(|(pending, _)| pending == id)
            })
            .map(|(entity, _)| entity);
        let Some(owner) = owner else {
            continue;
        };
        let _apply = nucleus::performance::Span::new("map-page-application");
        if !visible(world, owner) || {
            let current = key(world, owner);
            world.get::<Pages>(owner).unwrap().key != current
        } {
            world.entity_mut(owner).remove::<Pages>();
            feed::reset(world, owner);
            continue;
        }
        let generation = world.get::<Pages>(owner).unwrap().generation.clone();
        let mut pages = world.get_mut::<Pages>(owner).unwrap();
        pages.pending = None;
        let Some(data) = data.filter(|data| data["generation"] == generation) else {
            pages.done = true;
            pages.next = Instant::now() + Duration::from_secs(5);
            world.get_mut::<MapSand>(owner).unwrap().markers.clear();
            continue;
        };
        let features = data["features"].as_array().map_or(&[][..], Vec::as_slice);
        let query_micros = data["query_micros"].as_u64();
        if features.len() > 128 {
            pages.done = true;
            continue;
        }
        pages.seen += features.len();
        pages.after = data["next_after"].as_str().map(str::to_owned);
        pages.done = pages.after.is_none();
        pages.limited = pages.seen >= 2048 && !pages.done;
        if pages.done || pages.limited {
            pages.next = Instant::now() + Duration::from_secs(5);
        }
        let limited = pages.limited;
        let additions: Vec<_> = {
            let mut known: HashSet<_> = pages.rows.iter().map(|(uid, _)| uid.as_str()).collect();
            features
                .iter()
                .filter_map(|feature| {
                    let uid = feature["record"]
                        .as_str()
                        .filter(|uid| nucleus::valid_uid(uid, "r"))?;
                    known.insert(uid).then(|| {
                        (
                            uid.to_owned(),
                            feature["head"].as_str().unwrap_or("Record").to_owned(),
                        )
                    })
                })
                .collect()
        };
        pages.rows.extend(additions);
        let latest: HashMap<_, _> = features
            .iter()
            .enumerate()
            .filter_map(|(index, feature)| feature["record"].as_str().map(|uid| (uid, index)))
            .collect();
        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
        sand.markers
            .retain(|marker| marker.live || !latest.contains_key(marker.record.as_str()));
        for (index, feature) in features.iter().enumerate() {
            let Some(uid) = feature["record"].as_str() else {
                continue;
            };
            if latest.get(uid) != Some(&index) {
                continue;
            }
            if let Ok(location) =
                nucleus::vocabulary::LocationCandidate::deserialize(&feature["location"])
            {
                let coordinate = Coordinate {
                    latitude: location.latitude,
                    longitude: location.longitude,
                };
                if coordinate.valid() {
                    sand.markers.push(feed::Marker {
                        record: uid.into(),
                        coordinate,
                        live: false,
                        view: None,
                        observed: engine::location::clock::Instant::now(),
                        radius_metres: location.radius_metres,
                    });
                }
            }
        }
        if limited {
            sand.notice =
                "Showing 2048 permitted Records. Use Next locations for the following page.".into();
        }
        render_list(world, owner);
        if let Some(micros) = query_micros {
            diagnostics::query(world, owner, micros);
        }
    }
}

pub(super) fn list_page(world: &mut World, owner: Entity, delta: isize) {
    if let Some(mut pages) = world.get_mut::<Pages>(owner) {
        pages.list_page = pages
            .list_page
            .saturating_add_signed(delta)
            .min(pages.rows.len().saturating_sub(1) / 128);
    }
    render_list(world, owner);
}

pub(super) fn retained_bytes(world: &World, owner: Entity) -> usize {
    // Tracked row buffers only: the retained request's Config/query/scope and
    // allocator overhead are not covered by this diagnostic.
    world.get::<Pages>(owner).map_or(0, |pages| {
        pages.rows.capacity() * std::mem::size_of::<(String, String)>()
            + pages
                .rows
                .iter()
                .map(|(uid, head)| uid.capacity() + head.capacity())
                .sum::<usize>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn respond(app: &mut App, owner: Entity, index: usize, features: Vec<Value>) {
        let id = format!("page-{index}");
        let current = key(app.world_mut(), owner);
        if app.world().get::<Pages>(owner).is_none() {
            app.world_mut().entity_mut(owner).insert(Pages {
                key: current,
                generation: "test-generation".into(),
                pending: None,
                after: None,
                window_start: None,
                seen: 0,
                done: false,
                limited: false,
                rows: vec![],
                list_page: 0,
                next: Instant::now() + Duration::from_secs(3600),
            });
        }
        app.world_mut().get_mut::<Pages>(owner).unwrap().pending =
            Some((id.clone(), Instant::now()));
        app.world_mut().write_message(crate::native::cell_bridge::CellMessage::new(cell::ServerMessage::ActionOk {id,created:None,facts:0,warnings:vec![],data:Some(json!({"generation":"test-generation","features":features,"next_after":format!("cursor-{index}")}))}));
        app.update();
    }

    #[test]
    fn two_thousand_and_forty_eight_records_require_explicit_continuation_and_old_viewer_pages_clear()
     {
        let (mut app, _, owner) = super::super::tests::fixture();
        let records: Vec<_> = (0..2049).map(|_| nucleus::new_uid("r")).collect();
        app.world_mut().get_mut::<MapSand>(owner).unwrap().records = records.clone();
        for page in 0..16 {
            let features=(0..128).map(|index| {
                let uid=records[page*128+index].clone();
                json!({"record":uid,"head":format!("São Paulo / Київ / 東京 {page}-{index}"),"location":null})
            }).collect();
            respond(&mut app, owner, page, features);
        }
        let pages = app.world().get::<Pages>(owner).unwrap();
        assert_eq!(pages.rows.len(), 2048);
        assert!(pages.limited);
        assert!(pages.after.is_some());
        let list = app.world().get::<MapSand>(owner).unwrap().record_list;
        let label_count = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .filter(|text| text.0.starts_with("São Paulo"))
            .count();
        assert_eq!(label_count, 128);
        assert!(app.world().get::<Children>(list).is_some());
        more(app.world_mut(), owner);
        assert!(app.world().get::<Pages>(owner).unwrap().rows.is_empty());
        assert_eq!(
            app.world()
                .get::<Pages>(owner)
                .unwrap()
                .window_start
                .as_deref(),
            Some("cursor-15")
        );
        respond(
            &mut app,
            owner,
            16,
            vec![json!({"record":records[2048],"head":"Next permitted Record","location":null})],
        );
        assert_eq!(app.world().get::<Pages>(owner).unwrap().rows.len(), 1);
        let previous = key(app.world_mut(), owner);
        app.world_mut().get_mut::<MapSand>(owner).unwrap().person = nucleus::new_uid("r");
        assert_ne!(key(app.world_mut(), owner), previous);
        respond(
            &mut app,
            owner,
            17,
            vec![json!({"record":records[0],"head":"Old private result","location":null})],
        );
        assert!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .markers
                .is_empty()
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .all(|text| !text.0.contains("Old private result"))
        );
    }
}

fn render_list(world: &mut World, owner: Entity) {
    let _prepare = nucleus::performance::Span::new("map-record-list-preparation");
    let parent = world.get::<MapSand>(owner).unwrap().record_list;
    let page = world.get::<Pages>(owner).map_or(0, |pages| pages.list_page);
    let count = world
        .get::<Pages>(owner)
        .map_or(0, |pages| pages.rows.len());
    let rows = {
        let located: HashSet<_> = world
            .get::<MapSand>(owner)
            .unwrap()
            .markers
            .iter()
            .map(|marker| marker.record.as_str())
            .collect();
        world
            .get::<Pages>(owner)
            .map(|pages| {
                pages
                    .rows
                    .iter()
                    .skip(page * 128)
                    .take(128)
                    .map(|(uid, head)| (uid.clone(), head.clone(), located.contains(uid.as_str())))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    crate::native::sand_panel::clear(world, parent);
    let controls = crate::native::sand_panel::row(world, parent);
    if page > 0 {
        crate::native::sand_panel::button(
            world,
            controls,
            owner,
            "Previous Records",
            Command::RecordListPage(-1),
        );
    }
    if (page + 1) * 128 < count {
        crate::native::sand_panel::button(
            world,
            controls,
            owner,
            "Next Records",
            Command::RecordListPage(1),
        );
    }
    for (uid, head, located) in rows {
        crate::native::sand_panel::button(
            world,
            parent,
            owner,
            &format!(
                "{head}{}",
                if located {
                    ""
                } else {
                    " · no permitted location in this viewport"
                }
            ),
            Command::OpenRecord(uid.clone()),
        );
        if located {
            crate::native::sand_panel::button(
                world,
                parent,
                owner,
                "Focus on map",
                Command::FocusRecord(uid),
            );
        }
    }
}
