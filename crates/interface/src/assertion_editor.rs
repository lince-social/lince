use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::{EditableText, TextLayoutInfo},
};
use cell::{ClientMessage, ServerMessage};
use engine::{record_change::Mutation, record_creation::Assertion};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

use crate::{
    actions::{Action, ActionButton},
    protein_area::RecordBinding,
    tokens::Token,
};

mod completion;
mod syntax;

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(crate) struct Field {
    binding: RecordBinding,
    pub(crate) input: Entity,
    flow: Entity,
    suggestions: Entity,
    notice: Entity,
    draft: bool,
    values: Vec<Value>,
    chips: Vec<Entity>,
    queue: VecDeque<Assertion>,
    pending: bool,
    removing: Option<String>,
    records: Vec<Value>,
    concepts: Vec<Value>,
    signature: String,
    options: Vec<completion::Option>,
    selected: usize,
    dismissed: Option<String>,
    submit: bool,
    complete: bool,
    step: i32,
}

#[derive(Component)]
struct Chip {
    close: Entity,
}

#[derive(Resource, Default)]
struct Subscriptions(
    HashMap<Entity, (String, tokio::sync::mpsc::Sender<ClientMessage>)>,
    VecDeque<(tokio::sync::mpsc::Sender<ClientMessage>, ClientMessage)>,
);

pub(crate) struct AssertionEditorPlugin;
impl Plugin for AssertionEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Subscriptions>()
            .init_resource::<crate::tokens::ThemeSettings>()
            .init_resource::<InputFocus>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(
                PostUpdate,
                completion::keys.before(bevy::text::EditableTextSystems),
            )
            .add_systems(
                PostUpdate,
                update
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            )
            .add_systems(PostUpdate, fit.after(bevy::ui::UiSystems::PostLayout));
    }
}

