use crate::{
    area::{
        AreaShape, AttractionTarget, Direction, InfluenceArea, MAX_AREAS, MAX_RULES, Property,
        PropertyRule, ReachMode, ReachShape, ShapeKind, spawn_area,
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
    Reach(ReachMode),
    ReachShape(ReachShape),
    TargetCenter,
    TargetPoint,
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
            .get::<crate::protein_area::grouping::GeneratedGroup>(entity)
            .is_none()
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
        .query_filtered::<(&InfluenceArea, &ChildOf), Without<crate::protein_area::grouping::GeneratedGroup>>()
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
                AreaAction::Reach(mode) => area.reach.mode = mode,
                AreaAction::ReachShape(shape) => area.reach.shape = shape,
                AreaAction::TargetCenter => area.target = AttractionTarget::Center,
                AreaAction::TargetPoint => {
                    area.target = AttractionTarget::Point(
                        (area.target_position() - DVec2::from_array(area.center)).to_array(),
                    );
                }
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
    Radius,
    Depth,
    Target(usize),
    Rule(usize),
}

#[derive(Component)]
struct ForceSlider {
    root: Entity,
    area: Entity,
}

fn force_status(area: &InfluenceArea) -> &'static str {
    if area.strength == 0.0 {
        "Force is off. Raise the slider and choose matching properties to move Sands."
    } else if area.rules.is_empty() && area.filter.is_none() {
        "Choose matching properties below. No Sands are affected yet."
    } else {
        "Attracts or repels matching Sands within reach while workspace physics is on. Zero strength turns the force off."
    }
}

fn force_controls(world: &mut World, root: Entity, panel: Entity, entity: Entity) {
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    label(world, panel, "Force", 18.0);
    let row = row(world, panel);
    for (direction, title) in [(Direction::Attract, "Attract"), (Direction::Repel, "Repel")] {
        choice(
            world,
            root,
            row,
            AreaAction::Direction(direction),
            title,
            area.direction == direction,
        );
    }
    label(world, panel, "Strength", 14.0);
    let slider = crate::slider::spawn(
        world,
        panel,
        "Area force strength",
        crate::slider::SliderSand {
            start: 0.0,
            end: 1000.0_f32.max(area.strength as f32),
            step: 0.1,
            decimals: 1,
        },
        area.strength as f32,
        "",
    )
    .unwrap();
    world
        .entity_mut(slider)
        .insert((
            crate::icons::Tooltip(force_status(&area).into()),
            ForceSlider { root, area: entity },
        ))
        .observe(
            |event: On<crate::slider::SliderChanged>, mut commands: Commands| {
                let slider = event.entity;
                let value = event.value;
                commands.queue(move |world: &mut World| {
                    let Some(control) = world.get::<ForceSlider>(slider) else {
                        return;
                    };
                    let (root, area) = (control.root, control.area);
                    if !owns(world, root, area)
                        || !world
                            .get::<EditMode>(root)
                            .is_some_and(|mode| mode.enabled && mode.areas)
                        || !world
                            .get::<AreaEditor>(root)
                            .is_some_and(|editor| editor.selected == Some(area))
                    {
                        return;
                    }
                    let Some(value) = world
                        .get::<crate::slider::SliderSand>(slider)
                        .and_then(|config| config.snap(value))
                    else {
                        return;
                    };
                    let mut next = world.get::<InfluenceArea>(area).unwrap().clone();
                    next.strength = f64::from(value);
                    if !next.validate() || next == *world.get::<InfluenceArea>(area).unwrap() {
                        return;
                    }
                    crate::area_mutation::disarm(
                        world,
                        area,
                        "Disarmed after an edit. Preview again to arm.",
                    );
                    *world.get_mut::<InfluenceArea>(area).unwrap() = next;
                });
            },
        );
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
        crate::icons::Tooltip(match field {
            Field::Name => "Area name, up to 80 characters.",
            Field::Center(_) => "Position of the local Area boundary in workspace coordinates.",
            Field::Size(_) => "Local boundary size, from 1 to 100000 workspace units. Reach is measured outward from it.",
            Field::Radius => "Extra distance beyond the local boundary, from 0 to 100000 workspace units. Square encloses the Area; Follow shape expands its perimeter.",
            Field::Depth => "Thickness behind the Area's local plane, from 1 to 100000 units. Follows the smaller side until set manually. Controls the influence volume. Reset with Automatic depth in the spatial controls.",
            Field::Target(_) => "Target offset from the Area center in workspace units. Moving the Area carries it; resizing preserves this offset. The target does not extend reach.",
            Field::Rule(_) => "Exact property value. All or Any determines how filters combine.",
        }.into()),
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
    world.entity_mut(status).insert(crate::icons::Tooltip("Not saved. Enter a finite number within the property's range, or a valid nonempty name or property value.".into()));
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
                        Field::Radius => next.reach.radius = number,
                        Field::Depth => next.depth = number,
                        Field::Target(axis) => {
                            let mut offset = (next.target_position()
                                - DVec2::from_array(next.center))
                            .to_array();
                            offset[axis] = number;
                            next.target = AttractionTarget::Point(offset);
                        }
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
            if matches!(field, Field::Depth) {
                let mut placement = crate::topology::spatial(world, target);
                placement.depth = Some(next.depth);
                world.entity_mut(target).insert(placement);
            }
            *world.get_mut::<InfluenceArea>(target).unwrap() = next;
        }
        world.get_mut::<AreaField>(entity).unwrap().observed = value;
        if let Some(mut text) = world.get_mut::<Text>(status) {
            text.0 = if valid { "" } else { "Invalid value" }.into();
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
                Field::Radius => area.reach.radius,
                Field::Depth => area.depth,
                Field::Target(axis) => {
                    (area.target_position() - DVec2::from_array(area.center))[axis]
                }
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
    let sliders: Vec<_> = world
        .query::<(Entity, &ForceSlider)>()
        .iter(world)
        .filter_map(|(entity, slider)| {
            let area = world.get::<InfluenceArea>(slider.area)?;
            Some((entity, area.strength as f32, force_status(area)))
        })
        .collect();
    for (entity, value, message) in sliders {
        crate::slider::set_value(world, entity, value);
        if let Some(mut tip) = world.get_mut::<crate::icons::Tooltip>(entity)
            && tip.0 != message
        {
            tip.0 = message.into();
        }
    }
}

