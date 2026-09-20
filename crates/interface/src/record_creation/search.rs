use super::*;
use bevy::input_focus::{FocusCause, InputFocus};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

#[derive(Resource, Default)]
pub(super) struct Searches {
    active: HashMap<Entity, (String, Entity)>,
    outgoing: VecDeque<cell::ClientMessage>,
}

#[derive(Component)]
struct Search {
    rows: Vec<Value>,
    concepts: Vec<Value>,
    signature: String,
    options: Vec<(String, String)>,
    selected: usize,
    total: usize,
    message: String,
    enter: bool,
    step: i32,
    dismissed: bool,
}

pub(super) fn start(world: &mut World, form: Entity, root: Entity) {
    let popup = world
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                max_height: px(280),
                padding: UiRect::all(px(4)),
                border: UiRect::all(px(1)),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::scroll_sand::ScrollSand,
            crate::token_style::background(crate::tokens::Token::Surface),
            GlobalZIndex(80),
            crate::inspection::InspectionExcluded,
            ChildOf(root),
        ))
        .id();
    let id = format!("record-search-{}", form.to_bits());
    world
        .resource_mut::<Searches>()
        .active
        .insert(form, (id.clone(), popup));
    world.resource_mut::<Searches>().outgoing.push_back(cell::ClientMessage::Subscribe {
        id: id.clone(),
        protein: serde_json::from_value(json!({
            "source":"record", "fields":["uid","head","body","slug","quantity_exact","start_date","due_date","estimate_min","assertions","assignees","work_logs"],
            "order":[{"asc":"head"}], "limit":null
        })).unwrap(),
    });
    world
        .resource_mut::<Searches>()
        .outgoing
        .push_back(cell::ClientMessage::Subscribe {
            id: format!("{id}-concepts"),
            protein: serde_json::from_value(
                json!({"source":"concept", "fields":["uid","name"], "limit":null}),
            )
            .unwrap(),
        });
    world.entity_mut(form).insert(Search {
        rows: Vec::new(),
        concepts: Vec::new(),
        signature: String::new(),
        options: Vec::new(),
        selected: 0,
        total: 0,
        message: "Loading Records…".into(),
        enter: false,
        step: 0,
        dismissed: false,
    });
    let first = world.get::<Form>(form).unwrap().fields[0].1;
    world
        .resource_mut::<InputFocus>()
        .set(first, FocusCause::Pressed);
}

fn editors(form: &Form) -> impl Iterator<Item = Entity> + '_ {
    form.fields
        .iter()
        .map(|(_, entity)| *entity)
        .chain(
            form.relations
                .iter()
                .flat_map(|(_, _, fields)| fields.iter().copied()),
        )
        .chain(form.logs.iter().flat_map(|(_, start, end)| [*start, *end]))
}

pub(super) fn keys(world: &mut World) {
    let Some(focused) = world.resource::<InputFocus>().get() else {
        return;
    };
    let keys = world.resource::<ButtonInput<KeyCode>>();
    if keys.any_pressed([
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]) {
        return;
    }
    let enter = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]);
    let step = i32::from(keys.just_pressed(KeyCode::ArrowDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp));
    let escape = keys.just_pressed(KeyCode::Escape);
    if !enter && step == 0 && !escape {
        return;
    }
    let owner = world
        .query::<(Entity, &Form)>()
        .iter(world)
        .find(|(_, form)| form.pending.is_none() && editors(form).any(|entity| entity == focused))
        .map(|(entity, _)| entity);
    let Some(owner) = owner else { return };
    let Some(mut text) = world.get_mut::<EditableText>(focused) else {
        return;
    };
    if text.is_composing()
        || text.pending_edits.iter().any(|edit| {
            matches!(
                edit,
                bevy::text::TextEdit::ImeCommit { .. } | bevy::text::TextEdit::ImeSetCompose { .. }
            )
        })
    {
        return;
    }
    if enter {
        text.pending_edits.retain(|edit| !matches!(edit, bevy::text::TextEdit::Insert(value) if value.contains('\n') || value.contains('\r')));
    }
    if step != 0 {
        text.pending_edits.retain(|edit| {
            !matches!(
                edit,
                bevy::text::TextEdit::Up(_) | bevy::text::TextEdit::Down(_)
            )
        });
    }
    let mut search = world.get_mut::<Search>(owner).unwrap();
    search.enter = enter;
    search.step = step;
    search.dismissed = escape;
}

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn contains(actual: &Value, query: &str) -> bool {
    query.is_empty() || text(actual).to_lowercase().contains(query)
}

