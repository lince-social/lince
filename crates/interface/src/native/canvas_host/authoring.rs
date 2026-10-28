use super::*;
use crate::native::tokens::{Token, TokenOverrides, TokenValue};
use api::authoring::{AddedPart, Edits, Element, PartInfo, TokenInfo};
use std::collections::BTreeSet;

mod events;
pub(crate) mod inspector;
mod style;
#[cfg(test)]
mod tests;

#[derive(Component, Clone)]
pub(crate) struct PartName(pub String);

#[derive(Component, Clone)]
struct PartIdentity(BTreeMap<Entity, String>);

#[derive(Component)]
struct AuthoredTokenUses(BTreeMap<String, Token>);

pub(crate) fn token_use_override(world: &World, entity: Entity, usage: &str) -> bool {
    world
        .get::<AuthoredTokenUses>(entity)
        .is_some_and(|uses| uses.0.contains_key(usage))
}

pub(crate) fn metric_token(world: &World, entity: Entity, usage: &str, fallback: Token) -> Token {
    world
        .get::<AuthoredTokenUses>(entity)
        .and_then(|uses| uses.0.get(usage))
        .copied()
        .unwrap_or(fallback)
}

#[derive(Clone)]
struct Original {
    entity: Entity,
    parent: Option<String>,
    order: usize,
    node: Node,
    tokens: TokenOverrides,
    uses: BTreeMap<String, Token>,
    text: Option<String>,
    settings: Option<crate::native::sand_settings::Values>,
    action: Option<crate::native::actions::ActionButton>,
    geometry: Option<CanvasItem>,
    group: Option<crate::native::canvas_selection::SandGroup>,
    layout: Option<crate::native::layout::Rules>,
    layout_parent: Option<String>,
}

#[derive(Component, Clone, Default)]
struct Editing {
    original: BTreeMap<String, Original>,
    added: BTreeMap<String, (Entity, AddedPart)>,
    applied: Edits,
}

#[derive(Component)]
struct Maintained(std::time::Instant, Option<String>);

pub(super) fn token(name: &str) -> Result<Token, String> {
    serde_json::from_value(serde_json::Value::String(name.into()))
        .map_err(|_| format!("Unknown native token: {name}."))
}

fn token_name(token: Token) -> String {
    serde_json::to_value(token)
        .unwrap()
        .as_str()
        .unwrap()
        .into()
}

fn value(value: TokenValue) -> api::authoring::TokenValue {
    match value {
        TokenValue::Color(value) => api::authoring::TokenValue::Color(value),
        TokenValue::Number(value) => api::authoring::TokenValue::Number(value),
    }
}

fn native_value(value: api::authoring::TokenValue) -> TokenValue {
    match value {
        api::authoring::TokenValue::Color(value) => TokenValue::Color(value),
        api::authoring::TokenValue::Number(value) => TokenValue::Number(value),
    }
}

fn uses(world: &World, entity: Entity) -> BTreeMap<String, Token> {
    let mut result = BTreeMap::new();
    if let Some(binding) = world.get::<crate::native::token_style::BackgroundToken>(entity) {
        result.insert("background".into(), binding.0);
    }
    if let Some(binding) = world.get::<crate::native::token_style::BorderToken>(entity) {
        result.insert("border".into(), binding.0);
    }
    if let Some(binding) = world.get::<crate::native::token_style::TextToken>(entity) {
        result.insert("text".into(), binding.0);
    }
    if let Some(binding) = world.get::<crate::native::token_style::OutlineToken>(entity) {
        result.insert("outline".into(), binding.0);
    }
    if let Some(binding) = world.get::<crate::native::token_style::CursorToken>(entity) {
        result.insert("cursor".into(), binding.0);
    }
    if world.get::<Node>(entity).is_some() {
        result.insert(
            "spacing".into(),
            metric_token(world, entity, "spacing", Token::Spacing),
        );
        result.insert(
            "padding".into(),
            metric_token(world, entity, "padding", Token::Padding),
        );
    }
    if world.get::<TextFont>(entity).is_some() {
        result.insert(
            "font-size".into(),
            metric_token(world, entity, "font-size", Token::FontSize),
        );
    }
    if world
        .get::<crate::native::icons::IconStyle>(entity)
        .is_some()
    {
        result.insert(
            "icon-size".into(),
            metric_token(world, entity, "icon-size", Token::IconSize),
        );
        result.insert(
            "icon-padding".into(),
            metric_token(world, entity, "icon-padding", Token::IconPadding),
        );
    }
    result
}

fn set_uses(world: &mut World, entity: Entity, values: &BTreeMap<String, Token>) {
    world.entity_mut(entity).remove::<(
        crate::native::token_style::BackgroundToken,
        crate::native::token_style::BorderToken,
        crate::native::token_style::TextToken,
        crate::native::token_style::OutlineToken,
        crate::native::token_style::CursorToken,
    )>();
    for (usage, token) in values {
        match usage.as_str() {
            "background" => {
                world
                    .entity_mut(entity)
                    .insert(crate::native::token_style::background(*token));
            }
            "border" => {
                world
                    .entity_mut(entity)
                    .insert(crate::native::token_style::border(*token));
            }
            "text" => {
                world
                    .entity_mut(entity)
                    .insert(crate::native::token_style::text(*token));
            }
            "outline" => {
                world
                    .entity_mut(entity)
                    .insert(crate::native::token_style::OutlineToken(*token));
            }
            "cursor" => {
                world
                    .entity_mut(entity)
                    .insert(crate::native::token_style::CursorToken(*token));
            }
            _ => {}
        }
    }
}

fn text(world: &World, entity: Entity) -> Option<String> {
    world
        .get::<bevy::text::EditableText>(entity)
        .map(|text| text.value().to_string())
        .or_else(|| world.get::<Text>(entity).map(|text| text.0.clone()))
}

fn role(world: &World, entity: Entity) -> &'static str {
    if world
        .get::<crate::native::actions::ActionButton>(entity)
        .is_some()
    {
        "native-action"
    } else if world.get::<bevy::text::EditableText>(entity).is_some() {
        "editable-text"
    } else if world.get::<Text>(entity).is_some() {
        "text"
    } else if world.get::<CanvasItem>(entity).is_some() {
        "canvas-item"
    } else {
        "container"
    }
}

