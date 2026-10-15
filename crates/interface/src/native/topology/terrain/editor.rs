use super::*;
use crate::native::actions::{Action, ActionButton};
use bevy::picking::events::{Pointer, Scroll};
use nucleus::canvas::{self as api, Mutation};
use nucleus::terrain::Preset;

#[derive(Component, Default)]
struct Editor {
    open: bool,
    selected: Option<String>,
    undo: Vec<String>,
    redo: Vec<String>,
    rebuild: bool,
    picking: bool,
    notice: String,
    draft_error: Option<String>,
    edit_panel: Option<(Entity, Display)>,
    previous: HashMap<String, (DVec2, Option<SpatialBinding>)>,
    drags: HashMap<String, (DVec2, Option<SpatialBinding>)>,
    baseline: Option<TopologyEffect>,
}

#[derive(Component)]
struct Panel(Entity);
#[derive(Component)]
struct Field(u8);
#[derive(Component)]
struct Notice(Entity);
#[derive(Component)]
struct Toolbar;

#[derive(Clone)]
enum Control {
    Open,
    Add(Preset),
    Select(String),
    Apply,
    Cancel,
    Preset(Preset),
    Enable,
    Duplicate,
    Delete,
    Undo,
    Redo,
    Bind(Anchor),
    Pick,
    Follow,
    CameraFloor,
    Offsets,
    Align(Entity),
    AreaCamera(Entity),
    AreaDetach(Entity),
}

fn find(world: &mut World, root: Entity, id: &str) -> Option<Entity> {
    world
        .query::<(Entity, &crate::native::canvas_host::Identity, &ChildOf)>()
        .iter(world)
        .find(|(_, identity, parent)| identity.0 == id && parent.parent() == root)
        .map(|(e, _, _)| e)
}

fn transact(world: &mut World, root: Entity, mutation: Mutation) -> Result<String, String> {
    let revision = match crate::native::canvas_host::handle(
        world,
        root,
        &api::Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 1,
        },
    )? {
        api::Response::Snapshot { snapshot } => snapshot.revision,
        _ => return Err("Canvas inspection did not return a snapshot.".into()),
    };
    let request = api::Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: revision,
        mutation,
    };
    match crate::native::canvas_host::handle(world, root, &request)? {
        api::Response::Receipt { receipt } => {
            crate::native::workspace::request_save(world);
            Ok(receipt.request_id)
        }
        _ => Err("Canvas edit did not return a receipt.".into()),
    }
}

fn remember(world: &mut World, root: Entity, receipt: String) {
    let mut editor = world.get_mut::<Editor>(root).unwrap();
    editor.undo.push(receipt);
    if editor.undo.len() > 8 {
        editor.undo.remove(0);
    }
    editor.redo.clear();
    editor.notice.clear();
    editor.rebuild = true;
}

fn configure(
    world: &mut World,
    root: Entity,
    id: String,
    effect: TopologyEffect,
) -> Result<String, String> {
    effect.compile()?;
    if let Some(entity) = find(world, root, &id) {
        world.entity_mut(entity).remove::<Preview>();
    }
    transact(
        world,
        root,
        Mutation::Configure {
            placement: id,
            component: component(&effect),
        },
    )
}

