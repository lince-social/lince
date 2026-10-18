use super::*;
use crate::native::{
    actions::{Action, ActionButton},
    canvas::CanvasItem,
    icons::{Icon, IconButton, Tooltip},
    sand::{InBox, Square},
    tokens::Token,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, text::EditableText};
use lince_interface::records::baseline;
pub(super) use lince_interface::records::display;

#[derive(Component)]
pub(super) struct Row {
    pub(super) area: Entity,
    pub(super) data: Value,
    pub(super) config: std::sync::Arc<Config>,
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

#[derive(Component)]
pub(super) struct PropertyLabel;

#[derive(Component)]
#[require(crate::native::sand::Unsaved(false))]
pub(super) struct PropertyEditor {
    property: String,
    observed: String,
    baseline: Value,
    pending: Option<String>,
    attempted: Option<String>,
}

pub(super) fn authoring_draft(world: &World, entity: Entity) -> bool {
    world
        .get::<PropertyEditor>(entity)
        .zip(world.get::<EditableText>(entity))
        .is_some_and(|(editor, text)| {
            editor.pending.is_some() || text.value().to_string() != editor.observed
        })
}

pub(super) fn attach_scalar(
    world: &mut World,
    entity: Entity,
    binding: RecordBinding,
    property: &str,
    data: &Value,
) {
    if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) {
        world
            .entity_mut(parent)
            .insert(PropertyContainer(property.into()));
    }
    world.entity_mut(entity).insert((
        PropertyEditor {
            property: property.into(),
            observed: display(&data[property]),
            baseline: baseline(data, property),
            pending: None,
            attempted: None,
        },
        binding,
    ));
    super::history::attach_text(world, entity);
}

