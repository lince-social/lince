use super::*;
use crate::sand_settings::{
    Changed, Configuration, Control, Declaration, Definition, Value as Setting,
};

#[derive(Component, Clone)]
struct Field {
    row: Entity,
    property: String,
    node: Node,
    wrapping: Vec<(Entity, TextLayout)>,
    editors: Vec<(Entity, Node)>,
}

#[derive(Component)]
struct SettingsObserved;

#[derive(Component)]
struct PresentationLabel;

#[derive(Component, Clone)]
struct Original {
    nodes: Vec<(Entity, Node)>,
    parents: Vec<(Entity, Entity)>,
    host: Entity,
    stash: Entity,
    castle: bool,
    applied: Option<Presentation>,
}

#[derive(Component)]
struct Added(Entity);

pub(super) fn definitions(property: &str) -> Vec<Definition> {
    let binding = Binding::new(property);
    vec![
        Definition {
            id: "width".into(),
            label: "Field width".into(),
            default: Setting::Number(binding.width),
            control: Control::Number {
                min: 24.0,
                max: 4000.0,
                step: 1.0,
            },
        },
        Definition {
            id: "height".into(),
            label: "Field height".into(),
            default: Setting::Number(binding.height),
            control: Control::Number {
                min: 24.0,
                max: 4000.0,
                step: 1.0,
            },
        },
        Definition {
            id: "wrap".into(),
            label: "Wrap text".into(),
            default: Setting::Toggle(true),
            control: Control::Toggle,
        },
    ]
}

fn containers(world: &World, row: Entity) -> Vec<(Entity, String)> {
    let mut todo = vec![row];
    let mut out = Vec::new();
    while let Some(entity) = todo.pop() {
        if let Some(container) = world.get::<super::super::rows::PropertyContainer>(entity) {
            out.push((entity, container.0.clone()));
        } else if let Some(children) = world.get::<Children>(entity) {
            todo.extend(children.iter().rev());
        }
    }
    out
}

pub(super) fn prepare(world: &mut World, row: Entity) {
    if world.get::<State>(row).is_none() {
        world.entity_mut(row).insert(State::default());
    }
    if world.get::<Declaration>(row).is_none() {
        let tokens = world.get::<State>(row).unwrap().tokens.clone();
        let values = world.get::<State>(row).unwrap().settings.clone();
        world.entity_mut(row).insert(tokens);
        crate::sand_settings::declare(
            world,
            row,
            Declaration {
                name: "Protein presentation".into(),
                settings: lince_interface::presentation::settings(),
            },
            values,
        );
        if world.get::<SettingsObserved>(row).is_none() {
            world
                .entity_mut(row)
                .insert(SettingsObserved)
                .observe(view_changed);
        }
    }
    for (entity, property) in containers(world, row) {
        if world.get::<Field>(entity).is_some() {
            continue;
        }
        let Some(node) = world.get::<Node>(entity).cloned() else {
            continue;
        };
        let mut wrapping = Vec::new();
        let mut editors = Vec::new();
        let mut todo = vec![entity];
        while let Some(child) = todo.pop() {
            if let Some(layout) = world.get::<TextLayout>(child) {
                wrapping.push((child, layout.clone()));
            }
            if world.get::<bevy::text::EditableText>(child).is_some()
                && let Some(node) = world.get::<Node>(child)
            {
                editors.push((child, node.clone()));
            }
            if let Some(children) = world.get::<Children>(child) {
                todo.extend(children.iter());
            }
        }
        let values = world
            .get::<State>(row)
            .and_then(|state| state.fields.get(&property))
            .cloned()
            .unwrap_or_default();
        if let Some(tokens) = world
            .get::<State>(row)
            .and_then(|state| state.part_tokens.get(&property))
            .cloned()
        {
            world.entity_mut(entity).insert(tokens);
        }
        let mut definitions = definitions(&property);
        if let Some(binding) = world.get::<super::super::rows::Row>(row).and_then(|row| {
            row.config
                .bindings
                .iter()
                .find(|binding| binding.property == property)
        }) {
            definitions[0].default = Setting::Number(binding.width.round());
            definitions[1].default = Setting::Number(binding.height.round());
        }
        crate::sand_settings::declare(
            world,
            entity,
            Declaration {
                name: format!("{property} Sand"),
                settings: definitions,
            },
            values,
        );
        world
            .entity_mut(entity)
            .insert(Field {
                row,
                property,
                node,
                wrapping,
                editors,
            })
            .observe(field_changed);
        apply_field(world, entity);
    }
    let values = world.get::<State>(row).unwrap().settings.clone();
    if let Some(mut configuration) = world.get_mut::<Configuration>(row) {
        configuration.0 = values;
    }
    let mut definitions = lince_interface::presentation::settings();
    if let Some(view) = raw(world, row) {
        for definition in &mut definitions {
            definition.default = view.settings.resolve(definition);
        }
    } else if let Some(config) = base(world, row) {
        definitions[1].default = Setting::Toggle(config.show_labels);
        apply_native(world, row);
    }
    if let Some(mut declaration) = world.get_mut::<Declaration>(row) {
        declaration.settings = definitions;
    }
}