fn bind(world: &mut World, root: Entity, entity: Entity, anchor: Anchor) -> Result<String, String> {
    let snapshot = crate::native::canvas_host::capture(world, root)?;
    let id = world
        .get::<crate::native::canvas_host::Identity>(entity)
        .ok_or("Select a canvas item.")?
        .0
        .clone();
    let mut metadata = snapshot
        .placements
        .iter()
        .find(|p| p.id == id)
        .ok_or("Placement is unavailable.")?
        .metadata
        .clone();
    let point = super::super::position(world, entity).ok_or("Placement has no position.")?;
    let mut binding = metadata.binding.clone().unwrap_or_default();
    binding.position = point.to_array();
    binding.rotation = metadata.spatial.rotation;
    binding.anchor = anchor.clone();
    let anchor_pose = match anchor {
        Anchor::Fixed => point,
        Anchor::Cursor => cursor(
            world,
            root,
            world
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .unwrap()
                .0,
        )
        .unwrap_or(point),
        Anchor::Camera => {
            let view = world
                .get::<super::super::view::View>(root)
                .copied()
                .unwrap_or_default();
            let workspace = world
                .get::<crate::native::workspace::WorkspaceMember>(entity)
                .unwrap()
                .0;
            let canvas = world
                .get::<crate::native::canvas::CanvasView>(root)
                .copied()
                .unwrap_or_default();
            let mut p = if view.spatial {
                DVec3::from_array(view.position)
            } else {
                DVec3::new(canvas.center.x, view.position[1], canvas.center.y)
            };
            if binding.camera_floor {
                p.y = height(world, root, workspace, DVec2::new(p.x, p.z));
            }
            p
        }
        Anchor::Item(id) => find(world, root, &id)
            .and_then(|e| super::super::position(world, e))
            .ok_or("Anchor item is unavailable.")?,
    };
    let orientation = match &binding.anchor {
        Anchor::Camera => world
            .get::<super::super::view::View>(root)
            .map_or(DQuat::IDENTITY, |v| {
                DQuat::from_euler(EulerRot::YXZ, f64::from(v.yaw), f64::from(v.pitch), 0.0)
            }),
        Anchor::Item(id) => find(world, root, id).map_or(DQuat::IDENTITY, |e| {
            super::super::spatial(world, e).rotation()
        }),
        _ => DQuat::IDENTITY,
    };
    binding.offset = if binding.anchor == Anchor::Fixed {
        [0.0; 3]
    } else {
        ((if binding.follow_orientation {
            orientation.inverse()
        } else {
            DQuat::IDENTITY
        }) * (point - anchor_pose))
            .to_array()
    };
    metadata.binding = Some(binding);
    transact(
        world,
        root,
        Mutation::SetMetadata {
            placement: id,
            metadata,
        },
    )
}

fn draft(
    world: &mut World,
    panel: Entity,
    current: &TopologyEffect,
) -> Result<TopologyEffect, String> {
    let mut draft = lince_interface::terrain::Draft::from(current);
    for (field, text, parent) in world
        .query::<(&Field, &bevy::text::EditableText, &ChildOf)>()
        .iter(world)
    {
        if parent.parent() != panel {
            continue;
        }
        let value = text.editor.text().to_string();
        match field.0 {
            0 => draft.name = value,
            1 => draft.amplitude = value,
            2 => draft.radius_x = value,
            3 => draft.radius_y = value,
            4 => draft.rotation = value,
            5 => draft.falloff = value,
            6 => draft.formula = value,
            _ => {}
        }
    }
    draft.effect(current)
}

