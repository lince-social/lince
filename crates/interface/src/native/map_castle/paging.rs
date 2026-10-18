use super::*;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

#[derive(Component)]
pub(super) struct Pages {
    key: String,
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

fn source(world: &World, owner: Entity) -> (Option<Value>, String) {
    let sand = world.get::<MapSand>(owner).unwrap();
    let Some(id) = &sand.area else {
        return (None, String::new());
    };
    let mut stack = ui::scope(world, owner)
        .map(|(root, _)| root)
        .into_iter()
        .collect::<Vec<_>>();
    let mut source = None;
    while let Some(entity) = stack.pop() {
        if let Some(area) = world.get::<crate::native::area::InfluenceArea>(entity)
            && area.id == *id
            && ui::scope(world, entity) == ui::scope(world, owner)
        {
            source = Some((entity, area));
            break;
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    source.map_or((None, String::new()), |(entity, area)| {
        let query = area
            .protein
            .as_ref()
            .and_then(|config| config.query().ok())
            .and_then(|query| serde_json::to_value(query).ok());
        let stamp = crate::native::protein_area::calendar_feed(world, entity)
            .map(|(_, stamp)| stamp)
            .unwrap_or_default();
        (query, stamp)
    })
}

fn bounds(sand: &MapSand) -> [f64; 4] {
    let size = sand.size.map(f64::from);
    let southwest = sand.camera.coordinate([0.0, size[1]], size);
    let northeast = sand.camera.coordinate([size[0], 0.0], size);
    let whole_world = size[0] >= 256.0 * 2.0_f64.powf(sand.camera.zoom);
    [
        if whole_world {
            -180.0
        } else {
            southwest.longitude
        },
        southwest.latitude,
        if whole_world {
            180.0
        } else {
            northeast.longitude
        },
        northeast.latitude,
    ]
}

pub(super) fn key(world: &World, owner: Entity) -> String {
    let sand = world.get::<MapSand>(owner).unwrap();
    let (query, stamp) = source(world, owner);
    nucleus::fact::sha256_hex(
        &serde_json::to_vec(&json!([
            sand.person,
            sand.authority_node_id,
            sand.invitation,
            sand.records,
            query,
            stamp,
            bounds(sand)
        ]))
        .unwrap(),
    )
}

pub(super) fn records(world: &World, owner: Entity) -> Vec<String> {
    world
        .get::<Pages>(owner)
        .map(|pages| pages.rows.iter().map(|(uid, _)| uid.clone()).collect())
        .unwrap_or_else(|| world.get::<MapSand>(owner).unwrap().records.clone())
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
        let current = key(world, owner);
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
        let (query, _) = source(world, owner);
        let action = engine::actions::Action::SavedLocation {
            request: nucleus::vocabulary::SavedLocationCommand::Page {
                query,
                records: sand.records.clone(),
                after: pages.after.clone(),
                generation: pages.generation.clone(),
                bounds: bounds(sand),
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
            message.0,
            cell::ServerMessage::SessionAuthenticated { .. }
                | cell::ServerMessage::LiveHello {
                    login_required: true
                }
        ) || matches!(&message.0, cell::ServerMessage::Error { id, .. } if id == "connection")
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
        let (id, data) = match message.0 {
            cell::ServerMessage::ActionOk { id, data, .. } => (id, data),
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
                    .is_some_and(|(pending, _)| *pending == id)
            })
            .map(|(entity, _)| entity);
        let Some(owner) = owner else {
            continue;
        };
        if !visible(world, owner) || world.get::<Pages>(owner).unwrap().key != key(world, owner) {
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
        let features = data["features"].as_array().cloned().unwrap_or_default();
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
        for feature in &features {
            if let Some(uid) = feature["record"]
                .as_str()
                .filter(|uid| nucleus::valid_uid(uid, "r"))
            {
                if !pages.rows.iter().any(|(record, _)| record == uid) {
                    pages.rows.push((
                        uid.into(),
                        feature["head"].as_str().unwrap_or("Record").into(),
                    ));
                }
            }
        }
        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
        for feature in features {
            let Some(uid) = feature["record"].as_str() else {
                continue;
            };
            sand.markers
                .retain(|marker| marker.live || marker.record != uid);
            if let Ok(location) = serde_json::from_value::<nucleus::vocabulary::LocationCandidate>(
                feature["location"].clone(),
            ) {
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

    fn respond(app: &mut App, owner: Entity, index: usize, features: Vec<Value>) {
        let id = format!("page-{index}");
        let current = key(app.world(), owner);
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
        app.world_mut().write_message(crate::native::cell_bridge::CellMessage(cell::ServerMessage::ActionOk {id,created:None,facts:0,warnings:vec![],data:Some(json!({"generation":"test-generation","features":features,"next_after":format!("cursor-{index}")}))}));
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
        let previous = key(app.world(), owner);
        app.world_mut().get_mut::<MapSand>(owner).unwrap().person = nucleus::new_uid("r");
        assert_ne!(key(app.world(), owner), previous);
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
    let parent = world.get::<MapSand>(owner).unwrap().record_list;
    let rows = world
        .get::<Pages>(owner)
        .map(|pages| pages.rows.clone())
        .unwrap_or_default();
    let page = world.get::<Pages>(owner).map_or(0, |pages| pages.list_page);
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
    if (page + 1) * 128 < rows.len() {
        crate::native::sand_panel::button(
            world,
            controls,
            owner,
            "Next Records",
            Command::RecordListPage(1),
        );
    }
    for (uid, head) in rows.into_iter().skip(page * 128).take(128) {
        let located = world
            .get::<MapSand>(owner)
            .unwrap()
            .markers
            .iter()
            .any(|marker| marker.record == uid);
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
