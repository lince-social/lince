use super::*;
use crate::{
    actions::{Action, ActionButton},
    sand::Square,
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    text::EditableText,
};
use serde_json::json;

#[derive(Component, Clone)]
pub(super) struct Field {
    binding: RecordBinding,
    button: Entity,
    label: Entity,
}

#[derive(Component)]
struct Picker {
    field: Entity,
    input: Entity,
    list: Entity,
    subscription: String,
    rows: Vec<Value>,
    drawn: String,
    message: String,
    predicate_ready: bool,
}

#[derive(Component)]
struct Pending {
    field: Entity,
    uid: String,
    selected: bool,
    confirmed: bool,
}

#[derive(Clone)]
pub(super) struct Open;
#[derive(Clone)]
pub(super) struct Select(pub String);

pub(super) fn field(world: &mut World, parent: Entity, binding: RecordBinding, data: &Value) {
    let button = world
        .spawn((
            Square,
            crate::sand::button(0),
            Node {
                width: percent(100),
                min_height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
            ActionButton::new(parent, crate::actions![Open]),
            ChildOf(parent),
        ))
        .id();
    let label = crate::edit_mode::label(world, button, &caption(data), 14.0);
    world.entity_mut(parent).insert(Field {
        binding,
        button,
        label,
    });
}

fn caption(data: &Value) -> String {
    let text = rows::display(&data["assignees"]);
    if text.is_empty() {
        "Assignees ▾".into()
    } else {
        format!("{} ▾", text.replace('\n', ", "))
    }
}

pub(super) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(field) = world.get::<Field>(entity) else {
        return false;
    };
    let label = field.label;
    world.get_mut::<Text>(label).unwrap().0 = caption(data);
    true
}

fn data(world: &World, field: &Field) -> Value {
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&field.binding.area))
        .and_then(|state| {
            state
                .data
                .iter()
                .find(|row| row["uid"].as_str() == Some(&field.binding.uid))
        })
        .cloned()
        .unwrap_or(Value::Null)
}