impl Action for Control {
    fn apply(&self, world: &mut World, root: Entity) {
        if !world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled)
        {
            return;
        }
        if world.get::<Editor>(root).is_none() {
            world.entity_mut(root).insert(Editor::default());
        }
        let selected = world.get::<Editor>(root).unwrap().selected.clone();
        let entity = selected.as_deref().and_then(|id| find(world, root, id));
        let panel = world
            .query::<(Entity, &Panel)>()
            .iter(world)
            .find(|(_, p)| p.0 == root)
            .map(|(e, _)| e);
        let result: Result<Option<String>, String> = (|| {
            match self {
                Self::Open => {
                    let open = !world.get::<Editor>(root).unwrap().open;
                    world.get_mut::<Editor>(root).unwrap().open = open;
                    if !open {
                        clear_preview(world, root);
                        world.get_mut::<Editor>(root).unwrap().picking = false;
                    }
                    world.get_mut::<Editor>(root).unwrap().rebuild = true;
                }
                Self::Select(id) => {
                    clear_preview(world, root);
                    let mut editor = world.get_mut::<Editor>(root).unwrap();
                    editor.selected = Some(id.clone());
                    editor.rebuild = true;
                    editor.open = true;
                    if let Some(entity) = find(world, root, id) {
                        crate::native::canvas_item::select(world, root, entity);
                    }
                }
                Self::Add(preset) => {
                    let workspace = world
                        .get::<crate::native::workspace::Workspaces>(root)
                        .unwrap()
                        .active;
                    let position = world
                        .get::<crate::native::canvas::CanvasView>(root)
                        .unwrap()
                        .center
                        .to_array();
                    let id = nucleus::new_uid("placement");
                    let effect = TopologyEffect {
                        name: lince_interface::terrain::Draft::preset_name(*preset).into(),
                        preset: *preset,
                        ..Default::default()
                    };
                    let receipt = transact(
                        world,
                        root,
                        Mutation::Add {
                            placement: id.clone(),
                            workspace,
                            component: component(&effect),
                            geometry: api::Geometry {
                                position,
                                size: [32.0; 2],
                            },
                        },
                    )?;
                    let mut editor = world.get_mut::<Editor>(root).unwrap();
                    editor.selected = Some(id);
                    editor.open = true;
                    return Ok(Some(receipt));
                }
                Self::Apply => {
                    let entity = entity.ok_or("Select a topology effect.")?;
                    let effect = world
                        .get::<Effect>(entity)
                        .ok_or("Select a topology effect.")?
                        .0
                        .clone();
                    if world.get::<Editor>(root).unwrap().baseline.as_ref() != Some(&effect) {
                        return Err("Effect changed while you were editing. Reset preview to reload its settings.".into());
                    }
                    let effect = draft(world, panel.ok_or("Open the floor editor.")?, &effect)?;
                    return configure(world, root, selected.unwrap(), effect).map(Some);
                }
                Self::Cancel => {
                    clear_preview(world, root);
                    let mut editor = world.get_mut::<Editor>(root).unwrap();
                    editor.rebuild = true;
                    editor.notice.clear();
                    editor.draft_error = None;
                }
                Self::Preset(_) | Self::Enable => {
                    let entity = entity.ok_or("Select a topology effect.")?;
                    let mut effect = world
                        .get::<Effect>(entity)
                        .ok_or("Select a topology effect.")?
                        .0
                        .clone();
                    if let Self::Preset(preset) = self {
                        effect.preset = *preset;
                        if let Some(panel) = panel {
                            effect = draft(world, panel, &effect)?;
                        }
                    } else {
                        effect.enabled = !effect.enabled;
                    }
                    return configure(world, root, selected.unwrap(), effect).map(Some);
                }
                Self::Duplicate => {
                    let entity = entity.ok_or("Select a topology effect.")?;
                    let member = world
                        .get::<crate::native::workspace::WorkspaceMember>(entity)
                        .unwrap()
                        .0;
                    let position = world
                        .get::<crate::native::canvas::CanvasItem>(entity)
                        .unwrap()
                        .position
                        + DVec2::splat(64.0);
                    let id = nucleus::new_uid("placement");
                    let receipt = transact(
                        world,
                        root,
                        Mutation::Duplicate {
                            placement: selected.unwrap(),
                            copy: id.clone(),
                            workspace: member,
                            position: position.to_array(),
                        },
                    )?;
                    world.get_mut::<Editor>(root).unwrap().selected = Some(id);
                    return Ok(Some(receipt));
                }
                Self::Delete => {
                    return transact(
                        world,
                        root,
                        Mutation::Remove {
                            placement: selected.ok_or("Select a topology effect.")?,
                        },
                    )
                    .map(Some);
                }
                Self::Undo | Self::Redo => {
                    clear_preview(world, root);
                    let redo = matches!(self, Self::Redo);
                    let receipt = {
                        let mut editor = world.get_mut::<Editor>(root).unwrap();
                        if redo {
                            editor.redo.pop()
                        } else {
                            editor.undo.pop()
                        }
                    }
                    .ok_or("No edit to undo or redo.")?;
                    match transact(
                        world,
                        root,
                        Mutation::UndoChanges {
                            receipt: receipt.clone(),
                        },
                    ) {
                        Ok(next) => {
                            let mut editor = world.get_mut::<Editor>(root).unwrap();
                            if redo {
                                editor.undo.push(next);
                            } else {
                                editor.redo.push(next);
                            }
                            editor.rebuild = true;
                        }
                        Err(error) => {
                            let mut editor = world.get_mut::<Editor>(root).unwrap();
                            if redo {
                                editor.redo.push(receipt);
                            } else {
                                editor.undo.push(receipt);
                            }
                            return Err(error);
                        }
                    }
                }
                Self::Bind(anchor) => {
                    return bind(
                        world,
                        root,
                        entity.ok_or("Select a topology effect.")?,
                        anchor.clone(),
                    )
                    .map(Some);
                }
                Self::Pick => {
                    if let Some(entity) = entity {
                        crate::native::canvas_item::select(world, root, entity);
                    }
                    world.get_mut::<Editor>(root).unwrap().picking = true;
                    world.get_mut::<Editor>(root).unwrap().notice =
                        "Select the Sand or Area to use as the anchor.".into();
                }
                Self::Follow | Self::CameraFloor | Self::Offsets => {
                    let entity = entity.ok_or("Select a topology effect.")?;
                    let snapshot = crate::native::canvas_host::capture(world, root)?;
                    let id = selected.unwrap();
                    let mut metadata = snapshot
                        .placements
                        .iter()
                        .find(|p| p.id == id)
                        .unwrap()
                        .metadata
                        .clone();
                    let binding = metadata.binding.get_or_insert_with(|| SpatialBinding {
                        position: super::super::position(world, entity).unwrap().to_array(),
                        ..Default::default()
                    });
                    if matches!(self, Self::Follow) {
                        binding.follow_orientation = !binding.follow_orientation;
                    }
                    if matches!(self, Self::CameraFloor) {
                        binding.camera_floor = !binding.camera_floor;
                    }
                    if matches!(self, Self::Offsets) {
                        let panel = panel.ok_or("Open the floor editor.")?;
                        for (field, text, parent) in world
                            .query::<(&Field, &bevy::text::EditableText, &ChildOf)>()
                            .iter(world)
                        {
                            if parent.parent() == panel && (7..=9).contains(&field.0) {
                                binding.offset[usize::from(field.0 - 7)] = text
                                    .editor
                                    .text()
                                    .to_string()
                                    .trim()
                                    .parse()
                                    .map_err(|_| "Enter a finite offset.")?;
                            }
                        }
                    }
                    return transact(
                        world,
                        root,
                        Mutation::SetMetadata {
                            placement: id,
                            metadata,
                        },
                    )
                    .map(Some);
                }
                Self::Align(target) => {
                    let snapshot = crate::native::canvas_host::capture(world, root)?;
                    let id = world
                        .get::<crate::native::canvas_host::Identity>(*target)
                        .ok_or("Select a Sand.")?
                        .0
                        .clone();
                    let mut metadata = snapshot
                        .placements
                        .iter()
                        .find(|p| p.id == id)
                        .unwrap()
                        .metadata
                        .clone();
                    metadata.follow_slope = !metadata.follow_slope;
                    return transact(
                        world,
                        root,
                        Mutation::SetMetadata {
                            placement: id,
                            metadata,
                        },
                    )
                    .map(Some);
                }
                Self::AreaCamera(target) => {
                    return bind(world, root, *target, Anchor::Camera).map(Some);
                }
                Self::AreaDetach(target) => {
                    return bind(world, root, *target, Anchor::Fixed).map(Some);
                }
            }
            Ok(None)
        })();
        match result {
            Ok(Some(receipt)) => remember(world, root, receipt),
            Ok(None) => {}
            Err(error) => world.get_mut::<Editor>(root).unwrap().notice = error,
        }
    }
}

