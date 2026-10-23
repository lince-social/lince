use super::spatial;
use crate::native::actions::{Action, ActionButton};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub enum TopologyAction {
    ToggleView,
    Pin,
    AutoDepth,
    SetDepth,
    Move(usize, f64),
    Rotate(usize, f64),
    ResizeDepth(f64),
    Resize(usize, f32),
    SelectionDepth(f64),
    Plane(f64),
    Import,
    CancelImports,
    Duplicate,
    Frame,
}

impl Action for TopologyAction {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        use lince_interface::practice::Operation;
        if matches!(self, Self::ToggleView) {
            &[Operation::SwitchSpatialView, Operation::ReturnFlatView]
        } else {
            &[]
        }
    }
    fn practice_intent(&self) -> crate::native::actions::PracticeIntent {
        if matches!(self, Self::ToggleView) {
            crate::native::actions::PracticeIntent::Navigation
        } else {
            crate::native::actions::PracticeIntent::Target
        }
    }
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::native::inspection::Connection> {
        vec![crate::native::inspection::Connection {
            target,
            name: "Topology control activated".into(),
        }]
    }
    fn apply(&self, world: &mut World, entity: Entity) {
        if matches!(self, Self::ToggleView) {
            let mut view = world
                .get::<super::view::View>(entity)
                .copied()
                .unwrap_or_default();
            view.spatial = !view.spatial;
            if view.spatial {
                let canvas = world
                    .get::<crate::native::canvas::CanvasView>(entity)
                    .copied()
                    .unwrap_or_default();
                let viewport_height = world
                    .get_resource::<super::presentation::SceneCamera>()
                    .and_then(|camera| world.get::<Camera>(camera.0))
                    .and_then(Camera::logical_viewport_size)
                    .map_or(720.0, |size| f64::from(size.y));
                let tangent = f64::from((PerspectiveProjection::default().fov * 0.5).tan());
                let height = (viewport_height / (2.0 * canvas.zoom)
                    * (0.5 + std::f64::consts::FRAC_1_SQRT_2 / tangent)
                    * 1.05)
                    .max(16.0);
                view.position = [
                    canvas.center.x,
                    view.plane + height,
                    canvas.center.y + height,
                ];
                view.yaw = 0.0;
                view.pitch = -std::f32::consts::FRAC_PI_4;
                view.selection_depth = view.selection_depth.max(height * 4.0).min(100_000.0);
            } else {
                let point = world
                    .get_resource::<super::presentation::SceneCamera>()
                    .and_then(|camera| world.get::<Camera>(camera.0))
                    .and_then(Camera::logical_viewport_rect)
                    .and_then(|rect| {
                        super::input::plane_point(world, entity, rect.center(), view.plane)
                    });
                let center = point.map_or(
                    bevy::math::DVec2::new(view.position[0], view.position[2]),
                    |point| bevy::math::DVec2::new(point.x, point.z),
                );
                if let Some(mut canvas) = world.get_mut::<crate::native::canvas::CanvasView>(entity)
                {
                    canvas.center = center;
                }
            }
            world.entity_mut(entity).insert(view);
            return;
        }
        if let Self::SelectionDepth(delta) = *self {
            let mut view = world
                .get::<super::view::View>(entity)
                .copied()
                .unwrap_or_default();
            view.selection_depth = (view.selection_depth + delta).clamp(1.0, 100_000.0);
            world.entity_mut(entity).insert(view);
            return;
        }
        if let Self::Plane(delta) = *self {
            let mut view = world
                .get::<super::view::View>(entity)
                .copied()
                .unwrap_or_default();
            view.plane += delta;
            world.entity_mut(entity).insert(view);
            return;
        }
        if matches!(self, Self::CancelImports) {
            super::assets::cancel(world, entity);
            return;
        }
        if matches!(self, Self::Import) {
            let path = world
                .query::<(&ImportPath, &bevy::text::EditableText)>()
                .iter(world)
                .find(|(p, _)| p.0 == entity)
                .map(|(_, t)| t.editor.text().to_string());
            if let Some(path) = path {
                if let Err(error) =
                    super::assets::import(world, entity, std::path::PathBuf::from(path.trim()))
                {
                    crate::native::notifications::report(world, "Topology", &error);
                }
            }
            return;
        }
        let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
            return;
        };
        if !world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled)
        {
            return;
        }
        let mut placement = spatial(world, entity);
        if world
            .get::<crate::native::workspace_sync::HostedReplica>(entity)
            .is_some()
            && (world.get::<super::rooms::Room>(entity).is_some()
                || world.get::<super::rooms::Member>(entity).is_some())
            && !matches!(self, Self::Frame | Self::Duplicate)
        {
            crate::native::notifications::report(
                world,
                "Rooms",
                "Use the acting Actor's Workspace Sync to edit this hosted placement.",
            );
            return;
        }
        match *self {
            Self::Frame => {
                super::assets::frame(world, entity);
                return;
            }
            Self::Duplicate => {
                super::assets::duplicate(world, entity);
                return;
            }
            Self::SetDepth => {
                let depth = world
                    .query::<(&DepthField, &bevy::text::EditableText)>()
                    .iter(world)
                    .find(|(field, _)| field.0 == entity)
                    .and_then(|(_, text)| {
                        text.editor.text().to_string().trim().parse::<f64>().ok()
                    });
                let Some(depth) =
                    depth.filter(|depth| depth.is_finite() && (1.0..=100_000.0).contains(depth))
                else {
                    crate::native::notifications::report(
                        world,
                        "Topology",
                        "Depth must be between 1 and 100000.",
                    );
                    return;
                };
                placement.depth = Some(depth);
                if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity)
                {
                    area.depth = depth;
                }
            }
            Self::Pin => {
                if world
                    .get::<super::assets::ImportedAsset>(entity)
                    .is_some_and(super::splats::is_splat)
                {
                    return;
                }
                placement.world_pinned = !placement.world_pinned;
                if !placement.world_pinned
                    && let Some(member) =
                        world.get::<crate::native::workspace::WorkspaceMember>(entity)
                {
                    let workspace = member.0;
                    if !crate::native::workspace_config::set_physics(world, root, workspace, true) {
                        return;
                    }
                }
            }
            Self::AutoDepth => {
                placement.depth = None;
                let size = world
                    .get::<crate::native::canvas::CanvasItem>(entity)
                    .unwrap()
                    .size;
                if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity)
                {
                    area.depth = f64::from(size.min_element());
                }
            }
            Self::ResizeDepth(delta) => {
                let Some(item) = world.get::<crate::native::canvas::CanvasItem>(entity) else {
                    return;
                };
                let depth = world
                    .get::<crate::native::area::InfluenceArea>(entity)
                    .map_or_else(|| placement.depth(item.size), |a| a.depth);
                placement.depth = Some((depth + delta).clamp(1.0, 100_000.0));
                if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(entity)
                {
                    area.depth = placement.depth.unwrap();
                }
            }
            Self::Resize(axis, delta) => {
                if let Some(mut asset) = world.get_mut::<super::assets::ImportedAsset>(entity) {
                    asset.scale = (asset.scale * if delta > 0.0 { 1.1 } else { 1.0 / 1.1 })
                        .clamp(0.001, 100_000.0);
                } else if let Some(mut area) =
                    world.get_mut::<crate::native::area::InfluenceArea>(entity)
                {
                    area.size[axis] = (area.size[axis] + f64::from(delta)).clamp(10.0, 100_000.0);
                    if !matches!(area.shape, crate::native::area::AreaShape::Polygon(_)) {
                        area.size = [area.size[axis]; 2];
                    }
                } else if let Some(mut item) =
                    world.get_mut::<crate::native::canvas::CanvasItem>(entity)
                {
                    item.size[axis] = (item.size[axis] + delta).clamp(10.0, 100_000.0);
                }
                return;
            }
            Self::Move(axis, delta) => {
                let mut movement = bevy::math::DVec3::ZERO;
                movement[axis] = delta;
                super::groups::transform(world, entity, movement, bevy::math::DQuat::IDENTITY);
                return;
            }
            Self::Rotate(axis, angle) => {
                let rotation = bevy::math::DQuat::from_axis_angle(
                    [
                        bevy::math::DVec3::X,
                        bevy::math::DVec3::Y,
                        bevy::math::DVec3::Z,
                    ][axis],
                    angle,
                );
                super::groups::transform(world, entity, bevy::math::DVec3::ZERO, rotation);
                return;
            }
            Self::ToggleView
            | Self::Import
            | Self::CancelImports
            | Self::SelectionDepth(_)
            | Self::Plane(_) => {
                unreachable!()
            }
        }
        world.entity_mut(entity).insert(placement);
    }
}