fn index(world: &mut World, host: Entity) -> Result<Editing, String> {
    world.init_resource::<crate::native::tokens::ThemeSettings>();
    let mut editing = world.get::<Editing>(host).cloned().unwrap_or_default();
    let mut indexed = BTreeSet::new();
    editing
        .original
        .retain(|_, original| world.get_entity(original.entity).is_ok());
    let added: BTreeSet<_> = editing.added.values().map(|(entity, _)| *entity).collect();
    let mut owned = owned(world, host);
    let record = owned
        .iter()
        .find(|(_, id)| id == "record")
        .map(|(entity, _)| *entity);
    let single = world
        .get::<crate::native::area::InfluenceArea>(host)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.record_cards && config.draft.query["limit"] == 1);
    let mut pending = vec![(
        host,
        if single {
            "source".to_string()
        } else {
            "root".to_string()
        },
        None,
        0,
    )];
    for (entity, id) in &mut owned {
        if Some(*entity) == record {
            *id = "root".into();
        }
    }
    pending.extend(owned.into_iter().map(|(entity, id)| (entity, id, None, 0)));
    while let Some((entity, candidate, parent, order)) = pending.pop() {
        if world.get::<Node>(entity).is_none() && entity != host {
            continue;
        }
        let id = world
            .get::<PartIdentity>(entity)
            .and_then(|id| id.0.get(&host).cloned())
            .unwrap_or(candidate);
        if editing
            .original
            .get(&id)
            .is_some_and(|original| original.entity != entity)
        {
            return Err(format!("Ambiguous native part identity: {id}."));
        }
        if !editing.original.contains_key(&id) && !added.contains(&entity) {
            indexed.insert(id.clone());
            if editing.original.len() >= 2048 {
                return Err("This native subtree exceeds the 2048-part inspection limit.".into());
            }
            editing.original.insert(
                id.clone(),
                Original {
                    entity,
                    parent,
                    order,
                    node: world.get::<Node>(entity).cloned().unwrap_or_default(),
                    tokens: crate::native::token_style::overrides(world, entity),
                    uses: uses(world, entity),
                    text: text(world, entity),
                    settings: world
                        .get::<crate::native::sand_settings::Configuration>(entity)
                        .map(|configuration| configuration.0.clone()),
                    action: world
                        .get::<crate::native::actions::ActionButton>(entity)
                        .cloned(),
                    geometry: world.get::<CanvasItem>(entity).copied(),
                    group: world
                        .get::<crate::native::canvas_selection::SandGroup>(entity)
                        .copied(),
                    layout: world
                        .get::<crate::native::layout::LayoutBox>(entity)
                        .map(|layout| layout.rules)
                        .or_else(|| {
                            world
                                .get::<crate::native::sand_text::SandText>(entity)
                                .and_then(|text| text.layout)
                        }),
                    layout_parent: None,
                },
            );
            if let Some(mut identities) = world.get_mut::<PartIdentity>(entity) {
                identities.0.insert(host, id.clone());
            } else {
                world
                    .entity_mut(entity)
                    .insert(PartIdentity(BTreeMap::from([(host, id.clone())])));
            }
            if let Some(defaults) = world
                .get::<Content>(host)
                .map(|content| api::authoring::edits(&content.0))
                .and_then(|edits| edits.defaults.get(&id).cloned())
            {
                let original = editing.original.get_mut(&id).unwrap();
                style::apply(&mut original.node, &defaults.style);
                if let Some(ratio) = defaults.aspect_ratio {
                    original.node.aspect_ratio = match ratio {
                        api::authoring::Measure::Px(value) => Some(value),
                        _ => None,
                    };
                }
                original.parent = defaults.parent;
                original.order = defaults.order;
                for (name, value) in defaults.tokens {
                    let token = token(&name)?;
                    if let Some(value) = value {
                        original.tokens.set(token, native_value(value));
                    } else {
                        original.tokens.0.remove(&token);
                    }
                }
                for (usage, name) in defaults.token_uses {
                    if let Some(name) = name {
                        original.uses.insert(usage, token(&name)?);
                    } else {
                        original.uses.remove(&usage);
                    }
                }
                if let Some(text) = defaults.text {
                    original.text = Some(text);
                }
                if let Some(geometry) = defaults.geometry {
                    original.geometry = Some(CanvasItem {
                        position: DVec2::from_array(geometry.position),
                        size: DVec2::from_array(geometry.size).as_vec2(),
                    });
                }
                if let Some(layout) = defaults.layout {
                    original.layout = serde_json::from_value(layout["rules"].clone())
                        .map_err(|error| error.to_string())?;
                }
                original.layout_parent = defaults.layout_parent;
            }
        }
        if world.get::<bevy::text::EditableText>(entity).is_some() {
            continue;
        }
        let children: Vec<_> = world
            .get::<Children>(entity)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        for (order, child) in children.into_iter().enumerate().rev() {
            let name = world
                .get::<PartName>(child)
                .map(|name| {
                    let duplicate = world.get::<Children>(entity).is_some_and(|children| {
                        children
                            .iter()
                            .filter(|sibling| {
                                world
                                    .get::<PartName>(*sibling)
                                    .is_some_and(|other| other.0 == name.0)
                            })
                            .count()
                            > 1
                    });
                    if duplicate {
                        format!("{}#{order}", name.0)
                    } else {
                        name.0.clone()
                    }
                })
                .unwrap_or_else(|| order.to_string());
            pending.push((child, format!("{id}/{name}"), Some(id.clone()), order));
        }
    }
    let lookup = entities(&editing);
    for id in indexed {
        if api::authoring::edits(
            &world
                .get::<Content>(host)
                .map(|content| content.0.clone())
                .unwrap_or_else(|| capture_content(world, host)),
        )
        .defaults
        .get(&id)
        .is_none()
        {
            let original = editing.original.get_mut(&id).unwrap();
            original.layout_parent = layout_parent(world, original.entity, &lookup);
        }
    }
    world.entity_mut(host).insert(editing.clone());
    Ok(editing)
}

pub(crate) fn owner(world: &World, entity: Entity) -> Entity {
    if let Some(owner) = crate::native::protein_area::row_area(world, entity).or_else(|| {
        world
            .get::<crate::native::protein_area::RecordBinding>(entity)
            .map(|binding| binding.area)
    }) && world
        .get::<crate::native::area::InfluenceArea>(owner)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.record_cards && config.draft.query["limit"] == 1)
    {
        return owner;
    }
    if let Some(area) = world.get::<crate::native::area::InfluenceArea>(entity)
        && let Some(parent) = world.get::<ChildOf>(entity)
        && let Some(children) = world.get::<Children>(parent.parent())
    {
        if let Some(owner) = children.iter().find(|candidate| {
            world
                .get::<crate::native::kanban::Kanban>(*candidate)
                .is_some_and(|board| board.ids().any(|id| id == &area.id))
        }) {
            return owner;
        }
    }
    crate::native::kanban::part_owner(world, entity).unwrap_or(entity)
}