fn button(world: &mut World, parent: Entity, root: Entity, label: &str, action: Control) -> Entity {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            Node {
                padding: UiRect::all(px(6)),
                flex_shrink: 0.0,
                ..default()
            },
            ActionButton::new(root, crate::native::actions![action]),
            ChildOf(parent),
        ))
        .id();
    crate::native::edit_mode::label(world, entity, label, 13.0);
    entity
}

fn field(world: &mut World, panel: Entity, index: u8, label: &str, value: &str) {
    crate::native::edit_mode::label(world, panel, label, 12.0);
    let editor = crate::native::sand::single_line_editor(
        value,
        world.resource::<crate::native::theme::Typography>(),
        0,
        4096,
    );
    let entity = world.spawn((Field(index), editor, ChildOf(panel))).id();
    world.get_mut::<Node>(entity).unwrap().min_height = px(30);
    if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        accessibility.set_label(label);
    }
}

fn clear_preview(world: &mut World, root: Entity) {
    let entities: Vec<_> = world
        .query_filtered::<(Entity, &ChildOf), With<Preview>>()
        .iter(world)
        .filter(|(_, p)| p.parent() == root)
        .map(|(e, _)| e)
        .collect();
    for entity in entities {
        world.entity_mut(entity).remove::<Preview>();
        if world
            .get::<Fault>(entity)
            .zip(world.get::<Effect>(entity))
            .is_some_and(|(fault, effect)| fault.1 != effect.0)
        {
            world.entity_mut(entity).remove::<Fault>();
        }
    }
}