#[derive(Component)]
struct Controls;
#[derive(Component)]
pub(crate) struct Inspector(pub(crate) Entity);
#[derive(Component)]
struct ImportPath(Entity);
#[derive(Component)]
struct Readout(Entity);

#[derive(Component)]
struct Dimensions(Entity);

#[derive(Component)]
struct SelectionSettings(Entity);

#[derive(Component)]
struct DepthField(Entity);

fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(parent),
        ))
        .id()
}

fn button(
    world: &mut World,
    parent: Entity,
    target: Entity,
    name: &str,
    action: TopologyAction,
) -> Entity {
    let button = world
        .spawn((
            crate::native::sand::button(0),
            Node {
                padding: UiRect::all(px(6)),
                border: UiRect::all(px(crate::native::sand::BUTTON_BORDER_WIDTH)),
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            crate::native::icons::Tooltip(match action {
                TopologyAction::SelectionDepth(_) => "The vertical range of a dragged selection in 3D, centered on the creation height. Increase it to include objects farther above or below that height.",
                TopologyAction::Plane(_) => "The height where new Sands, models, and drawn areas are placed. Y is the vertical direction in 3D.",
                TopologyAction::ToggleView => "Switch between the flat canvas and the 3D view.",
                TopologyAction::Import => "Import a local .gltf, .glb, or .gcloud model from the file path above. Gaussian clouds are visual only.",
                TopologyAction::CancelImports => "Stop pending model imports. Models already imported stay on the canvas.",
                TopologyAction::Frame => "Move the view to show the whole imported asset.",
                _ => name,
            }.into()),
            ActionButton::new(target, crate::native::actions![action]),
            ChildOf(parent),
        ))
        .id();
    crate::native::edit_mode::label(world, button, name, 13.0);
    button
}

pub fn update(world: &mut World) {
    let settings: Vec<_> = world
        .query::<(Entity, &SelectionSettings)>()
        .iter(world)
        .map(|(e, s)| (e, s.0))
        .collect();
    for (label, root) in settings {
        if let Some(view) = world.get::<super::view::View>(root) {
            let value = format!(
                "Creation Y: {:.0}\nSelection depth: {:.0}",
                view.plane, view.selection_depth
            );
            if let Some(mut text) = world.get_mut::<Text>(label)
                && text.0 != value
            {
                text.0 = value;
            }
        }
    }
    let readouts: Vec<_> = world
        .query::<(Entity, &Dimensions)>()
        .iter(world)
        .map(|(e, d)| (e, d.0))
        .collect();
    for (label, target) in readouts {
        let Some(item) = world.get::<crate::native::canvas::CanvasItem>(target) else {
            continue;
        };
        let placement = spatial(world, target);
        let depth = world
            .get::<crate::native::area::InfluenceArea>(target)
            .map_or_else(|| placement.depth(item.size), |area| area.depth);
        let imported = world.get::<super::assets::ImportedAsset>(target);
        let scale = world
            .get::<crate::native::area_effects::AreaScale>(target)
            .map_or(1.0, |s| s.0);
        let dimensions = imported.and_then(|asset| {
            world.get::<super::assets::Bounds>(target).map(|bounds| {
                (bounds.max - bounds.min) * super::assets::effective_scale(world, target, asset)
            })
        });
        let (width, height, depth) = dimensions.map_or(
            (
                item.size.x * scale,
                item.size.y * scale,
                depth * f64::from(scale),
            ),
            |size| (size.x, size.z, f64::from(size.y)),
        );
        let value = format!(
            "{:.0} × {:.0} × {:.0}\n{} · {} depth",
            width,
            height,
            depth,
            if placement.world_pinned {
                "World pinned"
            } else {
                "Unpinned"
            },
            if imported.is_some_and(super::splats::is_splat) {
                "Visual bounds"
            } else if imported.is_some() {
                "Mesh"
            } else if placement.depth.is_some() {
                "Manual"
            } else {
                "Automatic"
            }
        );
        if let Some(mut text) = world.get_mut::<Text>(label)
            && text.0 != value
        {
            text.0 = value;
        }
    }
    let roots: Vec<_> = world
        .query::<(Entity, &crate::native::workspace::Workspaces)>()
        .iter(world)
        .map(|(e, _)| e)
        .collect();
    for root in roots {
        if world.get::<Controls>(root).is_none()
            && world
                .get::<crate::native::edit_mode::EditMode>(root)
                .is_some()
        {
            let toolbar = crate::native::canvas_controls::toolbar(world, root);
            let toggle = button(world, toolbar, root, "2D / 3D", TopologyAction::ToggleView);
            world.get_mut::<Node>(toggle).unwrap().height = px(40);
            world.get_mut::<Node>(toggle).unwrap().align_items = AlignItems::Center;
            if let Some(index) = world.get::<Children>(toolbar).and_then(|children| {
                children.iter().position(|child| {
                    world
                        .get::<crate::native::edit_mode::EditControl>(child)
                        .is_some_and(|control| {
                            control.action == crate::native::edit_mode::EditAction::Toggle
                        })
                })
            }) {
                world.entity_mut(toolbar).insert_children(index, &[toggle]);
            }
            world.entity_mut(root).insert(Controls);
        }
        let selected = world
            .get::<crate::native::inspection::Inspection>(root)
            .and_then(|i| i.selected);
        let editing = world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled);
        let old = world
            .query::<(Entity, &Inspector)>()
            .iter(world)
            .find(|(_, i)| i.0 == root)
            .map(|(e, _)| e);
        let target =
            selected.filter(|e| world.get::<crate::native::canvas::CanvasItem>(*e).is_some());
        let current = old.and_then(|e| world.get::<Readout>(e).map(|r| r.0));
        if editing && old.is_some() && current == target {
            continue;
        }
        if let Some(old) = old {
            world.despawn(old);
        }
        if !editing || target.is_none() {
            continue;
        }
        let panel = world
            .spawn((
                Inspector(root),
                crate::native::inspection::InspectionExcluded,
                crate::native::sand::Square,
                Node {
                    position_type: PositionType::Absolute,
                    left: px(12),
                    top: px(80),
                    width: px(300),
                    max_height: percent(65),
                    overflow: Overflow::scroll_y(),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(8)),
                    row_gap: px(4),
                    ..default()
                },
                ScrollPosition::default(),
                GlobalZIndex(24),
                ChildOf(root),
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ))
            .id();
        if let Some(target) = target {
            world.entity_mut(panel).insert(Readout(target));
            super::terrain::editor::item_controls(world, panel, root, target);
            if world.get::<super::terrain::Effect>(target).is_some() {
                crate::native::edit_mode::label(world, panel, "Floor effect", 14.0);
                continue;
            }
            let typography = world
                .resource::<crate::native::theme::Typography>()
                .text(13.0);
            world.spawn((
                Dimensions(target),
                Text::new(""),
                typography,
                crate::native::token_style::text(crate::native::tokens::Token::Ink),
                ChildOf(panel),
            ));
            if world.get::<super::assets::ImportedAsset>(target).is_none() {
                let item = world
                    .get::<crate::native::canvas::CanvasItem>(target)
                    .unwrap();
                let depth = world
                    .get::<crate::native::area::InfluenceArea>(target)
                    .map_or_else(
                        || spatial(world, target).depth(item.size),
                        |area| area.depth,
                    );
                let editor = crate::native::sand::text_editor(
                    &depth.to_string(),
                    world.resource::<crate::native::theme::Typography>(),
                    0,
                );
                world.spawn((DepthField(target), editor, ChildOf(panel)));
                button(world, panel, target, "Set depth", TopologyAction::SetDepth);
            }
            if world.get::<super::assets::ImportedAsset>(target).is_some() {
                button(world, panel, target, "Frame asset", TopologyAction::Frame);
                button(
                    world,
                    panel,
                    target,
                    "Duplicate asset",
                    TopologyAction::Duplicate,
                );
            }
            let controls = row(world, panel);
            for (name, action) in [
                ("World pin", TopologyAction::Pin),
                ("Automatic depth", TopologyAction::AutoDepth),
                ("Width −", TopologyAction::Resize(0, -10.0)),
                ("Width +", TopologyAction::Resize(0, 10.0)),
                ("Height −", TopologyAction::Resize(1, -10.0)),
                ("Height +", TopologyAction::Resize(1, 10.0)),
                ("Depth −", TopologyAction::ResizeDepth(-10.0)),
                ("Depth +", TopologyAction::ResizeDepth(10.0)),
                ("X −", TopologyAction::Move(0, -20.0)),
                ("X +", TopologyAction::Move(0, 20.0)),
                ("Y −", TopologyAction::Move(1, -20.0)),
                ("Y +", TopologyAction::Move(1, 20.0)),
                ("Z −", TopologyAction::Move(2, -20.0)),
                ("Z +", TopologyAction::Move(2, 20.0)),
                (
                    "Rotate X",
                    TopologyAction::Rotate(0, std::f64::consts::FRAC_PI_4),
                ),
                (
                    "Rotate Y",
                    TopologyAction::Rotate(1, std::f64::consts::FRAC_PI_4),
                ),
                (
                    "Rotate Z",
                    TopologyAction::Rotate(2, std::f64::consts::FRAC_PI_4),
                ),
            ] {
                let imported = world.get::<super::assets::ImportedAsset>(target).is_some();
                if matches!(action, TopologyAction::Pin)
                    && world
                        .get::<super::assets::ImportedAsset>(target)
                        .is_some_and(super::splats::is_splat)
                {
                    continue;
                }
                if imported
                    && matches!(
                        action,
                        TopologyAction::AutoDepth
                            | TopologyAction::ResizeDepth(_)
                            | TopologyAction::Resize(1, _)
                    )
                {
                    continue;
                }
                let name = if imported && matches!(action, TopologyAction::Resize(0, _)) {
                    if name.ends_with('+') {
                        "Larger"
                    } else {
                        "Smaller"
                    }
                } else {
                    name
                };
                button(world, controls, target, name, action);
            }
            if world.get::<super::assets::ImportedAsset>(target).is_none() {
                super::appearance::controls(world, panel, target);
            }
            super::rooms::controls(world, panel, target);
        }
    }
}