pub(crate) fn spawn(
    world: &mut World,
    parent: Entity,
    binding: RecordBinding,
    data: &Value,
    draft: bool,
) {
    crate::edit_mode::label(world, parent, "Assertions", 13.0);
    let flow = world
        .spawn((
            Node {
                width: percent(100),
                min_width: px(0),
                min_height: px(36),
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                column_gap: px(4),
                row_gap: px(4),
                padding: UiRect::all(px(4)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::border(Token::Accent),
            crate::sand::Unsaved(false),
            ChildOf(parent),
        ))
        .observe(completion::capture)
        .id();
    let bundle = crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0);
    let font = world.resource::<crate::theme::Typography>().text(16.0);
    let input = world.spawn(bundle).id();
    world.entity_mut(input).insert((font, crate::sand::Borderless,
        Node { min_width: px(100), max_width: percent(100), width: px(140),
            min_height: px(28), height: Val::Auto, flex_grow: 1.0, flex_basis: px(140),
            padding: UiRect::all(px(3)), ..default() },
        TextLayout::linebreak(bevy::text::LineBreak::WordBoundary),
        crate::icons::Tooltip("Assertions: #planned, #depends-on @project, #cost: 12.5 @unit. Enter adds; Tab completes.".into()),
        ChildOf(flow),
    ));
    {
        let mut text = world.get_mut::<EditableText>(input).unwrap();
        text.max_characters = Some(65536);
        text.visible_lines = None;
        text.allow_newlines = true;
        text.pending_edits.clear();
    }
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(input) {
        node.set_label("Assertions");
        node.set_placeholder("#concept or #concept @slug");
    }
    let suggestions = world
        .spawn((
            Node {
                display: Display::None,
                width: percent(100),
                flex_direction: FlexDirection::Column,
                max_height: px(220),
                overflow: Overflow::scroll_y(),
                flex_shrink: 0.0,
                ..default()
            },
            ScrollPosition::default(),
            crate::scroll_sand::ScrollSand,
            ChildOf(parent),
        ))
        .id();
    let notice = crate::edit_mode::label(world, parent, "", 13.0);
    world.get_mut::<Node>(notice).unwrap().display = Display::None;
    world.entity_mut(parent).insert(Field {
        binding,
        input,
        flow,
        suggestions,
        notice,
        draft,
        values: data["assertions"].as_array().cloned().unwrap_or_default(),
        chips: Vec::new(),
        queue: VecDeque::new(),
        pending: false,
        removing: None,
        records: Vec::new(),
        concepts: Vec::new(),
        signature: String::new(),
        options: Vec::new(),
        selected: 0,
        dismissed: None,
        submit: false,
        complete: false,
        step: 0,
    });
    draw(world, parent);
}

pub(crate) fn input(world: &World, parent: Entity) -> Option<Entity> {
    world.get::<Field>(parent).map(|field| field.input)
}

fn value(world: &World, input: Entity) -> String {
    world
        .get::<EditableText>(input)
        .unwrap()
        .value()
        .to_string()
}

pub(crate) fn draft(world: &World, parent: Entity) -> Result<Vec<Assertion>, String> {
    let field = world
        .get::<Field>(parent)
        .ok_or("Assertions are unavailable")?;
    let text = world.get::<EditableText>(field.input).unwrap();
    if text.is_composing() || text.pending_paste.is_some() || !text.pending_edits.is_empty() {
        return Err("Finish editing assertions before creating the Record".into());
    }
    let mut values: Vec<_> = field.values.iter().map(syntax::from_value).collect();
    values.extend(syntax::parse(&value(world, field.input))?);
    Ok(values)
}

fn notice(world: &mut World, entity: Entity, message: &str) {
    let label = world.get::<Field>(entity).unwrap().notice;
    world.get_mut::<Text>(label).unwrap().0 = message.into();
    world.get_mut::<Node>(label).unwrap().display = if message.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
}

fn draw(world: &mut World, entity: Entity) {
    let field = world.get::<Field>(entity).unwrap();
    let flow = field.flow;
    let input = field.input;
    let values = field.values.clone();
    for chip in std::mem::take(&mut world.get_mut::<Field>(entity).unwrap().chips) {
        world.despawn(chip);
    }
    for entry in values {
        let Some(uid) = entry["uid"].as_str() else {
            continue;
        };
        let mut assertion = syntax::from_value(&entry);
        let field = world.get::<Field>(entity).unwrap();
        if let Some(object) = &mut assertion.object {
            if let Some(slug) = field
                .records
                .iter()
                .find(|row| row["uid"].as_str() == Some(object))
                .and_then(|row| row["slug"].as_str())
            {
                *object = slug.into();
            }
        }
        if let Some(unit) = &mut assertion.unit {
            if let Some(name) = field
                .concepts
                .iter()
                .find(|row| row["uid"].as_str() == Some(unit))
                .and_then(|row| row["name"].as_str())
            {
                *unit = name.into();
            }
        }
        let chip = world
            .spawn((
                crate::sand::Square,
                Node {
                    max_width: percent(100),
                    min_width: px(0),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    padding: UiRect::axes(px(5), px(2)),
                    column_gap: px(3),
                    ..default()
                },
                BorderColor::all(Color::NONE),
                ChildOf(flow),
            ))
            .id();
        let text = crate::edit_mode::label(world, chip, &syntax::format(&assertion), 16.0);
        world.entity_mut(text).insert((
            Node {
                min_width: px(0),
                flex_shrink: 1.0,
                ..default()
            },
            TextLayout::linebreak(bevy::text::LineBreak::WordBoundary),
        ));
        let close = world
            .spawn((
                crate::sand::button(0),
                crate::sand::Borderless,
                Node {
                    width: px(20),
                    height: px(24),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                Visibility::Hidden,
                crate::icons::Tooltip("Remove this assertion".into()),
                ActionButton::new(entity, crate::actions![Remove(uid.into())]),
                ChildOf(chip),
            ))
            .id();
        crate::edit_mode::label(world, close, "×", 16.0);
        world.entity_mut(chip).insert(Chip { close });
        world.get_mut::<Field>(entity).unwrap().chips.push(chip);
    }
    world.entity_mut(flow).add_child(input);
}

pub(crate) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(field) = world.get::<Field>(entity) else {
        return false;
    };
    let values = data["assertions"].as_array().cloned().unwrap_or_default();
    if field.values != values {
        world.get_mut::<Field>(entity).unwrap().values = values;
        draw(world, entity);
    }
    true
}

#[derive(Clone)]
struct Remove(String);
impl Action for Remove {
    fn apply(&self, world: &mut World, entity: Entity) {
        let Some(field) = world.get::<Field>(entity) else {
            return;
        };
        if field.pending
            || world
                .get::<bevy::ui::InteractionDisabled>(field.input)
                .is_some()
        {
            return;
        }
        if field.draft {
            world
                .get_mut::<Field>(entity)
                .unwrap()
                .values
                .retain(|value| value["uid"] != self.0);
            draw(world, entity);
            return;
        }
        let binding = field.binding.clone();
        let action = engine::actions::Action::ChangeRecord {
            request: engine::record_change::Request {
                id: nucleus::new_uid("op"),
                record_uid: binding.uid.clone(),
                mutation: Mutation::RetractAssertion {
                    assertion: self.0.clone(),
                },
            },
        };
        match crate::protein_area::execute(world, &binding, entity, action) {
            Ok(()) => {
                let mut field = world.get_mut::<Field>(entity).unwrap();
                field.pending = true;
                field.removing = Some(self.0.clone());
            }
            Err(error) => notice(world, entity, &error),
        }
    }
}

fn submit(world: &mut World, entity: Entity) {
    let field = world.get::<Field>(entity).unwrap();
    if field.pending {
        return;
    }
    let input = field.input;
    let assertions = match syntax::parse(&value(world, input)) {
        Ok(values) => values,
        Err(error) => {
            notice(world, entity, &error);
            return;
        }
    };
    if assertions.is_empty() {
        return;
    }
    if field.draft && field.values.len() + assertions.len() > 40 {
        notice(world, entity, "Add at most 40 assertions");
        return;
    }
    notice(world, entity, "");
    world
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("");
    if world.get::<Field>(entity).unwrap().draft {
        let mut field = world.get_mut::<Field>(entity).unwrap();
        for assertion in assertions {
            field.values.push(
                json!({"uid":nucleus::new_uid("a"), "predicate":assertion.predicate,
                "object":assertion.object, "quantity":assertion.quantity, "unit":assertion.unit}),
            );
        }
        draw(world, entity);
    } else {
        world.get_mut::<Field>(entity).unwrap().queue = assertions.into();
        send_next(world, entity);
    }
}

fn send_next(world: &mut World, entity: Entity) {
    let field = world.get::<Field>(entity).unwrap();
    let Some(assertion) = field.queue.front().cloned() else {
        return;
    };
    let binding = field.binding.clone();
    let action = engine::actions::Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: binding.uid.clone(),
            mutation: Mutation::Assertion {
                predicate: assertion.predicate,
                object: assertion.object,
                quantity: assertion.quantity,
                unit: assertion.unit,
            },
        },
    };
    world.get_mut::<Field>(entity).unwrap().pending = true;
    if let Err(error) = crate::protein_area::execute(world, &binding, entity, action) {
        finished(world, entity, Some(error));
    }
}