fn view_changed(event: On<Changed>, mut commands: Commands) {
    let row = event.entity;
    commands.queue(move |world: &mut World| {
        let values = world.get::<Configuration>(row).unwrap().0.clone();
        world.get_mut::<State>(row).unwrap().settings = values;
        save(world, row);
        if raw(world, row).is_none() {
            apply_native(world, row);
        }
        update(world);
    });
}

fn field_changed(event: On<Changed>, mut commands: Commands) {
    let entity = event.entity;
    commands.queue(move |world: &mut World| {
        let Some(field) = world.get::<Field>(entity).cloned() else {
            return;
        };
        let values = world.get::<Configuration>(entity).unwrap().0.clone();
        world
            .get_mut::<State>(field.row)
            .unwrap()
            .fields
            .insert(field.property, values);
        save(world, field.row);
        apply_field(world, entity);
    });
}

fn apply_field(world: &mut World, entity: Entity) {
    let Some(field) = world.get::<Field>(entity).cloned() else {
        return;
    };
    let Some(values) = world
        .get::<Configuration>(entity)
        .map(|values| values.0.clone())
    else {
        return;
    };
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        if let Some(Setting::Number(width)) = values.0.get("width") {
            node.width = px(*width);
            node.max_width = px(*width);
        } else {
            node.width = field.node.width;
            node.max_width = field.node.max_width;
        }
        if let Some(Setting::Number(height)) = values.0.get("height") {
            node.min_height = px(*height);
            node.height = px(*height);
        } else {
            node.min_height = field.node.min_height;
            node.height = field.node.height;
        }
    }
    for (entity, original) in field.wrapping {
        if let Some(mut wrapping) = world.get_mut::<TextLayout>(entity) {
            wrapping.linebreak = match values.0.get("wrap") {
                Some(Setting::Toggle(false)) => bevy::text::LineBreak::NoWrap,
                Some(Setting::Toggle(true)) => bevy::text::LineBreak::WordBoundary,
                _ => original.linebreak,
            };
        }
    }
    for (entity, original) in field.editors {
        if values.0.is_empty() {
            world.entity_mut(entity).remove::<EditorSettings>();
        } else {
            world
                .entity_mut(entity)
                .insert(EditorSettings(values.clone()));
        }
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            node.width = if values.0.contains_key("width") {
                percent(100)
            } else {
                original.width
            };
            node.max_width = if values.0.contains_key("width") {
                percent(100)
            } else {
                original.max_width
            };
            node.height = match values.0.get("height") {
                Some(Setting::Number(height)) => px((height - 8.0).max(16.0)),
                _ => original.height,
            };
        }
    }
}

