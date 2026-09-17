use crate::{area::InfluenceArea, area_mutation, edit_mode::label};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};

#[derive(Clone)]
struct QuantityMode(Entity, engine::area_transition::QuantityOperation);

impl crate::actions::Action for QuantityMode {
    fn apply(&self, world: &mut World, _: Entity) {
        let Some(mut text) = world.get_mut::<EditableText>(self.0) else {
            return;
        };
        let value = text.value().to_string();
        let operand = engine::area_transition::QuantityOperation::parse(&value)
            .map(|(_, operand)| operand.to_string())
            .unwrap_or_else(|| "0".into());
        text.editor
            .set_text(&format!("{}{operand}", self.1.prefix()));
    }
}

#[derive(Clone, Copy)]
enum Property {
    Quantity,
    Assert,
    Retract,
}

#[derive(Component)]
struct Field {
    root: Entity,
    area: Entity,
    enter: bool,
    property: Property,
    observed: String,
    status: Entity,
    valid: bool,
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity, entity: Entity) {
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    let heading = label(world, panel, "Record changes", 18.0);
    world.entity_mut(heading).insert(crate::icons::Tooltip("Changes apply to matching Records when their Sands cross the local outline, with or without physics. Copies of one Record count together. Configured changes work automatically while the Area and property changes are enabled.".into()));
    let status = label(world, panel, "Waiting", 14.0);
    world.entity_mut(status).insert((
        area_mutation::StatusLabel(entity),
        crate::icons::Tooltip::default(),
    ));
    for (enter, changes, title) in [
        (true, &area.changes.enter, "On entry"),
        (false, &area.changes.leave, "On exit"),
    ] {
        label(world, panel, title, 18.0);
        for (property, title, value) in [
            (
                Property::Quantity,
                "Quantity",
                changes.quantity.clone().unwrap_or_default(),
            ),
            (
                Property::Assert,
                "Add Assertions",
                changes.assert.join(", "),
            ),
            (
                Property::Retract,
                "Remove Assertions",
                changes.retract.join(", "),
            ),
        ] {
            label(world, panel, title, 14.0);
            let input = world
                .spawn((
                    crate::sand::text_editor(
                        &value,
                        world.resource::<crate::theme::Typography>(),
                        0,
                    ),
                    AccessibilityNode::default(),
                    ChildOf(panel),
                ))
                .id();
            world.entity_mut(input).insert((
                crate::icons::Tooltip(match property {
                    Property::Quantity => "Set or calculate quantity on this crossing. Blank keeps it unchanged. Division by zero, overflow, and inexact results are rejected.",
                    Property::Assert => "Add Assertion names separated by commas. Remove them on the opposite crossing for a temporary Assertion.",
                    Property::Retract => "Remove Assertion names separated by commas. Add them on the opposite crossing to restore them.",
                }.into()),
                EditableText {
                    allow_newlines: false,
                    visible_lines: Some(1.0),
                    max_characters: Some(2048),
                    ..crate::sand::editable(&value)
                },
                Node {
                    width: percent(100),
                    min_height: px(32),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            world
                .get_mut::<AccessibilityNode>(input)
                .unwrap()
                .set_label(title);
            let status = label(world, panel, "", 12.0);
            world.entity_mut(status).insert(crate::icons::Tooltip("Not saved. Use a decimal quantity and up to 16 Assertion names. Do not add and remove the same name on one crossing.".into()));
            world.entity_mut(input).insert(Field {
                root,
                area: entity,
                enter,
                property,
                observed: value,
                status,
                valid: true,
            });
            if matches!(property, Property::Quantity) {
                let row = crate::area_panel::row(world, panel);
                for operation in engine::area_transition::QuantityOperation::ALL {
                    let button = world
                        .spawn((
                            crate::sand::Square,
                            crate::sand::button(0),
                            ChildOf(row),
                            Node {
                                min_width: px(32),
                                min_height: px(28),
                                ..default()
                            },
                            crate::icons::Tooltip(format!("{operation:?} quantity")),
                            crate::actions::ActionButton::new(
                                input,
                                crate::actions![QuantityMode(input, operation)],
                            ),
                        ))
                        .id();
                    label(world, button, operation.symbol(), 18.0);
                }
            }
        }
    }
}

pub(crate) fn autosave(world: &mut World) {
    let edits: Vec<_> = world
        .query::<(Entity, &Field, &EditableText)>()
        .iter(world)
        .filter(|(_, field, text)| {
            !text.is_composing()
                && !crate::record_view::pending_text(text)
                && text.value().to_string() != field.observed
                && world
                    .get::<crate::edit_mode::EditMode>(field.root)
                    .is_some_and(|mode| mode.enabled && mode.areas)
        })
        .map(|(entity, field, text)| {
            (
                entity,
                field.root,
                field.area,
                field.enter,
                field.property,
                field.status,
                text.value().to_string(),
            )
        })
        .collect();
    for (entity, root, target, enter, property, status, value) in edits {
        if !crate::area_panel::owns(world, root, target) {
            continue;
        }
        area_mutation::disarm(
            world,
            target,
            "Property changes inactive after an edit. Configured changes resume automatically.",
        );
        let mut area = world.get::<InfluenceArea>(target).unwrap().clone();
        let changes = if enter {
            &mut area.changes.enter
        } else {
            &mut area.changes.leave
        };
        match property {
            Property::Quantity => {
                changes.quantity = (!value.trim().is_empty()).then(|| value.trim().to_string())
            }
            Property::Assert => changes.assert = names(&value),
            Property::Retract => changes.retract = names(&value),
        }
        let valid = area.validate();
        if valid {
            *world.get_mut::<InfluenceArea>(target).unwrap() = area;
        }
        world.get_mut::<Field>(entity).unwrap().observed = value;
        world.get_mut::<Field>(entity).unwrap().valid = valid;
        if let Some(mut text) = world.get_mut::<Text>(status) {
            text.0 = if valid { "" } else { "Invalid value" }.into();
        }
    }
}

fn names(value: &str) -> Vec<String> {
    let mut names: Vec<_> = value
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    names
}

pub(crate) fn invalid_fields(world: &mut World, area: Entity) -> bool {
    world
        .query::<&Field>()
        .iter(world)
        .any(|field| field.area == area && !field.valid)
}

pub(crate) mod tests {
    use super::*;
    use crate::edit_mode::EditAction;
    use crate::{
        actions::Action,
        area::{Property as MatchProperty, PropertyRule, ShapeKind},
        area_panel::{AreaAction, AreaEditor},
    };

    #[cfg_attr(test, test)]
    fn boundary_fields_save_separately_and_invalid_drafts_block_a_grant() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                area_mutation::AreaMutationPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Areas.apply(app.world_mut(), root);
        EditAction::Area(AreaAction::Add(ShapeKind::Square)).apply(app.world_mut(), root);
        let area = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        app.world_mut()
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .rules = vec![PropertyRule {
            property: MatchProperty::Quantity,
            value: "0".into(),
        }];
        let input = app
            .world_mut()
            .query::<(Entity, &Field)>()
            .iter(app.world())
            .find(|(_, field)| field.enter && matches!(field.property, Property::Quantity))
            .unwrap()
            .0;
        for (value, valid) in [("-3.125", true), ("NaN", false)] {
            app.world_mut()
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            assert_eq!(!invalid_fields(app.world_mut(), area), valid);
            area_mutation::preview(app.world_mut(), root, area);
            assert_eq!(
                area_mutation::previewed(app.world(), area),
                valid,
                "area: {:?}; pending: {:?}; value: {}; observed: {}",
                app.world().get::<InfluenceArea>(area),
                app.world()
                    .get::<EditableText>(input)
                    .unwrap()
                    .pending_edits,
                app.world().get::<EditableText>(input).unwrap().value(),
                app.world().get::<Field>(input).unwrap().observed
            );
        }
        let saved = app.world().get::<InfluenceArea>(area).unwrap();
        assert_eq!(saved.changes.enter.quantity.as_deref(), Some("-3.125"));
        assert_eq!(saved.changes.leave.quantity, None);
        let restored: InfluenceArea =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert_eq!(restored.changes, saved.changes);
    }

    crate::laboratory_cases! {
        boundary_fields_save_separately_and_invalid_drafts_block_a_grant,
    }
}
