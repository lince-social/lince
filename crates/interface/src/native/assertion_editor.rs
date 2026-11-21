// non-lean-yet
// Scope: mod completion, mod preparation, mod syntax, mod tests.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 259, 412, 416,
//   418, 426, 480, 484, 569, 599, 635, 642.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (131, 158, 195, 220, 235, 236, 248, 256, 265, 278, 300, 301, 303, 374, 386, 407,
//   424, 434, 456, 459, 460, 470, 476, 493, 521, 594, 614, 629, 633, 647, 651, 657).

use std::sync::Arc;
use bevy::{
    input_focus::{FocusCause, InputFocus},
    prelude::*,
    text::{EditableText, TextLayoutInfo},
};
use cell::{ClientMessage, ServerMessage};
use engine::{record_change::Mutation, record_creation::Assertion};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};

use crate::native::{
    actions::{Action, ActionButton},
    protein_area::RecordBinding,
    tokens::Token,
};

mod completion;
mod preparation;
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
    values: Arc<Vec<Value>>,
    chips: Vec<Entity>,
    queue: VecDeque<Assertion>,
    pending: bool,
    removing: Option<String>,
    records: Arc<Vec<Value>>,
    concepts: Arc<Vec<Value>>,
    completion: completion::State,
    options: Vec<completion::Option>,
    selected: usize,
    dismissed: Option<String>,
    submit: bool,
    complete: bool,
    step: i32,
}

#[derive(Component)]
struct Chip {
    uid: String,
    text: Entity,
    close: Entity,
}

#[derive(Resource, Default)]
struct Subscriptions(
    HashMap<Entity, (String, crate::native::cell_bridge::Sender)>,
    VecDeque<(crate::native::cell_bridge::Sender, ClientMessage)>,
);

