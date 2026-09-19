use super::*;
use crate::{
    actions::{Action, ActionButton},
    canvas::CanvasItem,
    icons::{Icon, IconButton, Tooltip},
    sand::{InBox, Square},
    tokens::Token,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, text::EditableText};

#[derive(Component)]
struct Row {
    area: Entity,
    data: Value,
    config: std::sync::Arc<Config>,
    index: usize,
}

#[derive(Component)]
struct LastLayout(bevy::math::DVec3);

#[derive(Component)]
struct EditorSize {
    mode: OverflowMode,
    minimum: Vec2,
    maximum_width: f32,
    observed: String,
    measuring: bool,
}

#[derive(Component)]
pub(super) struct PropertyContainer(pub(super) String);

fn baseline(data: &Value, property: &str) -> Value {
    let mut value = serde_json::json!({property: data[property]});
    if matches!(property, "start_date" | "due_date" | "estimate_min") {
        value["extension"] = data["extension"].clone();
    }
    value
}

#[derive(Component)]
pub(super) struct PropertyEditor {
    property: String,
    observed: String,
    baseline: Value,
    pending: Option<String>,
    attempted: Option<String>,
}

pub(super) fn display(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                if let Some(head) = value["head"].as_str() {
                    head.into()
                } else if let Some(predicate) = value["predicate"].as_str() {
                    format!(
                        "#{predicate}{}{}",
                        value["quantity"]
                            .as_str()
                            .map(|q| format!(": {q}"))
                            .unwrap_or_default(),
                        value["object"]
                            .as_str()
                            .map(|uid| format!(" → {uid}"))
                            .unwrap_or_default()
                    )
                } else {
                    value.to_string()
                }
            })
            .collect::<Vec<String>>()
            .join("\n"),
        value => value.to_string(),
    }
}

fn click(
    mut event: On<Pointer<Click>>,
    bindings: Query<&RecordBinding>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    if event.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok(binding) = bindings.get(event.entity) else {
        return;
    };
    let mut root = event.entity;
    while let Ok(parent) = parents.get(root) {
        root = parent.parent();
    }
    commands.trigger(RecordClicked {
        entity: root,
        sand: event.entity,
        uid: binding.uid.clone(),
        source: binding.source.clone(),
    });
    event.propagate(false);
}

