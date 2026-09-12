use crate::{
    area::InfluenceArea,
    area_mutation,
    edit_mode::{EditAction, control, label},
};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText};
use engine::area_transition::RecordChanges;

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
    label(world, panel, "Record changes at the boundary", 20.0);
    label(
        world,
        panel,
        "Changes apply to matching Records when their Sands cross the outline, with or without physics. Copies of one Record count together. Editing or switching workspaces disarms the Area.",
        14.0,
    );
    let status = label(world, panel, "Disarmed", 14.0);
    world
        .entity_mut(status)
        .insert(area_mutation::StatusLabel(entity));
    for (enter, changes, title) in [
        (true, &area.changes.enter, "On entry"),
        (false, &area.changes.leave, "On exit"),
    ] {
        label(world, panel, title, 18.0);
        for (property, title, value) in [
            (
                Property::Quantity,
                "Set quantity (blank keeps it)",
                changes.quantity.clone().unwrap_or_default(),
            ),
            (
                Property::Assert,
                "Add Assertions (names separated by commas)",
                changes.assert.join(", "),
            ),
            (
                Property::Retract,
                "Remove Assertions (names separated by commas)",
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
            world.entity_mut(input).insert(Field {
                root,
                area: entity,
                enter,
                property,
                observed: value,
                status,
                valid: true,
            });
        }
    }
    label(
        world,
        panel,
        "For a temporary Assertion, add it on entry and remove it on exit. Reverse those choices to remove it while inside. Quantities are set to the values you choose; blank makes no change.",
        14.0,
    );
    control(
        world,
        root,
        panel,
        EditAction::Area(crate::area_panel::AreaAction::PreviewChanges),
        "Preview Record changes",
    );
    if area_mutation::previewed(world, entity) {
        for (title, changes) in [
            ("Entry", &area.changes.enter),
            ("Exit", &area.changes.leave),
        ] {
            label(
                world,
                panel,
                &format!("{title}: {}", describe(changes)),
                14.0,
            );
        }
        label(
            world,
            panel,
            "Arming grants these changes for future crossings in this workspace, for up to 128 requests. Existing Records are not changed now. Disarm stays available beside Edit mode.",
            14.0,
        );
        if !area_mutation::armed(world, entity) {
            control(
                world,
                root,
                panel,
                EditAction::Area(crate::area_panel::AreaAction::ArmChanges),
                "Grant these changes and arm Area",
            );
        }
    }
    control(
        world,
        root,
        panel,
        EditAction::Area(crate::area_panel::AreaAction::DisarmChanges),
        "Disarm Record changes",
    );
}

fn describe(changes: &RecordChanges) -> String {
    let mut parts = Vec::new();
    if let Some(quantity) = &changes.quantity {
        parts.push(format!("set quantity to {quantity}"));
    }
    if !changes.assert.is_empty() {
        parts.push(format!("add {}", changes.assert.join(", ")));
    }
    if !changes.retract.is_empty() {
        parts.push(format!("remove {}", changes.retract.join(", ")));
    }
    if parts.is_empty() {
        "keep properties unchanged".into()
    } else {
        parts.join("; ")
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
            "Disarmed after an edit. Preview again to arm.",
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
            text.0 = if valid { "" } else { "Not saved. Use a decimal quantity and up to 16 Assertion names. Do not add and remove the same name in one crossing." }.into();
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