fn owned(world: &mut World, host: Entity) -> Vec<(Entity, String)> {
    let single = world
        .get::<crate::native::area::InfluenceArea>(host)
        .and_then(|area| area.protein.as_ref())
        .is_some_and(|config| config.record_cards && config.draft.query["limit"] == 1);
    let mut result: Vec<_> = world
        .query::<(
            Entity,
            &crate::native::protein_area::RecordBinding,
            &CanvasItem,
        )>()
        .iter(world)
        .filter(|(_, binding, _)| binding.area == host)
        .map(|(entity, binding, _)| {
            (
                entity,
                if single {
                    "record".into()
                } else {
                    format!("generated/{}", binding.uid)
                },
            )
        })
        .collect();
    if let Some(board) = world.get::<crate::native::kanban::Kanban>(host).cloned() {
        let parent = world.get::<ChildOf>(host).map(ChildOf::parent);
        let workspace = world.get::<WorkspaceMember>(host).map(|member| member.0);
        result.extend(
            world
                .query::<(
                    Entity,
                    &crate::native::area::InfluenceArea,
                    &ChildOf,
                    &WorkspaceMember,
                )>()
                .iter(world)
                .filter(|(_, area, source, member)| {
                    Some(source.parent()) == parent
                        && Some(member.0) == workspace
                        && board.ids().any(|id| id == &area.id)
                })
                .map(|(entity, area, _, _)| {
                    (
                        entity,
                        if area.id == board.source {
                            "kanban/source".into()
                        } else {
                            format!(
                                "kanban/column-{}",
                                board
                                    .columns
                                    .iter()
                                    .position(|column| column.area == area.id)
                                    .unwrap()
                            )
                        },
                    )
                }),
        );
        let columns: Vec<_> = world
            .query::<(Entity, &ChildOf)>()
            .iter(world)
            .filter(|(entity, source)| {
                *entity != host
                    && Some(source.parent()) == parent
                    && crate::native::kanban::part_owner(world, *entity) == Some(host)
            })
            .map(|(entity, _)| entity)
            .collect();
        for (index, entity) in columns.into_iter().enumerate() {
            result.push((entity, format!("kanban/header-{}", index + 1)));
        }
    }
    result
}

fn entities(editing: &Editing) -> BTreeMap<String, Entity> {
    editing
        .original
        .iter()
        .map(|(id, original)| (id.clone(), original.entity))
        .chain(
            editing
                .added
                .iter()
                .map(|(id, (entity, _))| (id.clone(), *entity)),
        )
        .collect()
}

fn layout_parent(
    world: &World,
    entity: Entity,
    lookup: &BTreeMap<String, Entity>,
) -> Option<String> {
    let parent = world
        .get::<crate::native::layout::LayoutBox>(entity)?
        .parent?;
    lookup
        .iter()
        .find(|(_, entity)| {
            world
                .get::<crate::native::layout::LayoutBox>(**entity)
                .is_some_and(|layout| layout.id == parent)
        })
        .map(|(id, _)| id.clone())
}

fn has_draft(world: &World, entity: Entity) -> bool {
    let Some(text) = world.get::<bevy::text::EditableText>(entity) else {
        return false;
    };
    let value = text.value().to_string();
    world
        .get::<crate::native::record_view::RecordEditor>(entity)
        .is_some_and(|editor| {
            editor.pending.is_some()
                || crate::native::record_view::pending_text(text)
                || value != editor.confirmed
        })
        || world
            .get::<crate::native::record_binding::TextBinding>(entity)
            .is_some_and(|binding| binding.unsaved(&value))
        || world
            .get::<crate::native::sand::Unsaved>(entity)
            .is_some_and(|unsaved| unsaved.0)
        || crate::native::protein_area::authoring_draft(world, entity)
}

pub(super) fn compatible(before: &ContentKind, after: &ContentKind) -> bool {
    api::authoring::base(before) == api::authoring::base(after)
}