fn selection(world: &World, field: Entity) -> std::collections::HashSet<String> {
    let Some(binding) = world.get::<Field>(field) else {
        return default();
    };
    let mut selected: std::collections::HashSet<_> = data(world, binding)["assignees"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value["uid"].as_str().map(str::to_owned))
        .collect();
    if let Some(children) = world.get::<Children>(field) {
        for child in children {
            if let Some(pending) = world.get::<Pending>(*child) {
                if pending.selected {
                    selected.insert(pending.uid.clone());
                } else {
                    selected.remove(&pending.uid);
                }
            }
        }
    }
    selected
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world.get::<crate::workspace::Workspaces>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

fn close(world: &mut World, entity: Entity) {
    let mut focused = world
        .get_resource::<InputFocus>()
        .and_then(|focus| focus.get());
    while let Some(current) = focused {
        if current == entity {
            let button = world
                .get::<Picker>(entity)
                .and_then(|picker| world.get::<Field>(picker.field))
                .map(|field| field.button);
            if let Some(button) = button {
                world
                    .resource_mut::<InputFocus>()
                    .set(button, FocusCause::Pressed);
            }
            break;
        }
        focused = world.get::<ChildOf>(current).map(ChildOf::parent);
    }
    if let Some(picker) = world.get::<Picker>(entity) {
        let id = picker.subscription.clone();
        world
            .resource_mut::<Runtime>()
            .outgoing
            .push_back(ClientMessage::Unsubscribe {
                id: format!("{id}-predicate"),
            });
        world
            .resource_mut::<Runtime>()
            .outgoing
            .push_back(ClientMessage::Unsubscribe { id });
    }
    world.despawn(entity);
}

impl Action for Open {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(field) = world.get::<Field>(entity).cloned() else {
            return;
        };
        let Some(root) = root(world, entity) else {
            return;
        };
        let opened: Vec<_> = world
            .query_filtered::<Entity, With<Picker>>()
            .iter(world)
            .collect();
        let same = opened.iter().any(|popup| {
            world
                .get::<Picker>(*popup)
                .is_some_and(|picker| picker.field == entity)
        });
        for picker in opened {
            close(world, picker);
        }
        if same {
            return;
        }
        let popup = world
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(320),
                    max_height: px(340),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(6),
                    overflow: Overflow::clip(),
                    ..default()
                },
                crate::token_style::background(crate::tokens::Token::Surface),
                GlobalZIndex(90),
                super::history::Scope(entity),
                crate::inspection::InspectionExcluded,
                ChildOf(root),
            ))
            .id();
        let input = world
            .spawn(crate::sand::text_editor(
                "",
                world.resource::<crate::theme::Typography>(),
                0,
            ))
            .id();
        world.entity_mut(input).insert((
            Node {
                width: percent(100),
                min_height: px(32),
                flex_shrink: 0.0,
                ..default()
            },
            crate::icons::Tooltip("Filter people; Enter selects, Escape closes".into()),
            ChildOf(popup),
        ));
        world.get_mut::<EditableText>(input).unwrap().allow_newlines = false;
        super::history::attach_text(world, input);
        let list = world
            .spawn((
                Node {
                    width: percent(100),
                    max_height: px(280),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollPosition::default(),
                ChildOf(popup),
            ))
            .id();
        let subscription = format!("record-assignees-{}", popup.to_bits());
        world.entity_mut(popup).insert(Picker {
            field: entity,
            input,
            list,
            subscription: subscription.clone(),
            rows: Vec::new(),
            drawn: String::new(),
            message: "Loading people…".into(),
            predicate_ready: false,
        });
        world
            .resource_mut::<Runtime>()
            .outgoing
            .push_back(ClientMessage::Subscribe {
                id: format!("{subscription}-predicate"),
                protein: serde_json::from_value(
                    json!({"source":"concept", "fields":["uid","name"], "limit":null}),
                )
                .unwrap(),
            });
        world.resource_mut::<Runtime>().outgoing.push_back(ClientMessage::Subscribe {
            id: subscription,
            protein: serde_json::from_value(json!({"source":"record", "where":[{"kind_eq":"person"}], "fields":["uid","head","slug"], "order":[{"asc":"head"}], "limit":null})).unwrap(),
        });
        if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
            focus.set(input, FocusCause::Pressed);
        }
        position(world, popup, field.button);
    }
}

impl Action for Select {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(field) = world.get::<Field>(entity).cloned() else {
            return;
        };
        if world
            .query::<&Picker>()
            .iter(world)
            .any(|picker| picker.field == entity && !picker.predicate_ready)
        {
            return;
        }
        if world.get::<Children>(entity).is_some_and(|children| {
            children.iter().any(|child| {
                world
                    .get::<Pending>(child)
                    .is_some_and(|pending| pending.uid == self.0)
            })
        }) {
            return;
        }
        let row = data(world, &field);
        let assertion = row["assignees"]
            .as_array()
            .and_then(|values| values.iter().find(|value| value["uid"] == self.0))
            .and_then(|value| value["assertion"].as_str());
        let mutation = if let Some(assertion) = assertion {
            engine::record_change::Mutation::RetractAssertion {
                assertion: assertion.into(),
            }
        } else {
            engine::record_change::Mutation::Assertion {
                predicate: "assigned-to".into(),
                object: Some(self.0.clone()),
                quantity: None,
                unit: None,
            }
        };
        let action = engine::actions::Action::ChangeRecord {
            request: engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: field.binding.uid.clone(),
                mutation,
            },
        };
        let target = world
            .spawn((
                Pending {
                    field: entity,
                    uid: self.0.clone(),
                    selected: assertion.is_none(),
                    confirmed: false,
                },
                ChildOf(entity),
            ))
            .id();
        if let Err(error) = execute(world, &field.binding, target, action) {
            world.despawn(target);
            status(world, field.binding.area, &error);
            return;
        }
        let input = world
            .query::<&Picker>()
            .iter(world)
            .find(|picker| picker.field == entity)
            .map(|picker| picker.input);
        if let Some(input) = input {
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text("");
            super::history::attach_text(world, input);
            if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
                focus.set(input, FocusCause::Pressed);
            }
        }
    }
}

