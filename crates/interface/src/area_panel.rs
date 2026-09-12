use crate::{
    area::{
        AreaShape, Direction, InfluenceArea, MAX_AREAS, MAX_RULES, Property, PropertyRule,
        ShapeKind, spawn_area,
    },
    canvas::CanvasView,
    edit_mode::{EditAction, EditMode, control, label},
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{a11y::AccessibilityNode, math::DVec2, prelude::*, text::EditableText};

#[derive(Component, Default)]
pub struct AreaEditor {
    pub selected: Option<Entity>,
    pub tool: Option<ShapeKind>,
    pub points: Vec<DVec2>,
    pub cursor: Option<DVec2>,
    pub dragging: bool,
    pub workspace: u64,
    pub notice: String,
    pub redraw: Option<Entity>,
}

impl AreaEditor {
    pub fn cancel(&mut self) {
        self.tool = None;
        self.points.clear();
        self.cursor = None;
        self.dragging = false;
        self.redraw = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AreaAction {
    Add(ShapeKind),
    Draw(ShapeKind),
    Redraw,
    Select(Entity),
    Shape(ShapeKind),
    Direction(Direction),
    MatchAll,
    AddRule,
    RemoveRule(usize),
    Property(usize, Property),
    Finish,
    Cancel,
    Remove,
    PreviewChanges,
    ArmChanges,
    DisarmChanges,
}

pub(crate) fn owns(world: &World, root: Entity, entity: Entity) -> bool {
    world.get::<InfluenceArea>(entity).is_some()
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|parent| parent.parent() == root)
        && world.get::<Workspaces>(root).is_some_and(|spaces| {
            world
                .get::<WorkspaceMember>(entity)
                .is_some_and(|member| member.0 == spaces.active)
        })
}

pub(crate) fn insert(world: &mut World, root: Entity, mut area: InfluenceArea) -> Option<Entity> {
    if let Some(entity) = world
        .get::<AreaEditor>(root)
        .and_then(|editor| editor.redraw)
        .filter(|entity| owns(world, root, *entity))
    {
        if !area.validate() {
            return None;
        }
        let mut target = world.get_mut::<InfluenceArea>(entity)?;
        target.shape = area.shape;
        target.center = area.center;
        target.size = area.size;
        let mut editor = world.get_mut::<AreaEditor>(root)?;
        editor.cancel();
        editor.notice.clear();
        return Some(entity);
    }
    let count = world
        .query::<(&InfluenceArea, &ChildOf)>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == root)
        .count();
    if count >= MAX_AREAS {
        world.get_mut::<AreaEditor>(root)?.notice = "This Box already has 256 areas.".into();
        return None;
    }
    let workspace = world.get::<Workspaces>(root)?.active;
    if area.name == "Area of influence" {
        let shape = match area.shape.kind() {
            ShapeKind::Square => "Square",
            ShapeKind::Circle => "Circle",
            ShapeKind::Drawn => "Drawn",
        };
        let mut index = 1;
        loop {
            let name = format!("{shape} area {index}");
            if !world
                .query::<(&InfluenceArea, &ChildOf)>()
                .iter(world)
                .any(|(area, parent)| parent.parent() == root && area.name == name)
            {
                area.name = name;
                break;
            }
            index += 1;
        }
    }
    let entity = spawn_area(world, root, workspace, area)?;
    let mut editor = world.get_mut::<AreaEditor>(root)?;
    editor.selected = Some(entity);
    editor.cancel();
    editor.notice.clear();
    if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
        inspection.selected = Some(entity);
    }
    Some(entity)
}