pub(super) fn validate(
    world: &mut World,
    host: Entity,
    component: &ContentKind,
) -> Result<(), String> {
    let edits = api::authoring::edits(component);
    edits.validate(&registry())?;
    let editing = index(world, host)?;
    let existing = entities(&editing);
    let removed_added: BTreeSet<_> = editing
        .added
        .keys()
        .filter(|id| !edits.added.contains_key(*id))
        .cloned()
        .collect();
    let known = |id: &str| {
        (existing.contains_key(id)
            && !removed_added
                .iter()
                .any(|removed| id == removed || id.starts_with(&format!("{removed}/"))))
            || edits.added.contains_key(id)
    };
    let mut parents: BTreeMap<_, _> = editing
        .original
        .iter()
        .filter_map(|(id, original)| {
            original
                .parent
                .as_ref()
                .map(|parent| (id.clone(), parent.clone()))
        })
        .collect();
    let mut layout_parents: BTreeMap<_, _> = existing
        .iter()
        .filter_map(|(id, entity)| {
            layout_parent(world, *entity, &existing).map(|parent| (id.clone(), parent))
        })
        .collect();
    for (id, part) in &edits.added {
        if !known(&part.parent) {
            return Err(format!("Unknown parent part: {}.", part.parent));
        }
        parents.insert(id.clone(), part.parent.clone());
        if let Some((entity, old)) = editing.added.get(id) {
            if old.component != part.component
                && subtree(world, *entity)
                    .iter()
                    .any(|entity| has_draft(world, *entity))
            {
                return Err("Save the draft before changing an authored part's factory.".into());
            }
        }
    }
    for (id, edit) in &edits.parts {
        if id == "root" && edit.removed {
            return Err("Remove the placement to delete its root.".into());
        }
        if !known(id) {
            return Err(format!(
                "Unknown part: {id}. Inspect the current native tree."
            ));
        }
        if let Some(parent) = &edit.parent {
            if id == "root" || !known(parent) {
                return Err(
                    "Reparent within this item's known part tree; its root stays on the canvas."
                        .into(),
                );
            }
            parents.insert(id.clone(), parent.clone());
        }
        if let Some(entity) = existing.get(id) {
            if (edit.layout_parent.is_some() || edit.layout_detached)
                && world
                    .get::<crate::native::layout::LayoutBox>(*entity)
                    .is_some_and(|layout| layout.parent.is_some())
                && layout_parent(world, *entity, &existing).is_none()
            {
                return Err(
                    "Include the current layout parent in the authored composition before changing membership.".into(),
                );
            }
            if edit.layout_detached {
                layout_parents.remove(id);
            }
            if let Some(parent) = &edit.layout_parent {
                let target = existing
                    .get(parent)
                    .ok_or("Inspect rendered layout targets before attaching them.")?;
                if edit.layout_detached
                    || world.get::<CanvasItem>(*entity).is_none()
                    || world.get::<CanvasItem>(*target).is_none()
                    || world.get::<ChildOf>(*entity).map(ChildOf::parent)
                        != world.get::<ChildOf>(*target).map(ChildOf::parent)
                    || world.get::<WorkspaceMember>(*entity)
                        != world.get::<WorkspaceMember>(*target)
                {
                    return Err("Attach canvas parts sharing one canvas and workspace; detach and attach are distinct operations.".into());
                }
                layout_parents.insert(id.clone(), parent.clone());
            }
            if world.get::<Node>(*entity).is_none()
                && (!edit.style.dimensions.is_empty()
                    || !edit.style.numbers.is_empty()
                    || !edit.style.choices.is_empty()
                    || !edit.tokens.is_empty()
                    || !edit.token_uses.is_empty())
            {
                return Err("A spatial resource uses its typed native state and canvas geometry; scene internals are resource-managed.".into());
            }
            if let Some(layout) = &edit.layout {
                let rules: crate::native::layout::Rules =
                    serde_json::from_value(layout.clone()).map_err(|error| error.to_string())?;
                if !rules.valid()
                    || world.get::<CanvasItem>(*entity).is_none()
                        && world
                            .get::<crate::native::sand_text::SandText>(*entity)
                            .is_none()
                {
                    return Err("Layout rules apply to a Sand, text area or influence area.".into());
                }
                if world
                    .get::<crate::native::sand_text::SandText>(*entity)
                    .is_some()
                    && world
                        .get::<ChildOf>(*entity)
                        .is_none_or(|parent| world.get::<CanvasItem>(parent.parent()).is_none())
                {
                    return Err("This editor belongs to a property block. Use its Node flow constraints; canvas layout rules require a Sand parent.".into());
                }
                if world
                    .get::<crate::native::area::InfluenceArea>(*entity)
                    .is_some_and(|area| {
                        matches!(
                            area.shape,
                            crate::native::area::AreaShape::Square
                                | crate::native::area::AreaShape::Circle
                        )
                    })
                    && rules.axes[0].min.max(rules.axes[1].min)
                        > rules.axes[0].max.min(rules.axes[1].max)
                {
                    return Err("Square and circle layout limits must overlap.".into());
                }
            }
            if edit.removed
                && subtree(world, *entity)
                    .iter()
                    .any(|entity| has_draft(world, *entity))
            {
                return Err("Save the draft before removing its part.".into());
            }
            if edit.text.is_some()
                && (world
                    .get::<crate::native::record_view::RecordEditor>(*entity)
                    .is_some()
                    || world
                        .get::<crate::native::record_binding::TextBinding>(*entity)
                        .is_some()
                    || world
                        .get::<crate::native::protein_area::RecordBinding>(*entity)
                        .is_some()
                        && world.get::<bevy::text::EditableText>(*entity).is_some())
            {
                return Err("Edit Record data through the Record tools; authoring preserves its binding and draft.".into());
            }
            if edit.text.is_some() && text(world, *entity).is_none() {
                return Err("This part has no editable presentation text.".into());
            }
            if edit.native_actions_enabled.is_some()
                && editing
                    .original
                    .get(id)
                    .is_none_or(|original| original.action.is_none())
            {
                return Err("This part has no replaceable native Action button; compiled observers remain owned by its factory.".into());
            }
            if edit.geometry.is_some() {
                if world.get::<CanvasItem>(*entity).is_none() {
                    return Err("Flow parts use layout constraints; explicit geometry belongs to canvas parts.".into());
                }
                if world
                    .get::<crate::native::layout::LayoutBox>(*entity)
                    .is_some_and(|layout| {
                        layout.parent.is_some()
                            || layout
                                .rules
                                .axes
                                .iter()
                                .any(|axis| axis.sizing != crate::native::layout::Sizing::Fixed)
                    })
                {
                    return Err("This part's layout owner controls its geometry; configure its layout rules first.".into());
                }
            }
            if edit.parent.is_some()
                && world.get::<CanvasItem>(*entity).is_some()
                && edit
                    .parent
                    .as_ref()
                    .and_then(|id| existing.get(id))
                    .is_none_or(|parent| world.get::<Workspaces>(*parent).is_none())
            {
                return Err("Canvas parts need a canvas parent. Use an authored component container to place a Sand inside a flow layout.".into());
            }
            if !edit.settings.is_empty() {
                let declaration = world
                    .get::<crate::native::sand_settings::Declaration>(*entity)
                    .ok_or("This part has no registered settings.")?;
                for (key, value) in &edit.settings {
                    let value: crate::native::sand_settings::Value =
                        serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
                    if declaration
                        .settings
                        .iter()
                        .find(|definition| &definition.id == key)
                        .is_none_or(|definition| !definition.accepts(&value))
                    {
                        return Err(format!("Invalid registered setting: {key}."));
                    }
                }
            }
            if world.get::<CanvasItem>(*entity).is_some()
                && edit.style.dimensions.keys().any(|key| {
                    matches!(
                        key.as_str(),
                        "width" | "height" | "left" | "right" | "top" | "bottom"
                    )
                })
            {
                return Err("CanvasItem owns this part's geometry. Use Resize/Move or Width/Height tokens instead of Node dimensions.".into());
            }
        }
        for (name, value) in &edit.tokens {
            let token = token(name)?;
            if !token.accepts(native_value(*value)) {
                return Err(format!("Invalid token value for {name}."));
            }
            if existing.get(id).is_some_and(|entity| {
                crate::native::relation_castle::styles::override_value(world, *entity, token)
                    .is_some()
            }) {
                return Err(format!(
                    "{name} is controlled by a Relation color rule; edit that rule first."
                ));
            }
        }
        for (usage, name) in &edit.token_uses {
            let token = token(name)?;
            let numeric = match usage.as_str() {
                "font-size" => Some(Token::FontSize),
                "spacing" => Some(Token::Spacing),
                "padding" => Some(Token::Padding),
                "icon-size" => Some(Token::IconSize),
                "icon-padding" => Some(Token::IconPadding),
                _ => None,
            };
            if let Some(target) = numeric {
                let effective = existing
                    .get(id)
                    .map(|entity| crate::native::token_style::resolve(world, *entity, token).0)
                    .unwrap_or_else(|| token.default_value(Default::default()));
                if !target.accepts(effective) {
                    return Err("Metric bindings need a numeric token whose effective value fits the target range.".into());
                }
            } else if token.definition().range.is_some() {
                return Err("Color bindings require color tokens.".into());
            }
        }
    }
    for defaults in edits.defaults.values() {
        for (name, value) in &defaults.tokens {
            let token = token(name)?;
            if value.is_some_and(|value| !token.accepts(native_value(value))) {
                return Err("Invalid recovery token value.".into());
            }
        }
        for name in defaults.token_uses.values().flatten() {
            token(name)?;
        }
    }
    for id in parents.keys() {
        let mut seen = BTreeSet::from([id.as_str()]);
        let mut parent = parents.get(id);
        while let Some(current) = parent {
            if !seen.insert(current) || seen.len() > 32 {
                return Err(
                    "Reparenting would form an ownership cycle or exceed 32 levels.".into(),
                );
            }
            parent = parents.get(current);
        }
    }
    for id in layout_parents.keys() {
        let mut seen = BTreeSet::from([id.as_str()]);
        let mut parent = layout_parents.get(id);
        while let Some(current) = parent {
            if !seen.insert(current) || seen.len() >= crate::native::layout::MAX_DEPTH {
                return Err(
                    "Layout membership would create a cycle or exceed its depth limit.".into(),
                );
            }
            parent = layout_parents.get(current);
        }
    }
    for (id, (entity, _)) in &editing.added {
        if !edits.added.contains_key(id)
            && subtree(world, *entity)
                .iter()
                .any(|entity| has_draft(world, *entity))
        {
            return Err("Save the draft before deleting an authored part.".into());
        }
    }
    for behavior in edits.behaviors.values() {
        if !known(&behavior.part)
            || edits
                .parts
                .get(&behavior.part)
                .is_some_and(|edit| edit.removed)
        {
            return Err("A behavior needs a live part target.".into());
        }
        serde_json::from_value::<engine::actions::Action>(behavior.action.clone())
            .map_err(|error| error.to_string())?;
    }
    for link in edits.links.values() {
        if !known(&link.from)
            || !known(&link.to)
            || [&link.from, &link.to]
                .iter()
                .any(|id| edits.parts.get(*id).is_some_and(|edit| edit.removed))
        {
            return Err("An event link needs live endpoints in this item.".into());
        }
    }
    for members in edits.groups.values() {
        let mut scope = None;
        for member in members {
            let entity = existing
                .get(member)
                .ok_or("Group members must be rendered before grouping.")?;
            if !known(member) || world.get::<CanvasItem>(*entity).is_none() {
                return Err("Transform groups contain live canvas parts; flow groups use container ownership.".into());
            }
            let parent = world.get::<ChildOf>(*entity).map(ChildOf::parent);
            let workspace = world.get::<WorkspaceMember>(*entity).map(|member| member.0);
            if scope.is_some_and(|scope| scope != (parent, workspace)) {
                return Err("A transform group's members share a canvas and workspace.".into());
            }
            scope = Some((parent, workspace));
        }
    }
    Ok(())
}