pub(super) fn content(
    world: &mut World,
    row: Entity,
    config: &Config,
    data: &Value,
    binding: Option<RecordBinding>,
) {
    let sections = config
        .record_cards
        .then(|| super::record_layout::create(world, row));
    if config.record_cards {
        world.entity_mut(row).insert(crate::full_record::RecordCard);
    }
    for property in &config.bindings {
        let parent = sections
            .as_ref()
            .map_or(row, |sections| sections.parent(&property.property, data));
        let horizontal = matches!(
            property.overflow,
            OverflowMode::ScrollRight | OverflowMode::GrowRight
        );
        let grow_y = property.overflow == OverflowMode::GrowDown;
        let grow_x = property.overflow == OverflowMode::GrowRight;
        let width = property.width.min(config.width - 24.0).max(24.0);
        let container = world
            .spawn((
                Node {
                    align_self: AlignSelf::FlexStart,
                    align_items: AlignItems::FlexStart,
                    flex_direction: FlexDirection::Column,
                    width: if config.record_cards {
                        percent(100)
                    } else if grow_x {
                        Val::Auto
                    } else {
                        px(width)
                    },
                    min_width: if grow_x { px(width) } else { px(0) },
                    max_width: if config.record_cards {
                        percent(100)
                    } else {
                        px((config.width - 24.0).max(24.0))
                    },
                    height: if grow_y || grow_x {
                        Val::Auto
                    } else {
                        px(property.height)
                    },
                    min_height: px(property.height),
                    flex_shrink: 0.0,
                    padding: UiRect::all(px(4)),
                    overflow: match property.overflow {
                        OverflowMode::ScrollDown => Overflow::scroll_y(),
                        OverflowMode::ScrollRight => Overflow::scroll_x(),
                        _ => Overflow::clip(),
                    },
                    ..default()
                },
                ScrollPosition::default(),
                PropertyContainer(property.property.clone()),
                ChildOf(parent),
                Tooltip(
                    protein::record_schema::fields()
                        .iter()
                        .find(|f| f.key == property.property)
                        .map_or(property.property.clone(), |f| f.title.into()),
                ),
            ))
            .id();
        if property.square {
            world
                .entity_mut(container)
                .insert((Square, crate::token_style::background(Token::Surface)));
        }
        if let Some(binding) = binding.clone() {
            world.entity_mut(container).insert(binding).observe(click);
        }
        let text = display(&data[&property.property]);
        let editable = property.editable && binding.is_some();
        if config.show_labels
            && !matches!(
                property.property.as_str(),
                "threads" | "assertions" | "work_logs"
            )
        {
            let label = protein::record_schema::fields()
                .into_iter()
                .find(|field| field.key == property.property)
                .map_or(property.property.clone(), |field| field.title.into());
            let label = crate::edit_mode::label(world, container, &label, 13.0);
            world
                .entity_mut(label)
                .insert(crate::record_binding::BindingStatus);
        }
        if let Some(binding) = binding.clone() {
            if property.property == "work_timer" {
                crate::work_timer::populate(world, container, Some(binding), data, None);
                continue;
            }
            if property.property == "threads" {
                crate::thread_castle::populate(world, container, binding, data);
                continue;
            }
        }
        if editable && property.property == "assignees" {
            super::assignees::field(world, container, binding.clone().unwrap(), data);
            continue;
        }
        if editable && matches!(property.property.as_str(), "assertions" | "work_logs") {
            property_actions::spawn(
                world,
                container,
                &property.property,
                binding.clone().unwrap(),
                data,
                0,
            );
            continue;
        }
        if property.property == "body" && !editable {
            crate::description::spawn(
                world,
                container,
                &text,
                crate::description::Context {
                    owner: row,
                    source: binding
                        .as_ref()
                        .map_or(Source::Local, |binding| binding.source.clone()),
                },
            );
            continue;
        }
        let text_entity = if editable {
            let bundle =
                crate::sand::text_editor(&text, world.resource::<crate::theme::Typography>(), 0);
            let entity = world.spawn(bundle).id();
            let mut editor = world.get_mut::<EditableText>(entity).unwrap();
            editor.max_characters = Some(65_536);
            editor.visible_lines = None;
            world.entity_mut(entity).insert((
                PropertyEditor {
                    property: property.property.clone(),
                    observed: text,
                    baseline: baseline(data, &property.property),
                    pending: None,
                    attempted: None,
                },
                binding.clone().unwrap(),
                Tooltip(
                    "Edit this Record property. Changes save automatically. Ctrl-Z undoes edits."
                        .into(),
                ),
            ));
            if matches!(property.property.as_str(), "head" | "body")
                && crate::record_binding::enabled(world)
            {
                let status = crate::edit_mode::label(world, container, "Opening Record…", 12.0);
                crate::record_binding::attach(
                    world,
                    entity,
                    binding.clone().unwrap(),
                    &property.property,
                    Some(status),
                );
            }
            if !matches!(property.property.as_str(), "head" | "body")
                || !crate::record_binding::enabled(world)
            {
                super::history::attach_text(world, entity);
            }
            entity
        } else {
            let font = world.resource::<crate::theme::Typography>().text(18.0);
            world
                .spawn((Text::new(text), font, crate::token_style::text(Token::Ink)))
                .id()
        };
        world.entity_mut(text_entity).insert((
            Node {
                width: if config.record_cards {
                    percent(100)
                } else if horizontal && !editable {
                    Val::Auto
                } else {
                    px((width - 8.0).max(16.0))
                },
                height: if editable {
                    px((property.height - 8.0).max(24.0))
                } else {
                    Val::Auto
                },
                min_width: if horizontal { Val::Auto } else { px(0) },
                flex_shrink: 0.0,
                max_width: if config.record_cards {
                    percent(100)
                } else if horizontal && !grow_x {
                    Val::Auto
                } else {
                    px((config.width - 32.0).max(16.0))
                },
                ..default()
            },
            TextLayout::linebreak(if horizontal && !grow_x {
                bevy::text::LineBreak::NoWrap
            } else {
                bevy::text::LineBreak::WordBoundary
            }),
            ChildOf(container),
        ));
        if editable && property.overflow != OverflowMode::Clip {
            world.entity_mut(text_entity).insert(EditorSize {
                mode: property.overflow,
                minimum: Vec2::new((width - 8.0).max(16.0), (property.height - 8.0).max(24.0)),
                maximum_width: (config.width - 32.0).max(16.0),
                observed: String::new(),
                measuring: false,
            });
        }
        if property.property == "body" && editable {
            crate::description::attach_editor(
                world,
                container,
                text_entity,
                crate::description::Context {
                    owner: row,
                    source: binding.as_ref().unwrap().source.clone(),
                },
            );
        }
        if editable && !matches!(property.property.as_str(), "head" | "body") {
            if matches!(property.property.as_str(), "start_date" | "due_date") {
                let button =
                    crate::calendar::date_button(world, row, text_entity, &property.property);
                world.entity_mut(button).insert(ChildOf(container));
            }
        }
    }
    if let Some(sections) = sections {
        super::record_layout::arrange(world, row, &sections, data);
    }
    if !config.record_cards
        && let Some(binding) = binding.clone()
    {
        let button = world
            .spawn((
                Square,
                ActionButton::new(row, crate::actions![crate::full_record::Open(binding)]),
                ChildOf(row),
            ))
            .id();
        crate::edit_mode::label(world, button, "Open Record", 14.0);
    }
    if config.delete_button {
        if let Some(binding) = binding {
            world.spawn((
                Square,
                ActionButton::new(row, crate::actions![Delete(binding.clone())]),
                IconButton::new(Icon::Delete, "Delete this Record"),
                ChildOf(row),
            ));
        }
    }
}