pub(super) fn arrange(world: &mut World, row: Entity, view: &Presentation) {
    if world
        .get::<Original>(row)
        .is_some_and(|original| original.applied.as_ref() == Some(view))
    {
        refresh_added(world, row, view);
        return;
    }
    if world.get::<Original>(row).is_none() {
        let nodes = world
            .get::<Children>(row)
            .into_iter()
            .flatten()
            .filter(|entity| world.get::<Controls>(**entity).is_none())
            .filter_map(|entity| {
                world
                    .get::<Node>(*entity)
                    .cloned()
                    .map(|node| (*entity, node))
            })
            .collect();
        let parents = containers(world, row)
            .into_iter()
            .filter_map(|(entity, _)| {
                world
                    .get::<ChildOf>(entity)
                    .map(|parent| (entity, parent.parent()))
            })
            .collect();
        let host = crate::sand_panel::column(world, row);
        let stash = crate::sand_panel::column(world, row);
        world.get_mut::<Node>(stash).unwrap().display = Display::None;
        let castle = world.get::<crate::castle::Castle>(row).is_some();
        world.entity_mut(row).insert(Original {
            nodes,
            parents,
            host,
            stash,
            castle,
            applied: None,
        });
    }
    let original = world.get::<Original>(row).unwrap().clone();
    for (entity, _) in &original.nodes {
        if let Some(mut node) = world.get_mut::<Node>(*entity) {
            node.display = Display::None;
        }
    }
    if view.layout == Layout::Sand {
        world.entity_mut(row).remove::<crate::castle::Castle>();
    } else {
        world.entity_mut(row).insert(crate::castle::Castle);
    }
    let Setting::Number(spacing) = view
        .settings
        .resolve(&lince_interface::presentation::settings()[0])
    else {
        return;
    };
    world.get_mut::<Node>(original.host).unwrap().row_gap = px(spacing);
    let labels = view
        .settings
        .resolve(&lince_interface::presentation::settings()[1])
        == Setting::Toggle(true);
    let mut fields = containers(world, row);
    for property in &view.fields {
        if fields.iter().any(|(_, field)| field == property) {
            continue;
        }
        let container = crate::sand_panel::column(world, original.host);
        world
            .entity_mut(container)
            .insert(super::super::rows::PropertyContainer(property.clone()));
        let label = crate::edit_mode::label(world, container, property, 13.0);
        world
            .entity_mut(label)
            .insert(super::super::rows::PropertyLabel);
        let text = crate::edit_mode::label(world, container, "", 16.0);
        world.entity_mut(container).insert(Added(text));
        fields.push((container, property.clone()));
    }
    for (entity, field) in &fields {
        let visible = view.fields.contains(field);
        world.entity_mut(*entity).insert(ChildOf(if visible {
            original.host
        } else {
            original.stash
        }));
        if let Some(mut node) = world.get_mut::<Node>(*entity) {
            node.display = if visible {
                Display::Flex
            } else {
                Display::None
            };
        }
        if let Some(added) = world.get::<Added>(*entity) {
            let text = added.0;
            let data = world
                .get::<super::super::rows::Row>(row)
                .map(|row| row.data[field].clone())
                .unwrap_or(Value::Null);
            let value = if data.is_null() {
                view.fills
                    .get(field)
                    .cloned()
                    .unwrap_or_else(|| "No value in this Protein".into())
            } else {
                super::super::rows::display(&data)
            };
            if let Some(mut target) = world.get_mut::<Text>(text) {
                target.0 = value;
            }
        }
        let children: Vec<_> = world
            .get::<Children>(*entity)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        if !children.iter().any(|child| {
            world
                .get::<super::super::rows::PropertyLabel>(*child)
                .is_some()
        }) {
            let caption = protein::record_schema::fields()
                .into_iter()
                .find(|definition| definition.key == field)
                .map(|definition| definition.title.to_string())
                .unwrap_or_else(|| field.clone());
            let label = crate::edit_mode::label(world, *entity, &caption, 13.0);
            world
                .entity_mut(label)
                .insert((super::super::rows::PropertyLabel, PresentationLabel));
            world.get_mut::<Node>(label).unwrap().display =
                if labels { Display::Flex } else { Display::None };
            world.entity_mut(*entity).insert_children(0, &[label]);
        }
        for child in children {
            if world
                .get::<super::super::rows::PropertyLabel>(child)
                .is_some()
            {
                world.get_mut::<Node>(child).unwrap().display =
                    if labels { Display::Flex } else { Display::None };
            }
        }
    }
    let order: Vec<_> = view
        .fields
        .iter()
        .filter_map(|property| {
            fields
                .iter()
                .find(|(_, field)| field == property)
                .map(|(entity, _)| *entity)
        })
        .collect();
    world.entity_mut(original.host).replace_children(&order);
    prepare(world, row);
    for (entity, _) in fields {
        apply_field(world, entity);
    }
    world.get_mut::<Original>(row).unwrap().applied = Some(view.clone());
}