pub fn store_controls(world: &mut World, root: Entity, panel: Entity) {
    super::rooms::store_controls(world, root, panel);
    crate::native::edit_mode::label(world, panel, "Placement and selection", 18.0);
    let typography = world
        .resource::<crate::native::theme::Typography>()
        .text(13.0);
    world.spawn((
        SelectionSettings(root),
        Text::new(""),
        typography,
        crate::native::token_style::text(crate::native::tokens::Token::Ink),
        ChildOf(panel),
    ));
    let controls = row(world, panel);
    button(
        world,
        controls,
        root,
        "Selection depth −",
        TopologyAction::SelectionDepth(-100.0),
    );
    button(
        world,
        controls,
        root,
        "Selection depth +",
        TopologyAction::SelectionDepth(100.0),
    );
    button(
        world,
        controls,
        root,
        "Creation Y −",
        TopologyAction::Plane(-20.0),
    );
    button(
        world,
        controls,
        root,
        "Creation Y +",
        TopologyAction::Plane(20.0),
    );
    crate::native::edit_mode::label(world, panel, "glTF / GLB / Gaussian cloud", 14.0);
    let typography = world.resource::<crate::native::theme::Typography>();
    let editor = crate::native::sand::text_editor("", typography, 0);
    let import_path = world
        .spawn((
            ImportPath(root),
            editor,
            crate::native::icons::Tooltip(
                "Full path to a local .gltf, .glb, or .gcloud file.".into(),
            ),
            ChildOf(panel),
        ))
        .insert(Node {
            width: percent(100),
            min_height: px(30),
            ..default()
        })
        .id();
    crate::native::sand_panel::button(
        world,
        panel,
        root,
        "Browse…",
        crate::native::file_explorer::BrowseFor {
            input: import_path,
            extensions: vec!["gltf".into(), "glb".into(), "gcloud".into()],
            directories: false,
        },
    );
    button(world, panel, root, "Import", TopologyAction::Import);
    button(
        world,
        panel,
        root,
        "Cancel imports",
        TopologyAction::CancelImports,
    );
}