pub(super) fn finished(world: &mut World, entity: Entity, error: Option<String>) -> bool {
    if world.get::<Pending>(entity).is_none() {
        return false;
    }
    if let Some(error) = error {
        let field = world.get::<Pending>(entity).unwrap().field;
        for mut picker in world.query::<&mut Picker>().iter_mut(world) {
            if picker.field == field {
                picker.message = error.clone();
            }
        }
        world.despawn(entity);
    } else {
        world.get_mut::<Pending>(entity).unwrap().confirmed = true;
    }
    true
}

fn position(world: &mut World, popup: Entity, anchor: Entity) {
    let Some(bounds) = crate::topology::presentation::bounds(world, anchor) else {
        return;
    };
    let viewport = world
        .query::<&Window>()
        .iter(world)
        .next()
        .map_or(Vec2::new(800.0, 600.0), |window| {
            Vec2::new(window.width(), window.height())
        });
    let height = world.get::<ComputedNode>(popup).map_or(300.0, |node| {
        (node.size().y * node.inverse_scale_factor()).max(100.0)
    });
    let mut node = world.get_mut::<Node>(popup).unwrap();
    node.width = px(320.0_f32.min(viewport.x - 16.0));
    node.left = px(bounds.min.x.clamp(8.0, (viewport.x - 328.0).max(8.0)));
    node.top = px((bounds.max.y + 4.0).min(viewport.y - height - 8.0).max(8.0));
}