fn refresh_added(world: &mut World, row: Entity, view: &Presentation) {
    for (entity, field) in containers(world, row) {
        let Some(added) = world.get::<Added>(entity) else {
            continue;
        };
        let text = added.0;
        let data = world
            .get::<super::super::rows::Row>(row)
            .map(|row| row.data[&field].clone())
            .unwrap_or(Value::Null);
        let value = if data.is_null() {
            view.fills
                .get(&field)
                .cloned()
                .unwrap_or_else(|| "No value in this Protein".into())
        } else {
            super::super::rows::display(&data)
        };
        if let Some(mut target) = world.get_mut::<Text>(text)
            && target.0 != value
        {
            target.0 = value;
        }
    }
}

pub(super) fn refresh(world: &mut World, row: Entity) {
    for (entity, _) in containers(world, row) {
        apply_field(world, entity);
    }
}

pub(super) fn restore(world: &mut World, row: Entity) {
    let Some(original) = world.entity_mut(row).take::<Original>() else {
        return;
    };
    for (entity, node) in original.nodes {
        if let Ok(mut target) = world.get_entity_mut(entity) {
            target.insert(node);
        }
    }
    for (entity, parent) in original.parents {
        if world.get_entity(entity).is_ok() && world.get_entity(parent).is_ok() {
            world.entity_mut(entity).insert(ChildOf(parent));
            world.get_mut::<Node>(entity).unwrap().display = Display::Flex;
        }
    }
    let remaining = containers(world, row);
    for (entity, _) in remaining {
        let labels: Vec<_> = world
            .get::<Children>(entity)
            .into_iter()
            .flatten()
            .copied()
            .filter(|child| world.get::<PresentationLabel>(*child).is_some())
            .collect();
        for label in labels {
            world.despawn(label);
        }
        if world.get::<Added>(entity).is_some() {
            world.entity_mut(entity).insert(ChildOf(row));
            world.get_mut::<Node>(entity).unwrap().display = Display::None;
        }
    }
    world.despawn(original.host);
    world.despawn(original.stash);
    if original.castle {
        world.entity_mut(row).insert(crate::castle::Castle);
    } else {
        world.entity_mut(row).remove::<crate::castle::Castle>();
    }
    super::super::record_layout::restore(world, row);
    prepare(world, row);
    for (entity, _) in containers(world, row) {
        apply_field(world, entity);
    }
}

fn apply_native(world: &mut World, row: Entity) {
    let values = world
        .get::<State>(row)
        .map(|state| state.settings.clone())
        .unwrap_or_default();
    if let Some(mut node) = world.get_mut::<Node>(row) {
        node.row_gap = match values.0.get("spacing") {
            Some(Setting::Number(value)) => px(*value),
            _ => px(8),
        };
    }
    let explicit = values.0.get("labels");
    for (container, property) in containers(world, row) {
        let children: Vec<_> = world
            .get::<Children>(container)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        let mut labelled = false;
        for child in children {
            if world
                .get::<super::super::rows::PropertyLabel>(child)
                .is_none()
            {
                continue;
            }
            if explicit.is_none() && world.get::<PresentationLabel>(child).is_some() {
                world.despawn(child);
                continue;
            }
            labelled = true;
            if let Some(mut node) = world.get_mut::<Node>(child) {
                node.display = if explicit == Some(&Setting::Toggle(false)) {
                    Display::None
                } else {
                    Display::Flex
                };
            }
        }
        if !labelled && explicit == Some(&Setting::Toggle(true)) {
            let caption = protein::record_schema::fields()
                .into_iter()
                .find(|field| field.key == property)
                .map(|field| field.title.to_string())
                .unwrap_or(property);
            let label = crate::edit_mode::label(world, container, &caption, 13.0);
            world
                .entity_mut(label)
                .insert((super::super::rows::PropertyLabel, PresentationLabel));
            world.entity_mut(container).insert_children(0, &[label]);
        }
    }
}

pub(super) fn rebuilding(world: &mut World, row: Entity) {
    world
        .entity_mut(row)
        .remove::<Original>()
        .remove::<Declaration>()
        .remove::<Configuration>();
}

pub(super) fn capture_tokens(world: &World, row: Entity, state: &mut State) {
    for (entity, property) in containers(world, row) {
        let tokens = crate::token_style::overrides(world, entity);
        if !tokens.0.is_empty() {
            state.part_tokens.insert(property, tokens);
        } else {
            state.part_tokens.remove(&property);
        }
    }
}

pub(super) fn owner(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<Field>(entity).map(|field| field.row)
}