#[derive(Debug)]
struct Filter {
    fields: Vec<(&'static str, String)>,
    relations: Vec<(bool, Vec<String>)>,
    logs: Vec<(String, String)>,
}

impl Filter {
    fn read(world: &World, form: &Form) -> Self {
        let read = |entity| value(world, entity).trim().to_lowercase();
        Self {
            fields: form
                .fields
                .iter()
                .map(|(key, entity)| (*key, read(*entity)))
                .collect(),
            relations: form
                .relations
                .iter()
                .map(|(_, assignee, fields)| {
                    (
                        *assignee,
                        fields.iter().map(|entity| read(*entity)).collect(),
                    )
                })
                .collect(),
            logs: form
                .logs
                .iter()
                .map(|(_, start, end)| (read(*start), read(*end)))
                .collect(),
        }
    }

    fn matches(&self, row: &Value, references: &HashMap<&str, String>) -> bool {
        let reference = |value: &Value, query: &str| {
            contains(value, query)
                || value
                    .as_str()
                    .and_then(|uid| references.get(uid))
                    .is_some_and(|name| name.contains(query.trim_start_matches('@')))
        };
        self.fields
            .iter()
            .all(|(key, query)| contains(&row[*key], query))
            && self.relations.iter().all(|(assignee, fields)| {
                fields.iter().all(String::is_empty)
                    || if *assignee {
                        row["assignees"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|person| {
                                reference(&person["uid"], &fields[0])
                                    || contains(&person["head"], &fields[0])
                            })
                    } else {
                        row["assertions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|assertion| {
                                (contains(
                                    &assertion["predicate"],
                                    fields[0].trim_start_matches('#'),
                                ) || reference(
                                    &assertion["predicate_uid"],
                                    fields[0].trim_start_matches('#'),
                                )) && reference(&assertion["object"], &fields[1])
                                    && contains(&assertion["quantity"], &fields[2])
                                    && reference(&assertion["unit"], &fields[3])
                            })
                    }
            })
            && self.logs.iter().all(|(start, end)| {
                start.is_empty() && end.is_empty()
                    || row["work_logs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|log| contains(&log["start"], start) && contains(&log["end"], end))
            })
    }
}

pub(super) fn open_record(world: &mut World, entity: Entity, uid: &str) {
    let Some(form) = world.get::<Form>(entity) else {
        return;
    };
    let area = form.area;
    if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(area) {
        let config = area.protein.as_mut().unwrap();
        config.draft.query["where"] = json!([{"all":[{"uid_eq":uid}]}]);
        config.enabled = true;
    }
    world.resource_mut::<InputFocus>().clear();
    world.despawn(entity);
}

#[derive(Clone)]
struct Open(String);
impl Action for Open {
    fn apply(&self, world: &mut World, owner: Entity) {
        if world
            .get::<Form>(owner)
            .is_some_and(|form| form.pending.is_none())
        {
            open_record(world, owner, &self.0);
        }
    }
}

fn draw(world: &mut World, form: Entity, popup: Entity) {
    if let Some(children) = world.get::<Children>(popup) {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    let search = world.get::<Search>(form).unwrap();
    let options = search.options.clone();
    let selected = search.selected;
    let total = search.total;
    let message = search.message.clone();
    if !message.is_empty() {
        crate::edit_mode::label(world, popup, &message, 13.0);
    } else if options.is_empty() {
        crate::edit_mode::label(
            world,
            popup,
            "No matches. Create a Record with these values.",
            13.0,
        );
    } else {
        for (index, (uid, label)) in options.into_iter().enumerate() {
            let button = world
                .spawn((
                    crate::sand::button(0),
                    ActionButton::new(form, crate::actions![Open(uid)]),
                    Node {
                        width: percent(100),
                        min_height: px(32),
                        flex_shrink: 0.0,
                        padding: UiRect::all(px(6)),
                        ..default()
                    },
                    ChildOf(popup),
                ))
                .id();
            if index == selected {
                world
                    .entity_mut(button)
                    .insert(crate::token_style::background(crate::tokens::Token::Accent));
            }
            crate::edit_mode::label(world, button, &label, 14.0);
        }
        if total > 100 {
            crate::edit_mode::label(
                world,
                popup,
                &format!("100 of {total} matches shown. Keep typing to narrow the list."),
                13.0,
            );
        }
    }
    world.get_mut::<ScrollPosition>(popup).unwrap().0.y = (selected.saturating_sub(5) * 32) as f32;
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let events: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    for event in events {
        let (id, result) = match event {
            cell::ServerMessage::Snapshot { id, rows }
            | cell::ServerMessage::Update { id, rows } => (id, Ok(rows)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let forms: Vec<_> = world
            .resource::<Searches>()
            .active
            .iter()
            .filter(|(_, (subscription, _))| {
                subscription == &id
                    || format!("{subscription}-concepts") == id
                    || id == crate::cell_bridge::CONNECTION
            })
            .map(|(form, _)| *form)
            .collect();
        for form in forms {
            if let Some(mut search) = world.get_mut::<Search>(form) {
                search.signature.clear();
                match &result {
                    Ok(rows) if id.ends_with("-concepts") => {
                        search.concepts = rows.clone();
                    }
                    Ok(rows) => {
                        search.rows = rows.clone();
                        search.message.clear();
                    }
                    Err(error) => {
                        search.rows.clear();
                        search.concepts.clear();
                        search.message = error.clone();
                    }
                }
            }
        }
    }
    let forms: Vec<_> = world
        .resource::<Searches>()
        .active
        .iter()
        .map(|(form, (id, popup))| (*form, id.clone(), *popup))
        .collect();
    for (entity, id, popup) in forms {
        let Some(form) = world.get::<Form>(entity) else {
            world.despawn(popup);
            world.resource_mut::<Searches>().active.remove(&entity);
            world
                .resource_mut::<Searches>()
                .outgoing
                .push_back(cell::ClientMessage::Unsubscribe {
                    id: format!("{id}-concepts"),
                });
            world
                .resource_mut::<Searches>()
                .outgoing
                .push_back(cell::ClientMessage::Unsubscribe { id });
            continue;
        };
        if world.get::<crate::area::InfluenceArea>(form.area).is_none() {
            world.despawn(entity);
            continue;
        }
        let focused = world.resource::<InputFocus>().get();
        let anchor = editors(form).find(|field| Some(*field) == focused);
        let filter = Filter::read(world, form);
        let signature = format!("{filter:?}:{anchor:?}");
        if signature != world.get::<Search>(entity).unwrap().signature {
            world.get_mut::<Search>(entity).unwrap().dismissed = false;
        }
        let form = world.get::<Form>(entity).unwrap();
        let root = world.get::<ChildOf>(popup).unwrap().parent();
        let active_workspace =
            world
                .get::<crate::workspace::Workspaces>(root)
                .is_none_or(|spaces| {
                    world
                        .get::<crate::workspace::WorkspaceMember>(form.area)
                        .is_some_and(|member| member.0 == spaces.active)
                });
        let visible = anchor.is_some()
            && active_workspace
            && form.pending.is_none()
            && !world.get::<Search>(entity).unwrap().dismissed;
        world.get_mut::<Node>(popup).unwrap().display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if !visible {
            continue;
        }
        let search = world.get::<Search>(entity).unwrap();
        let changed = signature != search.signature;
        if changed {
            let mut references: HashMap<_, _> = search
                .rows
                .iter()
                .filter_map(|row| {
                    Some((
                        row["uid"].as_str()?,
                        format!("{} {}", text(&row["head"]), text(&row["slug"])).to_lowercase(),
                    ))
                })
                .collect();
            references.extend(
                search.concepts.iter().filter_map(|row| {
                    Some((row["uid"].as_str()?, text(&row["name"]).to_lowercase()))
                }),
            );
            let matches: Vec<_> = search
                .rows
                .iter()
                .filter(|row| filter.matches(row, &references))
                .collect();
            let total = matches.len();
            let options: Vec<_> = matches
                .into_iter()
                .take(100)
                .filter_map(|row| {
                    let uid = row["uid"].as_str()?.to_owned();
                    let head = row["head"]
                        .as_str()
                        .filter(|head| !head.is_empty())
                        .unwrap_or("Untitled");
                    let label = row["slug"]
                        .as_str()
                        .map_or_else(|| head.to_string(), |slug| format!("{head}  @{slug}"));
                    Some((uid, label))
                })
                .collect();
            let mut search = world.get_mut::<Search>(entity).unwrap();
            search.options = options;
            search.total = total;
            search.selected = 0;
            search.signature = signature;
        }
        let mut search = world.get_mut::<Search>(entity).unwrap();
        let moved = search.step != 0;
        search.selected = (search.selected as i32 + search.step)
            .clamp(0, search.options.len().saturating_sub(1) as i32)
            as usize;
        search.step = 0;
        let selected = std::mem::take(&mut search.enter)
            .then(|| {
                search
                    .options
                    .get(search.selected)
                    .map(|(uid, _)| uid.clone())
            })
            .flatten();
        if let Some(uid) = selected {
            open_record(world, entity, &uid);
            continue;
        }
        if changed || moved {
            draw(world, entity, popup);
        }
        let anchor = anchor.unwrap();
        if let Some(bounds) = crate::topology::presentation::bounds(world, anchor)
            .or_else(|| crate::inspection::bounds(world, anchor))
        {
            let root = world.get::<ChildOf>(popup).unwrap().parent();
            let viewport = crate::inspection::bounds(world, root)
                .unwrap_or(Rect::from_corners(Vec2::ZERO, Vec2::new(1920.0, 1080.0)));
            let width = bounds.width().max(240.0).min(viewport.width());
            let height = world
                .get::<ComputedNode>(popup)
                .map_or(280.0, |node| node.size().y * node.inverse_scale_factor())
                .min(280.0);
            let top = if bounds.max.y + height <= viewport.max.y {
                bounds.max.y
            } else {
                bounds.min.y - height
            };
            let mut node = world.get_mut::<Node>(popup).unwrap();
            node.width = px(width);
            node.left =
                px((bounds.min.x - viewport.min.x).clamp(0.0, (viewport.width() - width).max(0.0)));
            node.top = px((top - viewport.min.y).max(0.0));
        }
    }
    while let Some(message) = world.resource_mut::<Searches>().outgoing.pop_front() {
        let Some(bridge) = world.get_non_send::<crate::cell_bridge::CellBridge>() else {
            world
                .resource_mut::<Searches>()
                .outgoing
                .push_front(message);
            break;
        };
        if let Err(error) = bridge.outgoing.try_send(message) {
            world
                .resource_mut::<Searches>()
                .outgoing
                .push_front(error.into_inner());
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{app, fill};
    use super::*;

    #[test]
    fn every_search_field_combines_with_relations_and_logs() {
        let row = json!({"head":"Test","slug":"alpha","quantity_exact":"3.125","assertions":[{"predicate":"needs", "object":"r_target", "quantity":"12.5", "unit":"r_unit"}], "assignees":[{"uid":"r_person", "head":"Ana"}], "work_logs":[{"start":"2026-09-20T12:00:00Z","end":"2026-09-20T13:00:00Z"}]});
        let references = HashMap::from([
            ("r_target", "some target".into()),
            ("r_unit", "hours".into()),
        ]);
        let mut filter = Filter {
            fields: vec![
                ("head", "t".into()),
                ("slug", "a".into()),
                ("quantity_exact", ".125".into()),
            ],
            relations: vec![
                (
                    false,
                    vec!["#need".into(), "@target".into(), "12".into(), "hour".into()],
                ),
                (true, vec!["ana".into()]),
            ],
            logs: vec![("09-20".into(), "13:00".into())],
        };
        assert!(filter.matches(&row, &references));
        filter.fields.push(("body", "absent".into()));
        assert!(!filter.matches(&row, &references));
        filter.fields.pop();
        filter.relations[0].1[1] = "hidden identity".into();
        assert!(!filter.matches(&row, &references));
    }

    #[tokio::test]
    async fn live_search_combines_fields_and_enter_retargets_without_creating() {
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        for (slug, head) in [("alpha", "Test"), ("beta", "Other"), ("sky", "Test too")] {
            engine
                .act(
                    engine::actions::Action::CreateRecord {
                        slug: Some(slug.into()),
                        kind: nucleus::RecordKind::Plain,
                        head: head.into(),
                        body: "Description".into(),
                        quantity: 0.0,
                    },
                    None,
                )
                .await
                .unwrap();
        }
        let runtime = cell::CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            information: None,
        };
        let (mut app, root) = app();
        app.insert_resource(crate::app::CellHandle(runtime))
            .insert_resource(crate::wake::WakeSignal::new(|| {}))
            .add_plugins(crate::cell_bridge::CellBridgePlugin);
        app.update();
        let form = super::super::open(app.world_mut(), root).unwrap();
        let area = app.world().get::<Form>(form).unwrap().area;
        fill(app.world_mut(), form, "body", "Description");
        fill(app.world_mut(), form, "slug", "a");
        for _ in 0..500 {
            app.update();
            if app.world().get::<Search>(form).unwrap().options.len() == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert_eq!(app.world().get::<Search>(form).unwrap().options.len(), 2);
        fill(app.world_mut(), form, "head", "T");
        app.update();
        assert_eq!(app.world().get::<Search>(form).unwrap().options.len(), 2);
        fill(app.world_mut(), form, "head", "Test");
        app.update();
        let search = app.world().get::<Search>(form).unwrap();
        assert_eq!(search.options.len(), 1);
        assert_eq!(search.selected, 0);
        let uid = search.options[0].0.clone();
        let popup = app.world().resource::<Searches>().active[&form].1;
        let input = app.world().get::<Form>(form).unwrap().fields[0].1;
        app.world_mut()
            .get_mut::<EditableText>(input)
            .unwrap()
            .queue_edit(bevy::text::TextEdit::Insert("\n".into()));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert!(app.world().get::<Form>(form).is_none());
        let config = app
            .world()
            .get::<crate::area::InfluenceArea>(area)
            .unwrap()
            .protein
            .as_ref()
            .unwrap();
        assert!(config.enabled);
        assert_eq!(config.draft.query["where"][0]["all"][0]["uid_eq"], uid);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        assert!(app.world().get_entity(popup).is_err());
        let query = serde_json::from_value(json!({"source":"record", "where":[{"kind_eq":"plain"}], "fields":["uid"],"limit":null})).unwrap();
        assert_eq!(
            protein::execute(&engine.store, &query).await.unwrap().len(),
            3
        );
    }

    #[test]
    fn arrows_select_results_escape_hides_and_shift_enter_keeps_editing() {
        let (mut app, root) = app();
        let form = super::super::open(app.world_mut(), root).unwrap();
        app.world_mut().get_mut::<Search>(form).unwrap().rows = vec![
            json!({"uid":"first","head":"A"}),
            json!({"uid":"second","head":"B"}),
        ];
        app.world_mut()
            .get_mut::<Search>(form)
            .unwrap()
            .message
            .clear();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        app.update();
        assert_eq!(app.world().get::<Search>(form).unwrap().selected, 1);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert!(app.world().get::<Form>(form).is_some());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        let popup = app.world().resource::<Searches>().active[&form].1;
        assert_eq!(
            app.world().get::<Node>(popup).unwrap().display,
            Display::None
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        fill(app.world_mut(), form, "head", "A");
        app.update();
        assert_eq!(
            app.world().get::<Node>(popup).unwrap().display,
            Display::Flex
        );
    }
}