fn drag_edits(world: &mut World, root: Entity) {
    let items: Vec<_> = world
        .query_filtered::<(
            Entity,
            &crate::native::canvas_host::Identity,
            &crate::native::canvas::CanvasItem,
            Option<&Bound>,
            &ChildOf,
        ), Or<(With<Effect>, With<Bound>)>>()
        .iter(world)
        .filter(|(_, _, _, _, p)| p.parent() == root)
        .map(|(e, id, item, bound, _)| (e, id.0.clone(), item.position, bound.map(|b| b.0.clone())))
        .collect();
    for (entity, id, position, binding) in &items {
        let dragging = crate::native::canvas_pan::dragged(world) == Some(*entity)
            || world
                .get_resource::<super::super::input::PointerState>()
                .is_some_and(|p| p.drag.is_some_and(|(e, _)| e == *entity));
        if dragging {
            let before = world
                .get::<Editor>(root)
                .unwrap()
                .previous
                .get(id)
                .cloned()
                .unwrap_or((*position, binding.clone()));
            world
                .get_mut::<Editor>(root)
                .unwrap()
                .drags
                .entry(id.clone())
                .or_insert(before);
        } else if let Some((old_position, old_binding)) =
            world.get_mut::<Editor>(root).unwrap().drags.remove(id)
        {
            let result = if binding.as_ref().is_some_and(|b| b.anchor != Anchor::Fixed) {
                if old_binding
                    .as_ref()
                    .zip(binding.as_ref())
                    .is_none_or(|(old, new)| old.same_attachment(new))
                {
                    continue;
                }
                let snapshot = crate::native::canvas_host::capture(world, root);
                snapshot.and_then(|snapshot| {
                    let mut metadata = snapshot
                        .placements
                        .iter()
                        .find(|p| &p.id == id)
                        .ok_or("Dragged placement is unavailable.")?
                        .metadata
                        .clone();
                    metadata.binding = binding.clone();
                    if let Some(before) = old_binding {
                        world.entity_mut(*entity).insert(Bound(before));
                    }
                    let result = transact(
                        world,
                        root,
                        Mutation::SetMetadata {
                            placement: id.clone(),
                            metadata,
                        },
                    );
                    if result.is_err()
                        && let Some(binding) = binding
                    {
                        world.entity_mut(*entity).insert(Bound(binding.clone()));
                    }
                    result
                })
            } else {
                if old_position == *position {
                    continue;
                }
                let workspace = world
                    .get::<crate::native::workspace::WorkspaceMember>(*entity)
                    .unwrap()
                    .0;
                world
                    .get_mut::<crate::native::canvas::CanvasItem>(*entity)
                    .unwrap()
                    .position = old_position;
                if let Some(mut area) = world.get_mut::<crate::native::area::InfluenceArea>(*entity)
                {
                    area.center = old_position.to_array();
                }
                let result = transact(
                    world,
                    root,
                    Mutation::Move {
                        placement: id.clone(),
                        workspace,
                        position: position.to_array(),
                    },
                );
                if result.is_err() {
                    world
                        .get_mut::<crate::native::canvas::CanvasItem>(*entity)
                        .unwrap()
                        .position = *position;
                    if let Some(mut area) =
                        world.get_mut::<crate::native::area::InfluenceArea>(*entity)
                    {
                        area.center = position.to_array();
                    }
                }
                result
            };
            match result {
                Ok(receipt) => remember(world, root, receipt),
                Err(error) => world.get_mut::<Editor>(root).unwrap().notice = error,
            }
        }
    }
    world.get_mut::<Editor>(root).unwrap().previous = items
        .into_iter()
        .map(|(_, id, position, binding)| (id, (position, binding)))
        .collect();
}