#[derive(Clone)]
struct Delete(RecordBinding);
impl Action for Delete {
    fn apply(&self, world: &mut World, row: Entity) {
        if let Err(error) = execute(
            world,
            &self.0,
            row,
            engine::actions::Action::DeleteRecord {
                target: self.0.uid.clone(),
            },
        ) {
            status(world, self.0.area, error);
        }
    }
}

pub(super) fn reconcile(world: &mut World, owner: Entity) {
    if world.get::<filter::Subscription>(owner).is_some() {
        return;
    }
    let Some(mut state) = world.resource_mut::<Runtime>().areas.remove(&owner) else {
        return;
    };
    if !state.dirty {
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        return;
    }
    let Some(config) = state.applied.clone() else {
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        return;
    };
    let shared_config = std::sync::Arc::new(config.clone());
    let Some(parent) = world.get::<ChildOf>(owner).map(ChildOf::parent) else {
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        return;
    };
    let workspace = world
        .get::<WorkspaceMember>(owner)
        .map_or(1, |member| member.0);
    state.page = state.page.min(state.data.len().saturating_sub(1) / 200);
    if let Some(entity) = state.navigation.take() {
        let _ = world.despawn(entity);
    }
    if state.data.len() > 200 {
        state.navigation = Some(ui::navigation(world, owner, state.page, state.data.len()));
    }
    let rows = grouping::page(&state.data, &config, state.page);
    let cells = grouping::prepare(world, owner, &mut state, &rows, &config);
    let stale: Vec<_> = state
        .row_entities
        .iter()
        .filter(|(uid, _)| !rows.iter().any(|row| row["uid"].as_str() == Some(uid)))
        .map(|(uid, entity)| (uid.clone(), *entity))
        .collect();
    for (uid, entity) in stale {
        let _ = world.despawn(entity);
        state.row_entities.remove(&uid);
    }
    for (index, data) in rows.into_iter().enumerate() {
        let Some(uid) = data["uid"]
            .as_str()
            .filter(|uid| uid.len() <= 128 && !uid.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let binding = RecordBinding {
            area: owner,
            uid: uid.clone(),
            source: config.source.clone(),
        };
        let entity = state
            .row_entities
            .get(&uid)
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
            .unwrap_or_else(|| {
                let entity = world
                    .spawn((
                        crate::castle::Castle,
                        Square,
                        InBox(parent),
                        ChildOf(parent),
                        WorkspaceMember(workspace),
                        crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::new(config.width, 160.0),
                        },
                        Node {
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(px(12)),
                            row_gap: px(8),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        crate::token_style::background(Token::Surface),
                        binding.clone(),
                        super::placement::Pending,
                        Visibility::Hidden,
                    ))
                    .observe(click)
                    .id();
                if config.source != Source::Local {
                    world.entity_mut(entity).insert(RemoteRecord);
                }
                state.row_entities.insert(uid.clone(), entity);
                entity
            });
        if config.grouping.active() {
            let cell = cells[&uid];
            if world.get::<grouping::Cell>(entity) != Some(&cell) {
                world.entity_mut(entity).remove::<LastLayout>().insert(cell);
            }
        } else {
            world.entity_mut(entity).remove::<grouping::Cell>();
        }
        let same = world.get::<Row>(entity).is_some_and(|row| {
            row.data == data && row.config.as_ref() == &config && row.index == index
        });
        if !same {
            let template_changed = world
                .get::<Row>(entity)
                .is_none_or(|row| row.config.as_ref() != &config)
                || state.template_dirty;
            if template_changed {
                if let Some(children) = world.get::<Children>(entity) {
                    let children: Vec<_> = children.iter().collect();
                    for child in children {
                        world.despawn(child);
                    }
                }
                if config.viewport_height.is_some() {
                    crate::scroll_sand::attach(world, entity);
                }
                content(world, entity, &config, &data, Some(binding));
            } else {
                refresh(world, entity, &data);
            }
            let mut properties = data.clone();
            if let Some(quantity) = data["quantity_exact"]
                .as_str()
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
            {
                properties["quantity"] = serde_json::json!(quantity);
            }
            world.entity_mut(entity).insert((
                Row {
                    area: owner,
                    data: data.clone(),
                    config: shared_config.clone(),
                    index,
                },
                crate::area::RecordProperties(properties),
            ));
        }
    }
    state.dirty = false;
    state.template_dirty = false;
    world.resource_mut::<Runtime>().areas.insert(owner, state);
}

fn descendants(world: &World, entity: Entity) -> Vec<Entity> {
    let mut out = Vec::new();
    if let Some(children) = world.get::<Children>(entity) {
        for child in children {
            out.push(*child);
            out.extend(descendants(world, *child));
        }
    }
    out
}

fn refresh(world: &mut World, row: Entity, data: &Value) {
    let children: Vec<_> = descendants(world, row)
        .into_iter()
        .filter_map(|child| {
            world
                .get::<PropertyContainer>(child)
                .map(|property| (child, property.0.clone()))
        })
        .collect();
    for (container, property) in children {
        if crate::work_timer::refresh(world, container, data)
            || crate::thread_castle::refresh(world, container, data)
        {
            continue;
        }
        if super::assignees::refresh(world, container, data)
            || property_actions::refresh(world, container, data)
        {
            continue;
        }
        let text = display(&data[&property]);
        if property == "body" {
            crate::description::refresh_readonly(world, container, &text);
        }
        for entity in descendants(world, container) {
            if crate::description::protects(world, entity) {
                continue;
            }
            if world
                .get::<crate::record_binding::BindingStatus>(entity)
                .is_some()
            {
                continue;
            }
            if crate::record_binding::active(world, entity) {
                continue;
            }
            if let Some(editor) = world.get::<PropertyEditor>(entity) {
                let dirty = world
                    .get::<EditableText>(entity)
                    .is_some_and(|value| value.value().to_string() != editor.observed);
                if editor.pending.is_some() || dirty {
                    continue;
                }
                if world
                    .get::<EditableText>(entity)
                    .unwrap()
                    .value()
                    .to_string()
                    != text
                {
                    world
                        .get_mut::<EditableText>(entity)
                        .unwrap()
                        .editor
                        .set_text(&text);
                }
                super::history::synced_text(world, entity, &text);
                let mut editor = world.get_mut::<PropertyEditor>(entity).unwrap();
                editor.observed = text.clone();
                editor.baseline = baseline(data, &property);
            } else if let Some(mut value) = world.get_mut::<Text>(entity) {
                value.0 = text.clone();
            }
        }
    }
}

pub(super) fn action_finished(world: &mut World, entity: Entity, error: Option<String>) {
    if super::assignees::finished(world, entity, error.clone()) {
        return;
    }
    if crate::thread_castle::finished(world, entity, error.clone()) {
        return;
    }
    let Some(mut editor) = world.get_mut::<PropertyEditor>(entity) else {
        property_actions::finished(world, entity, error);
        return;
    };
    let submitted = editor.pending.take();
    if error.is_none() {
        let property = editor.property.clone();
        let value = submitted.unwrap_or_else(|| editor.observed.clone());
        let mut editor = world.get_mut::<PropertyEditor>(entity).unwrap();
        editor.observed = value.clone();
        editor.baseline[&property] = Value::String(value);
    }
}

pub(super) fn pick_date(world: &mut World, entity: Entity, date: &str) -> Result<(), String> {
    let property = world
        .get::<PropertyEditor>(entity)
        .ok_or("Date field is closed")?;
    if !matches!(property.property.as_str(), "start_date" | "due_date") {
        return Err("Choose a date field".into());
    }
    if property.pending.is_some() {
        return Err("Wait for the date to finish saving".into());
    }
    let text = world
        .get::<EditableText>(entity)
        .ok_or("Date field is closed")?;
    if text.is_composing() || text.pending_paste.is_some() {
        return Err("Finish editing the date first".into());
    }
    let binding = world
        .get::<RecordBinding>(entity)
        .ok_or("Record is unavailable")?
        .clone();
    if world
        .query::<(&PropertyEditor, &RecordBinding)>()
        .iter(world)
        .any(|(editor, other)| {
            other.area == binding.area
                && other.uid == binding.uid
                && editor.pending.is_some()
                && matches!(
                    editor.property.as_str(),
                    "start_date" | "due_date" | "estimate_min"
                )
        })
    {
        return Err("Wait for this Record's dates to finish saving".into());
    }
    world
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(date);
    save_field(world, entity);
    let property = world.get::<PropertyEditor>(entity).unwrap();
    if property.pending.is_some() || property.observed == date {
        Ok(())
    } else {
        Err(calendar_status(world, binding.area).into())
    }
}

pub(super) fn save_field(world: &mut World, entity: Entity) {
    if world
        .get::<crate::record_binding::TextBinding>(entity)
        .is_some()
    {
        return;
    }
    let Some(binding) = world.get::<RecordBinding>(entity).cloned() else {
        return;
    };
    let Some(editor) = world.get::<PropertyEditor>(entity) else {
        return;
    };
    let Some(text) = world.get::<EditableText>(entity) else {
        return;
    };
    if editor.pending.is_some() || text.is_composing() || text.pending_paste.is_some() {
        return;
    }
    let value = text.value().to_string();
    let submitted = value.clone();
    if value == editor.observed || editor.attempted.as_ref() == Some(&value) {
        return;
    }
    let property = editor.property.clone();
    world.get_mut::<PropertyEditor>(entity).unwrap().attempted = Some(value.clone());
    let mutation = match property.as_str() {
        "slug" => engine::record_change::Mutation::Slug {
            value: (!value.trim().is_empty()).then(|| value.trim().to_owned()),
        },
        "quantity_exact" => engine::record_change::Mutation::Quantity { value },
        "start_date" | "due_date" | "estimate_min" => {
            let field = match property.as_str() {
                "start_date" => engine::record_change::WorkField::Start,
                "due_date" => engine::record_change::WorkField::Due,
                _ => engine::record_change::WorkField::Estimate,
            };
            let value = if value.trim().is_empty() {
                Value::Null
            } else if matches!(field, engine::record_change::WorkField::Estimate) {
                let Ok(number) = value.trim().parse::<f64>() else {
                    status(world, binding.area, "Estimate must be a number");
                    return;
                };
                if !number.is_finite() {
                    status(world, binding.area, "Estimate must be finite");
                    return;
                }
                serde_json::json!(number)
            } else {
                Value::String(value.trim().into())
            };
            engine::record_change::Mutation::Work { field, value }
        }
        "head" | "body" => {
            let action = engine::actions::Action::EditRecordText {
                target: binding.uid.clone(),
                head: (property == "head").then(|| value.clone()),
                body: (property == "body").then_some(value),
            };
            match execute(world, &binding, entity, action) {
                Ok(()) => {
                    world.get_mut::<PropertyEditor>(entity).unwrap().pending = Some(submitted)
                }
                Err(error) => status(world, binding.area, error),
            }
            return;
        }
        _ => return,
    };
    let action = engine::actions::Action::ChangeRecord {
        request: engine::record_change::Request {
            id: nucleus::new_uid("op"),
            record_uid: binding.uid.clone(),
            mutation,
        },
    };
    match execute(world, &binding, entity, action) {
        Ok(()) => {
            world.get_mut::<PropertyEditor>(entity).unwrap().pending = Some(submitted);
        }
        Err(error) => status(world, binding.area, error),
    }
}

pub(super) fn commit_edits(world: &mut World) {
    let editors: Vec<_> = world
        .query_filtered::<Entity, With<PropertyEditor>>()
        .iter(world)
        .collect();
    for entity in editors {
        save_field(world, entity);
    }
    super::record_layout::update(world);
}

pub(super) fn layout(world: &mut World) {
    super::placement::begin_frame(world);
    let mut resized = false;
    for (text, layout, computed, mut size, mut node, mut wrapping, scroll) in world
        .query::<(
            &EditableText,
            &bevy::text::TextLayoutInfo,
            &ComputedNode,
            &mut EditorSize,
            &mut Node,
            &mut TextLayout,
            Option<&mut bevy::ui::widget::TextScroll>,
        )>()
        .iter_mut(world)
    {
        if size.mode == OverflowMode::GrowRight {
            let content = text.value().to_string();
            if size.observed != content {
                size.observed = content;
                size.measuring = true;
                wrapping.linebreak = bevy::text::LineBreak::NoWrap;
                resized = true;
                continue;
            }
        }
        let measured = layout.size * computed.inverse_scale_factor();
        if size.mode == OverflowMode::GrowDown
            && measured.y > 0.0
            && computed.content_box().height() * computed.inverse_scale_factor() >= measured.y
            && let Some(mut scroll) = scroll
            && scroll.0 != Vec2::ZERO
        {
            scroll.0 = Vec2::ZERO;
        }
        let inset = (computed.size().y - computed.content_box().height()).max(0.0)
            * computed.inverse_scale_factor();
        let height = (measured.y + inset).ceil().max(size.minimum.y);
        if height.is_finite() && node.height != px(height) {
            node.height = px(height);
            resized = true;
        }
        if size.mode == OverflowMode::ScrollRight || size.measuring {
            let maximum = if size.mode == OverflowMode::GrowRight {
                size.maximum_width
            } else {
                100_000.0
            };
            let width = measured.x.ceil().max(size.minimum.x).min(maximum);
            if width.is_finite() && node.width != px(width) {
                node.width = px(width);
                resized = true;
            }
        }
        if size.measuring {
            size.measuring = false;
            wrapping.linebreak = bevy::text::LineBreak::WordBoundary;
            resized = true;
        }
    }
    if resized {
        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
    let rows: Vec<_> = world
        .query::<(Entity, &Row)>()
        .iter(world)
        .map(|(entity, row)| (entity, row.area, row.index, row.config.clone()))
        .collect();
    let mut heights = HashMap::<(Entity, usize), f32>::new();
    let mut grouped = HashMap::<Entity, Vec<(Entity, grouping::Cell, f32)>>::new();
    for (entity, area, index, config) in &rows {
        let visible: Vec<_> = world
            .get::<Children>(*entity)
            .into_iter()
            .flatten()
            .filter(|child| {
                world
                    .get::<Node>(**child)
                    .is_some_and(|node| node.display != Display::None)
            })
            .filter_map(|child| world.get::<ComputedNode>(*child))
            .collect();
        let count = visible.len();
        let height = visible
            .into_iter()
            .map(|node| node.size().y * node.inverse_scale_factor())
            .sum::<f32>()
            + 24.0
            + (count.saturating_sub(1) as f32 * 8.0);
        let height = config.viewport_height.unwrap_or(height);
        if let Some(cell) = world.get::<grouping::Cell>(*entity).copied() {
            grouped
                .entry(*area)
                .or_default()
                .push((*entity, cell, height.max(48.0)));
            continue;
        }
        let value = heights
            .entry((*area, index / config.columns))
            .or_insert(0.0);
        *value = value.max(height.max(48.0));
    }
    for (entity, area, index, config) in rows {
        if let Some(rows) = grouped.remove(&area) {
            grouping::layout(world, area, &config, &rows);
        }
        if world.get::<grouping::Cell>(entity).is_some() {
            continue;
        }
        let Some(influence) = world.get::<InfluenceArea>(area) else {
            continue;
        };
        let origin = DVec2::from_array(influence.center) - DVec2::from_array(influence.size) * 0.5;
        let group = index / config.columns;
        let height = heights.get(&(area, group)).copied().unwrap_or(160.0);
        let y = (0..group)
            .map(|index| heights.get(&(area, index)).copied().unwrap_or(0.0) + config.gap)
            .sum::<f32>();
        let position = origin
            + DVec2::new(
                f64::from(
                    12.0 + (index % config.columns) as f32 * (config.width + config.gap)
                        + config.width / 2.0,
                ),
                f64::from(12.0 + y + height / 2.0),
            );
        let size = Vec2::new(config.width, height);
        place(world, entity, position, size);
    }
}

pub(super) fn place(world: &mut World, entity: Entity, position: DVec2, size: Vec2) {
    if let Some(row) = world.get::<Row>(entity) {
        let owner = row.area;
        let config = row.config.clone();
        if world.get::<super::placement::Placed>(entity).is_none() {
            let area = world.get::<InfluenceArea>(owner).unwrap();
            let center = DVec2::from_array(area.center);
            let spatial = crate::topology::spatial(world, owner);
            let offset = position - center;
            let fallback = spatial.position(center)
                + spatial.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
            let Some(point) =
                super::placement::initial(world, entity, owner, &config, fallback, size)
            else {
                return;
            };
            super::placement::finish(world, entity, point);
            world.entity_mut(entity).insert(LastLayout(fallback));
            if config.group_with_source {
                let mut placement = crate::topology::spatial(world, entity);
                placement.rotation = spatial.rotation;
                world.entity_mut(entity).insert(placement);
                let group = world
                    .get::<crate::canvas_selection::SandGroup>(owner)
                    .copied()
                    .unwrap_or_else(|| {
                        let mut id = [0; 16];
                        getrandom::fill(&mut id).expect("group identity");
                        crate::canvas_selection::SandGroup(id)
                    });
                world.entity_mut(owner).insert(group);
                world.entity_mut(entity).insert(group);
                let members = crate::topology::groups::members(world, owner);
                crate::topology::groups::attach(world, &members);
            }
        }
        if config.placement != super::SpawnPlacement::Source
            && world.get::<crate::layout::LayoutBox>(entity).is_none()
        {
            if let Some(mut item) = world.get_mut::<CanvasItem>(entity) {
                item.size = size;
            }
            return;
        }
    }
    if let Some(mut layout) = world.get_mut::<crate::layout::LayoutBox>(entity) {
        if layout.rules.axes[1].sizing == crate::layout::Sizing::Fit {
            let axis = &mut layout.rules.axes[1];
            axis.min = size.y.max(1.0).min(axis.max);
            axis.size = axis.size.clamp(axis.min, axis.max);
        }
        return;
    }
    let pinned = world.get::<crate::sand_placement::Pinned>(entity).is_some();
    let owner = world.get::<Row>(entity).map(|row| row.area);
    let owner_placement = owner
        .map(|owner| crate::topology::spatial(world, owner))
        .unwrap_or_default();
    let center = owner
        .and_then(|owner| world.get::<InfluenceArea>(owner))
        .map_or(DVec2::ZERO, |area| DVec2::from_array(area.center));
    let offset = position - center;
    let position = owner_placement.position(center)
        + owner_placement.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
    let previous = world.get::<LastLayout>(entity).map(|last| last.0);
    let current = crate::topology::position(world, entity).unwrap_or_default();
    let grouped = world
        .get::<crate::canvas_selection::SandGroup>(entity)
        .is_some();
    let Some(mut item) = world.get_mut::<CanvasItem>(entity) else {
        return;
    };
    if item.size != size {
        item.size = size;
    }
    let next = previous.map_or(position, |previous| current + position - previous);
    if !pinned && !grouped && current != next {
        crate::topology::set_position(world, entity, next);
    }
    if previous.is_none() && !grouped {
        let mut placement = crate::topology::spatial(world, entity);
        placement.rotation = owner_placement.rotation;
        world.entity_mut(entity).insert(placement);
    }
    if previous != Some(position) {
        world.entity_mut(entity).insert(LastLayout(position));
    }
}
