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
struct LastLayout(DVec2);

#[derive(Component)]
struct EditorSize {
    mode: OverflowMode,
    minimum: Vec2,
    maximum_width: f32,
    observed: String,
    measuring: bool,
}

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
    focused: bool,
    conflict: bool,
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

fn scroll(
    mut event: On<Pointer<bevy::picking::events::Scroll>>,
    mut scrolls: Query<&mut ScrollPosition>,
) {
    if let Ok(mut position) = scrolls.get_mut(event.entity) {
        let multiplier = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
            24.0
        } else {
            1.0
        };
        position.0.x = (position.0.x
            - (event.x + if event.x == 0.0 { event.y } else { 0.0 }) * multiplier)
            .max(0.0);
        position.0.y = (position.0.y - event.y * multiplier).max(0.0);
        event.propagate(false);
    }
}

pub(super) fn content(
    world: &mut World,
    row: Entity,
    config: &Config,
    data: &Value,
    binding: Option<RecordBinding>,
) {
    for property in &config.bindings {
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
                    width: if grow_x { Val::Auto } else { px(width) },
                    min_width: if grow_x { px(width) } else { px(0) },
                    max_width: px((config.width - 24.0).max(24.0)),
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
                ChildOf(row),
                Tooltip(
                    protein::record_schema::fields()
                        .iter()
                        .find(|f| f.key == property.property)
                        .map_or(property.property.clone(), |f| f.title.into()),
                ),
            ))
            .observe(scroll)
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
        if editable
            && matches!(
                property.property.as_str(),
                "assertions" | "assignees" | "work_logs"
            )
        {
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
        let text_entity = if editable {
            let bundle =
                crate::sand::text_editor(&text, world.resource::<crate::theme::Typography>(), 0);
            let entity = world.spawn(bundle).id();
            let mut editor = world.get_mut::<EditableText>(entity).unwrap();
            editor.max_characters = Some(65_536);
            editor.visible_lines = None;
            world.entity_mut(entity).insert((
                PropertyEditor { property: property.property.clone(), observed: text, baseline: baseline(data, &property.property), pending: None, focused: false, conflict: false },
                binding.clone().unwrap(), Tooltip("Edit this Record property. Leaving the field saves it through its Organ; Escape restores the current value.".into()),
            ));
            entity
        } else {
            let font = world.resource::<crate::theme::Typography>().text(18.0);
            world
                .spawn((Text::new(text), font, crate::token_style::text(Token::Ink)))
                .id()
        };
        world.entity_mut(text_entity).insert((
            Node {
                width: if horizontal && !editable {
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
                max_width: if horizontal && !grow_x {
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
        if editable {
            let save = world
                .spawn((
                    Square,
                    ActionButton::new(text_entity, crate::actions![SaveField(text_entity)]),
                    IconButton::new(Icon::Save, "Save this Record property"),
                    ChildOf(row),
                ))
                .id();
            let _ = save;
        }
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
struct SaveField(Entity);
impl Action for SaveField {
    fn apply(&self, world: &mut World, _: Entity) {
        save_field(world, self.0);
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
    let config = world.get::<Row>(row).unwrap().config.clone();
    let children: Vec<_> = world
        .get::<Children>(row)
        .into_iter()
        .flatten()
        .copied()
        .filter(|child| world.get::<ScrollPosition>(*child).is_some())
        .collect();
    for (container, property) in children.into_iter().zip(&config.bindings) {
        if property_actions::refresh(world, container, data) {
            continue;
        }
        let text = display(&data[&property.property]);
        for entity in descendants(world, container) {
            if let Some(editor) = world.get::<PropertyEditor>(entity) {
                let dirty = world
                    .get::<EditableText>(entity)
                    .is_some_and(|value| value.value().to_string() != editor.observed);
                if editor.pending.is_some() || dirty {
                    if editor.pending.is_none()
                        && data[&property.property] != editor.baseline[&property.property]
                    {
                        world.get_mut::<PropertyEditor>(entity).unwrap().conflict = true;
                    }
                    continue;
                }
                world
                    .get_mut::<EditableText>(entity)
                    .unwrap()
                    .editor
                    .set_text(&text);
                let mut editor = world.get_mut::<PropertyEditor>(entity).unwrap();
                editor.observed = text.clone();
                editor.baseline = baseline(data, &property.property);
            } else if let Some(mut value) = world.get_mut::<Text>(entity) {
                value.0 = text.clone();
            }
        }
    }
}

pub(super) fn action_finished(world: &mut World, entity: Entity, error: Option<String>) {
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
        editor.conflict = false;
    }
}

pub(super) fn save_field(world: &mut World, entity: Entity) {
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
    if value == editor.observed {
        return;
    }
    if editor.conflict {
        status(
            world,
            binding.area,
            "This property changed elsewhere. Escape reloads its current value.",
        );
        return;
    }
    let target = binding.uid.clone();
    let action = match editor.property.as_str() {
        "head" => engine::actions::Action::EditRecordText {
            target,
            head: Some(value),
            body: None,
        },
        "body" => engine::actions::Action::EditRecordText {
            target,
            head: None,
            body: Some(value),
        },
        "slug" => engine::actions::Action::SetSlug {
            target,
            slug: (!value.trim().is_empty()).then_some(value),
        },
        "quantity_exact" => engine::actions::Action::SetQuantityExact {
            target,
            amount: value,
        },
        "start_date" | "due_date" | "estimate_min" => {
            let latest = world
                .resource::<Runtime>()
                .areas
                .get(&binding.area)
                .and_then(|state| {
                    state
                        .data
                        .iter()
                        .find(|row| row["uid"].as_str() == Some(&binding.uid))
                });
            let mut fds = latest.map_or_else(
                || editor.baseline["extension"].clone(),
                |row| row["extension"].clone(),
            );
            if !fds.is_object() {
                fds = serde_json::json!({});
            }
            let key = match editor.property.as_str() {
                "start_date" => "start",
                "due_date" => "due",
                _ => "estimate_min",
            };
            fds[key] = if value.trim().is_empty() {
                Value::Null
            } else if key == "estimate_min" {
                let Ok(number) = value.parse::<f64>() else {
                    status(world, binding.area, "Estimate must be a number");
                    return;
                };
                if !number.is_finite() {
                    status(world, binding.area, "Estimate must be finite");
                    return;
                }
                serde_json::json!(number)
            } else {
                Value::String(value)
            };
            if let Err(error) = engine::private_work::WorkMetadata::parse(&fds) {
                status(world, binding.area, error.to_string());
                return;
            }
            engine::actions::Action::SetExtension {
                target,
                namespace: "work".into(),
                fds,
            }
        }
        _ => return,
    };
    match execute(world, &binding, entity, action) {
        Ok(()) => {
            world.get_mut::<PropertyEditor>(entity).unwrap().pending = Some(submitted);
        }
        Err(error) => status(world, binding.area, error),
    }
}

pub(super) fn commit_edits(world: &mut World) {
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|focus| focus.get());
    let escape = world
        .get_resource::<ButtonInput<KeyCode>>()
        .is_some_and(|keys| keys.just_pressed(KeyCode::Escape));
    let editors: Vec<_> = world
        .query::<(Entity, &PropertyEditor)>()
        .iter(world)
        .map(|(entity, editor)| (entity, editor.focused))
        .collect();
    for (entity, focused) in editors {
        if escape && focus == Some(entity) {
            if let Some(binding) = world.get::<RecordBinding>(entity).cloned() {
                let data = world
                    .resource::<Runtime>()
                    .areas
                    .get(&binding.area)
                    .and_then(|state| {
                        state
                            .data
                            .iter()
                            .find(|row| row["uid"].as_str() == Some(&binding.uid))
                    })
                    .cloned();
                if let Some(data) = data {
                    let property = world
                        .get::<PropertyEditor>(entity)
                        .unwrap()
                        .property
                        .clone();
                    let value = display(&data[&property]);
                    world
                        .get_mut::<EditableText>(entity)
                        .unwrap()
                        .editor
                        .set_text(&value);
                    let mut editor = world.get_mut::<PropertyEditor>(entity).unwrap();
                    editor.observed = value;
                    editor.baseline = baseline(&data, &property);
                    editor.conflict = false;
                }
            }
        } else if focused && focus != Some(entity) {
            save_field(world, entity);
        }
        world.get_mut::<PropertyEditor>(entity).unwrap().focused = focus == Some(entity);
    }
}

pub(super) fn layout(world: &mut World) {
    let mut resized = false;
    for (text, layout, computed, mut size, mut node, mut wrapping) in world
        .query::<(
            &EditableText,
            &bevy::text::TextLayoutInfo,
            &ComputedNode,
            &mut EditorSize,
            &mut Node,
            &mut TextLayout,
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
        let height = measured.y.ceil().max(size.minimum.y);
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
        let count = world
            .get::<Children>(*entity)
            .map_or(0, |children| children.len());
        let height = world
            .get::<Children>(*entity)
            .into_iter()
            .flatten()
            .filter_map(|child| world.get::<ComputedNode>(*child))
            .map(|node| node.size().y * node.inverse_scale_factor())
            .sum::<f32>()
            + 24.0
            + (count.saturating_sub(1) as f32 * 8.0);
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
    let pinned = world.get::<crate::sand_placement::Pinned>(entity).is_some();
    let previous = world.get::<LastLayout>(entity).map(|last| last.0);
    let Some(mut item) = world.get_mut::<CanvasItem>(entity) else {
        return;
    };
    if item.size != size {
        item.size = size;
    }
    let next = previous.map_or(position, |previous| item.position + position - previous);
    if !pinned && item.position != next {
        item.position = next;
    }
    if previous != Some(position) {
        world.entity_mut(entity).insert(LastLayout(position));
    }
}