fn render(world: &mut World, root: Entity) {
    let existing = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .find(|(_, p)| p.0 == root)
        .map(|(e, _)| e);
    if !world.get::<Editor>(root).is_some_and(|e| e.open) {
        if let Some(panel) = existing {
            world.get_mut::<Node>(panel).unwrap().display = Display::None;
        }
        return;
    }
    let panel = if let Some(panel) = existing {
        if let Some(mut surface) = world.get_resource_mut::<bevy::ui::ui_surface::UiSurface>() {
            surface.try_remove_children(panel);
        }
        let children: Vec<_> = world
            .get::<Children>(panel)
            .into_iter()
            .flatten()
            .copied()
            .collect();
        for child in children {
            world.despawn(child);
        }
        world.get_mut::<Node>(panel).unwrap().display = Display::Flex;
        panel
    } else {
        world
            .spawn((
                Panel(root),
                crate::native::inspection::InspectionExcluded,
                crate::native::sand::Square,
                Node {
                    position_type: PositionType::Absolute,
                    right: px(12),
                    top: px(80),
                    width: px(320),
                    max_height: percent(80),
                    overflow: Overflow::scroll_y(),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(px(10)),
                    row_gap: px(4),
                    ..default()
                },
                ScrollPosition::default(),
                GlobalZIndex(30),
                bevy::input_focus::tab_navigation::TabGroup::modal(),
                ChildOf(root),
                crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ))
            .observe(
                |mut event: On<Pointer<Scroll>>, mut scrolls: Query<&mut ScrollPosition>| {
                    if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
                        let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
                            24.0
                        } else {
                            1.0
                        };
                        scroll.0.y -= event.y * scale;
                        event.propagate(false);
                    }
                },
            )
            .id()
    };
    world.get_mut::<Editor>(root).unwrap().baseline = None;
    crate::native::edit_mode::label(world, panel, "Floor topology", 17.0);
    button(world, panel, root, "Close floor editor", Control::Open);
    let tools = world
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
    for preset in [
        Preset::Hill,
        Preset::Valley,
        Preset::Slope,
        Preset::Ring,
        Preset::Formula,
    ] {
        button(
            world,
            tools,
            root,
            &format!(
                "Add {}",
                lince_interface::terrain::Draft::preset_name(preset).to_lowercase()
            ),
            Control::Add(preset),
        );
    }
    button(world, tools, root, "Undo", Control::Undo);
    button(world, tools, root, "Redo", Control::Redo);
    let workspace = world
        .get::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active;
    let mut effects: Vec<_> = world
        .query::<(
            &Effect,
            &crate::native::canvas_host::Identity,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>()
        .iter(world)
        .filter(|(_, _, p, m)| p.parent() == root && m.0 == workspace)
        .map(|(effect, id, _, _)| (id.0.clone(), effect.0.name.clone()))
        .collect();
    effects.sort_by(|a, b| a.0.cmp(&b.0));
    for (id, name) in effects {
        button(world, panel, root, &name, Control::Select(id));
    }
    let selected = world.get::<Editor>(root).unwrap().selected.clone();
    if let Some(entity) = selected.as_deref().and_then(|id| find(world, root, id)) {
        if let Some(effect) = world.get::<Effect>(entity).map(|e| e.0.clone()) {
            world.get_mut::<Editor>(root).unwrap().baseline = Some(effect.clone());
            let draft = lince_interface::terrain::Draft::from(&effect);
            for (index, label, value) in [
                (0, "Name", &draft.name),
                (1, "Height", &draft.amplitude),
                (2, "Radius X", &draft.radius_x),
                (3, "Radius Y", &draft.radius_y),
                (4, "Rotation (degrees)", &draft.rotation),
                (5, "Edge falloff", &draft.falloff),
                (6, "Formula (normalized x, y, r, theta)", &draft.formula),
            ] {
                field(world, panel, index, label, value);
            }
            crate::native::edit_mode::label(world, panel, "Preset", 12.0);
            let presets = world
                .spawn((
                    Node {
                        flex_wrap: FlexWrap::Wrap,
                        ..default()
                    },
                    ChildOf(panel),
                ))
                .id();
            for preset in [
                Preset::Hill,
                Preset::Valley,
                Preset::Slope,
                Preset::Ring,
                Preset::Formula,
            ] {
                button(
                    world,
                    presets,
                    root,
                    lince_interface::terrain::Draft::preset_name(preset),
                    Control::Preset(preset),
                );
            }
            for (label, control) in [
                ("Apply", Control::Apply),
                ("Reset preview", Control::Cancel),
                (
                    if effect.enabled { "Disable" } else { "Enable" },
                    Control::Enable,
                ),
                ("Duplicate", Control::Duplicate),
                ("Delete effect", Control::Delete),
                ("Fixed point", Control::Bind(Anchor::Fixed)),
                ("Follow cursor", Control::Bind(Anchor::Cursor)),
                ("Follow camera", Control::Bind(Anchor::Camera)),
                ("Pick Sand / Area anchor", Control::Pick),
            ] {
                button(world, panel, root, label, control);
            }
        }
        binding_fields(world, panel, root, entity);
    }
    world.spawn((
        Notice(root),
        Text::new(""),
        world
            .resource::<crate::native::theme::Typography>()
            .text(12.0),
        ChildOf(panel),
    ));
}