fn subtree(world: &World, entity: Entity) -> Vec<Entity> {
    let mut result = Vec::new();
    let mut pending = vec![entity];
    while let Some(entity) = pending.pop() {
        result.push(entity);
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    result
}

pub(super) fn protected_draft(world: &mut World, host: Entity) -> bool {
    let mut roots = vec![host];
    let mut cursor = 0;
    let mut seen = BTreeSet::new();
    while cursor < roots.len() && roots.len() <= 2048 {
        let entity = roots[cursor];
        cursor += 1;
        if !seen.insert(entity) {
            continue;
        }
        roots.extend(owned(world, entity).into_iter().map(|(entity, _)| entity));
    }
    roots.into_iter().any(|entity| {
        subtree(world, entity)
            .into_iter()
            .any(|entity| has_draft(world, entity))
    })
}

pub(super) fn discard(world: &mut World, host: Entity) {
    let mut roots = vec![host];
    let mut cursor = 0;
    let mut seen = BTreeSet::new();
    while cursor < roots.len() && roots.len() <= 2048 {
        let entity = roots[cursor];
        cursor += 1;
        if !seen.insert(entity) {
            continue;
        }
        roots.extend(owned(world, entity).into_iter().map(|(entity, _)| entity));
    }
    for entity in roots.into_iter().rev() {
        if world.get_entity(entity).is_ok() {
            crate::native::area_mutation::disarm(
                world,
                entity,
                "The component or its owner was removed.",
            );
            world.despawn(entity);
        }
    }
}

pub(super) fn reconcile(
    world: &mut World,
    host: Entity,
    component: &ContentKind,
) -> Result<(), String> {
    validate(world, host, component)?;
    let mut edits = api::authoring::edits(component);
    let mut editing = world.get::<Editing>(host).cloned().unwrap();
    for (id, edit) in &edits.parts {
        if let Some(original) = editing.original.get(id) {
            let defaults =
                edits
                    .defaults
                    .entry(id.clone())
                    .or_insert_with(|| api::authoring::PartDefaults {
                        parent: original.parent.clone(),
                        order: original.order,
                        ..default()
                    });
            let style = style::capture(&original.node);
            for key in edit.style.dimensions.keys() {
                if let Some(value) = style.dimensions.get(key) {
                    defaults
                        .style
                        .dimensions
                        .entry(key.clone())
                        .or_insert(value.clone());
                }
            }
            for key in edit.style.numbers.keys() {
                if let Some(value) = style.numbers.get(key) {
                    defaults.style.numbers.entry(key.clone()).or_insert(*value);
                }
            }
            if edit.style.numbers.contains_key("aspect-ratio") && defaults.aspect_ratio.is_none() {
                defaults.aspect_ratio = Some(
                    original
                        .node
                        .aspect_ratio
                        .map(api::authoring::Measure::Px)
                        .unwrap_or(api::authoring::Measure::Auto),
                );
            }
            for key in edit.style.choices.keys() {
                if let Some(value) = style.choices.get(key) {
                    defaults
                        .style
                        .choices
                        .entry(key.clone())
                        .or_insert(value.clone());
                }
            }
            for name in edit.tokens.keys() {
                defaults.tokens.entry(name.clone()).or_insert_with(|| {
                    original
                        .tokens
                        .0
                        .get(&token(name).unwrap())
                        .copied()
                        .map(value)
                });
            }
            for usage in edit.token_uses.keys() {
                defaults
                    .token_uses
                    .entry(usage.clone())
                    .or_insert_with(|| original.uses.get(usage).copied().map(token_name));
            }
            if edit.text.is_some() && defaults.text.is_none() {
                defaults.text = original.text.clone();
            }
            if edit.geometry.is_some() && defaults.geometry.is_none() {
                defaults.geometry = original.geometry.map(|item| api::Geometry {
                    position: item.position.to_array(),
                    size: item.size.as_dvec2().to_array(),
                });
            }
            if edit.layout.is_some() && defaults.layout.is_none() {
                defaults.layout = Some(serde_json::json!({"rules":original.layout}));
            }
            if edit.layout_parent.is_some() || edit.layout_detached {
                defaults.layout_parent = original.layout_parent.clone();
            }
        }
    }
    ContentKind::Authored {
        base: Box::new(api::authoring::base(component).clone()),
        edits: edits.clone(),
    }
    .validate_snapshot(&registry())?;
    let mut staged = Vec::new();
    for (id, definition) in &edits.added {
        if editing
            .added
            .get(id)
            .is_some_and(|(_, old)| old.component == definition.component)
        {
            continue;
        }
        let result = spawn_part(world, host, definition);
        match result {
            Ok(entity) => staged.push((id.clone(), entity, definition.clone())),
            Err(error) => {
                for (_, entity, _) in staged {
                    world.despawn(entity);
                }
                return Err(error);
            }
        }
    }
    for (id, entity, definition) in staged {
        if let Some((old, _)) = editing.added.insert(id.clone(), (entity, definition)) {
            world.despawn(old);
        }
        world
            .entity_mut(entity)
            .insert(PartIdentity(BTreeMap::from([(host, id)])));
    }
    if let Some(size) = edits.size
        && editing.applied.size != Some(size)
    {
        world
            .get_mut::<CanvasItem>(host)
            .ok_or("The authored root is not a canvas item.")?
            .size = DVec2::from_array(size).as_vec2();
        if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(host) {
            area.size = size;
        }
    }
    let removed: Vec<_> = editing
        .added
        .keys()
        .filter(|id| !edits.added.contains_key(*id))
        .cloned()
        .collect();
    for id in removed {
        if let Some((entity, _)) = editing.added.remove(&id) {
            if world.get_entity(entity).is_ok() {
                world.despawn(entity);
            }
        }
    }
    let lookup = entities(&editing);
    for (id, original) in &editing.original {
        if let Some(previous) = editing.applied.parts.get(id)
            && !edits.parts.contains_key(id)
        {
            restore_part(world, original, &lookup, previous);
        }
    }
    for (id, definition) in &edits.added {
        if let Some((_, current)) = editing.added.get_mut(id) {
            *current = definition.clone();
        }
        let parent = edits
            .parts
            .get(id)
            .and_then(|edit| edit.parent.as_ref())
            .unwrap_or(&definition.parent);
        attach(
            world,
            lookup[id],
            lookup[parent],
            edits.parts.get(id).and_then(|edit| edit.order),
        );
    }
    for (id, edit) in &edits.parts {
        let entity = lookup[id];
        if let Some(geometry) = &edit.geometry {
            world.entity_mut(entity).insert(CanvasItem {
                position: DVec2::from_array(geometry.position),
                size: DVec2::from_array(geometry.size).as_vec2(),
            });
            if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity) {
                area.center = geometry.position;
                area.size = geometry.size;
            }
        }
        if editing.applied.parts.get(id) != Some(edit) {
            if let Some(original) = editing.original.get(id)
                && let Some(previous) = editing.applied.parts.get(id)
            {
                let mut changed = previous.clone();
                if previous.text == edit.text {
                    changed.text = None;
                }
                if previous.layout == edit.layout {
                    changed.layout = None;
                }
                changed
                    .settings
                    .retain(|key, value| edit.settings.get(key) != Some(value));
                restore_part(world, original, &lookup, &changed);
            }
            let mut tokens = crate::native::token_style::overrides(world, entity);
            for (name, value) in &edit.tokens {
                tokens.set(token(name)?, native_value(*value));
            }
            crate::native::token_style::set_overrides(world, entity, tokens);
            let mut token_uses = uses(world, entity);
            for (usage, name) in &edit.token_uses {
                token_uses.insert(usage.clone(), token(name)?);
            }
            set_uses(world, entity, &token_uses);
            world.entity_mut(entity).insert(AuthoredTokenUses(
                edit.token_uses
                    .iter()
                    .map(|(usage, name)| (usage.clone(), token(name).unwrap()))
                    .collect(),
            ));
            if let Some(value) = &edit.text
                && editing
                    .applied
                    .parts
                    .get(id)
                    .and_then(|previous| previous.text.as_ref())
                    != Some(value)
            {
                if let Some(mut editor) = world.get_mut::<bevy::text::EditableText>(entity) {
                    editor.editor.set_text(value);
                } else if let Some(mut text) = world.get_mut::<Text>(entity) {
                    text.0 = value.clone();
                }
            }
            for (key, value) in &edit.settings {
                if editing
                    .applied
                    .parts
                    .get(id)
                    .and_then(|previous| previous.settings.get(key))
                    != Some(value)
                {
                    crate::native::sand_settings::set(
                        world,
                        entity,
                        key,
                        Some(serde_json::from_value(value.clone()).unwrap()),
                    );
                }
            }
            if let Some(layout) = &edit.layout
                && editing
                    .applied
                    .parts
                    .get(id)
                    .and_then(|previous| previous.layout.as_ref())
                    != Some(layout)
            {
                crate::native::layout::configure(
                    world,
                    entity,
                    serde_json::from_value(layout.clone()).unwrap(),
                )
                .map_err(str::to_string)?;
                crate::native::area_mutation::disarm(
                    world,
                    entity,
                    "Layout rules changed; review area effects again.",
                );
            }
            if edit.layout_parent.is_some() || edit.layout_detached {
                if edit.layout_detached {
                    crate::native::layout::detach(world, entity);
                } else if let Some(parent) = &edit.layout_parent {
                    crate::native::layout::attach(world, entity, lookup[parent])
                        .map_err(str::to_string)?;
                }
            }
        }
        if let Some(parent) = &edit.parent {
            attach(world, entity, lookup[parent], edit.order);
        } else if let Some(order) = edit.order {
            if let Some(parent) = world.get::<ChildOf>(entity).map(ChildOf::parent) {
                attach(world, entity, parent, Some(order));
            }
        }
        if let Some(mut node) = world.get_mut::<Node>(entity) {
            let mut next = node.clone();
            style::apply(&mut next, &edit.style);
            if edit.removed {
                next.display = Display::None;
            }
            node.set_if_neq(next);
        }
        if edit.removed {
            if let Some(focus) = world
                .get_resource::<bevy::input_focus::InputFocus>()
                .and_then(|focus| focus.get())
            {
                if subtree(world, entity).contains(&focus) {
                    world
                        .resource_mut::<bevy::input_focus::InputFocus>()
                        .clear();
                }
            }
        }
        if let Some(mut item) = world.get_mut::<CanvasItem>(entity) {
            for (axis, token) in [Token::Width, Token::Height].into_iter().enumerate() {
                if let Some(value) = edit.tokens.get(&token_name(token)) {
                    item.size[axis] = native_value(*value).number();
                }
            }
            let size = item.size;
            world
                .entity_mut(entity)
                .insert(crate::native::token_style::AppliedSize(size));
        }
    }
    for (id, original) in &editing.original {
        if let Some(action) = &original.action {
            let disabled = edits
                .parts
                .get(id)
                .is_some_and(|edit| edit.native_actions_enabled == Some(false))
                || edits.parts.iter().any(|(removed, edit)| {
                    edit.removed
                        && (id == removed
                            || subtree(world, lookup[removed]).contains(&original.entity))
                });
            if disabled {
                world
                    .entity_mut(original.entity)
                    .remove::<crate::native::actions::ActionButton>();
            } else if world
                .get::<crate::native::actions::ActionButton>(original.entity)
                .is_none()
            {
                world.entity_mut(original.entity).insert(action.clone());
            }
        }
    }
    if editing.applied.groups != edits.groups {
        for original in editing.original.values() {
            if let Some(group) = original.group {
                world.entity_mut(original.entity).insert(group);
            } else {
                world
                    .entity_mut(original.entity)
                    .remove::<crate::native::canvas_selection::SandGroup>();
            }
        }
        for (id, members) in &edits.groups {
            let bytes = (0..16)
                .map(|index| u8::from_str_radix(&id[index * 2..index * 2 + 2], 16).unwrap())
                .collect::<Vec<_>>();
            for member in members {
                let entity = lookup[member];
                world
                    .entity_mut(entity)
                    .insert(crate::native::canvas_selection::SandGroup(
                        bytes.clone().try_into().unwrap(),
                    ));
                crate::native::area_mutation::disarm(
                    world,
                    entity,
                    "Authoring changed a transform group; review area effects again.",
                );
            }
        }
    }
    events::install(world, host, &lookup, &edits);
    editing.applied = edits.clone();
    let component = match component {
        ContentKind::Authored { base, .. } => ContentKind::Authored {
            base: base.clone(),
            edits,
        },
        _ => component.clone(),
    };
    world.entity_mut(host).insert((editing, Content(component)));
    Ok(())
}