pub(super) fn save_indicators(
    mut fields: Query<(
        &PropertyEditor,
        &EditableText,
        Option<&crate::native::record_binding::TextBinding>,
        &mut crate::native::sand::Unsaved,
    )>,
) {
    for (editor, text, binding, mut unsaved) in &mut fields {
        let value = text.value().to_string();
        let dirty = binding.map_or_else(
            || editor.pending.is_some() || value != editor.observed,
            |binding| binding.unsaved(&value),
        );
        unsaved.set_if_neq(crate::native::sand::Unsaved(dirty));
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
    if config.record_cards && (config.command.is_some() || data["kind"] == "command") {
        if let Some(binding) = binding {
            crate::native::command_castle::populate(world, row, config, data, binding);
        }
        return;
    }
    if config.fiote {
        if let Some(binding) = binding {
            crate::native::fiote::session::populate(world, row, binding);
        }
        return;
    }
    crate::native::record_presentation::restore(world, row, config.hide_filled);
    crate::native::shader_effects::record(world, row, config, data, binding.clone());
    let sections = config
        .record_cards
        .then(|| super::record_layout::create(world, row));
    if config.record_cards {
        world
            .entity_mut(row)
            .insert(crate::native::full_record::RecordCard);
    }
    fields(world, row, config, data, binding.clone(), sections.as_ref());
    let actions = super::record_layout::action_parent(world, row);
    if config.record_cards
        && let Some(binding) = &binding
        && matches!(binding.source, Source::Local)
    {
        crate::native::location::record_controls(world, actions, &binding.uid, None, None);
        crate::native::trust_castle::record_controls(world, actions, binding, data["kind"].as_str().unwrap_or_default());
    }
    if config.record_cards
        && let Some(binding) = binding.clone()
        && matches!(binding.source, Source::Local)
    {
        let button = world
            .spawn((
                Square,
                ActionButton::new(
                    row,
                    crate::native::actions![crate::native::full_record::Simulate(binding)],
                ),
                ChildOf(actions),
            ))
            .id();
        crate::native::edit_mode::label(world, button, "Simulate", 14.0);
    }
    if !config.record_cards
        && let Some(binding) = binding.clone()
    {
        let button = world
            .spawn((
                Square,
                ActionButton::new(
                    row,
                    crate::native::actions![crate::native::full_record::Open(binding)],
                ),
                ChildOf(row),
            ))
            .id();
        crate::native::edit_mode::label(world, button, "Open Record", 14.0);
    }
    super::presentation::controls(world, row);
    if config.record_cards
        && let Some(binding) = binding.clone()
    {
        crate::native::sand_panel::button(
            world,
            actions,
            row,
            "WGSL effect…",
            crate::native::shader_effects::OpenRecord(binding.clone()),
        );
        crate::native::sand_panel::button(
            world,
            actions,
            row,
            "Extensions…",
            crate::native::record_extensions::Open(binding),
        );
    }
    if config.delete_button {
        if let Some(binding) = binding {
            world.spawn((
                Square,
                ActionButton::new(row, crate::native::actions![Delete(binding.clone())]),
                IconButton::new(Icon::Delete, "Delete this Record"),
                ChildOf(actions),
            ));
        }
    }
    if let Some(sections) = sections {
        super::record_layout::arrange(world, row, &sections, data);
    }
}

pub(super) fn fields(
    world: &mut World,
    row: Entity,
    config: &Config,
    data: &Value,
    binding: Option<RecordBinding>,
    sections: Option<&super::record_layout::Sections>,
) {
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
                    height: if config.record_cards || grow_y || grow_x {
                        Val::Auto
                    } else {
                        px(property.height)
                    },
                    min_height: px(if config.record_cards {
                        0.0
                    } else {
                        property.height
                    }),
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
                crate::native::canvas_host::authoring::PartName(format!(
                    "property-{}",
                    property.property
                )),
                ChildOf(parent),
                Tooltip(
                    protein::record_schema::fields()
                        .iter()
                        .find(|f| f.key == property.property)
                        .map_or(property.property.clone(), |f| f.title.into()),
                ),
            ))
            .id();
        if property.square && !(config.record_cards && property.property == "head") {
            world.entity_mut(container).insert((
                Square,
                crate::native::token_style::background(Token::Surface),
            ));
        }
        if let Some(binding) = binding.clone() {
            world.entity_mut(container).insert(binding).observe(click);
        }
        let text = display(&data[&property.property]);
        let editable = property.editable && binding.is_some();
        if let Some((schema, field)) = nucleus::record_extension::column_binding(&property.property)
        {
            if let Some(binding) = binding.clone() {
                crate::native::record_extensions::field(
                    world, container, binding, schema, field, editable,
                );
            }
            continue;
        }
        if config.show_labels
            && !(config.record_cards && matches!(property.property.as_str(), "head" | "body"))
            && !matches!(
                property.property.as_str(),
                "threads" | "assertions" | "work_logs"
            )
        {
            let label = protein::record_schema::fields()
                .into_iter()
                .find(|field| field.key == property.property)
                .map_or(property.property.clone(), |field| field.title.into());
            let label = crate::native::edit_mode::label(world, container, &label, 13.0);
            world
                .entity_mut(label)
                .insert((crate::native::record_binding::BindingStatus, PropertyLabel));
        }
        if let Some(binding) = binding.clone() {
            if property.property == "work_timer" {
                crate::native::work_timer::populate(world, container, Some(binding), data, None);
                continue;
            }
            if property.property == "threads" {
                crate::native::thread_castle::populate(world, container, binding, data);
                continue;
            }
        }
        if editable && property.property == "assignees" {
            super::assignees::field(world, container, binding.clone().unwrap(), data);
            continue;
        }
        if editable && property.property == "assertions" {
            crate::native::assertion_editor::spawn(
                world,
                container,
                binding.clone().unwrap(),
                data,
                false,
            );
            continue;
        }
        if editable && property.property == "work_logs" {
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
            crate::native::description::readonly(
                world,
                container,
                &text,
                crate::native::description::Context {
                    owner: row,
                    source: binding
                        .as_ref()
                        .map_or(Source::Local, |binding| binding.source.clone()),
                },
            );
            continue;
        }
        let text_entity = if editable {
            let bundle = crate::native::sand::text_editor(
                &text,
                world.resource::<crate::native::theme::Typography>(),
                0,
            );
            let entity = world.spawn(bundle).id();
            let mut editor = world.get_mut::<EditableText>(entity).unwrap();
            editor.max_characters = Some(65_536);
            editor.visible_lines =
                if config.record_cards && !matches!(property.property.as_str(), "head" | "body") {
                    Some(1.0)
                } else {
                    None
                };
            editor.allow_newlines = matches!(property.property.as_str(), "head" | "body");
            world.entity_mut(entity).insert((
                PropertyEditor {
                    property: property.property.clone(),
                    observed: text,
                    baseline: baseline(data, &property.property),
                    pending: None,
                    attempted: None,
                },
                binding.clone().unwrap(),
            ));
            if matches!(property.property.as_str(), "head" | "body")
                && crate::native::record_binding::enabled(world)
            {
                let status = (!config.record_cards).then(|| {
                    crate::native::edit_mode::label(world, container, "Opening Record…", 12.0)
                });
                crate::native::record_binding::attach(
                    world,
                    entity,
                    binding.clone().unwrap(),
                    &property.property,
                    status,
                );
            }
            if !matches!(property.property.as_str(), "head" | "body")
                || !crate::native::record_binding::enabled(world)
            {
                super::history::attach_text(world, entity);
            }
            entity
        } else {
            let font = world
                .resource::<crate::native::theme::Typography>()
                .text(18.0);
            world
                .spawn((
                    Text::new(text),
                    font,
                    crate::native::token_style::text(Token::Ink),
                ))
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
                    px(if config.record_cards {
                        32.0
                    } else {
                        (property.height - 8.0).max(24.0)
                    })
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
            TextLayout::linebreak(
                if (horizontal && !grow_x)
                    || (config.record_cards
                        && !matches!(property.property.as_str(), "head" | "body"))
                {
                    bevy::text::LineBreak::NoWrap
                } else {
                    bevy::text::LineBreak::WordBoundary
                },
            ),
            ChildOf(container),
        ));
        if config.record_cards && property.property == "head" {
            world
                .entity_mut(text_entity)
                .insert(crate::native::sand::Borderless);
            world.get_mut::<Node>(text_entity).unwrap().border = UiRect::all(px(0));
        }
        if config.record_cards && !matches!(property.property.as_str(), "head" | "body") {
            let font = world
                .resource::<crate::native::theme::Typography>()
                .text(16.0);
            world.entity_mut(text_entity).insert(font);
            world.get_mut::<Node>(text_entity).unwrap().padding = UiRect::all(px(4));
        }
        if editable && property.overflow != OverflowMode::Clip {
            world.entity_mut(text_entity).insert(EditorSize {
                mode: property.overflow,
                minimum: if config.record_cards {
                    Vec2::new(48.0, 32.0)
                } else {
                    Vec2::new((width - 8.0).max(16.0), (property.height - 8.0).max(24.0))
                },
                maximum_width: (config.width - 32.0).max(16.0),
                observed: String::new(),
                measuring: false,
            });
        }
        if property.property == "body" && editable {
            world.entity_mut(text_entity).remove::<EditorSize>();
            let font = world
                .resource::<crate::native::theme::Typography>()
                .text(14.0);
            world.entity_mut(text_entity).insert(font);
            crate::native::description::attach_editor(
                world,
                container,
                text_entity,
                crate::native::description::Context {
                    owner: row,
                    source: binding.as_ref().unwrap().source.clone(),
                },
            );
            let shader_castle = binding.as_ref().is_some_and(|binding| {
                world
                    .get::<crate::native::shader_castle::ShaderFeed>(binding.area)
                    .is_some()
            });
            if shader_castle {
                world.get_mut::<Node>(text_entity).unwrap().height = px(480);
            } else if config.record_cards {
                crate::native::description::set_mode(
                    world,
                    container,
                    crate::native::description::Mode::Pretty,
                );
                world.get_mut::<Node>(text_entity).unwrap().height = px(280);
            }
        }
        if editable && !matches!(property.property.as_str(), "head" | "body") {
            if matches!(property.property.as_str(), "start_date" | "due_date") {
                let controls = world
                    .spawn((
                        Node {
                            width: percent(100),
                            min_width: px(0),
                            column_gap: px(4),
                            align_items: AlignItems::Center,
                            flex_shrink: 0.0,
                            ..default()
                        },
                        ChildOf(container),
                    ))
                    .id();
                world.entity_mut(text_entity).insert(ChildOf(controls));
                {
                    let mut node = world.get_mut::<Node>(text_entity).unwrap();
                    node.width = px(0);
                    node.flex_grow = 1.0;
                    node.flex_shrink = 1.0;
                    node.min_width = px(48);
                }
                let button = crate::native::calendar::date_button(
                    world,
                    row,
                    text_entity,
                    &property.property,
                );
                world.entity_mut(button).insert(ChildOf(controls));
                crate::native::schedule_editor::attach(world, controls, text_entity);
            }
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
    if world.get::<filter::Subscription>(owner).is_some()
        || world
            .get::<crate::native::assertion_castle::AssertionFeed>(owner)
            .is_some()
    {
        return;
    }
    let Some(mut state) = world.resource_mut::<Runtime>().areas.remove(&owner) else {
        return;
    };
    if !state.dirty {
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        return;
    }
    crate::native::record_presentation::remember(world, owner);
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
                        crate::native::castle::Castle,
                        Square,
                        InBox(parent),
                        ChildOf(parent),
                        WorkspaceMember(workspace),
                        crate::native::sand_store::SandCredits(
                            crate::native::credits::ATTRIBUTIONS,
                        ),
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
                        crate::native::token_style::background(Token::Surface),
                        binding.clone(),
                        super::placement::Pending,
                        Visibility::Hidden,
                    ))
                    .observe(click)
                    .id();
                if config.source != Source::Local {
                    world.entity_mut(entity).insert(RemoteRecord);
                }
                if crate::native::practice_cells::owns_source(world, &config.source) {
                    world
                        .entity_mut(entity)
                        .insert(crate::native::practice_cells::PracticeRecord);
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
                .is_none_or(|row| !row.config.same_template(&config))
                || state.template_dirty;
            if template_changed {
                super::presentation::rebuilding(world, entity);
                if let Some(children) = world.get::<Children>(entity) {
                    let children: Vec<_> = children.iter().collect();
                    for child in children {
                        world.despawn(child);
                    }
                }
                if config.viewport_height.is_some() || config.max_height.is_some() {
                    crate::native::scroll_sand::attach(world, entity);
                }
                content(world, entity, &config, &data, Some(binding));
            } else {
                refresh(world, entity, &data);
            }
            let properties = data.clone();
            world.entity_mut(entity).insert((
                Row {
                    area: owner,
                    data: data.clone(),
                    config: shared_config.clone(),
                    index,
                },
                crate::native::area::RecordProperties(properties),
            ));
        }
    }
    state.dirty = false;
    state.template_dirty = false;
    crate::native::relation_castle::reconcile(
        world,
        owner,
        &config,
        &state.data,
        &state.row_entities,
    );
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