pub(crate) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

pub(crate) fn button(
    world: &mut World,
    root: Entity,
    parent: Entity,
    action: AreaAction,
    title: &str,
) -> Entity {
    let entity = control(world, root, parent, EditAction::Area(action), title);
    let tip = match action {
        AreaAction::Reach(ReachMode::Limited) => {
            "Limit movement influence to the Area and its outward radius. Entry and exit use the local boundary."
        }
        AreaAction::Reach(ReachMode::Unlimited) => {
            "Affect every matching unpinned Sand in this workspace, including off-screen Sands. Entry and exit use the local boundary."
        }
        AreaAction::ReachShape(ReachShape::Square) => {
            "Enclose the local Area in a square, then extend each side by the radius."
        }
        AreaAction::ReachShape(ReachShape::FollowShape) => {
            "Extend outward from the perimeter by the radius. Zero uses the local shape exactly."
        }
        AreaAction::TargetCenter => {
            "Reset the attraction or repulsion target to the center. For a concave outline, uses an interior point near the center. Follows shape edits."
        }
        AreaAction::TargetPoint => {
            "Use a separate target. Drag its marker or edit its offsets. This leaves the local boundary and reach in place. Simple forces keep their vector until recalculated; Newtonian forces keep steering toward the target."
        }
        _ => title,
    };
    world
        .entity_mut(entity)
        .insert(crate::icons::Tooltip(tip.into()));
    entity
}

fn choice(
    world: &mut World,
    root: Entity,
    parent: Entity,
    action: AreaAction,
    title: &str,
    selected: bool,
) {
    let entity = button(world, root, parent, action, title);
    world
        .get_mut::<AccessibilityNode>(entity)
        .unwrap()
        .set_selected(selected);
    if selected {
        world.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(25),
                bottom: px(1),
                width: percent(50),
                height: px(3),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Accent),
            Pickable::IGNORE,
            ChildOf(entity),
        ));
    }
}