pub(crate) fn apply(world: &mut World, root: Entity, action: AreaAction) {
    if !world.get::<EditMode>(root).is_some_and(|mode| mode.enabled) {
        return;
    }
    if world.get::<AreaEditor>(root).is_none() {
        world.entity_mut(root).insert(AreaEditor::default());
    }
    let selected = world
        .get::<AreaEditor>(root)
        .and_then(|editor| editor.selected)
        .filter(|entity| owns(world, root, *entity));
    match action {
        AreaAction::PreviewChanges | AreaAction::ArmChanges | AreaAction::DisarmChanges => {
            if let Some(entity) = selected {
                match action {
                    AreaAction::PreviewChanges => {
                        crate::area_mutation::preview(world, root, entity)
                    }
                    AreaAction::ArmChanges => crate::area_mutation::arm(world, root, entity),
                    _ => crate::area_mutation::disarm(
                        world,
                        entity,
                        "Disarmed. Already submitted changes may still finish.",
                    ),
                }
            }
        }
        AreaAction::Add(kind) => {
            let Some(view) = world.get::<CanvasView>(root) else {
                return;
            };
            let shape = match kind {
                ShapeKind::Square => AreaShape::Square,
                ShapeKind::Circle => AreaShape::Circle,
                ShapeKind::Drawn => return,
            };
            let area = InfluenceArea::new(shape, view.center, DVec2::splat(320.0));
            world.get_mut::<AreaEditor>(root).unwrap().cancel();
            insert(world, root, area);
        }
        AreaAction::Redraw => {
            let Some(entity) = selected else { return };
            let workspace = world.get::<Workspaces>(root).unwrap().active;
            let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
            editor.cancel();
            editor.tool = Some(ShapeKind::Drawn);
            editor.redraw = Some(entity);
            editor.workspace = workspace;
            editor.notice.clear();
        }
        AreaAction::Draw(kind) => {
            let workspace = world.get::<Workspaces>(root).unwrap().active;
            let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
            editor.cancel();
            editor.tool = Some(kind);
            editor.workspace = workspace;
            editor.notice.clear();
        }
        AreaAction::Select(entity) if owns(world, root, entity) => {
            let mut editor = world.get_mut::<AreaEditor>(root).unwrap();
            editor.cancel();
            editor.selected = Some(entity);
            editor.notice.clear();
            if let Some(mut inspection) = world.get_mut::<crate::inspection::Inspection>(root) {
                inspection.selected = Some(entity);
            }
        }
        AreaAction::Cancel => world.get_mut::<AreaEditor>(root).unwrap().cancel(),
        AreaAction::Finish => {
            let points = &world.get::<AreaEditor>(root).unwrap().points;
            if let Some(area) = InfluenceArea::drawn(points) {
                insert(world, root, area);
            } else {
                world.get_mut::<AreaEditor>(root).unwrap().notice =
                    "Use at least three points. The outline cannot cross or touch itself.".into();
            }
        }
        AreaAction::Remove => {
            if let Some(entity) = selected {
                crate::area_mutation::disarm(world, entity, "Disarmed after deletion.");
                world.despawn(entity);
                world.get_mut::<AreaEditor>(root).unwrap().selected = None;
            }
        }
        _ => {
            let Some(entity) = selected else { return };
            let mut area = world.get_mut::<InfluenceArea>(entity).unwrap();
            match action {
                AreaAction::Shape(kind) => {
                    area.shape = match kind {
                        ShapeKind::Square => AreaShape::Square,
                        ShapeKind::Circle => AreaShape::Circle,
                        ShapeKind::Drawn => return,
                    };
                    area.size = [area.size[0]; 2];
                }
                AreaAction::Direction(direction) => area.direction = direction,
                AreaAction::MatchAll => area.match_all = !area.match_all,
                AreaAction::AddRule if area.rules.len() < MAX_RULES => {
                    area.rules.push(PropertyRule {
                        property: Property::Kind,
                        value: "record".into(),
                    })
                }
                AreaAction::RemoveRule(index) if index < area.rules.len() => {
                    area.rules.remove(index);
                }
                AreaAction::Property(index, property) if index < area.rules.len() => {
                    let rule = &mut area.rules[index];
                    rule.property = property;
                    if property == Property::Quantity && !rule.validate() {
                        rule.value = "0".into();
                    }
                }
                _ => {}
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Field {
    Name,
    Center(usize),
    Size(usize),
    Strength,
    Rule(usize),
}

#[derive(Component)]
struct AreaField {
    root: Entity,
    area: Entity,
    field: Field,
    observed: String,
    status: Entity,
}

fn field(
    world: &mut World,
    root: Entity,
    parent: Entity,
    area: Entity,
    field: Field,
    title: &str,
    value: String,
) {
    label(world, parent, title, 14.0);
    let bundle = crate::sand::text_editor(&value, world.resource::<crate::theme::Typography>(), 0);
    let input = world
        .spawn((bundle, AccessibilityNode::default(), ChildOf(parent)))
        .id();
    world.entity_mut(input).insert((
        EditableText {
            allow_newlines: false,
            visible_lines: Some(1.0),
            max_characters: Some(if matches!(field, Field::Name) {
                80
            } else {
                4096
            }),
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
    let status = label(world, parent, "", 12.0);
    world.entity_mut(input).insert(AreaField {
        root,
        area,
        field,
        observed: value,
        status,
    });
}

pub(crate) fn autosave(world: &mut World) {
    let edits: Vec<_> = world
        .query::<(Entity, &AreaField, &EditableText)>()
        .iter(world)
        .filter(|(_, field, text)| {
            !text.is_composing()
                && text.pending_paste.is_none()
                && text.value().to_string() != field.observed
                && world
                    .get::<EditMode>(field.root)
                    .is_some_and(|mode| mode.enabled && mode.areas)
        })
        .map(|(entity, field, text)| {
            (
                entity,
                field.root,
                field.area,
                field.field,
                field.status,
                text.value().to_string(),
            )
        })
        .collect();
    for (entity, root, target, field, status, value) in edits {
        if !owns(world, root, target) {
            continue;
        }
        crate::area_mutation::disarm(
            world,
            target,
            "Disarmed after an edit. Preview again to arm.",
        );
        let mut next = world.get::<InfluenceArea>(target).unwrap().clone();
        let number = value
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|number| number.is_finite());
        let parsed = match field {
            Field::Name => {
                next.name = value.clone();
                true
            }
            Field::Rule(index) => {
                if let Some(rule) = next.rules.get_mut(index) {
                    rule.value = value.clone();
                    true
                } else {
                    false
                }
            }
            _ => {
                if let Some(number) = number {
                    match field {
                        Field::Center(axis) => next.center[axis] = number,
                        Field::Size(axis) => {
                            next.size[axis] = number;
                            if next.shape.kind() != ShapeKind::Drawn {
                                next.size = [number; 2];
                            }
                        }
                        Field::Strength => next.strength = number,
                        _ => {}
                    }
                    true
                } else {
                    false
                }
            }
        };
        let valid = parsed && next.validate();
        if valid {
            *world.get_mut::<InfluenceArea>(target).unwrap() = next;
        }
        world.get_mut::<AreaField>(entity).unwrap().observed = value;
        if let Some(mut text) = world.get_mut::<Text>(status) {
            text.0 = if valid {
                ""
            } else {
                "Not saved. Use a valid value; size 1–100000, force 0–1000000."
            }
            .into();
        }
    }
    let updates: Vec<_> = world
        .query::<(Entity, &AreaField, &EditableText)>()
        .iter(world)
        .filter_map(|(entity, field, text)| {
            if world.resource::<bevy::input_focus::InputFocus>().get() == Some(entity)
                || crate::record_view::pending_text(text)
                || text.is_composing()
                || text.value().to_string() != field.observed
                || world
                    .get::<Text>(field.status)
                    .is_none_or(|status| !status.0.is_empty())
            {
                return None;
            }
            let area = world.get::<InfluenceArea>(field.area)?;
            let value = match field.field {
                Field::Center(axis) => area.center[axis],
                Field::Size(axis) => area.size[axis],
                _ => return None,
            };
            (field.observed.parse::<f64>().ok() != Some(value))
                .then_some((entity, value.to_string()))
        })
        .collect();
    for (entity, value) in updates {
        world
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor
            .set_text(&value);
        world.get_mut::<AreaField>(entity).unwrap().observed = value;
    }
}

fn button(world: &mut World, root: Entity, parent: Entity, action: AreaAction, title: &str) {
    control(world, root, parent, EditAction::Area(action), title);
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity) {
    if world.get::<AreaEditor>(root).is_none() {
        world.entity_mut(root).insert(AreaEditor::default());
    }
    label(world, panel, "Areas of influence", 22.0);
    crate::workspace_config::controls(world, root, panel);
    label(
        world,
        panel,
        "Set forces on matching Sands. Arrows show their direction.",
        14.0,
    );
    for (action, title) in [
        (AreaAction::Add(ShapeKind::Square), "Add square"),
        (AreaAction::Add(ShapeKind::Circle), "Add circle"),
        (
            AreaAction::Draw(ShapeKind::Square),
            "Drag a square on canvas",
        ),
        (
            AreaAction::Draw(ShapeKind::Circle),
            "Drag a circle on canvas",
        ),
        (AreaAction::Draw(ShapeKind::Drawn), "Draw a closed outline"),
    ] {
        button(world, root, panel, action, title);
    }
    let editor = world.get::<AreaEditor>(root).unwrap();
    let notice = editor.notice.clone();
    if let Some(tool) = editor.tool {
        label(
            world,
            panel,
            if tool == ShapeKind::Drawn {
                "Click to start, move to draw freely, then click to close with a straight line. Escape cancels."
            } else {
                "Drag on the canvas to set the area. Escape cancels."
            },
            14.0,
        );
        if tool == ShapeKind::Drawn {
            button(world, root, panel, AreaAction::Finish, "Close outline");
        }
        button(world, root, panel, AreaAction::Cancel, "Cancel drawing");
    }
    if !notice.is_empty() {
        label(world, panel, &notice, 14.0);
    }
    let mut areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter(|(entity, _)| owns(world, root, *entity))
        .map(|(entity, area)| (entity, area.name.clone(), area.id.clone()))
        .collect();
    areas.sort_by(|a, b| a.2.cmp(&b.2));
    label(
        world,
        panel,
        "Click inside an area or choose it here to edit.",
        14.0,
    );
    for (entity, name, _) in areas {
        button(world, root, panel, AreaAction::Select(entity), &name);
    }
    let Some(entity) = world
        .get::<AreaEditor>(root)
        .unwrap()
        .selected
        .filter(|entity| owns(world, root, *entity))
    else {
        return;
    };
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    crate::area_mutation_panel::render(world, root, panel, entity);
    field(
        world,
        root,
        panel,
        entity,
        Field::Name,
        "Name",
        area.name.clone(),
    );
    label(
        world,
        panel,
        &format!("Shape: {:?}", area.shape.kind()),
        16.0,
    );
    button(
        world,
        root,
        panel,
        AreaAction::Shape(ShapeKind::Square),
        "Use square",
    );
    button(
        world,
        root,
        panel,
        AreaAction::Shape(ShapeKind::Circle),
        "Use circle",
    );
    button(
        world,
        root,
        panel,
        AreaAction::Redraw,
        "Draw a new outline for this area",
    );
    field(
        world,
        root,
        panel,
        entity,
        Field::Center(0),
        "Horizontal position",
        area.center[0].to_string(),
    );
    field(
        world,
        root,
        panel,
        entity,
        Field::Center(1),
        "Vertical position",
        area.center[1].to_string(),
    );
    field(
        world,
        root,
        panel,
        entity,
        Field::Size(0),
        if area.shape.kind() == ShapeKind::Drawn {
            "Width"
        } else {
            "Size"
        },
        area.size[0].to_string(),
    );
    if area.shape.kind() == ShapeKind::Drawn {
        field(
            world,
            root,
            panel,
            entity,
            Field::Size(1),
            "Height",
            area.size[1].to_string(),
        );
    }
    label(world, panel, &format!("Force: {:?}", area.direction), 16.0);
    button(
        world,
        root,
        panel,
        AreaAction::Direction(Direction::Attract),
        "Attract toward center",
    );
    button(
        world,
        root,
        panel,
        AreaAction::Direction(Direction::Repel),
        "Repel from center",
    );
    field(
        world,
        root,
        panel,
        entity,
        Field::Strength,
        "Force strength",
        area.strength.to_string(),
    );
    label(
        world,
        panel,
        "Applies while a Sand's center is inside. At the exact center, force is zero. Pinned Sands stay fixed.",
        14.0,
    );
    button(
        world,
        root,
        panel,
        AreaAction::MatchAll,
        if area.match_all {
            "Match all properties"
        } else {
            "Match any property"
        },
    );
    label(
        world,
        panel,
        "Values must match exactly. No properties means no force.",
        14.0,
    );
    for (index, rule) in area.rules.iter().enumerate() {
        label(
            world,
            panel,
            &format!("Property {}: {}", index + 1, rule.property.name()),
            16.0,
        );
        let row = world
            .spawn((
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(4),
                    row_gap: px(4),
                    ..default()
                },
                ChildOf(panel),
            ))
            .id();
        for property in Property::ALL {
            button(
                world,
                root,
                row,
                AreaAction::Property(index, property),
                property.name(),
            );
        }
        field(
            world,
            root,
            panel,
            entity,
            Field::Rule(index),
            "Equals",
            rule.value.clone(),
        );
        button(
            world,
            root,
            panel,
            AreaAction::RemoveRule(index),
            "Remove property",
        );
    }
    if area.rules.len() < MAX_RULES {
        button(world, root, panel, AreaAction::AddRule, "Add property");
    }
    button(world, root, panel, AreaAction::Remove, "Remove area");
}

pub(crate) mod tests {
    use super::*;
    use crate::actions::Action;

    fn fixture() -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
            ))
            .add_systems(
                PostUpdate,
                autosave
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Areas.apply(app.world_mut(), root);
        (app, root)
    }

    #[cfg_attr(test, test)]
    fn selecting_the_same_area_preserves_panel_fields_and_scroll() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Square)).apply(app.world_mut(), root);
        let entity = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let panel = app.world().get::<EditMode>(root).unwrap().panel;
        let children: Vec<_> = app.world().get::<Children>(panel).unwrap().iter().collect();
        app.world_mut()
            .get_mut::<ScrollPosition>(panel)
            .unwrap()
            .0
            .y = 160.0;
        EditAction::Area(AreaAction::Select(entity)).apply(app.world_mut(), root);
        assert_eq!(
            app.world()
                .get::<Children>(panel)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
        assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 160.0);
        crate::sand_placement::PlacementAction::Delete.apply(app.world_mut(), entity);
        assert!(app.world().get_entity(entity).is_err());
        assert_eq!(app.world().get::<AreaEditor>(root).unwrap().selected, None);
    }

    #[cfg_attr(test, test)]
    fn area_editor_saves_valid_values_keeps_invalid_drafts_and_enforces_ownership() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Circle)).apply(app.world_mut(), root);
        let area = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let field = app
            .world_mut()
            .query::<(Entity, &AreaField)>()
            .iter(app.world())
            .find(|(_, field)| matches!(field.field, Field::Strength))
            .unwrap()
            .0;
        for (value, expected) in [("-10", 100.0), ("NaN", 100.0), ("42.5", 42.5)] {
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            assert_eq!(
                app.world().get::<InfluenceArea>(area).unwrap().strength,
                expected
            );
            assert_eq!(
                app.world()
                    .get::<EditableText>(field)
                    .unwrap()
                    .value()
                    .to_string(),
                value
            );
        }
        let foreign_root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        let foreign = spawn_area(
            app.world_mut(),
            foreign_root,
            1,
            InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(200.0)),
        )
        .unwrap();
        EditAction::Area(AreaAction::Select(foreign)).apply(app.world_mut(), root);
        assert_eq!(
            app.world().get::<AreaEditor>(root).unwrap().selected,
            Some(area)
        );
        assert_eq!(
            app.world().get::<InfluenceArea>(foreign).unwrap().strength,
            100.0
        );
        EditAction::Close.apply(app.world_mut(), root);
        EditAction::Area(AreaAction::Remove).apply(app.world_mut(), root);
        assert!(app.world().get::<InfluenceArea>(area).is_some());
    }

    #[cfg_attr(test, test)]
    fn redraw_keeps_identity_rules_and_force_and_escape_cancels_without_replacing() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Square)).apply(app.world_mut(), root);
        EditAction::Area(AreaAction::AddRule).apply(app.world_mut(), root);
        EditAction::Area(AreaAction::Direction(Direction::Repel)).apply(app.world_mut(), root);
        let entity = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let before = app.world().get::<InfluenceArea>(entity).unwrap().clone();
        EditAction::Area(AreaAction::Redraw).apply(app.world_mut(), root);
        app.world_mut().get_mut::<AreaEditor>(root).unwrap().points =
            vec![DVec2::ZERO, DVec2::new(120.0, 0.0), DVec2::new(60.0, 100.0)];
        EditAction::Area(AreaAction::Finish).apply(app.world_mut(), root);
        let after = app.world().get::<InfluenceArea>(entity).unwrap().clone();
        assert_eq!(after.id, before.id);
        assert_eq!(after.direction, before.direction);
        assert_eq!(after.rules, before.rules);
        assert_eq!(after.shape.kind(), ShapeKind::Drawn);
        EditAction::Area(AreaAction::Redraw).apply(app.world_mut(), root);
        app.world_mut().get_mut::<AreaEditor>(root).unwrap().points = vec![DVec2::ZERO, DVec2::ONE];
        EditAction::Area(AreaAction::Finish).apply(app.world_mut(), root);
        assert!(
            !app.world()
                .get::<AreaEditor>(root)
                .unwrap()
                .notice
                .is_empty()
        );
        assert_eq!(app.world().get::<InfluenceArea>(entity).unwrap(), &after);
        crate::inspection::Deselect.apply(app.world_mut(), root);
        assert!(app.world().get::<AreaEditor>(root).unwrap().tool.is_none());
        assert!(app.world().get::<EditMode>(root).unwrap().enabled);
        assert_eq!(app.world().get::<InfluenceArea>(entity).unwrap(), &after);
        EditAction::Area(AreaAction::Remove).apply(app.world_mut(), root);
        assert!(app.world().get::<InfluenceArea>(entity).is_none());
    }

    crate::laboratory_cases! {
        selecting_the_same_area_preserves_panel_fields_and_scroll,
        area_editor_saves_valid_values_keeps_invalid_drafts_and_enforces_ownership,
        redraw_keeps_identity_rules_and_force_and_escape_cancels_without_replacing,
    }
}