fn spawn_part(world: &mut World, host: Entity, definition: &AddedPart) -> Result<Entity, String> {
    let entity = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(host),
        ))
        .id();
    if let Some(component) = &definition.component {
        world.entity_mut(entity).insert((
            Node {
                width: px(definition.size[0] as f32),
                height: px(definition.size[1] as f32),
                overflow: Overflow::clip(),
                ..default()
            },
            CanvasView::default(),
            Workspaces::default(),
            crate::native::component_push::composition::GeneratedCanvas,
            crate::native::scoped_events::IsolatedEvents,
        ));
        match super::spawn(world, entity, 1, DVec2::ZERO, component) {
            Ok(child) => {
                world.get_mut::<CanvasItem>(child).unwrap().size =
                    DVec2::from_array(definition.size).as_vec2();
            }
            Err(error) => {
                world.despawn(entity);
                return Err(error);
            }
        }
    }
    Ok(entity)
}

fn attach(world: &mut World, entity: Entity, parent: Entity, order: Option<usize>) {
    let old = world.get::<ChildOf>(entity).map(ChildOf::parent);
    if old != Some(parent) {
        world.entity_mut(entity).insert(ChildOf(parent));
    }
    if let Some(order) = order {
        let current = world
            .get::<Children>(parent)
            .and_then(|children| children.iter().position(|child| child == entity));
        if current != Some(order) {
            world.entity_mut(parent).insert_children(order, &[entity]);
        }
    }
}

