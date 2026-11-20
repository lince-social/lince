use crate::native::{area::InfluenceArea, area_mutation, edit_mode::label};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};

#[derive(Clone)]
struct QuantityMode(Entity, engine::area_transition::QuantityOperation);

impl crate::native::actions::Action for QuantityMode {
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
    Assign,
    Unassign,
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

fn field_values(
    changes: &engine::area_transition::RecordChanges,
) -> [(Property, &'static str, String); 5] {
    [
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
        (Property::Assign, "Assign people", changes.assign.join(", ")),
        (
            Property::Unassign,
            "Unassign people",
            changes.unassign.join(", "),
        ),
    ]
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity, entity: Entity) {
    let Some(area) = world.get::<InfluenceArea>(entity) else {
        return;
    };
    let changes = &area.changes;
    let groups = [
        (true, field_values(&changes.enter), "On entry"),
        (false, field_values(&changes.leave), "On exit"),
    ];
    let heading = label(world, panel, "Record changes", 18.0);
    world.entity_mut(heading).insert(crate::native::icons::Tooltip("Changes apply to matching Records when their Sands cross the local outline, with or without physics. Copies of one Record count together. Configured changes work automatically while the Area and property changes are enabled.".into()));
    let status = label(world, panel, "Waiting", 14.0);
    world.entity_mut(status).insert((
        area_mutation::StatusLabel(entity),
        crate::native::icons::Tooltip::default(),
    ));
    for (enter, fields, title) in groups {
        label(world, panel, title, 18.0);
        for (property, title, value) in fields {
            label(world, panel, title, 14.0);
            let input = world
                .spawn((
                    crate::native::sand::text_editor(
                        &value,
                        world.resource::<crate::native::theme::Typography>(),
                        0,
                    ),
                    AccessibilityNode::default(),
                    ChildOf(panel),
                ))
                .id();
            world.entity_mut(input).insert((
                crate::native::icons::Tooltip(match property {
                    Property::Quantity => "Set or calculate quantity on this crossing. Blank keeps it unchanged. Division by zero, overflow, and inexact results are rejected.",
                    Property::Assign => "Assign people by Record slug or UID, separated by commas.",
                    Property::Unassign => "Unassign people by Record slug or UID, separated by commas.",
                    Property::Assert => "Add Assertion names separated by commas. Remove them on the opposite crossing for a temporary Assertion.",
                    Property::Retract => "Remove Assertion names separated by commas. Add them on the opposite crossing to restore them.",
                }.into()),
                Node {
                    width: percent(100),
                    min_height: px(32),
                    flex_shrink: 0.0,
                    ..default()
                },
            ));
            {
                let mut text = world
                    .get_mut::<EditableText>(input)
                    .expect("the crossing field is spawned as editable text");
                text.allow_newlines = false;
                text.visible_lines = Some(1.0);
                text.max_characters = Some(2048);
            }
            world
                .get_mut::<AccessibilityNode>(input)
                .expect("the crossing field bundle includes an accessibility node")
                .set_label(title);
            let status = label(world, panel, "", 12.0);
            world.entity_mut(status).insert(crate::native::icons::Tooltip("Not saved. Use a decimal quantity and up to 16 Assertion names or people. Do not add and remove the same name on one crossing.".into()));
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
                world
                    .entity_mut(input)
                    .insert(crate::native::tutorial::TutorialField::Quantity(
                        entity, enter,
                    ));
                let row = crate::native::area_panel::row(world, panel);
                for operation in engine::area_transition::QuantityOperation::ALL {
                    let button = world
                        .spawn((
                            crate::native::sand::Square,
                            crate::native::sand::button(0),
                            ChildOf(row),
                            Node {
                                min_width: px(32),
                                min_height: px(28),
                                ..default()
                            },
                            crate::native::icons::Tooltip(format!("{operation:?} quantity")),
                            crate::native::actions::ActionButton::new(
                                input,
                                crate::native::actions![QuantityMode(input, operation)],
                            ),
                        ))
                        .id();
                    label(world, button, operation.symbol(), 18.0);
                }
            }
        }
    }
}

pub(crate) struct Preparation {
    fields: bevy::ecs::query::QueryState<(Entity, &'static Field, &'static EditableText)>,
    edits: Vec<(Entity, Entity, Entity, bool, Property, Entity, String)>,
}

impl FromWorld for Preparation {
    fn from_world(world: &mut World) -> Self {
        Self {
            fields: world.query(),
            edits: Vec::new(),
        }
    }
}

pub(crate) fn autosave(world: &mut World, mut preparation: Local<Preparation>) {
    let _capture = nucleus::performance::Span::new("area-change-editor-maintenance");
    let Preparation { fields, edits } = &mut *preparation;
    edits.clear();
    edits.extend(
        fields
            .iter(world)
            .filter(|(_, field, text)| {
                !text.is_composing()
                    && !crate::native::record_view::pending_text(text)
                    && world
                        .get::<crate::native::edit_mode::EditMode>(field.root)
                        .is_some_and(|mode| mode.enabled && mode.areas)
                    && text.value() != field.observed.as_str()
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
            }),
    );
    for (entity, root, target, enter, property, status, value) in edits.drain(..) {
        if !crate::native::area_panel::owns(world, root, target) {
            continue;
        }
        area_mutation::disarm(
            world,
            target,
            "Property changes inactive after an edit. Configured changes resume automatically.",
        );
        let Some(area) = world.get::<InfluenceArea>(target) else {
            continue;
        };
        let mut changes = if enter {
            area.changes.enter.clone()
        } else {
            area.changes.leave.clone()
        };
        match property {
            Property::Quantity => {
                changes.quantity = (!value.trim().is_empty()).then(|| value.trim().to_string())
            }
            Property::Assert => changes.assert = names(&value),
            Property::Retract => changes.retract = names(&value),
            Property::Assign => changes.assign = names(&value),
            Property::Unassign => changes.unassign = names(&value),
        }
        let edit = crate::native::area::editing::Edit::Crossing { enter, changes };
        let valid = edit.valid(area);
        if valid
            && edit.differs(area)
            && let Some(mut area) = world.get_mut::<InfluenceArea>(target)
        {
            edit.apply(&mut area);
        }
        if let Some(mut field) = world.get_mut::<Field>(entity) {
            field.observed = value;
            field.valid = valid;
        }
        let message = if valid { "" } else { "Invalid value" };
        if let Some(mut text) = world.get_mut::<Text>(status)
            && text.0 != message
        {
            message.clone_into(&mut text.0);
        }
    }
    if fields.iter(world).next().is_none() {
        *edits = Vec::new();
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
        .run_system_cached_with(has_invalid_fields, area)
        .expect("inspect Area change fields")
}

fn has_invalid_fields(In(area): In<Entity>, fields: Query<&Field>) -> bool {
    fields
        .iter()
        .any(|field| field.area == area && !field.valid)
}

#[allow(
    clippy::unwrap_used,
    reason = "Area mutation Laboratory fixtures must fail when an expected control, application, or Record is missing"
)]
pub(crate) mod tests {
    use super::*;
    use crate::native::edit_mode::EditAction;
    use crate::native::{
        actions::Action,
        area::{Property as MatchProperty, PropertyRule, ShapeKind},
        area_panel::{AreaAction, AreaEditor},
    };