fn binding_fields(world: &mut World, panel: Entity, root: Entity, entity: Entity) {
    let binding = world
        .get::<Bound>(entity)
        .map(|b| b.0.clone())
        .unwrap_or_default();
    crate::native::edit_mode::label(world, panel, &format!("Anchor: {:?}", binding.anchor), 12.0);
    for (index, label) in [(7, "Offset X"), (8, "Offset height"), (9, "Offset Y")] {
        if index == 8 && world.get::<Effect>(entity).is_some() {
            continue;
        }
        field(
            world,
            panel,
            index,
            label,
            &binding.offset[usize::from(index - 7)].to_string(),
        );
    }
    button(world, panel, root, "Apply offsets", Control::Offsets);
    button(
        world,
        panel,
        root,
        if binding.follow_orientation {
            "Follow orientation: on"
        } else {
            "Follow orientation: off"
        },
        Control::Follow,
    );
    if world.get::<Effect>(entity).is_none() {
        button(
            world,
            panel,
            root,
            if binding.camera_floor {
                "Camera anchor: floor"
            } else {
                "Camera anchor: 3D position"
            },
            Control::CameraFloor,
        );
    }
}

pub fn item_controls(world: &mut World, panel: Entity, root: Entity, target: Entity) {
    if world.get::<Unsupported>(target).is_some() {
        crate::native::edit_mode::label(
            world,
            panel,
            "Terrain physics paused: reduce this item's size or disable floor effects.",
            12.0,
        );
    }
    if world.get::<Effect>(target).is_some() {
        if let Some(id) = world
            .get::<crate::native::canvas_host::Identity>(target)
            .map(|id| id.0.clone())
        {
            button(world, panel, root, "Edit floor effect", Control::Select(id));
        }
        return;
    }
    if world
        .get::<crate::native::area::InfluenceArea>(target)
        .is_some()
    {
        button(
            world,
            panel,
            root,
            "Bind Area to camera",
            Control::AreaCamera(target),
        );
        button(
            world,
            panel,
            root,
            "Detach Area binding",
            Control::AreaDetach(target),
        );
        button(
            world,
            panel,
            root,
            "Edit Area binding",
            Control::Select(
                world
                    .get::<crate::native::canvas_host::Identity>(target)
                    .map(|id| id.0.clone())
                    .unwrap_or_default(),
            ),
        );
    } else {
        button(
            world,
            panel,
            root,
            if world.get::<FollowSlope>(target).is_some() {
                "Follow slope: on"
            } else {
                "Follow slope: off"
            },
            Control::Align(target),
        );
    }
}