fn matching_controls(
    world: &mut World,
    root: Entity,
    panel: Entity,
    entity: Entity,
    area: &InfluenceArea,
) {
    label(world, panel, "Filter", 18.0);
    let mode = button(
        world,
        root,
        panel,
        AreaAction::MatchAll,
        if area.match_all { "All" } else { "Any" },
    );
    world.entity_mut(mode).insert(crate::icons::Tooltip(
        "Match all or any property exactly. Without rules, sorting, immunity and size affect all Sands. Attraction and repulsion need rules or a Protein filter. Pinned Sands stay fixed.".into()
    ));
    for (index, rule) in area.rules.iter().enumerate() {
        label(world, panel, &format!("Property {}", index + 1), 14.0);
        let options = row(world, panel);
        for property in Property::ALL {
            choice(
                world,
                root,
                options,
                AreaAction::Property(index, property),
                property.name(),
                rule.property == property,
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
}

fn reach_controls(
    world: &mut World,
    root: Entity,
    panel: Entity,
    entity: Entity,
    area: &InfluenceArea,
) {
    label(world, panel, "Reach", 18.0);
    let modes = row(world, panel);
    for (mode, title) in [
        (ReachMode::Limited, "Limited"),
        (ReachMode::Unlimited, "Unlimited"),
    ] {
        choice(
            world,
            root,
            modes,
            AreaAction::Reach(mode),
            title,
            area.reach.mode == mode,
        );
    }
    if area.reach.mode == ReachMode::Limited {
        let shapes = row(world, panel);
        for (shape, title) in [
            (ReachShape::Square, "Square"),
            (ReachShape::FollowShape, "Follow shape"),
        ] {
            choice(
                world,
                root,
                shapes,
                AreaAction::ReachShape(shape),
                title,
                area.reach.shape == shape,
            );
        }
        field(
            world,
            root,
            panel,
            entity,
            Field::Radius,
            "Radius",
            area.reach.radius.to_string(),
        );
    }
}

pub(crate) fn render(world: &mut World, root: Entity, panel: Entity) {
    if world.get::<AreaEditor>(root).is_none() {
        world.entity_mut(root).insert(AreaEditor::default());
    }
    label(world, panel, "Areas", 22.0);
    crate::workspace_config::controls(world, root, panel);
    label(world, panel, "Add", 14.0);
    let add = row(world, panel);
    for (action, title) in [
        (AreaAction::Add(ShapeKind::Square), "Add square"),
        (AreaAction::Add(ShapeKind::Circle), "Add circle"),
    ] {
        button(world, root, add, action, title);
    }
    label(world, panel, "Draw", 14.0);
    let draw = row(world, panel);
    let tool = world.get::<AreaEditor>(root).unwrap().tool;
    for (kind, title) in [
        (
            ShapeKind::Square,
            "Drag a square on the canvas. Escape cancels.",
        ),
        (
            ShapeKind::Circle,
            "Drag a circle on the canvas. Escape cancels.",
        ),
        (
            ShapeKind::Drawn,
            "Click to start, draw freely, then click to close the outline. Escape cancels.",
        ),
    ] {
        choice(
            world,
            root,
            draw,
            AreaAction::Draw(kind),
            title,
            tool == Some(kind),
        );
    }
    if let Some(tool) = tool {
        if tool == ShapeKind::Drawn {
            button(world, root, draw, AreaAction::Finish, "Close outline");
        }
        button(world, root, draw, AreaAction::Cancel, "Cancel drawing");
    }
    let notice = world.get::<AreaEditor>(root).unwrap().notice.clone();
    if !notice.is_empty() {
        let status = label(world, panel, "Drawing error", 14.0);
        world
            .entity_mut(status)
            .insert(crate::icons::Tooltip(notice));
    }
    let selected = world
        .get::<AreaEditor>(root)
        .unwrap()
        .selected
        .filter(|entity| owns(world, root, *entity));
    let mut areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter(|(entity, _)| owns(world, root, *entity))
        .map(|(entity, area)| (entity, area.name.clone(), area.id.clone()))
        .collect();
    areas.sort_by(|a, b| a.2.cmp(&b.2));
    label(world, panel, "Selection", 14.0);
    for (entity, name, _) in areas {
        choice(
            world,
            root,
            panel,
            AreaAction::Select(entity),
            &name,
            selected == Some(entity),
        );
    }
    let Some(entity) = selected else { return };
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    field(
        world,
        root,
        panel,
        entity,
        Field::Name,
        "Name",
        area.name.clone(),
    );
    crate::protein_area::controls(world, root, panel, entity);
    crate::protein_area::filter::controls(world, panel, entity);
    crate::area_effects::controls(world, root, panel, entity);
    force_controls(world, root, panel, entity);
    reach_controls(world, root, panel, entity, &area);
    label(world, panel, "Target", 18.0);
    let targets = row(world, panel);
    choice(
        world,
        root,
        targets,
        AreaAction::TargetCenter,
        "Center",
        area.target == AttractionTarget::Center,
    );
    choice(
        world,
        root,
        targets,
        AreaAction::TargetPoint,
        "Point",
        matches!(area.target, AttractionTarget::Point(_)),
    );
    if let AttractionTarget::Point(offset) = area.target {
        for (axis, title) in [(0, "Offset X"), (1, "Offset Y")] {
            field(
                world,
                root,
                panel,
                entity,
                Field::Target(axis),
                title,
                offset[axis].to_string(),
            );
        }
    }
    if area.filter.is_none() {
        matching_controls(world, root, panel, entity, &area);
    }
    label(world, panel, "Shape", 18.0);
    let shapes = row(world, panel);
    for (kind, title) in [(ShapeKind::Square, "Square"), (ShapeKind::Circle, "Circle")] {
        choice(
            world,
            root,
            shapes,
            AreaAction::Shape(kind),
            title,
            area.shape.kind() == kind,
        );
    }
    choice(
        world,
        root,
        shapes,
        AreaAction::Redraw,
        "Redraw the selected Area",
        area.shape.kind() == ShapeKind::Drawn,
    );
    for (axis, title) in [(0, "Position X"), (1, "Position Y")] {
        field(
            world,
            root,
            panel,
            entity,
            Field::Center(axis),
            title,
            area.center[axis].to_string(),
        );
    }
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
    field(
        world,
        root,
        panel,
        entity,
        Field::Depth,
        "Depth",
        area.depth.to_string(),
    );
    crate::area_mutation_panel::render(world, root, panel, entity);
    button(world, root, panel, AreaAction::Remove, "Delete Area");
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
    fn reach_controls_validate_radius_and_preserve_it_when_switching_modes() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Circle)).apply(app.world_mut(), root);
        let area = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let input = app
            .world_mut()
            .query::<(Entity, &AreaField)>()
            .iter(app.world())
            .find(|(_, field)| matches!(field.field, Field::Radius))
            .unwrap()
            .0;
        assert!(app.world().get::<crate::icons::Tooltip>(input).is_some());
        for (value, expected) in [
            ("75.5", 75.5),
            ("-1", 75.5),
            ("inf", 75.5),
            ("0", 0.0),
            ("75.5", 75.5),
        ] {
            app.world_mut()
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            assert_eq!(
                app.world().get::<InfluenceArea>(area).unwrap().reach.radius,
                expected
            );
        }
        for action in [
            AreaAction::ReachShape(ReachShape::Square),
            AreaAction::Reach(ReachMode::Unlimited),
            AreaAction::Reach(ReachMode::Limited),
        ] {
            EditAction::Area(action).apply(app.world_mut(), root);
        }
        let saved = app.world().get::<InfluenceArea>(area).unwrap();
        assert_eq!(saved.reach.radius, 75.5);
        assert_eq!(saved.reach.shape, ReachShape::Square);
        assert_eq!(saved.reach.mode, ReachMode::Limited);
        let controls: Vec<_> = app
            .world_mut()
            .query::<(Entity, &crate::edit_mode::EditControl)>()
            .iter(app.world())
            .filter(|(_, control)| {
                matches!(
                    control.action,
                    EditAction::Area(
                        AreaAction::Remove
                            | AreaAction::Add(_)
                            | AreaAction::Draw(_)
                            | AreaAction::Direction(_)
                    )
                )
            })
            .map(|(entity, _)| entity)
            .collect();
        assert!(!controls.is_empty());
        for entity in controls {
            assert!(
                app.world()
                    .get::<crate::icons::IconButton>(entity)
                    .is_some()
            );
            assert!(
                !app.world()
                    .get::<crate::icons::Tooltip>(entity)
                    .unwrap()
                    .0
                    .is_empty()
            );
        }
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
            .find(|(_, field)| matches!(field.field, Field::Size(0)))
            .unwrap()
            .0;
        for (value, expected) in [("-10", 320.0), ("NaN", 320.0), ("42.5", 42.5)] {
            app.world_mut()
                .get_mut::<EditableText>(field)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            assert_eq!(
                app.world().get::<InfluenceArea>(area).unwrap().size[0],
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
            0.0
        );
        EditAction::Close.apply(app.world_mut(), root);
        EditAction::Area(AreaAction::Remove).apply(app.world_mut(), root);
        assert!(app.world().get::<InfluenceArea>(area).is_some());
    }

    #[cfg_attr(test, test)]
    fn force_slider_changes_only_the_selected_area_and_never_arms_record_changes() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Square)).apply(app.world_mut(), root);
        let area = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        let slider = app
            .world_mut()
            .query::<(Entity, &ForceSlider)>()
            .iter(app.world())
            .find(|(_, slider)| slider.area == area)
            .unwrap()
            .0;
        assert_eq!(
            app.world()
                .get::<bevy::ui_widgets::SliderValue>(slider)
                .unwrap()
                .0,
            0.0
        );
        for (value, expected) in [(250.0, 250.0), (f32::NAN, 250.0), (0.0, 0.0)] {
            app.world_mut().trigger(bevy::ui_widgets::ValueChange {
                source: slider,
                value,
                is_final: false,
            });
            app.world_mut().flush();
            app.update();
            let current = app.world().get::<InfluenceArea>(area).unwrap();
            assert_eq!(current.strength, expected);
            assert!(current.rules.is_empty());
            assert!(current.changes.is_empty());
            assert!(!crate::area_mutation::armed(app.world(), area));
            assert_eq!(
                app.world()
                    .get::<bevy::ui_widgets::SliderValue>(slider)
                    .unwrap()
                    .0,
                expected as f32
            );
        }
        let other = spawn_area(
            app.world_mut(),
            root,
            1,
            InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(200.0)),
        )
        .unwrap();
        app.world_mut()
            .get_mut::<AreaEditor>(root)
            .unwrap()
            .selected = Some(other);
        app.world_mut().trigger(crate::slider::SliderChanged {
            entity: slider,
            value: 500.0,
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<InfluenceArea>(area).unwrap().strength,
            0.0
        );
        assert_eq!(
            app.world().get::<InfluenceArea>(other).unwrap().strength,
            0.0
        );
        app.world_mut()
            .get_mut::<AreaEditor>(root)
            .unwrap()
            .selected = Some(area);
        app.world_mut().get_mut::<EditMode>(root).unwrap().enabled = false;
        app.world_mut().trigger(crate::slider::SliderChanged {
            entity: slider,
            value: 500.0,
        });
        app.world_mut().flush();
        assert_eq!(
            app.world().get::<InfluenceArea>(area).unwrap().strength,
            0.0
        );
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
        target_and_depth_controls_validate_and_preserve_offsets_on_redraw,
        reach_controls_validate_radius_and_preserve_it_when_switching_modes,
        selecting_the_same_area_preserves_panel_fields_and_scroll,
        area_editor_saves_valid_values_keeps_invalid_drafts_and_enforces_ownership,
        force_slider_changes_only_the_selected_area_and_never_arms_record_changes,
        redraw_keeps_identity_rules_and_force_and_escape_cancels_without_replacing,
    }

    #[cfg_attr(test, test)]
    fn target_and_depth_controls_validate_and_preserve_offsets_on_redraw() {
        let (mut app, root) = fixture();
        EditAction::Area(AreaAction::Add(ShapeKind::Circle)).apply(app.world_mut(), root);
        let area = app
            .world()
            .get::<AreaEditor>(root)
            .unwrap()
            .selected
            .unwrap();
        EditAction::Area(AreaAction::TargetPoint).apply(app.world_mut(), root);
        for (field, values) in [
            (
                Field::Target(0),
                vec![("500", 500.0), ("NaN", 500.0), ("-40", -40.0)],
            ),
            (
                Field::Depth,
                vec![("12", 12.0), ("0", 12.0), ("inf", 12.0), ("100001", 12.0)],
            ),
        ] {
            let input = app
                .world_mut()
                .query::<(Entity, &AreaField)>()
                .iter(app.world())
                .find(|(_, f)| {
                    matches!(
                        (f.field, field),
                        (Field::Depth, Field::Depth) | (Field::Target(0), Field::Target(0))
                    )
                })
                .unwrap()
                .0;
            assert!(app.world().get::<crate::icons::Tooltip>(input).is_some());
            for (value, expected) in values {
                app.world_mut()
                    .get_mut::<EditableText>(input)
                    .unwrap()
                    .editor
                    .set_text(value);
                app.update();
                let area = app.world().get::<InfluenceArea>(area).unwrap();
                assert_eq!(
                    match field {
                        Field::Depth => area.depth,
                        _ => area.target_position().x - area.center[0],
                    },
                    expected
                );
            }
        }
        assert_eq!(
            crate::topology::spatial(app.world(), area).depth,
            Some(12.0)
        );
        EditAction::Area(AreaAction::Redraw).apply(app.world_mut(), root);
        insert(
            app.world_mut(),
            root,
            InfluenceArea::drawn(&[
                DVec2::ZERO,
                DVec2::new(80.0, 0.0),
                DVec2::new(80.0, 30.0),
                DVec2::new(0.0, 30.0),
            ])
            .unwrap(),
        )
        .unwrap();
        let saved = app.world().get::<InfluenceArea>(area).unwrap();
        assert_eq!(saved.depth, 12.0);
        assert_eq!(saved.target, AttractionTarget::Point([-40.0, 0.0]));
        EditAction::Area(AreaAction::TargetCenter).apply(app.world_mut(), root);
        let saved = app.world().get::<InfluenceArea>(area).unwrap();
        assert_eq!(saved.target, AttractionTarget::Center);
        assert_eq!(saved.target_position(), DVec2::from_array(saved.center));
    }
}