pub(super) fn drafts(world: &World, row: Entity) -> Vec<super::presentation::Draft> {
    let uid = world
        .get::<RecordBinding>(row)
        .map(|binding| binding.uid.clone())
        .unwrap_or_default();
    descendants(world, row)
        .into_iter()
        .filter_map(|entity| {
            let text = world.get::<EditableText>(entity)?;
            let mut cursor = Some(entity);
            let mut property = None;
            while let Some(entity) = cursor {
                if let Some(container) = world.get::<PropertyContainer>(entity) {
                    property = Some(container.0.clone());
                    break;
                }
                cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            let value = text.value().to_string();
            let dirty = world
                .get::<crate::native::record_binding::TextBinding>(entity)
                .map(|binding| binding.unsaved(&value))
                .or_else(|| {
                    world
                        .get::<PropertyEditor>(entity)
                        .map(|editor| editor.pending.is_some() || value != editor.observed)
                })
                .unwrap_or_else(|| {
                    world
                        .get::<crate::native::sand::Unsaved>(entity)
                        .is_some_and(|state| state.0)
                });
            Some(super::presentation::Draft {
                uid: uid.clone(),
                field: property?,
                text: value,
                dirty,
                busy: text.is_composing() || text.pending_paste.is_some(),
            })
        })
        .collect()
}

fn refresh(world: &mut World, row: Entity, data: &Value) {
    crate::native::shader_effects::refresh_record(world, row, data);
    let children: Vec<_> = descendants(world, row)
        .into_iter()
        .filter_map(|child| {
            world
                .get::<PropertyContainer>(child)
                .map(|property| (child, property.0.clone()))
        })
        .collect();
    for (container, property) in children {
        if nucleus::record_extension::column_binding(&property).is_some() {
            continue;
        }
        if crate::native::work_timer::refresh(world, container, data)
            || crate::native::thread_castle::refresh(world, container, data)
        {
            continue;
        }
        if super::assignees::refresh(world, container, data)
            || crate::native::assertion_editor::refresh(world, container, data)
            || property_actions::refresh(world, container, data)
        {
            continue;
        }
        let text = display(&data[&property]);
        if property == "body" {
            crate::native::description::refresh_readonly(world, container, &text);
        }
        for entity in descendants(world, container) {
            if crate::native::description::protects(world, entity) {
                continue;
            }
            if world
                .get::<crate::native::record_binding::BindingStatus>(entity)
                .is_some()
                || world.get::<PropertyLabel>(entity).is_some()
            {
                continue;
            }
            if crate::native::record_binding::active(world, entity) {
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
    crate::native::save_feedback::set_failed(world, entity, error.is_some());
    if crate::native::assertion_editor::finished(world, entity, error.clone()) {
        return;
    }
    if crate::native::assertion_castle::finished(world, entity, error.clone()) {
        return;
    }
    if super::assignees::finished(world, entity, error.clone()) {
        return;
    }
    if crate::native::thread_castle::finished(world, entity, error.clone()) {
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
        .get::<crate::native::record_binding::TextBinding>(entity)
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
        "quantity" => engine::record_change::Mutation::Quantity { value },
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
                    crate::native::save_feedback::set_failed(world, entity, true);
                    status(world, binding.area, "Estimate must be a number");
                    return;
                };
                if !number.is_finite() {
                    crate::native::save_feedback::set_failed(world, entity, true);
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
    super::presentation::update(world);
}

pub(super) fn layout(world: &mut World) {
    super::placement::begin_frame(world);
    let mut resized = false;
    for (text, layout, computed, mut size, mut node, mut wrapping, scroll, settings) in world
        .query::<(
            &EditableText,
            &bevy::text::TextLayoutInfo,
            &ComputedNode,
            &mut EditorSize,
            &mut Node,
            &mut TextLayout,
            Option<&mut bevy::ui::widget::TextScroll>,
            Option<&super::presentation::EditorSettings>,
        )>()
        .iter_mut(world)
    {
        let settings = settings.map(|settings| &settings.0.0);
        let fixed_width = settings.is_some_and(|settings| settings.contains_key("width"));
        let fixed_height = settings.is_some_and(|settings| settings.contains_key("height"));
        let fixed_wrap = settings.is_some_and(|settings| settings.contains_key("wrap"));
        if size.mode == OverflowMode::GrowRight && !fixed_width && !fixed_wrap {
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
        if !fixed_height && height.is_finite() && node.height != px(height) {
            node.height = px(height);
            resized = true;
        }
        if !fixed_width && (size.mode == OverflowMode::ScrollRight || size.measuring) {
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
            if !fixed_wrap {
                wrapping.linebreak = bevy::text::LineBreak::WordBoundary;
            }
            resized = true;
        }
    }
    if resized {
        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
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
                world.get::<Node>(**child).is_some_and(|node| {
                    node.display != Display::None && node.position_type != PositionType::Absolute
                })
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
        let height = config
            .viewport_height
            .unwrap_or(height)
            .min(config.max_height.unwrap_or(f32::MAX));
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
        let inset = if config.group_with_source { 0.0 } else { 12.0 };
        let position = if config.group_with_source && index == 0 {
            DVec2::from_array(influence.center)
        } else {
            origin
                + DVec2::new(
                    f64::from(
                        inset
                            + (index % config.columns) as f32 * (config.width + config.gap)
                            + config.width / 2.0,
                    ),
                    f64::from(inset + y + height / 2.0),
                )
        };
        let size = Vec2::new(config.width, height);
        place(world, entity, position, size);
        if config.group_with_source && index == 0 {
            crate::native::full_record::fit_source(world, area, entity);
        }
    }
}

pub(super) fn place(world: &mut World, entity: Entity, position: DVec2, size: Vec2) {
    if crate::native::castle_feed::place(world, entity, size) {
        return;
    }
    if world
        .get::<crate::native::topology::rooms::Member>(entity)
        .is_some()
        || world
            .get::<crate::native::topology::rooms::WorldPlacement>(entity)
            .is_some()
    {
        if let Some(row) = world.get::<Row>(entity) {
            let owner = row.area;
            if let Some(area) = world.get::<InfluenceArea>(owner) {
                let center = DVec2::from_array(area.center);
                let spatial = crate::native::topology::spatial(world, owner);
                let offset = position - center;
                let target = spatial.position(center)
                    + spatial.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
                world.entity_mut(entity).insert(LastLayout(target));
            }
        }
        if let Some(mut item) = world.get_mut::<CanvasItem>(entity) {
            item.size = size;
        }
        if let Some(mut layout) = world.get_mut::<crate::native::layout::LayoutBox>(entity)
            && layout.rules.axes[1].sizing == crate::native::layout::Sizing::Fit
        {
            let axis = &mut layout.rules.axes[1];
            axis.min = size.y.max(1.0).min(axis.max);
            axis.size = axis.size.clamp(axis.min, axis.max);
        }
        if let Some(point) = crate::native::topology::position(world, entity) {
            super::placement::finish(world, entity, point);
        }
        return;
    }
    if let Some(row) = world.get::<Row>(entity) {
        let owner = row.area;
        let config = row.config.clone();
        if world.get::<super::placement::Placed>(entity).is_none() {
            let area = world.get::<InfluenceArea>(owner).unwrap();
            let center = DVec2::from_array(area.center);
            let spatial = crate::native::topology::spatial(world, owner);
            let offset = position - center;
            let fallback = spatial.position(center)
                + spatial.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
            let fallback = if config.motion.is_some() {
                crate::native::record_presentation::position(world, entity).unwrap_or_else(|| {
                    let angle = row.index as f64 * 2.399963229728653;
                    let radius = (row.index as f64 + 1.0).sqrt() * f64::from(config.width);
                    spatial.position(center)
                        + spatial.rotation()
                            * bevy::math::DVec3::new(
                                angle.cos() * radius,
                                0.0,
                                angle.sin() * radius,
                            )
                })
            } else {
                fallback
            };
            let Some(point) =
                super::placement::initial(world, entity, owner, &config, fallback, size)
            else {
                return;
            };
            super::placement::finish(world, entity, point);
            if config.motion.is_some()
                || config.placement == super::SpawnPlacement::Source
                || world.get::<super::placement::RoomFrame>(entity).is_some()
            {
                let frame = room_frame(world, entity).unwrap_or(owner);
                let rotation = crate::native::topology::spatial(world, frame).rotation;
                let mut placement = crate::native::topology::spatial(world, entity);
                placement.rotation = rotation;
                world.entity_mut(entity).insert(placement);
            }
            world.entity_mut(entity).insert(LastLayout(fallback));
            if config.group_with_source {
                let mut placement = crate::native::topology::spatial(world, entity);
                placement.rotation = spatial.rotation;
                world.entity_mut(entity).insert(placement);
                let group = world
                    .get::<crate::native::canvas_selection::SandGroup>(owner)
                    .copied()
                    .unwrap_or_else(|| {
                        let mut id = [0; 16];
                        getrandom::fill(&mut id).expect("group identity");
                        crate::native::canvas_selection::SandGroup(id)
                    });
                world.entity_mut(owner).insert(group);
                world.entity_mut(entity).insert(group);
                let members = crate::native::topology::groups::members(world, owner);
                crate::native::topology::groups::attach(world, &members);
            }
        }
        if (config.placement != super::SpawnPlacement::Source || config.motion.is_some())
            && world
                .get::<crate::native::layout::LayoutBox>(entity)
                .is_none()
        {
            if let Some(mut item) = world.get_mut::<CanvasItem>(entity) {
                item.size = size;
            }
            return;
        }
    }
    if let Some(mut layout) = world.get_mut::<crate::native::layout::LayoutBox>(entity) {
        if layout.rules.axes[1].sizing == crate::native::layout::Sizing::Fit {
            let axis = &mut layout.rules.axes[1];
            axis.min = size.y.max(1.0).min(axis.max);
            axis.size = axis.size.clamp(axis.min, axis.max);
        }
        return;
    }
    let pinned = world
        .get::<crate::native::sand_placement::Pinned>(entity)
        .is_some();
    let owner = world.get::<Row>(entity).map(|row| row.area);
    let owner_placement = owner
        .map(|owner| crate::native::topology::spatial(world, owner))
        .unwrap_or_default();
    let center = owner
        .and_then(|owner| world.get::<InfluenceArea>(owner))
        .map_or(DVec2::ZERO, |area| DVec2::from_array(area.center));
    let offset = position - center;
    let position = owner_placement.position(center)
        + owner_placement.rotation() * bevy::math::DVec3::new(offset.x, 0.0, offset.y);
    let previous = world.get::<LastLayout>(entity).map(|last| last.0);
    let current = crate::native::topology::position(world, entity).unwrap_or_default();
    let grouped = world
        .get::<crate::native::canvas_selection::SandGroup>(entity)
        .is_some();
    let Some(mut item) = world.get_mut::<CanvasItem>(entity) else {
        return;
    };
    if item.size != size {
        item.size = size;
    }
    let next = previous.map_or(position, |previous| current + position - previous);
    if !pinned && !grouped && current != next {
        crate::native::topology::set_position(world, entity, next);
    }
    if previous.is_none() && !grouped {
        let mut placement = crate::native::topology::spatial(world, entity);
        placement.rotation = owner_placement.rotation;
        world.entity_mut(entity).insert(placement);
    }
    if previous != Some(position) {
        world.entity_mut(entity).insert(LastLayout(position));
    }
}

pub(super) fn room_frame(world: &World, entity: Entity) -> Option<Entity> {
    let binding = world.get::<RecordBinding>(entity)?;
    let config = world
        .get::<Row>(entity)
        .map(|row| row.config.as_ref())
        .or_else(|| world.get::<InfluenceArea>(binding.area)?.protein.as_ref())?;
    let owner = if config.motion.is_some() || config.placement == super::SpawnPlacement::Source {
        binding.area
    } else {
        world.get::<super::placement::RoomFrame>(entity)?.0
    };
    (world.get::<ChildOf>(entity) == world.get::<ChildOf>(owner)
        && world.get::<crate::native::workspace::WorkspaceMember>(entity)
            == world.get::<crate::native::workspace::WorkspaceMember>(owner))
    .then_some(owner)
}

pub(super) fn room_fixed(world: &World, entity: Entity) -> bool {
    world.get::<Row>(entity).is_some_and(|row| {
        row.config.motion.is_none() && row.config.placement == super::SpawnPlacement::Source
    })
}

pub(super) fn room_default(world: &mut World, entity: Entity) {
    let Some(row) = world.get::<Row>(entity) else {
        return;
    };
    let owner = row.area;
    let grouped = row.config.group_with_source;
    let matching = row.config.placement != super::SpawnPlacement::Source;
    if let Some(target) = world.get::<LastLayout>(entity).map(|target| target.0) {
        crate::native::topology::set_position(world, entity, target);
    }
    let rotation = crate::native::topology::spatial(world, owner).rotation;
    let mut placement = crate::native::topology::spatial(world, entity);
    placement.rotation = rotation;
    world.entity_mut(entity).insert(placement);
    if grouped {
        let group = world
            .get::<crate::native::canvas_selection::SandGroup>(owner)
            .copied()
            .unwrap_or_else(|| {
                let mut id = [0; 16];
                getrandom::fill(&mut id).expect("group identity");
                crate::native::canvas_selection::SandGroup(id)
            });
        world.entity_mut(owner).insert(group);
        world.entity_mut(entity).insert(group);
        let members = crate::native::topology::groups::members(world, owner);
        crate::native::topology::groups::attach(world, &members);
    }
    if matching {
        super::placement::restart(world, entity);
        layout(world);
    }
}

pub(super) fn room_transform(
    world: &mut World,
    owner: Entity,
    before: &nucleus::canvas::Pose,
    after: &nucleus::canvas::Pose,
) {
    use nucleus::canvas::room::{compose, relative};
    let rows: Vec<_> = world
        .query::<(Entity, &RecordBinding, &CanvasItem)>()
        .iter(world)
        .filter(|(entity, _, _)| room_frame(world, *entity) == Some(owner))
        .map(|(entity, _, _)| entity)
        .collect();
    for entity in rows {
        if let Some(last) = world.get::<LastLayout>(entity).map(|last| last.0) {
            let point = nucleus::canvas::Pose {
                position: last.to_array(),
                rotation: before.rotation,
            };
            let point = compose(after, &relative(before, &point));
            world
                .entity_mut(entity)
                .insert(LastLayout(bevy::math::DVec3::from_array(point.position)));
        }
        if world
            .get::<crate::native::topology::rooms::Member>(entity)
            .is_some()
            || world
                .get::<crate::native::topology::rooms::WorldPlacement>(entity)
                .is_some()
        {
            continue;
        }
        let grouped = crate::native::topology::rooms::group_moved(world, owner, entity);
        if !grouped && let Some(position) = crate::native::topology::position(world, entity) {
            let mut spatial = crate::native::topology::spatial(world, entity);
            let point = nucleus::canvas::Pose {
                position: position.to_array(),
                rotation: spatial.rotation,
            };
            let target = compose(after, &relative(before, &point));
            crate::native::topology::set_position(
                world,
                entity,
                bevy::math::DVec3::from_array(target.position),
            );
            spatial.rotation = target.rotation;
            spatial.elevation = target.position[1];
            world.entity_mut(entity).insert(spatial);
            if let Some(group) = world
                .get::<crate::native::topology::groups::GroupPose>(entity)
                .copied()
            {
                let point = nucleus::canvas::Pose {
                    position: group.position,
                    rotation: group.rotation,
                };
                let target = compose(after, &relative(before, &point));
                world
                    .entity_mut(entity)
                    .insert(crate::native::topology::groups::GroupPose {
                        position: target.position,
                        rotation: target.rotation,
                    });
            }
        }
        super::placement::room_transform(world, entity, before, after);
        crate::native::physics::room_transform(world, entity, before, after);
        crate::native::topology::physics::room_transform(world, entity, before, after);
    }
    crate::native::protein_motion::room_transform(world, owner, before, after);
}

#[cfg(test)]
mod room_tests {
    use super::*;
    use crate::native::{
        canvas_host::Identity,
        topology::{
            self,
            rooms::{self, Member, Room, WorldPlacement},
        },
        workspace::WorkspaceMember,
    };
    use bevy::math::{DQuat, DVec3};
    use nucleus::canvas::{
        Pose,
        room::{self as model, compose, relative},
    };

    fn pose(world: &World, entity: Entity) -> Pose {
        Pose {
            position: topology::position(world, entity).unwrap().to_array(),
            rotation: topology::spatial(world, entity).rotation,
        }
    }

    fn near(actual: &Pose, expected: &Pose) {
        assert!(
            (DVec3::from_array(actual.position) - DVec3::from_array(expected.position)).length()
                < 1e-6,
            "{actual:?} != {expected:?}"
        );
        assert!(
            DQuat::from_array(actual.rotation)
                .dot(DQuat::from_array(expected.rotation))
                .abs()
                > 1.0 - 1e-10
        );
    }

    #[test]
    fn room_record_defaults_reselect_the_current_matching_area() {
        let (mut app, root, owner) = crate::native::protein_area::tests::fixture();
        let world = app.world_mut();
        let target = crate::native::area::spawn_area(
            world,
            root,
            1,
            InfluenceArea::new(
                crate::native::area::AreaShape::Square,
                DVec2::new(2000.0, 500.0),
                DVec2::splat(800.0),
            ),
        )
        .unwrap();
        world.get_mut::<InfluenceArea>(target).unwrap().filter = Some(Config::default());
        world
            .entity_mut(target)
            .insert(super::super::filter::Matches {
                source: super::super::Source::Local,
                current: true,
                uids: std::collections::HashSet::from(["matching".into()]),
            });
        let mut config = Config::default();
        config.placement = super::super::SpawnPlacement::MatchingAreas;
        config.spawn_targets = vec![world.get::<InfluenceArea>(target).unwrap().id.clone()];
        world.get_mut::<InfluenceArea>(owner).unwrap().protein = Some(config.clone());
        world.resource_mut::<Runtime>().areas.insert(
            owner,
            super::super::State {
                applied: Some(config),
                data: vec![
                    serde_json::json!({"uid":"matching", "head":"Matching", "quantity":"2"}),
                ],
                ready: true,
                dirty: true,
                ..default()
            },
        );
        reconcile(world, owner);
        layout(world);
        let row = world.resource::<Runtime>().areas[&owner].row_entities["matching"];
        assert_eq!(
            topology::position(world, row),
            topology::position(world, target)
        );
        let detached = Pose {
            position: [-500.0, 30.0, -200.0],
            ..model::identity_pose()
        };
        world
            .entity_mut(row)
            .insert(WorldPlacement(detached.clone()));
        rooms::synchronize(world);
        near(&pose(world, row), &detached);
        topology::set_position(world, target, DVec3::new(4000.0, 80.0, 1000.0));
        let rotation = DQuat::from_rotation_y(0.4).to_array();
        world.entity_mut(target).insert(topology::Spatial {
            rotation,
            elevation: 80.0,
            ..default()
        });
        world.entity_mut(row).remove::<WorldPlacement>();
        room_default(world, row);
        near(&pose(world, row), &pose(world, target));
        assert_eq!(room_frame(world, row), Some(target));
    }

    #[test]
    fn room_producer_transforms_records_targets_and_overrides_across_snapshot_cache_resets() {
        let (mut app, root, owner) = crate::native::protein_area::tests::fixture();
        let world = app.world_mut();
        let config = world
            .get::<InfluenceArea>(owner)
            .unwrap()
            .protein
            .clone()
            .unwrap();
        world.resource_mut::<Runtime>().areas.insert(
            owner,
            super::super::State {
                applied: Some(config),
                data: vec![
                    serde_json::json!({"uid":"first", "head":"First", "quantity":"7"}),
                    serde_json::json!({"uid":"second", "head":"Second", "quantity":"3"}),
                ],
                ready: true,
                dirty: true,
                ..default()
            },
        );
        reconcile(world, owner);
        layout(world);
        let rows = ["first", "second"]
            .map(|uid| world.resource::<Runtime>().areas[&owner].row_entities[uid]);
        let room_id = nucleus::new_uid("placement");
        let room = world
            .spawn((
                CanvasItem {
                    position: DVec2::ZERO,
                    size: Vec2::splat(1000.0),
                },
                Room(default()),
                Identity(room_id.clone()),
                ChildOf(root),
                WorkspaceMember(1),
            ))
            .id();
        let owner_pose = pose(world, owner);
        world.entity_mut(owner).insert(Member(model::Member {
            room: room_id.clone(),
            parent: None,
            local: owner_pose,
            role: model::Role::Content,
        }));
        rooms::synchronize(world);
        let before_owner = pose(world, owner);
        let before = rows.map(|row| pose(world, row));
        let targets = rows.map(|row| world.get::<LastLayout>(row).unwrap().0);
        let moved = Pose {
            position: [1e9, 60.0, -1e9],
            rotation: DQuat::from_euler(EulerRot::XYZ, 0.2, 0.7, 0.1).to_array(),
        };
        topology::set_position(world, room, DVec3::from_array(moved.position));
        world.entity_mut(room).insert(topology::Spatial {
            rotation: moved.rotation,
            elevation: moved.position[1],
            ..default()
        });
        rooms::synchronize(world);
        let after_owner = pose(world, owner);
        for (index, row) in rows.iter().enumerate() {
            near(
                &pose(world, *row),
                &compose(&after_owner, &relative(&before_owner, &before[index])),
            );
            let target = compose(
                &after_owner,
                &relative(
                    &before_owner,
                    &Pose {
                        position: targets[index].to_array(),
                        ..model::identity_pose()
                    },
                ),
            );
            assert!(
                (world.get::<LastLayout>(*row).unwrap().0 - DVec3::from_array(target.position))
                    .length()
                    < 1e-6
            );
            assert!(rooms::fixed(world, *row));
        }
        let current = rows.map(|row| pose(world, row));
        rooms::synchronize(world);
        for (row, expected) in rows.iter().zip(&current) {
            near(&pose(world, *row), expected);
        }
        for entity in [owner, rows[0], rows[1]] {
            world
                .entity_mut(entity)
                .insert(crate::native::canvas_selection::SandGroup([7; 16]));
        }
        topology::groups::attach(world, &[owner, rows[0], rows[1]]);
        topology::groups::transform(
            world,
            owner,
            DVec3::new(20.0, 10.0, 30.0),
            DQuat::from_rotation_y(0.2),
        );
        let grouped = rows.map(|row| pose(world, row));
        rooms::synchronize(world);
        for (row, expected) in rows.iter().zip(&grouped) {
            near(&pose(world, *row), expected);
        }
        for entity in [owner, rows[0], rows[1]] {
            crate::native::canvas_selection::detach(world, entity);
        }
        let current = rows.map(|row| pose(world, row));
        let local = Pose {
            position: [10.0, 30.0, 50.0],
            ..model::identity_pose()
        };
        world.entity_mut(rows[0]).insert(Member(model::Member {
            room: room_id,
            parent: None,
            local: local.clone(),
            role: model::Role::Content,
        }));
        world
            .entity_mut(rows[1])
            .insert(WorldPlacement(current[1].clone()));
        rooms::synchronize(world);
        rooms::accept_snapshot(world, root);
        let moved_again = Pose {
            position: [1e9 + 100.0, 80.0, -1e9 + 50.0],
            rotation: DQuat::from_rotation_y(1.2).to_array(),
        };
        topology::set_position(world, room, DVec3::from_array(moved_again.position));
        world.entity_mut(room).insert(topology::Spatial {
            rotation: moved_again.rotation,
            elevation: moved_again.position[1],
            ..default()
        });
        rooms::synchronize(world);
        near(&pose(world, rows[0]), &compose(&moved_again, &local));
        near(&pose(world, rows[1]), &current[1]);
        let target = world.get::<LastLayout>(rows[0]).unwrap().0;
        assert!((target - targets[0]).length() > 1000.0);
        {
            let mut runtime = world.resource_mut::<Runtime>();
            let state = runtime.areas.get_mut(&owner).unwrap();
            state
                .data
                .push(serde_json::json!({"uid":"third", "head":"Third", "quantity":"9"}));
            state.dirty = true;
        }
        reconcile(world, owner);
        layout(world);
        let third = world.resource::<Runtime>().areas[&owner].row_entities["third"];
        assert_eq!(
            topology::spatial(world, third).rotation,
            topology::spatial(world, owner).rotation
        );
        near(&pose(world, rows[0]), &compose(&moved_again, &local));
        near(&pose(world, rows[1]), &current[1]);
        let saved = pose(world, rows[0]);
        world.despawn(room);
        rooms::synchronize(world);
        assert!(world.get::<Member>(rows[0]).is_none());
        assert_eq!(world.get::<WorldPlacement>(rows[0]).unwrap().0, saved);
    }
}