pub fn update(world: &mut World) {
    if !world.contains_resource::<crate::native::theme::Typography>() {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<crate::native::workspace::Workspaces>>()
        .iter(world)
        .collect();
    for root in roots {
        if world.get::<Editor>(root).is_none() {
            world.entity_mut(root).insert(Editor::default());
        }
        if world.get::<Toolbar>(root).is_none() {
            let toolbar = crate::native::canvas_controls::toolbar(world, root);
            button(world, toolbar, root, "Floor", Control::Open);
            world.entity_mut(root).insert(Toolbar);
        }
        let editing = world
            .get::<crate::native::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled);
        let open = editing && world.get::<Editor>(root).unwrap().open;
        if open {
            if let Some(panel) = world
                .get::<crate::native::edit_mode::EditMode>(root)
                .map(|m| m.panel)
                && world
                    .get::<Editor>(root)
                    .unwrap()
                    .edit_panel
                    .is_none_or(|(current, _)| current != panel)
                && let Some(display) = world.get::<Node>(panel).map(|n| n.display)
            {
                world.get_mut::<Editor>(root).unwrap().edit_panel = Some((panel, display));
                world.get_mut::<Node>(panel).unwrap().display = Display::None;
            }
        } else if let Some((panel, display)) =
            world.get_mut::<Editor>(root).unwrap().edit_panel.take()
            && let Some(mut node) = world.get_mut::<Node>(panel)
        {
            node.display = display;
        }
        if !editing {
            clear_preview(world, root);
            let panels: Vec<_> = world
                .query::<(Entity, &Panel)>()
                .iter(world)
                .filter(|(_, p)| p.0 == root)
                .map(|(e, _)| e)
                .collect();
            for panel in panels {
                world.get_mut::<Node>(panel).unwrap().display = Display::None;
            }
            world.get_mut::<Editor>(root).unwrap().rebuild = true;
            let mut editor = world.get_mut::<Editor>(root).unwrap();
            editor.picking = false;
            editor.drags.clear();
            editor.previous.clear();
            continue;
        }
        drag_edits(world, root);
        if world.get::<Editor>(root).unwrap().picking {
            let inspected = world
                .get::<crate::native::inspection::Inspection>(root)
                .and_then(|i| i.selected);
            let selected = world.get::<Editor>(root).unwrap().selected.clone();
            if let Some(target) = inspected.and_then(|e| {
                world
                    .get::<crate::native::canvas_host::Identity>(e)
                    .map(|id| id.0.clone())
            }) && Some(&target) != selected.as_ref()
                && let Some(entity) = selected.as_deref().and_then(|id| find(world, root, id))
            {
                let result = bind(world, root, entity, Anchor::Item(target));
                world.get_mut::<Editor>(root).unwrap().picking = false;
                match result {
                    Ok(receipt) => remember(world, root, receipt),
                    Err(error) => world.get_mut::<Editor>(root).unwrap().notice = error,
                }
            }
        }
        if world.get::<Editor>(root).unwrap().rebuild {
            world.get_mut::<Editor>(root).unwrap().rebuild = false;
            render(world, root);
        }
        let panel = world
            .query::<(Entity, &Panel)>()
            .iter(world)
            .find(|(_, p)| p.0 == root)
            .map(|(e, _)| e);
        let panel = panel.filter(|_| world.get::<Editor>(root).unwrap().open);
        let selected = world.get::<Editor>(root).unwrap().selected.clone();
        if let Some(entity) = selected.as_deref().and_then(|id| find(world, root, id))
            && let Some(current) = world.get::<Effect>(entity).map(|e| e.0.clone())
            && let Some(panel) = panel
        {
            match draft(world, panel, &current) {
                Ok(preview) if preview != current => {
                    world.get_mut::<Editor>(root).unwrap().draft_error = None;
                    if world.get::<Preview>(entity).is_none_or(|p| p.0 != preview) {
                        world.entity_mut(entity).insert(Preview(preview));
                    }
                }
                Ok(_) => {
                    world.get_mut::<Editor>(root).unwrap().draft_error = None;
                    world.entity_mut(entity).remove::<Preview>();
                }
                Err(error) => {
                    world.entity_mut(entity).remove::<Preview>();
                    world.get_mut::<Editor>(root).unwrap().draft_error = Some(error);
                }
            }
        }
        let editor = world.get::<Editor>(root).unwrap();
        let mut notice = editor
            .draft_error
            .clone()
            .unwrap_or_else(|| editor.notice.clone());
        if let Some(entity) = selected.as_deref().and_then(|id| find(world, root, id))
            && let Some(fault) = world.get::<Fault>(entity)
        {
            notice = fault.0.clone();
        }
        let labels: Vec<_> = world
            .query::<(Entity, &Notice)>()
            .iter(world)
            .filter(|(_, n)| n.0 == root)
            .map(|(e, _)| e)
            .collect();
        for label in labels {
            if let Some(mut text) = world.get_mut::<Text>(label)
                && text.0 != notice
            {
                text.0 = notice.clone();
            }
        }
    }
}
