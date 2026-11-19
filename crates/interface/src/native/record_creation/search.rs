use std::sync::Arc;
use super::*;
use bevy::input_focus::{FocusCause, InputFocus};
use serde_json::Value;
use std::{
    borrow::{Borrow, Cow},
    collections::{HashMap, VecDeque},
    hash::Hash,
};

mod preparation;

#[derive(Resource, Default)]
pub(super) struct Searches {
    active: HashMap<Entity, (String, Entity)>,
    outgoing: VecDeque<cell::ClientMessage>,
}

#[derive(Component)]
struct Search {
    rows: Arc<Vec<Value>>,
    concepts: Arc<Vec<Value>>,
    filter: Option<preparation::PreparedFilter>,
    presented: Option<(Arc<Filter>, Option<Entity>)>,
    references: Option<preparation::References>,
    options: Arc<Vec<(String, String)>>,
    selected: usize,
    total: usize,
    message: String,
    enter: bool,
    step: i32,
    dismissed: bool,
}

impl Search {
    fn changed(&self, anchor: Option<Entity>) -> bool {
        let current = &self
            .filter
            .as_ref()
            .expect("prepared creation filter")
            .value;
        self.presented
            .as_ref()
            .is_none_or(|(previous, old_anchor)| {
                *old_anchor != anchor || !Arc::ptr_eq(previous, current)
            })
    }
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
            crate::native::scroll_sand::ScrollSand,
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            GlobalZIndex(80),
            crate::native::inspection::InspectionExcluded,
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
            "source":"record", "fields":["uid","head","body","slug","quantity","start_date","due_date","estimate_min","assertions","assignees","work_logs"],
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
        rows: Arc::default(),
        concepts: Arc::default(),
        filter: None,
        presented: None,
        references: None,
        options: Arc::default(),
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

pub(super) fn keys(world: &mut World, forms: &mut QueryState<(Entity, &'static Form)>) {
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
    let owner = forms
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

fn text(value: &Value) -> Cow<'_, str> {
    match value {
        Value::String(value) => Cow::Borrowed(value),
        Value::Null => Cow::Borrowed(""),
        other => Cow::Owned(other.to_string()),
    }
}

fn contains(actual: &Value, query: &str) -> bool {
    query.is_empty() || text(actual).to_lowercase().contains(query)
}

#[derive(Debug, PartialEq, Eq)]
struct Filter {
    fields: Vec<(&'static str, String)>,
    relations: Vec<(bool, Vec<String>)>,
    logs: Vec<(String, String)>,
}

impl Filter {
    fn uses_references(&self) -> bool {
        self.relations.iter().any(|(assignee, fields)| {
            if *assignee {
                fields.first().is_some_and(|value| !value.is_empty())
            } else {
                fields
                    .first()
                    .is_some_and(|value| !value.trim_start_matches('#').is_empty())
                    || fields.get(1).is_some_and(|value| !value.is_empty())
                    || fields.get(3).is_some_and(|value| !value.is_empty())
            }
        })
    }

    fn matches<K: Borrow<str> + Eq + Hash>(
        &self,
        row: &Value,
        references: &HashMap<K, String>,
    ) -> bool {
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
    if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(area) {
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

#[derive(Component)]
struct PopupPresentation {
    choices: Vec<Entity>,
    children: Vec<Entity>,
    selected: usize,
    font: Handle<Font>,
}

impl PopupPresentation {
    fn current(&self, world: &World, popup: Entity) -> bool {
        if self.font != world.resource::<crate::native::theme::Typography>().0 {
            return false;
        }
        let mut children = world
            .get::<Children>(popup)
            .into_iter()
            .flat_map(|children| children.iter());
        self.children
            .iter()
            .all(|expected| children.by_ref().any(|child| child == *expected))
    }
}

fn scroll_to_selection(world: &mut World, popup: Entity, selected: usize) {
    let offset = (selected.saturating_sub(5) * 32) as f32;
    let mut position = world.get_mut::<ScrollPosition>(popup).unwrap();
    if position.0.y != offset {
        position.0.y = offset;
    }
}

fn select(world: &mut World, form: Entity, popup: Entity) {
    let _selection = nucleus::performance::Span::new("record-creation-search-selection");
    let Some(presented) = world
        .get::<PopupPresentation>(popup)
        .filter(|view| view.current(world, popup))
    else {
        draw(world, form, popup);
        return;
    };
    let selected = world.get::<Search>(form).unwrap().selected;
    if presented.selected != selected {
        let previous = presented.choices.get(presented.selected).copied();
        let next = presented.choices.get(selected).copied();
        if let Some(previous) = previous {
            world
                .entity_mut(previous)
                .remove::<crate::native::token_style::BackgroundToken>()
                .insert(BackgroundColor::default());
        }
        if let Some(next) = next {
            world
                .entity_mut(next)
                .insert(crate::native::token_style::background(
                    crate::native::tokens::Token::Accent,
                ));
        }
        world.get_mut::<PopupPresentation>(popup).unwrap().selected = selected;
    }
    scroll_to_selection(world, popup, selected);
}

fn draw(world: &mut World, form: Entity, popup: Entity) {
    let _draw = nucleus::performance::Span::new("record-creation-search-draw");
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
    let mut choices = Vec::new();
    if !message.is_empty() {
        crate::native::edit_mode::label(world, popup, &message, 13.0);
    } else if options.is_empty() {
        crate::native::edit_mode::label(
            world,
            popup,
            "No matches. Create a Record with these values.",
            13.0,
        );
    } else {
        for (index, (uid, label)) in options.iter().enumerate() {
            let button = world
                .spawn((
                    crate::native::sand::button(0),
                    ActionButton::new(form, crate::native::actions![Open(uid.clone())]),
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
            choices.push(button);
            if index == selected {
                world
                    .entity_mut(button)
                    .insert(crate::native::token_style::background(
                        crate::native::tokens::Token::Accent,
                    ));
            }
            crate::native::edit_mode::label(world, button, label, 14.0);
        }
        if total > 100 {
            crate::native::edit_mode::label(
                world,
                popup,
                &format!("100 of {total} matches shown. Keep typing to narrow the list."),
                13.0,
            );
        }
    }
    nucleus::performance::add("record-creation-search-rows-created", choices.len() as u64);
    let children = world
        .get::<Children>(popup)
        .into_iter()
        .flat_map(|children| children.iter())
        .collect();
    let font = world
        .resource::<crate::native::theme::Typography>()
        .0
        .clone();
    world.entity_mut(popup).insert(PopupPresentation {
        choices,
        children,
        selected,
        font,
    });
    scroll_to_selection(world, popup, selected);
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
    mut wave: Local<crate::native::cell_bridge::replies::Wave>,
    mut recipients: Local<Vec<Entity>>,
    mut forms: Local<Vec<(Entity, Entity)>>,
) {
    use crate::native::cell_bridge::{CONNECTION, CellMessage, replies};
    let _update = nucleus::performance::Span::new("record-creation-search-update");
    if let Some(messages) = world.get_resource::<Messages<CellMessage>>() {
        wave.capture(&mut cursor, messages, |message| {
            matches!(
                message,
                cell::ServerMessage::Snapshot { .. }
                    | cell::ServerMessage::Update { .. }
                    | cell::ServerMessage::Error { .. }
            )
        });
    }
    if wave.len() == 0 {
        recipients.shrink_to_fit();
    }
    for sequence in wave.drain() {
        let event = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
        let id = match event.as_ref() {
            cell::ServerMessage::Snapshot { id, .. }
            | cell::ServerMessage::Update { id, .. }
            | cell::ServerMessage::Error { id, .. } => id,
            _ => unreachable!(),
        };
        recipients.clear();
        recipients.extend(
            world
                .resource::<Searches>()
                .active
                .iter()
                .filter(|(_, (subscription, _))| {
                    subscription == id
                        || id.strip_suffix("-concepts") == Some(subscription.as_str())
                        || id == CONNECTION
                })
                .map(|(form, _)| *form),
        );
        if recipients.is_empty() {
            continue;
        }
        let event = event.clone();
        nucleus::performance::add("record-creation-search-payloads", 1);
        let (id, result) = match event.as_ref() {
            cell::ServerMessage::Snapshot { id, rows }
            | cell::ServerMessage::Update { id, rows } => (id, Ok(rows)),
            cell::ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => unreachable!(),
        };
        for form in recipients.drain(..) {
            if let Some(mut search) = world.get_mut::<Search>(form) {
                search.presented = None;
                search.references = None;
                match &result {
                    Ok(rows) if id.ends_with("-concepts") => {
                        search.concepts = (*rows).clone();
                    }
                    Ok(rows) => {
                        search.rows = (*rows).clone();
                        search.message.clear();
                    }
                    Err(error) => {
                        search.rows = Arc::default();
                        search.concepts = Arc::default();
                        search.message = (*error).clone();
                    }
                }
            }
        }
    }
    forms.clear();
    forms.extend(
        world
            .resource::<Searches>()
            .active
            .iter()
            .map(|(form, (_, popup))| (*form, *popup)),
    );
    if forms.is_empty() {
        forms.shrink_to_fit();
        recipients.shrink_to_fit();
    }
    for (entity, popup) in forms.drain(..) {
        let Some(form) = world.get::<Form>(entity) else {
            world.despawn(popup);
            let (id, _) = world
                .resource_mut::<Searches>()
                .active
                .remove(&entity)
                .unwrap();
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
        if world
            .get::<crate::native::area::InfluenceArea>(form.area)
            .is_none()
        {
            world.despawn(entity);
            continue;
        }
        let focused = world.resource::<InputFocus>().get();
        let anchor = editors(form).find(|field| Some(*field) == focused);
        let prepared = preparation::PreparedFilter::changed(
            world,
            form,
            world.get::<Search>(entity).unwrap().filter.as_ref(),
        );
        if let Some(mut prepared) = prepared {
            if let Some((presented, _)) = &world.get::<Search>(entity).unwrap().presented
                && !Arc::ptr_eq(presented, &prepared.value)
                && presented == &prepared.value
            {
                prepared.value = Arc::clone(presented);
            }
            world.get_mut::<Search>(entity).unwrap().filter = Some(prepared);
        }
        if world.get::<Search>(entity).unwrap().changed(anchor) {
            world.get_mut::<Search>(entity).unwrap().dismissed = false;
        }
        let form = world.get::<Form>(entity).unwrap();
        let root = world.get::<ChildOf>(popup).unwrap().parent();
        let active_workspace = world
            .get::<crate::native::workspace::Workspaces>(root)
            .is_none_or(|spaces| {
                world
                    .get::<crate::native::workspace::WorkspaceMember>(form.area)
                    .is_some_and(|member| member.0 == spaces.active)
            });
        let visible = anchor.is_some()
            && active_workspace
            && form.pending.is_none()
            && !world.get::<Search>(entity).unwrap().dismissed;
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        world
            .get_mut::<Node>(popup)
            .unwrap()
            .map_unchanged(|node| &mut node.display)
            .set_if_neq(display);
        if !visible {
            continue;
        }
        let search = world.get::<Search>(entity).unwrap();
        let changed = search.changed(anchor);
        if changed {
            let uses_references = search.filter.as_ref().unwrap().value.uses_references();
            if uses_references
                && search
                    .references
                    .as_ref()
                    .is_none_or(|references| !references.current(&search.rows, &search.concepts))
            {
                let references = preparation::References::capture(&search.rows, &search.concepts);
                world.get_mut::<Search>(entity).unwrap().references = Some(references);
            } else if !uses_references && search.references.is_some() {
                world.get_mut::<Search>(entity).unwrap().references = None;
            }
            let search = world.get::<Search>(entity).unwrap();
            let filter = Arc::clone(&search.filter.as_ref().unwrap().value);
            let (total, options) =
                preparation::options(&filter, &search.rows, search.references.as_ref());
            let mut search = world.get_mut::<Search>(entity).unwrap();
            search.options = Arc::new(options);
            search.total = total;
            search.selected = 0;
            search.presented = Some((filter, anchor));
        }
        let mut search = world.get_mut::<Search>(entity).unwrap();
        let moved = search.step != 0;
        if moved {
            search.selected = (search.selected as i32 + search.step)
                .clamp(0, search.options.len().saturating_sub(1) as i32)
                as usize;
            search.step = 0;
        }
        let selected = if search.enter {
            search.enter = false;
            search
                .options
                .get(search.selected)
                .map(|(uid, _)| uid.clone())
        } else {
            None
        };
        if let Some(uid) = selected {
            open_record(world, entity, &uid);
            continue;
        }
        if changed {
            draw(world, entity, popup);
        } else if moved {
            select(world, entity, popup);
        }
        let anchor = anchor.unwrap();
        if let Some(bounds) = crate::native::topology::presentation::bounds(world, anchor)
            .or_else(|| crate::native::inspection::bounds(world, anchor))
        {
            let root = world.get::<ChildOf>(popup).unwrap().parent();
            let viewport = crate::native::inspection::bounds(world, root)
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
            let left =
                px((bounds.min.x - viewport.min.x).clamp(0.0, (viewport.width() - width).max(0.0)));
            let top = px((top - viewport.min.y).max(0.0));
            if node.width != px(width) {
                node.width = px(width);
            }
            if node.left != left {
                node.left = left;
            }
            if node.top != top {
                node.top = top;
            }
        }
    }
    while let Some(message) = world.resource_mut::<Searches>().outgoing.pop_front() {
        let Some(bridge) = world.get_non_send::<crate::native::cell_bridge::CellBridge>() else {
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
        let row = json!({"head":"Test","slug":"alpha","quantity":"3.125","assertions":[{"predicate":"needs", "object":"r_target", "quantity":"12.5", "unit":"r_unit"}], "assignees":[{"uid":"r_person", "head":"Ana"}], "work_logs":[{"start":"2026-09-20T12:00:00Z","end":"2026-09-20T13:00:00Z"}]});
        let references = HashMap::from([
            ("r_target", "some target".into()),
            ("r_unit", "hours".into()),
        ]);
        let mut filter = Filter {
            fields: vec![
                ("head", "t".into()),
                ("slug", "a".into()),
                ("quantity", ".125".into()),
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
                        kind: if slug == "alpha" {
                            nucleus::RecordKind::Command
                        } else {
                            nucleus::RecordKind::Plain
                        },
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
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: None,
            speech: None,
            information: None,
        };
        let (mut app, root) = app();
        app.insert_resource(crate::native::app::CellHandle(runtime))
            .insert_resource(crate::native::wake::WakeSignal::new(|| {}))
            .add_plugins(crate::native::cell_bridge::CellBridgePlugin);
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
            .get::<crate::native::area::InfluenceArea>(area)
            .unwrap()
            .protein
            .as_ref()
            .unwrap();
        assert!(config.enabled);
        assert_eq!(config.draft.query["where"][0]["all"][0]["uid_eq"], uid);
        let command = serde_json::from_value(
            json!({"source":"record", "where":[{"uid_eq":uid}], "fields":["kind"]}),
        )
        .unwrap();
        assert_eq!(
            protein::execute(&engine.store, &command).await.unwrap()[0]["kind"],
            "command"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.update();
        assert!(app.world().get_entity(popup).is_err());
        let query = serde_json::from_value(json!({"source":"record", "where":[{"kind_eq":"plain"}], "fields":["uid"],"limit":null})).unwrap();
        assert_eq!(
            protein::execute(&engine.store, &query).await.unwrap().len(),
            2
        );
    }

    #[test]
    fn arrows_select_results_escape_hides_and_shift_enter_keeps_editing() {
        let (mut app, root) = app();
        let form = super::super::open(app.world_mut(), root).unwrap();
        app.world_mut().get_mut::<Search>(form).unwrap().rows = vec![
            json!({"uid":"first","head":"A"}),
            json!({"uid":"second","head":"B"}),
        ]
        .into();
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