fn restore_part(
    world: &mut World,
    original: &Original,
    lookup: &BTreeMap<String, Entity>,
    previous: &api::authoring::PartEdit,
) {
    if world.get_entity(original.entity).is_err() {
        return;
    }
    if let Some(mut node) = world.get_mut::<Node>(original.entity) {
        style::reset(&mut node, &original.node, &previous.style);
        if previous.removed {
            node.display = original.node.display;
        }
    }
    if previous.geometry.is_some()
        && let Some(geometry) = original.geometry
    {
        world.entity_mut(original.entity).insert(geometry);
    }
    let mut tokens = crate::native::token_style::overrides(world, original.entity);
    for name in previous.tokens.keys() {
        if let Ok(token) = token(name) {
            if let Some(value) = original.tokens.0.get(&token) {
                tokens.set(token, *value);
            } else {
                tokens.0.remove(&token);
            }
        }
    }
    crate::native::token_style::set_overrides(world, original.entity, tokens);
    let mut token_uses = uses(world, original.entity);
    for usage in previous.token_uses.keys() {
        if let Some(token) = original.uses.get(usage) {
            token_uses.insert(usage.clone(), *token);
        } else {
            token_uses.remove(usage);
        }
    }
    set_uses(world, original.entity, &token_uses);
    world
        .entity_mut(original.entity)
        .remove::<AuthoredTokenUses>();
    if previous.parent.is_some() || previous.order.is_some() {
        if let Some(parent) = original
            .parent
            .as_ref()
            .and_then(|parent| lookup.get(parent))
        {
            attach(world, original.entity, *parent, Some(original.order));
        }
    }
    if previous.text.is_some()
        && let Some(text) = &original.text
    {
        if let Some(mut editor) = world.get_mut::<bevy::text::EditableText>(original.entity) {
            editor.editor.set_text(text);
        } else if let Some(mut value) = world.get_mut::<Text>(original.entity) {
            value.0 = text.clone();
        }
    }
    for key in previous.settings.keys() {
        let value = original
            .settings
            .as_ref()
            .and_then(|settings| settings.0.get(key))
            .cloned();
        crate::native::sand_settings::set(world, original.entity, key, value);
    }
    if previous.layout.is_some() {
        if let Some(layout) = original.layout {
            let _ = crate::native::layout::configure(world, original.entity, layout);
        } else if let Some(mut text) =
            world.get_mut::<crate::native::sand_text::SandText>(original.entity)
        {
            text.layout = None;
        } else {
            world
                .entity_mut(original.entity)
                .remove::<crate::native::layout::LayoutBox>();
        }
    }
    if previous.layout_parent.is_some() || previous.layout_detached {
        crate::native::layout::detach(world, original.entity);
        if let Some(parent) = original
            .layout_parent
            .as_ref()
            .and_then(|parent| lookup.get(parent))
        {
            let _ = crate::native::layout::attach(world, original.entity, *parent);
        }
    }
}