pub(crate) struct AssertionEditorPlugin;
impl Plugin for AssertionEditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Subscriptions>()
            .init_resource::<crate::native::tokens::ThemeSettings>()
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
                    .before(crate::native::actions::ApplyActions),
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
    crate::native::edit_mode::label(world, parent, "Assertions", 13.0);
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
            crate::native::token_style::border(Token::Accent),
            crate::native::sand::Unsaved(false),
            ChildOf(parent),
        ))
        .observe(completion::capture)
        .id();
    let bundle = crate::native::sand::text_editor(
        "",
        world.resource::<crate::native::theme::Typography>(),
        0,
    );
    let font = world
        .resource::<crate::native::theme::Typography>()
        .text(16.0);
    let input = world.spawn(bundle).id();
    world.entity_mut(input).insert((font, crate::native::sand::Borderless,
        Node { min_width: px(100), max_width: percent(100), width: px(140),
            min_height: px(28), height: Val::Auto, flex_grow: 1.0, flex_basis: px(140),
            padding: UiRect::all(px(3)), ..default() },
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        crate::native::icons::Tooltip("Assertions: #planned, #depends-on @project, #cost: 12.5 @unit. Enter adds; Tab completes.".into()),
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
            crate::native::scroll_sand::ScrollSand,
            ChildOf(parent),
        ))
        .id();
    let notice = crate::native::edit_mode::label(world, parent, "", 13.0);
    world.get_mut::<Node>(notice).unwrap().display = Display::None;
    world.entity_mut(parent).insert(Field {
        binding,
        input,
        flow,
        suggestions,
        notice,
        draft,
        values: data["assertions"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into(),
        chips: Vec::new(),
        queue: VecDeque::new(),
        pending: false,
        removing: None,
        records: Arc::default(),
        concepts: Arc::default(),
        completion: completion::State::default(),
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

pub(crate) struct DraftInput<'a> {
    pub(crate) values: &'a Arc<Vec<Value>>,
    pub(crate) text: &'a EditableText,
}

impl DraftInput<'_> {
    pub(crate) fn collect(&self) -> Result<Vec<Assertion>, String> {
        let value = self.text.value();
        let pending = syntax::parse(&value.to_cow())?;
        let mut values = Vec::with_capacity(self.values.len() + pending.len());
        values.extend(self.values.iter().map(syntax::from_value));
        values.extend(pending);
        Ok(values)
    }
}

pub(crate) fn draft_input(world: &World, parent: Entity) -> Result<DraftInput<'_>, &'static str> {
    let field = world
        .get::<Field>(parent)
        .ok_or("Assertions are unavailable")?;
    let text = world.get::<EditableText>(field.input).unwrap();
    if text.is_composing() || text.pending_paste.is_some() || !text.pending_edits.is_empty() {
        return Err("Finish editing assertions before creating the Record");
    }
    Ok(DraftInput {
        values: &field.values,
        text,
    })
}

pub(crate) fn draft(world: &World, parent: Entity) -> Result<Vec<Assertion>, String> {
    draft_input(world, parent).map_err(str::to_owned)?.collect()
}

fn notice(world: &mut World, entity: Entity, message: &str) {
    let label = world.get::<Field>(entity).unwrap().notice;
    let mut text = world.get_mut::<Text>(label).unwrap();
    if text.0 != message {
        text.0.clear();
        text.0.push_str(message);
    }
    let display = if message.is_empty() {
        Display::None
    } else {
        Display::Flex
    };
    world
        .get_mut::<Node>(label)
        .unwrap()
        .map_unchanged(|node| &mut node.display)
        .set_if_neq(display);
}

fn draw(world: &mut World, entity: Entity) {
    let _draw =
        crate::host::performance::Span::new(crate::host::performance::Stage::AssertionChipDraw);
    let field = world.get::<Field>(entity).unwrap();
    let flow = field.flow;
    let input = field.input;
    let values = field.values.clone();
    crate::host::performance::add(
        crate::host::performance::Count::AssertionRedrawValuesShared,
        values.len() as u64,
    );
    let mut previous = VecDeque::from(std::mem::take(
        &mut world.get_mut::<Field>(entity).unwrap().chips,
    ));
    let mut chips = Vec::new();
    for entry in values.iter() {
        let Some(uid) = entry["uid"].as_str() else {
            continue;
        };
        let predicate = entry["predicate"]
            .as_str()
            .or_else(|| entry["predicate_uid"].as_str())
            .unwrap_or_default();
        let object = entry["uri"].as_str().or_else(|| entry["object"].as_str());
        let unit = entry["unit"].as_str();
        let field = world.get::<Field>(entity).unwrap();
        let object = object.map(|object| {
            field
                .records
                .iter()
                .find(|row| row["uid"].as_str() == Some(object))
                .and_then(|row| row["slug"].as_str())
                .unwrap_or(object)
        });
        let unit = unit.map(|unit| {
            field
                .concepts
                .iter()
                .find(|row| row["uid"].as_str() == Some(unit))
                .and_then(|row| row["name"].as_str())
                .unwrap_or(unit)
        });
        let label = syntax::format_parts(predicate, object, entry["quantity"].as_str(), unit);
        if let Some(index) = previous
            .iter()
            .position(|chip| world.get::<Chip>(*chip).is_some_and(|chip| chip.uid == uid))
        {
            let chip = previous.remove(index).unwrap();
            let text = world.get::<Chip>(chip).unwrap().text;
            if world.get::<Text>(text).is_some_and(|text| text.0 != label) {
                world.get_mut::<Text>(text).unwrap().0 = label;
            }
            chips.push(chip);
            continue;
        }
        let chip = world
            .spawn((
                crate::native::sand::Square,
                Node {
                    max_width: percent(100),
                    min_width: px(0),
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    padding: UiRect::axes(px(5), px(2)),
                    column_gap: px(3),
                    flex_shrink: 0.0,
                    ..default()
                },
                BorderColor::all(Color::NONE),
                Outline {
                    width: px(0),
                    ..default()
                },
                ChildOf(flow),
            ))
            .id();
        let text = crate::native::edit_mode::label(world, chip, &label, 16.0);
        world.entity_mut(text).insert((
            Node {
                flex_shrink: 1.0,
                ..default()
            },
            TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        ));
        let close = world
            .spawn((
                crate::native::sand::button(0),
                crate::native::sand::Borderless,
                Node {
                    width: px(20),
                    height: px(24),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    flex_shrink: 0.0,
                    ..default()
                },
                Visibility::Hidden,
                crate::native::icons::Tooltip("Remove this assertion".into()),
                ActionButton::new(entity, crate::native::actions![Remove(uid.into())]),
                ChildOf(chip),
            ))
            .id();
        crate::native::edit_mode::label(world, close, "×", 16.0);
        world.entity_mut(chip).insert(Chip {
            uid: uid.into(),
            text,
            close,
        });
        chips.push(chip);
    }
    for chip in previous {
        world.despawn(chip);
    }
    if world
        .get::<Children>(flow)
        .is_none_or(|current| !current.iter().eq(chips.iter().copied().chain([input])))
    {
        let children: Vec<_> = chips.iter().copied().chain([input]).collect();
        world.entity_mut(flow).replace_children(&children);
    }
    world.get_mut::<Field>(entity).unwrap().chips = chips;
}

pub(crate) fn refresh(world: &mut World, entity: Entity, data: &Value) -> bool {
    let Some(field) = world.get::<Field>(entity) else {
        return false;
    };
    let values = data["assertions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    if field.values.as_slice() != values {
        world.get_mut::<Field>(entity).unwrap().values = values.to_vec().into();
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
            Arc::make_mut(&mut world.get_mut::<Field>(entity).unwrap().values)
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
        match crate::native::protein_area::execute(world, &binding, entity, action) {
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
        let values = Arc::make_mut(&mut field.values);
        for assertion in assertions {
            values.push(
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
    if let Err(error) = crate::native::protein_area::execute(world, &binding, entity, action) {
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
    let _receive = crate::host::performance::Span::new(
        crate::host::performance::Stage::AssertionEditorReceive,
    );
    let concept_key = id.strip_suffix("-concepts");
    let subscriptions = world.get_resource::<Subscriptions>();
    crate::host::performance::add(
        crate::host::performance::Count::AssertionReplySubscriptions,
        subscriptions.map_or(0, |subscriptions| subscriptions.0.len() as u64),
    );
    let fields: Vec<_> = subscriptions
        .into_iter()
        .flat_map(|subscriptions| subscriptions.0.iter())
        .filter(|(_, (key, _))| id == key || concept_key == Some(key.as_str()))
        .map(|(entity, _)| *entity)
        .collect();
    crate::host::performance::add(
        crate::host::performance::Count::AssertionReplyRecipients,
        fields.len() as u64,
    );
    for entity in fields {
        if let Some(mut field) = world.get_mut::<Field>(entity) {
            if let Some(rows) = rows {
                let current = if concept_key.is_some() {
                    &mut field.concepts
                } else {
                    &mut field.records
                };
                if current != rows {
                    *current = rows.clone();
                    field.completion.invalidate();
                    draw(world, entity);
                }
            }
            if let Some(error) = error {
                notice(world, entity, error);
            }
        }
    }
}

fn update(world: &mut World, mut preparation: Local<preparation::Preparation>) {
    let _update =
        crate::host::performance::Span::new(crate::host::performance::Stage::AssertionEditorUpdate);
    preparation.messages(world);
    for message in preparation.messages.drain(..) {
        receive(world, &message);
    }
    preparation.stale(world);
    for entity in preparation.stale.drain(..) {
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
    preparation.fields(world);
    for entity in preparation.fields.drain(..) {
        let field = world.get::<Field>(entity).unwrap();
        let input = field.input;
        let flow = field.flow;
        if !world.resource::<Subscriptions>().0.contains_key(&entity) {
            if let Some(sender) = crate::native::protein_area::editor_sender(world, &field.binding)
            {
                if let Ok(permits) = sender.try_reserve_many(2) {
                    crate::host::performance::add(
                        crate::host::performance::Count::AssertionSubscriptionPayloads,
                        2,
                    );
                    let id = format!("assertion-editor-{}", entity.to_bits());
                    let records = serde_json::from_value(
                        json!({"source":"record", "fields":["uid","head","slug"], "limit":null}),
                    )
                    .unwrap();
                    let concepts = serde_json::from_value(
                        json!({"source":"concept", "fields":["uid","name"], "limit":null}),
                    )
                    .unwrap();
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
        let dirty = field.pending
            || world
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .chars()
                .any(|ch| !ch.is_whitespace());
        world
            .get_mut::<crate::native::sand::Unsaved>(flow)
            .unwrap()
            .set_if_neq(crate::native::sand::Unsaved(dirty));
    }
    preparation.chips(world);
}

fn fit(
    fields: Query<&Field>,
    wake: Option<Res<crate::native::wake::WakeSignal>>,
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