    #[cfg_attr(test, test)]
    fn boundary_fields_save_separately_and_invalid_drafts_block_a_grant() {
        let mut app = App::new();
        crate::native::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::native::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::native::workspace::WorkspacePlugin,
                crate::native::edit_mode::EditModePlugin,
                area_mutation::AreaMutationPlugin,
            ));
        let root = app
            .world_mut()
            .spawn(crate::native::container::BoxRoot)
            .id();
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
        for (enter, property) in [(true, Property::Assign), (false, Property::Unassign)] {
            let input = app
                .world_mut()
                .query::<(Entity, &Field)>()
                .iter(app.world())
                .find(|(_, field)| {
                    field.enter == enter
                        && std::mem::discriminant(&field.property)
                            == std::mem::discriminant(&property)
                })
                .unwrap()
                .0;
            app.world_mut()
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text("alice, bob, alice");
            app.update();
        }
        let saved = app.world().get::<InfluenceArea>(area).unwrap();
        assert_eq!(saved.changes.enter.assign, vec!["alice", "bob"]);
        assert_eq!(saved.changes.leave.unassign, vec!["alice", "bob"]);
        assert!(saved.changes.enter.unassign.is_empty());
        assert!(saved.changes.leave.assign.is_empty());
        assert_eq!(saved.changes.enter.quantity.as_deref(), Some("-3.125"));
        assert_eq!(saved.changes.leave.quantity, None);
        let restored: InfluenceArea =
            serde_json::from_value(serde_json::to_value(saved).unwrap()).unwrap();
        assert_eq!(restored.changes, saved.changes);
    }

    crate::native::laboratory_cases! {
        boundary_fields_save_separately_and_invalid_drafts_block_a_grant,
    }
}