pub(crate) fn finished(world: &mut World, entity: Entity, error: Option<String>) -> bool {
    let Some(mut field) = world.get_mut::<Field>(entity) else {
        return false;
    };
    field.pending = false;
    let removing = field.removing.take();
    if let Some(error) = error {
        let input = field.input;
        let mut text = field
            .queue
            .drain(..)
            .map(|assertion| syntax::format(&assertion))
            .collect::<Vec<_>>()
            .join(", ");
        let current = value(world, input);
        if !text.is_empty() {
            if !current.is_empty() {
                text.push_str(", ");
                text.push_str(&current);
            }
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(&text);
        }
        notice(world, entity, &error);
    } else if removing.is_none() {
        field.queue.pop_front();
        send_next(world, entity);
    }
    true
}

pub(crate) fn receive(world: &mut World, message: &ServerMessage) {
    let (id, rows, error) = match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
            (id, Some(rows), None)
        }
        ServerMessage::Error { id, message, .. } => (id, None, Some(message)),
        _ => return,
    };
    let fields: Vec<_> = world
        .get_resource::<Subscriptions>()
        .into_iter()
        .flat_map(|subscriptions| subscriptions.0.iter())
        .filter(|(_, (key, _))| id == key || *id == format!("{key}-concepts"))
        .map(|(entity, _)| *entity)
        .collect();
    for entity in fields {
        if let Some(mut field) = world.get_mut::<Field>(entity) {
            field.signature.clear();
            if let Some(rows) = rows {
                if id.ends_with("-concepts") {
                    field.concepts = rows.clone();
                } else {
                    field.records = rows.clone();
                }
            }
            if let Some(error) = error {
                notice(world, entity, error);
            }
            draw(world, entity);
        }
    }
}

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::cell_bridge::CellMessage>>,
) {
    let messages: Vec<_> = world
        .get_resource::<Messages<crate::cell_bridge::CellMessage>>()
        .map(|messages| {
            cursor
                .read(messages)
                .map(|message| message.0.clone())
                .collect()
        })
        .unwrap_or_default();
    for message in messages {
        receive(world, &message);
    }
    let stale: Vec<_> = world
        .resource::<Subscriptions>()
        .0
        .keys()
        .filter(|entity| {
            world.get::<Field>(**entity).is_none()
                || world.resource::<Subscriptions>().0[entity].1.is_closed()
        })
        .copied()
        .collect();
    for entity in stale {
        let (id, sender) = world
            .resource_mut::<Subscriptions>()
            .0
            .remove(&entity)
            .unwrap();
        for id in [format!("{id}-concepts"), id] {
            world
                .resource_mut::<Subscriptions>()
                .1
                .push_back((sender.clone(), ClientMessage::Unsubscribe { id }));
        }
    }
    while let Some((sender, message)) = world.resource_mut::<Subscriptions>().1.pop_front() {
        if let Err(tokio::sync::mpsc::error::TrySendError::Full(message)) = sender.try_send(message)
        {
            world
                .resource_mut::<Subscriptions>()
                .1
                .push_front((sender, message));
            break;
        }
    }
    let fields: Vec<_> = world
        .query_filtered::<Entity, With<Field>>()
        .iter(world)
        .collect();
    for entity in fields {
        let field = world.get::<Field>(entity).unwrap();
        let input = field.input;
        let flow = field.flow;
        if !world.resource::<Subscriptions>().0.contains_key(&entity) {
            if let Some(sender) = crate::protein_area::editor_sender(world, &field.binding) {
                let id = format!("assertion-editor-{}", entity.to_bits());
                let records = serde_json::from_value(
                    json!({"source":"record", "fields":["uid","head","slug"], "limit":null}),
                )
                .unwrap();
                let concepts = serde_json::from_value(
                    json!({"source":"concept", "fields":["uid","name"], "limit":null}),
                )
                .unwrap();
                if let Ok(permits) = sender.try_reserve_many(2) {
                    for (permit, (id, protein)) in
                        permits.zip([(id.clone(), records), (format!("{id}-concepts"), concepts)])
                    {
                        permit.send(ClientMessage::Subscribe { id, protein });
                    }
                    world
                        .resource_mut::<Subscriptions>()
                        .0
                        .insert(entity, (id, sender.clone()));
                }
            }
        }
        completion::update(world, entity);
        let field = world.get::<Field>(entity).unwrap();
        let dirty = field.pending || !value(world, input).trim().is_empty();
        world
            .get_mut::<crate::sand::Unsaved>(flow)
            .unwrap()
            .set_if_neq(crate::sand::Unsaved(dirty));
    }
    let hovered: Vec<_> = world
        .get_resource::<bevy::picking::hover::HoverMap>()
        .into_iter()
        .flat_map(|hover| hover.values().flat_map(|hits| hits.keys().copied()))
        .collect();
    let focus = world.resource::<InputFocus>().get();
    let chips: Vec<_> = world
        .query::<(Entity, &Chip)>()
        .iter(world)
        .map(|(entity, chip)| (entity, chip.close))
        .collect();
    for (entity, close) in chips {
        let active = focus == Some(close)
            || hovered.iter().any(|hit| {
                let mut cursor = Some(*hit);
                while let Some(current) = cursor {
                    if current == entity {
                        return true;
                    }
                    cursor = world.get::<ChildOf>(current).map(ChildOf::parent);
                }
                false
            });
        let color = if active {
            crate::token_style::resolve(world, entity, Token::Accent)
                .0
                .color()
        } else {
            Color::NONE
        };
        if world.get::<BorderColor>(entity) != Some(&BorderColor::all(color)) {
            world.entity_mut(entity).insert(BorderColor::all(color));
        }
        world
            .get_mut::<Visibility>(close)
            .unwrap()
            .set_if_neq(if active {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
    }
}

fn fit(
    fields: Query<&Field>,
    wake: Option<Res<crate::wake::WakeSignal>>,
    mut editors: Query<(
        &ComputedNode,
        &TextLayoutInfo,
        &mut Node,
        Option<&mut bevy::ui::widget::TextScroll>,
    )>,
) {
    let mut resized = false;
    for field in &fields {
        let Ok((computed, layout, mut node, scroll)) = editors.get_mut(field.input) else {
            continue;
        };
        let scale = computed.inverse_scale_factor();
        let height = ((layout.size.y + computed.size().y - computed.content_box().height())
            * scale)
            .ceil()
            .max(28.0);
        if height.is_finite() && node.height != px(height) {
            node.height = px(height);
            resized = true;
        }
        if let Some(mut scroll) = scroll
            && scroll.0 != Vec2::ZERO
        {
            scroll.0 = Vec2::ZERO;
        }
    }
    if resized && let Some(wake) = wake {
        wake.ring();
    }
}