pub(super) fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        let mut outgoing = Vec::new();
        for mut picker in world.query::<&mut Picker>().iter_mut(world) {
            match &message {
                ServerMessage::Snapshot { id, rows }
                    if *id == format!("{}-predicate", picker.subscription) =>
                {
                    outgoing.push(ClientMessage::Unsubscribe { id: id.clone() });
                    if rows.iter().any(|row| row["name"] == "assigned-to") {
                        picker.predicate_ready = true;
                    } else {
                        outgoing.push(ClientMessage::Act {
                            id: format!("{}-ensure", picker.subscription),
                            action: engine::actions::Action::CreateConcept {
                                lingua: "g_local".into(),
                                name: "assigned-to".into(),
                                parents: Vec::new(),
                            },
                        });
                    }
                }
                ServerMessage::ActionOk { id, .. }
                    if *id == format!("{}-ensure", picker.subscription) =>
                {
                    picker.predicate_ready = true;
                }
                ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
                    if id == &picker.subscription =>
                {
                    picker.rows = rows.clone();
                    picker.message.clear();
                }
                ServerMessage::Error { id, message, .. }
                    if id.starts_with(&picker.subscription)
                        || id == crate::cell_bridge::CONNECTION =>
                {
                    picker.message = message.clone();
                }
                _ => {}
            }
        }
        world.resource_mut::<Runtime>().outgoing.extend(outgoing);
    }
    let done: Vec<_> = world
        .query::<(Entity, &Pending)>()
        .iter(world)
        .filter(|(_, pending)| {
            pending.confirmed
                && world.get::<Field>(pending.field).is_none_or(|field| {
                    data(world, field)["assignees"]
                        .as_array()
                        .is_some_and(|values| {
                            values.iter().any(|value| value["uid"] == pending.uid)
                        })
                        == pending.selected
                })
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in done {
        world.despawn(entity);
    }
    let popups: Vec<_> = world
        .query::<(Entity, &Picker)>()
        .iter(world)
        .map(|(entity, picker)| (entity, picker.field, picker.input, picker.list))
        .collect();
    for (popup, field, input, list) in popups {
        let Some(binding) = world.get::<Field>(field).cloned() else {
            close(world, popup);
            continue;
        };
        let outside = world
            .get_resource::<ButtonInput<MouseButton>>()
            .is_some_and(|buttons| buttons.just_pressed(MouseButton::Left))
            && world
                .get_resource::<bevy::picking::hover::HoverMap>()
                .is_some_and(|hover| {
                    !hover.values().flat_map(|hits| hits.keys()).any(|entity| {
                        let mut cursor = Some(*entity);
                        while let Some(entity) = cursor {
                            if entity == popup || entity == binding.button {
                                return true;
                            }
                            cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                        }
                        false
                    })
                });
        let hidden = root(world, field)
            .and_then(|root| world.get::<crate::workspace::Workspaces>(root))
            .is_none_or(|spaces| {
                world
                    .get::<crate::workspace::WorkspaceMember>(binding.binding.area)
                    .is_none_or(|member| member.0 != spaces.active)
            });
        if hidden
            || outside
            || world
                .get_resource::<ButtonInput<KeyCode>>()
                .is_some_and(|keys| keys.just_pressed(KeyCode::Escape))
        {
            close(world, popup);
            continue;
        }
        position(world, popup, binding.button);
        let text = world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string()
            .to_lowercase();
        let picker = world.get::<Picker>(popup).unwrap();
        let options: Vec<_> = picker
            .rows
            .iter()
            .filter(|row| {
                format!(
                    "{} {}",
                    row["head"].as_str().unwrap_or_default(),
                    row["slug"].as_str().unwrap_or_default()
                )
                .to_lowercase()
                .contains(&text)
            })
            .take(100)
            .filter_map(|row| {
                Some((
                    row["uid"].as_str()?.to_owned(),
                    row["head"].as_str().unwrap_or("Unnamed person").to_owned(),
                ))
            })
            .collect();
        let selected = selection(world, field);
        let signature = format!(
            "{:?}:{}:{:?}",
            options,
            picker.message,
            options
                .iter()
                .map(|(uid, _)| selected.contains(uid))
                .collect::<Vec<_>>()
        );
        if world
            .get_resource::<InputFocus>()
            .and_then(|focus| focus.get())
            == Some(input)
            && world
                .get_resource::<ButtonInput<KeyCode>>()
                .is_some_and(|keys| {
                    keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter)
                })
            && !world.get::<EditableText>(input).unwrap().is_composing()
            && let Some((uid, _)) = options.iter().find(|(uid, _)| !selected.contains(uid))
        {
            Select(uid.clone()).apply(world, field);
        }
        if signature == world.get::<Picker>(popup).unwrap().drawn {
            continue;
        }
        let children: Vec<_> = world
            .get::<Children>(list)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        for child in children {
            world.despawn(child);
        }
        let message = world.get::<Picker>(popup).unwrap().message.clone();
        if !message.is_empty() {
            crate::edit_mode::label(world, list, &message, 14.0);
        }
        if options.is_empty() && message.is_empty() {
            crate::edit_mode::label(world, list, "No matching people", 14.0);
        }
        for (uid, title) in options {
            let checked = selected.contains(&uid);
            let button = world
                .spawn((
                    crate::sand::button(0),
                    Node {
                        width: percent(100),
                        min_height: px(32),
                        flex_shrink: 0.0,
                        ..default()
                    },
                    ActionButton::new(field, crate::actions![Select(uid)]),
                    ChildOf(list),
                ))
                .id();
            crate::edit_mode::label(
                world,
                button,
                &format!("{} {title}", if checked { "[x]" } else { "[ ]" }),
                14.0,
            );
        }
        world.get_mut::<Picker>(popup).unwrap().drawn = signature;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn floating_picker_filters_local_people_selects_many_and_undoes_one_change() {
        let (mut app, root, owner) = super::super::tests::fixture();
        app.init_resource::<ButtonInput<KeyCode>>();
        let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
        let mut ids = Vec::new();
        for (head, kind) in [
            ("Record", nucleus::RecordKind::Plain),
            ("Alice", nucleus::RecordKind::Person),
            ("Bob", nucleus::RecordKind::Person),
            ("Alice document", nucleus::RecordKind::Plain),
        ] {
            ids.push(
                engine
                    .act(
                        engine::actions::Action::CreateRecord {
                            slug: None,
                            kind,
                            head: head.into(),
                            body: String::new(),
                            quantity: 0.0,
                        },
                        None,
                    )
                    .await
                    .unwrap()
                    .created
                    .unwrap(),
            );
        }
        app.insert_resource(crate::app::CellHandle(cell::CellRuntime {
            commands: Default::default(),
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: std::sync::Arc::new(cell::LaneHub::new()),
            wire: default(),
            fiote: None,
            information: None,
        }))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
        let mut config = Config::records();
        config.draft.query["where"] = json!([{"uid_eq":ids[0]}]);
        app.world_mut()
            .get_mut::<InfluenceArea>(owner)
            .unwrap()
            .protein = Some(config);
        super::super::tests::until(&mut app, |world| {
            world
                .resource::<Runtime>()
                .areas
                .get(&owner)
                .is_some_and(|state| !state.row_entities.is_empty())
        })
        .await;
        let field = app
            .world_mut()
            .query_filtered::<Entity, With<Field>>()
            .single(app.world())
            .unwrap();
        Open.apply(app.world_mut(), field);
        let popup = app
            .world_mut()
            .query_filtered::<Entity, With<Picker>>()
            .single(app.world())
            .unwrap();
        assert_eq!(app.world().get::<ChildOf>(popup).unwrap().parent(), root);
        assert_eq!(
            app.world().get::<Node>(popup).unwrap().position_type,
            PositionType::Absolute
        );
        super::super::tests::until(&mut app, |world| {
            world
                .get::<Picker>(popup)
                .is_some_and(|picker| picker.rows.len() == 2 && picker.predicate_ready)
        })
        .await;
        let input = app.world().get::<Picker>(popup).unwrap().input;
        for (filter, uid) in [("ALI", &ids[1]), ("bob", &ids[2])] {
            app.world_mut()
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(filter);
            app.update();
            let list = app.world().get::<Picker>(popup).unwrap().list;
            assert_eq!(app.world().get::<Children>(list).unwrap().len(), 1);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::Enter);
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            super::super::tests::until(&mut app, |world| {
                world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|value| value["uid"] == *uid))
            })
            .await;
            assert!(
                app.world()
                    .get::<EditableText>(input)
                    .unwrap()
                    .value()
                    .to_string()
                    .is_empty()
            );
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ControlLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyZ);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        super::super::tests::until(&mut app, |world| {
            world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
                .as_array()
                .is_some_and(|values| values.len() == 1 && values[0]["uid"] == ids[1])
        })
        .await;
        super::super::history::undo(app.world_mut(), field, true);
        super::super::tests::until(&mut app, |world| {
            world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
                .as_array()
                .is_some_and(|values| values.len() == 2)
        })
        .await;
        Select(ids[1].clone()).apply(app.world_mut(), field);
        super::super::tests::until(&mut app, |world| {
            world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
                .as_array()
                .is_some_and(|values| values.len() == 1 && values[0]["uid"] == ids[2])
        })
        .await;
        super::super::history::undo(app.world_mut(), field, false);
        super::super::tests::until(&mut app, |world| {
            world.resource::<Runtime>().areas[&owner].data[0]["assignees"]
                .as_array()
                .is_some_and(|values| values.len() == 2)
        })
        .await;
        Open.apply(app.world_mut(), field);
        assert!(app.world().get_entity(popup).is_err());
        assert_eq!(
            app.world()
                .resource::<Runtime>()
                .outgoing
                .iter()
                .filter(|message| matches!(message, ClientMessage::Unsubscribe { .. }))
                .count(),
            2
        );
        let quantity = app
            .world_mut()
            .query::<(Entity, &EditableText, &ChildOf)>()
            .iter(app.world())
            .find(|(_, _, parent)| {
                app.world()
                    .get::<rows::PropertyContainer>(parent.parent())
                    .is_some_and(|property| property.0 == "quantity")
            })
            .map(|(entity, _, _)| entity)
            .unwrap();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(quantity, FocusCause::Pressed);
        for value in ["12", "25"] {
            app.world_mut()
                .get_mut::<EditableText>(quantity)
                .unwrap()
                .editor
                .set_text(value);
            super::super::tests::until(&mut app, |world| {
                world.resource::<Runtime>().areas[&owner].data[0]["quantity"] == value
            })
            .await;
        }
        for (redo, expected) in [(false, "12"), (true, "25")] {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlLeft);
            keys.press(KeyCode::KeyZ);
            if redo {
                keys.press(KeyCode::ShiftLeft);
            }
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            super::super::tests::until(&mut app, |world| {
                world.resource::<Runtime>().areas[&owner].data[0]["quantity"] == expected
            })
            .await;
        }
    }
}