pub(super) fn inspect(
    world: &mut World,
    root: Entity,
    element: &mut Element,
    offset: usize,
    limit: usize,
) -> Result<(), String> {
    let host = world
        .get::<Children>(root)
        .into_iter()
        .flatten()
        .find(|entity| {
            world
                .get::<Identity>(**entity)
                .is_some_and(|id| id.0 == element.placement.id)
        })
        .copied()
        .ok_or("The placement no longer exists.")?;
    let editing = index(world, host)?;
    let lookup = entities(&editing);
    let reverse: BTreeMap<_, _> = lookup.iter().map(|(id, entity)| (*entity, id)).collect();
    element.total_parts = lookup.len();
    element.next_offset = (offset + limit < lookup.len()).then_some(offset + limit);
    element.parts = lookup.iter().skip(offset).take(limit).map(|(id, entity)| {
        let token_uses = uses(world, *entity);
        let overrides = crate::native::token_style::overrides(world, *entity);
        let mut used: BTreeSet<_> = token_uses.values().copied().chain(overrides.0.keys().copied()).collect();
        if *entity == host { used.extend(crate::native::tokens::TOKENS.iter().map(|definition| definition.token)); }
        let tokens = used.into_iter().map(|token| {
            let (effective, source) = crate::native::token_style::resolve(world, *entity, token);
            let mut ancestor = Some(*entity);
            let mut source_part = None;
            while let Some(current) = ancestor {
                if crate::native::token_style::overrides(world, current).0.contains_key(&token) { source_part = reverse.get(&current).map(|id| (*id).clone()); break; }
                ancestor = world.get::<ChildOf>(current).map(ChildOf::parent);
            }
            if source == "Relation color rule" { source_part = None; }
            TokenInfo { token: token_name(token), name: token.definition().name.into(), range: token.definition().range.map(|(min, max)| [min, max]), authored: overrides.0.get(&token).copied().map(value), effective: value(effective), source: source.into(), source_part, uses: token_uses.iter().filter(|(_, used)| **used == token).map(|(usage, _)| usage.clone()).collect() }
        }).collect();
        PartInfo {
            id: id.clone(), parent: world.get::<ChildOf>(*entity).and_then(|parent| reverse.get(&parent.parent()).map(|id| (*id).clone())),
            name: world.get::<PartName>(*entity).map(|name| name.0.clone()).or_else(|| world.get::<crate::native::icons::Tooltip>(*entity).map(|tooltip| tooltip.0.clone())).or_else(|| text(world, *entity).map(|text| text.chars().take(80).collect())).unwrap_or_else(|| role(world, *entity).into()),
            role: role(world, *entity).into(), style: world.get::<Node>(*entity).map(style::capture).unwrap_or_default(),
            measured_size: world.get::<ComputedNode>(*entity).map(|node| node.size().to_array()).unwrap_or([0.0; 2]),
            geometry: world.get::<CanvasItem>(*entity).map(|item| api::Geometry { position: item.position.to_array(), size: item.size.as_dvec2().to_array() }),
            group: world.get::<crate::native::canvas_selection::SandGroup>(*entity).map(|group| group.0.iter().map(|byte| format!("{byte:02x}")).collect()),
            layout: world.get::<crate::native::layout::LayoutBox>(*entity).map(|layout| serde_json::to_value(layout.rules).unwrap()).or_else(|| world.get::<crate::native::sand_text::SandText>(*entity).and_then(|text| text.layout).map(|layout| serde_json::to_value(layout).unwrap())),
            layout_parent: layout_parent(world, *entity, &lookup),
            tokens, text: text(world, *entity), settings: world.get::<crate::native::sand_settings::Declaration>(*entity).map(|declaration| serde_json::to_value(&declaration.settings).unwrap()).unwrap_or(serde_json::Value::Null),
            removed: editing.applied.parts.get(id).is_some_and(|edit| edit.removed),
            structural_reason: (!id.starts_with("user/")).then(|| "Renderer-owned controls retain their native state when removed from presentation; reset restores them. Registered authored children are deleted with their subtree.".into()),
        }
    }).collect();
    while api::bounded(element).is_err() && !element.parts.is_empty() {
        element.parts.pop();
    }
    if element.parts.is_empty() && offset < element.total_parts {
        return Err("This part plus its native definition exceeds the inspection byte limit. Export or reduce the definition before inspecting its details.".into());
    }
    element.next_offset = (offset + element.parts.len() < element.total_parts)
        .then_some(offset + element.parts.len());
    api::bounded(element)
}

pub(super) fn maintain(world: &mut World) {
    let mut hosts: Vec<_> = world
        .query::<(Entity, &Content)>()
        .iter(world)
        .filter_map(|(entity, content)| {
            matches!(content.0, ContentKind::Authored { .. }).then(|| (entity, content.0.clone()))
        })
        .collect();
    hosts.sort_by_key(|(host, _)| {
        let mut depth = 0;
        let mut parent = world.get::<ChildOf>(*host).map(ChildOf::parent);
        while let Some(entity) = parent {
            depth += 1;
            parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        std::cmp::Reverse(depth)
    });
    for (host, component) in hosts {
        if world
            .get::<Maintained>(host)
            .is_some_and(|last| last.0.elapsed() < std::time::Duration::from_millis(200))
        {
            if let Some(editing) = world.get::<Editing>(host).cloned() {
                let lookup = entities(&editing);
                for (id, edit) in &editing.applied.parts {
                    let Some(entity) = lookup.get(id).copied() else {
                        continue;
                    };
                    if world.get_entity(entity).is_err() {
                        continue;
                    }
                    if let Some(parent) = edit.parent.as_ref().and_then(|parent| lookup.get(parent))
                    {
                        attach(world, entity, *parent, edit.order);
                    }
                    if let Some(mut node) = world.get_mut::<Node>(entity) {
                        let mut next = node.clone();
                        style::apply(&mut next, &edit.style);
                        if edit.removed {
                            next.display = Display::None;
                        }
                        node.set_if_neq(next);
                    }
                    if !edit.tokens.is_empty() {
                        let mut tokens = crate::native::token_style::overrides(world, entity);
                        for (name, value) in &edit.tokens {
                            if let Ok(token) = token(name) {
                                tokens.set(token, native_value(*value));
                            }
                        }
                        crate::native::token_style::set_overrides(world, entity, tokens);
                    }
                }
            }
            continue;
        }
        let error = reconcile(world, host, &component).err();
        if let Some(error) = &error
            && world
                .get::<Maintained>(host)
                .is_none_or(|last| last.1.as_ref() != Some(error))
        {
            crate::native::notifications::report(world, "interface::canvas-authoring", error);
        }
        world
            .entity_mut(host)
            .insert(Maintained(std::time::Instant::now(), error));
    }
}
