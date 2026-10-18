use super::LayoutBox;
use crate::{
    host::{
        ContentMapping, DesktopAction, GeometrySnapshot, ItemGeometry, ItemId, OutputId,
        OutputViewport, Rectangle, SurfaceId, WorkspaceLayout,
    },
    layout::desktop::{Geometry, Item, ItemKind, PlacementId},
    native::{
        canvas::{CanvasItem, CanvasView},
        workspace::{WorkspaceMember, Workspaces},
    },
};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

mod commands;
mod transients;
pub use commands::COMMANDS;

#[derive(Resource)]
pub struct DefaultLayout(pub WorkspaceLayout);

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Managed {
    pub id: PlacementId,
    pub floating: bool,
}

#[derive(Component)]
pub struct Suppressed;

#[derive(Component, Clone, Copy)]
pub struct ApplicationWindow {
    pub surface: SurfaceId,
    pub content_size: [u32; 2],
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransientParent(pub Entity);

#[derive(Component, Clone, Copy)]
pub struct Output(pub OutputId);

#[derive(Component, Clone)]
pub struct HostViewport(pub OutputViewport);

#[derive(Resource, Default)]
pub struct PublishedGeometry(pub Option<GeometrySnapshot>);

#[derive(Resource)]
pub struct HostRequests(pub std::sync::mpsc::SyncSender<crate::host::HostRequest>);

pub fn request_close(world: &World, entity: Entity) -> bool {
    let Some(window) = world.get::<ApplicationWindow>(entity) else {
        return false;
    };
    host_request(world, crate::host::HostRequest::Close(window.surface));
    true
}

fn host_request(world: &World, request: crate::host::HostRequest) {
    let sent = world
        .get_resource::<HostRequests>()
        .is_some_and(|requests| requests.0.try_send(request).is_ok());
    if !sent {
        crate::native::notifications::report(
            world,
            "Park",
            "The desktop is busy or unavailable; try the action again.",
        );
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
}

#[derive(Resource, Default)]
struct Runtime {
    seen: HashMap<(Entity, u64), BTreeSet<PlacementId>>,
    views: HashMap<(Entity, u64), (WorkspaceLayout, [f64; 2], [f64; 2])>,
    geometry: HashMap<Entity, Geometry>,
    fullscreen_views: HashMap<(Entity, u64), CanvasView>,
    influences: HashMap<(Entity, u64), InfluenceStamp>,
}

#[derive(PartialEq)]
struct InfluenceStamp {
    sources: Vec<(Entity, [u32; 4])>,
    constraints: Vec<(Entity, u32)>,
    items: Vec<PlacementId>,
    bounds: [f64; 4],
    settings: u32,
}

fn influence_stamp(
    world: &mut World,
    root: Entity,
    workspace: u64,
    bounds: Rectangle,
    entities: &BTreeMap<PlacementId, Entity>,
) -> InfluenceStamp {
    let mut sources: Vec<_> = world
        .query::<(
            Entity,
            &ChildOf,
            &WorkspaceMember,
            Option<Ref<crate::native::area::InfluenceArea>>,
            Option<Ref<crate::native::protein_area::filter::Matches>>,
            Option<Ref<crate::native::area::RecordProperties>>,
            Option<Ref<crate::native::protein_area::RecordBinding>>,
        )>()
        .iter(world)
        .filter(|(_, parent, member, area, filter, record, binding)| {
            parent.parent() == root
                && member.0 == workspace
                && (area.is_some() || filter.is_some() || record.is_some() || binding.is_some())
        })
        .map(|(entity, _, _, area, filter, record, binding)| {
            (
                entity,
                [
                    area.map_or(0, |value| value.last_changed().get()),
                    filter.map_or(0, |value| value.last_changed().get()),
                    record.map_or(0, |value| value.last_changed().get()),
                    binding.map_or(0, |value| value.last_changed().get()),
                ],
            )
        })
        .collect();
    sources.sort_by_key(|(entity, _)| *entity);
    InfluenceStamp {
        sources,
        constraints: entities
            .values()
            .map(|entity| {
                (
                    *entity,
                    world
                        .get_entity(*entity)
                        .ok()
                        .and_then(|entity| {
                            entity
                                .get_ref::<LayoutBox>()
                                .map(|layout| layout.last_changed().get())
                        })
                        .unwrap_or(0),
                )
            })
            .collect(),
        items: entities.keys().copied().collect(),
        bounds: [
            bounds.origin()[0],
            bounds.origin()[1],
            bounds.size()[0],
            bounds.size()[1],
        ],
        settings: world.get_entity(root).ok().map_or(0, |entity| {
            entity
                .get_ref::<crate::native::workspace_config::WorkspaceSettings>()
                .map_or(0, |settings| settings.last_changed().get())
        }),
    }
}

pub fn tiled(world: &World, entity: Entity) -> bool {
    world
        .get::<Managed>(entity)
        .is_some_and(|managed| !managed.floating)
}

pub fn focus_item(world: &mut World, entity: Entity) {
    let Some(root) = root(world, entity) else {
        return;
    };
    let Some(bounds) = usable(world, root) else {
        return;
    };
    let Some(id) = world.get::<Managed>(entity).map(|managed| managed.id) else {
        return;
    };
    let transient = world.get::<TransientParent>(entity).is_some();
    let mut ancestors = Vec::new();
    let mut ancestor = entity;
    for _ in 0..32 {
        let Some(parent) = world.get::<TransientParent>(ancestor) else { break };
        if parent.0 == ancestor { break; }
        ancestor = parent.0;
        if let Some(layout) = world.get::<LayoutBox>(ancestor) { ancestors.push(PlacementId(layout.id)); }
    }
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let active = spaces.active;
    if let Some(workspace) = spaces
        .entries
        .iter_mut()
        .find(|workspace| workspace.id == active)
    {
        for ancestor in ancestors { workspace.desktop.set_minimized(ancestor, false); }
        let fullscreen = workspace.desktop.fullscreen().filter(|_| transient);
        let _ = workspace.desktop.focus(id, bounds);
        if let Some(id) = fullscreen {
            workspace.desktop.request_state(
                id,
                crate::host::WindowRequest {
                    minimized: None,
                    fullscreen: Some(true),
                    maximized: None,
                },
            );
        }
    }
}

pub(super) fn clicked(
    event: On<Pointer<Press>>,
    managed: Query<&Managed>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    if event.button != PointerButton::Primary || event.entity != event.original_event_target() {
        return;
    }
    let mut entity = event.entity;
    for _ in 0..super::MAX_DEPTH {
        if managed.contains(entity) {
            commands.queue(move |world: &mut World| focus_item(world, entity));
            return;
        }
        let Ok(parent) = parents.get(entity) else {
            return;
        };
        entity = parent.parent();
    }
}

fn viewport(world: &World, root: Entity) -> Option<Rectangle> {
    if let Some(viewport) = world.get::<HostViewport>(root) {
        return Some(viewport.0.bounds);
    }
    let node = world.get::<ComputedNode>(root)?;
    let size = node.size() * node.inverse_scale_factor();
    Rectangle::new([0.0; 2], size.as_dvec2().to_array())
}

fn usable(world: &World, root: Entity) -> Option<Rectangle> {
    world
        .get::<HostViewport>(root)
        .map_or_else(|| viewport(world, root), |viewport| viewport.0.usable)
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..super::MAX_DEPTH {
        if world.get::<Workspaces>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
    None
}

fn candidate(world: &World, entity: Entity) -> bool {
    world
        .get::<crate::native::area::InfluenceArea>(entity)
        .is_none()
        && world
            .get::<crate::native::arrow_sand::ArrowSand>(entity)
            .is_none()
        && world
            .get::<crate::native::sand_placement::Pinned>(entity)
            .is_none()
        && world
            .get::<crate::native::time_castle::AttachedCard>(entity)
            .is_none()
        && world
            .get::<crate::native::topology::terrain::Effect>(entity)
            .is_none()
        && world
            .get::<crate::native::external_drop::Preview>(entity)
            .is_none()
        && world
            .get::<LayoutBox>(entity)
            .is_none_or(|layout| layout.parent.is_none())
}

pub(crate) fn resolve(world: &mut World) {
    transients::inherit(world);
    world.init_resource::<Runtime>();
    world.init_resource::<PublishedGeometry>();
    let roots: Vec<_> = world
        .query::<(Entity, &Workspaces)>()
        .iter(world)
        .flat_map(|(root, spaces)| {
            if let Some(views) = world
                .get_resource::<crate::native::output_views::OutputViews>()
                .filter(|views| views.applies_to(root))
            {
                return views
                    .outputs
                    .iter()
                    .filter_map(|output| {
                        spaces
                            .entries
                            .iter()
                            .any(|space| space.id == output.workspace)
                            .then_some(())?;
                        Some((
                            root,
                            output.workspace,
                            output.bounds,
                            output.usable?,
                            output.id,
                        ))
                    })
                    .collect::<Vec<_>>();
            }
            viewport(world, root)
                .zip(usable(world, root))
                .map(|(bounds, usable)| {
                    (
                        root,
                        spaces.active,
                        bounds,
                        usable,
                        world
                            .get::<Output>(root)
                            .map_or(OutputId(root.to_bits()), |output| output.0),
                    )
                })
                .into_iter()
                .collect()
        })
        .collect();
    let mut outputs = Vec::new();
    let mut geometries = Vec::new();
    let mut focused = None;
    for (root, active, bounds, usable, output) in roots {
        let focused_view = world.get::<Workspaces>(root).unwrap().active == active;
        let mode = world
            .get::<Workspaces>(root)
            .unwrap()
            .entries
            .iter()
            .find(|workspace| workspace.id == active)
            .map(|workspace| workspace.desktop.mode())
            .unwrap_or(WorkspaceLayout::Free);
        if mode == WorkspaceLayout::Free
            && !world.contains_resource::<DefaultLayout>()
            && world
                .get::<Workspaces>(root)
                .unwrap()
                .entries
                .iter()
                .find(|workspace| workspace.id == active)
                .is_none_or(|workspace| workspace.desktop.items().is_empty())
        {
            let managed: Vec<_> = world
                .query::<(Entity, &ChildOf, &Managed)>()
                .iter(world)
                .filter(|(_, parent, _)| parent.parent() == root)
                .map(|(entity, _, _)| entity)
                .collect();
            for entity in managed {
                world.entity_mut(entity).remove::<Managed>();
            }
            let hidden: Vec<_> = world
                .query_filtered::<(Entity, &ChildOf), With<Suppressed>>()
                .iter(world)
                .filter(|(_, parent)| parent.parent() == root)
                .map(|(entity, _)| entity)
                .collect();
            for entity in hidden {
                world.entity_mut(entity).remove::<Suppressed>();
            }
            world
                .resource_mut::<Runtime>()
                .views
                .remove(&(root, active));
            continue;
        }
        let mut model = {
            let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
            let Some(workspace) = spaces
                .entries
                .iter_mut()
                .find(|workspace| workspace.id == active)
            else {
                continue;
            };
            std::mem::take(&mut workspace.desktop)
        };
        let rows: Vec<_> = world
            .query::<(Entity, &CanvasItem, &ChildOf, &WorkspaceMember)>()
            .iter(world)
            .filter(|(entity, _, parent, member)| {
                parent.parent() == root && member.0 == active && candidate(world, *entity)
            })
            .map(|(entity, item, _, _)| (entity, *item))
            .collect();
        let mut entities = BTreeMap::new();
        let mut inserted = false;
        let previous = world.resource_mut::<Runtime>().seen.remove(&(root, active));
        if previous.is_none() {
            let stale: Vec<_> = model
                .items()
                .iter()
                .filter(|item| item.kind == ItemKind::Application)
                .map(|item| item.id)
                .collect();
            for id in stale {
                model.remove(id);
            }
        }
        for (entity, item) in rows {
            if world.get::<LayoutBox>(entity).is_none() {
                world.entity_mut(entity).insert(LayoutBox::new(item.size));
            }
            let layout = *world.get::<LayoutBox>(entity).unwrap();
            let id = PlacementId(layout.id);
            if entities.insert(id, entity).is_some() {
                continue;
            }
            let mut floating = model
                .items()
                .iter()
                .any(|item| item.id == id && item.floating);
            if !floating
                && world.get::<TransientParent>(entity).is_some()
                && model.items().iter().any(|item| item.id == id)
                && let Some(rectangle) = Rectangle::new(
                    (item.position - item.size.as_dvec2() * 0.5).to_array(),
                    item.size.as_dvec2().to_array(),
                )
            {
                floating = model.toggle_floating(id, rectangle).is_ok();
            }
            let origin = item.position
                - item.size.as_dvec2() * 0.5
                - if floating {
                    bevy::math::DVec2::from_array(model.scroll())
                } else {
                    bevy::math::DVec2::ZERO
                };
            let geometry = Geometry {
                origin: origin.to_array(),
                size: item.size.as_dvec2().to_array(),
            };
            let previous_geometry = world.resource::<Runtime>().geometry.get(&entity).copied();
            let raw_geometry = Geometry {
                origin: (item.position - item.size.as_dvec2() * 0.5).to_array(),
                size: item.size.as_dvec2().to_array(),
            };
            if model.items().iter().any(|item| item.id == id) {
                let _ = model.configure(id, layout.rules);
                if (floating || mode == WorkspaceLayout::Free)
                    && model.fullscreen() != Some(id)
                    && previous_geometry != Some(raw_geometry)
                {
                    let _ = model.set_free_geometry(id, geometry);
                }
            } else {
                inserted |= model
                    .insert(Item {
                        id,
                        kind: if world.get::<ApplicationWindow>(entity).is_some() {
                            ItemKind::Application
                        } else {
                            ItemKind::Native
                        },
                        rules: layout.rules,
                        free: geometry,
                        floating: world.get::<TransientParent>(entity).is_some(),
                    })
                    .is_ok();
            }
        }
        if inserted && let Some(id) = model.focused() {
            let fullscreen = model.fullscreen().filter(|_| {
                entities
                    .get(&id)
                    .is_some_and(|entity| world.get::<TransientParent>(*entity).is_some())
            });
            let _ = model.focus(id, usable);
            if let Some(id) = fullscreen {
                model.request_state(
                    id,
                    crate::host::WindowRequest {
                        minimized: None,
                        fullscreen: Some(true),
                        maximized: None,
                    },
                );
            }
        }
        if let Some(previous) = previous {
            for id in previous.difference(&entities.keys().copied().collect()) {
                model.remove(*id);
            }
        }
        let stamp = influence_stamp(world, root, active, usable, &entities);
        if mode != WorkspaceLayout::Free
            && world.resource::<Runtime>().influences.get(&(root, active)) != Some(&stamp)
        {
            if let Ok(solution) = model.solve(usable, bounds) {
                for item in &solution.items {
                    if let Some(entity) = entities.get(&item.id)
                        && let Some(mut canvas) = world.get_mut::<CanvasItem>(*entity)
                    {
                        canvas.position =
                            bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                                item.rectangle.origin()[axis]
                                    + solution.scroll[axis]
                                    + item.rectangle.size()[axis] * 0.5
                            }));
                        canvas.size =
                            bevy::math::DVec2::from_array(item.rectangle.size()).as_vec2();
                    }
                }
            }
            crate::native::area_effects::refresh(world);
            let inputs: Vec<_> = entities
                .iter()
                .map(|(id, entity)| {
                    let item = world.get::<CanvasItem>(*entity).unwrap();
                    let input = world
                        .resource::<crate::native::area_effects::Influences>()
                        .layout_input(
                            *entity,
                            root,
                            active,
                            item.position,
                            world.get::<crate::native::area::RecordProperties>(*entity),
                            world.get::<crate::native::protein_area::RecordBinding>(*entity),
                        );
                    (*id, input)
                })
                .collect();
            for (id, input) in inputs {
                let _ = model.influence(id, input);
            }
            if let Some(id) = model.focused() {
                let _ = model.focus(id, usable);
            }
            world
                .resource_mut::<Runtime>()
                .influences
                .insert((root, active), stamp);
        }
        match model.solve(usable, bounds) {
            Ok(mut solution) => {
                transients::place(world, &entities, &mut model, &mut solution, usable);
                if solution.focused.and_then(|id| entities.get(&id)).is_some_and(|entity| transients::minimized(world, *entity, &model)) {
                    if let Some(id) = solution.items.first().map(|item| item.id) { let _ = model.focus(id, usable); }
                    else { model.clear_focus(); }
                    solution.focused = model.focused();
                }
                let fullscreen = model.fullscreen().and_then(|id| entities.get(&id).copied());
                let members: Vec<_> = world
                    .query::<(Entity, &ChildOf, &WorkspaceMember)>()
                    .iter(world)
                    .filter(|(entity, parent, member)| {
                        parent.parent() == root
                            && member.0 == active
                            && world.get::<CanvasItem>(*entity).is_some()
                    })
                    .map(|(entity, _, _)| entity)
                    .collect();
                for entity in members {
                    if transients::minimized(world, entity, &model) || fullscreen.is_some_and(|focused| {
                        focused != entity && !transients::descendant(world, entity, focused)
                    }) {
                        if world.get::<Suppressed>(entity).is_none() {
                            world.entity_mut(entity).insert(Suppressed);
                        }
                    } else {
                        world.entity_mut(entity).remove::<Suppressed>();
                    }
                }
                let view = (mode, solution.scroll, bounds.size());
                let changed_view =
                    world.resource::<Runtime>().views.get(&(root, active)) != Some(&view);
                let mut canvas = crate::native::output_views::canvas(world, root, active);
                if model.fullscreen().is_some() {
                    if !world
                        .resource::<Runtime>()
                        .fullscreen_views
                        .contains_key(&(root, active))
                    {
                        world
                            .resource_mut::<Runtime>()
                            .fullscreen_views
                            .insert((root, active), canvas);
                    }
                } else if let Some(view) = world
                    .resource_mut::<Runtime>()
                    .fullscreen_views
                    .remove(&(root, active))
                {
                    canvas = view;
                }
                if model.fullscreen().is_some() || changed_view && mode != WorkspaceLayout::Free {
                    canvas.center = bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                        bounds.size()[axis] * 0.5 + solution.scroll[axis]
                    }));
                    canvas.zoom = 1.0;
                    world
                        .resource_mut::<Runtime>()
                        .views
                        .insert((root, active), view);
                }
                if focused_view {
                    if let Some(mut current) = world.get_mut::<CanvasView>(root) {
                        if current.center != canvas.center || current.zoom != canvas.zoom {
                            *current = canvas;
                        }
                    }
                } else if let Some(space) = world
                    .get_mut::<Workspaces>(root)
                    .unwrap()
                    .entries
                    .iter_mut()
                    .find(|space| space.id == active)
                {
                    if space.center != canvas.center.to_array() || space.zoom != canvas.zoom {
                        space.center = canvas.center.to_array();
                        space.zoom = canvas.zoom;
                    }
                }
                outputs.push(OutputViewport {
                    id: output,
                    bounds,
                    usable: Some(usable),
                    physical_size: bounds.size().map(|size| size.round() as u32),
                    workspace: active,
                });
                for item in &solution.items {
                    let Some(entity) = entities.get(&item.id).copied() else {
                        continue;
                    };
                    let position = bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                        item.rectangle.origin()[axis]
                            + solution.scroll[axis]
                            + item.rectangle.size()[axis] * 0.5
                    }));
                    let size = bevy::math::DVec2::from_array(item.rectangle.size()).as_vec2();
                    let mut canvas_item = world.get_mut::<CanvasItem>(entity).unwrap();
                    if canvas_item.position != position || canvas_item.size != size {
                        canvas_item.position = position;
                        canvas_item.size = size;
                    }
                    world.resource_mut::<Runtime>().geometry.insert(
                        entity,
                        Geometry {
                            origin: (position - size.as_dvec2() * 0.5).to_array(),
                            size: size.as_dvec2().to_array(),
                        },
                    );
                    let managed = Managed {
                        id: item.id,
                        floating: (item.floating || mode == WorkspaceLayout::Free)
                            && model.fullscreen() != Some(item.id),
                    };
                    if world.get::<Managed>(entity) != Some(&managed) {
                        world.entity_mut(entity).insert(managed);
                    }
                    if !managed.floating {
                        world
                            .entity_mut(entity)
                            .remove::<crate::native::area_effects::AreaScale>();
                    }
                    let (id, surface) = if let Some(window) = world.get::<ApplicationWindow>(entity)
                    {
                        (
                            ItemId::Application(window.surface),
                            Rectangle::new([0.0; 2], window.content_size.map(f64::from)),
                        )
                    } else {
                        (
                            ItemId::Native(item.id.0),
                            Rectangle::new([0.0; 2], item.rectangle.size()),
                        )
                    };
                    if let Some(surface) = surface {
                        let view = canvas;
                        let scale = if managed.floating {
                            world
                                .get::<crate::native::area_effects::AreaScale>(entity)
                                .map_or(1.0, |scale| f64::from(scale.0))
                        } else {
                            1.0
                        };
                        let origin = std::array::from_fn(|axis| {
                            (position[axis]
                                - view.center[axis]
                                - f64::from(size[axis]) * scale * 0.5)
                                * view.zoom
                                + bounds.size()[axis] * 0.5
                        });
                        let Some(canvas) = Rectangle::new(
                            origin,
                            size.as_dvec2()
                                .to_array()
                                .map(|size| size * scale * view.zoom),
                        ) else {
                            continue;
                        };
                        geometries.push(ItemGeometry {
                            id,
                            output,
                            mapping: ContentMapping { canvas, surface },
                            floating: managed.floating,
                            clip: None,
                        });
                        if focused_view && solution.focused == Some(item.id) {
                            focused = Some(id);
                        }
                    }
                }
            }
            Err(_) => crate::native::notifications::report(
                world,
                "interface::layout",
                "The workspace layout is invalid.",
            ),
        }
        world
            .resource_mut::<Runtime>()
            .seen
            .insert((root, active), entities.keys().copied().collect());
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        if let Some(workspace) = spaces
            .entries
            .iter_mut()
            .find(|workspace| workspace.id == active)
        {
            workspace.desktop = model;
        }
    }
    let previous = &world.resource::<PublishedGeometry>().0;
    let unchanged = previous.as_ref().is_some_and(|previous| {
        previous.outputs == outputs && previous.items == geometries && previous.focused == focused
    });
    if !unchanged {
        let revision = previous
            .as_ref()
            .map_or(1, |previous| previous.revision.saturating_add(1));
        world.resource_mut::<PublishedGeometry>().0 = Some(GeometrySnapshot {
            revision,
            outputs,
            items: geometries,
            focused,
        });
    }
}

impl crate::native::actions::Action for DesktopAction {
    fn apply(&self, world: &mut World, target: Entity) {
        let Some(root) = root(world, target) else {
            return;
        };
        if *self == Self::OpenOperation {
            crate::native::operation::OpenOperation.apply(world, root);
            return;
        }
        if matches!(
            self,
            Self::FocusOutput(_)
                | Self::MoveToOutput(_)
                | Self::MoveWorkspaceToOutput(_)
                | Self::InspectWindowRecord
                | Self::OpenTerminal
                | Self::OpenLauncher
                | Self::Lock
                | Self::Logout
        ) {
            host_request(world, crate::host::HostRequest::Action(*self));
            return;
        }
        if let Self::Workspace(index) | Self::MoveToWorkspace(index) = *self {
            let Some(target_workspace) = index.checked_sub(1).and_then(|index| {
                world
                    .get::<Workspaces>(root)?
                    .entries
                    .get(index as usize)
                    .map(|entry| entry.id)
            }) else {
                return;
            };
            if matches!(self, Self::Workspace(_)) {
                crate::native::workspace::switch(world, root, target_workspace);
            } else if let Some(entity) = focused_entity(world, root) {
                world
                    .entity_mut(entity)
                    .insert(WorkspaceMember(target_workspace));
            }
            if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
                wake.ring();
            }
            return;
        }
        if *self == Self::Close {
            if let Some(entity) = focused_entity(world, root)
                && !request_close(world, entity)
            {
                crate::native::deletion::request(world, root, vec![entity]);
            }
            return;
        }
        let Some(bounds) = usable(world, root) else {
            return;
        };
        let output_bounds = viewport(world, root).unwrap_or(bounds);
        let Some(mut spaces) = world.get_mut::<Workspaces>(root) else {
            return;
        };
        let active = spaces.active;
        let Some(workspace) = spaces
            .entries
            .iter_mut()
            .find(|workspace| workspace.id == active)
        else {
            return;
        };
        let model = &mut workspace.desktop;
        match *self {
            Self::SetLayout(mode) => model.set_mode(mode),
            Self::Focus(direction) => {
                let _ = model.focus_direction(direction, bounds);
            }
            Self::Move(direction) => {
                model.move_focused(direction);
            }
            Self::Stack(direction) => {
                model.stack_focused(direction);
            }
            Self::ResizeColumn(delta) => {
                model.resize_column(f64::from(delta), bounds.size()[0]);
            }
            Self::ResizeItem(delta) => {
                model.resize_focused_height(delta as f32);
            }
            Self::CycleColumnWidth => {
                model.cycle_width();
            }
            Self::ToggleMaximized => {
                model.toggle_maximized();
            }
            Self::ToggleFullscreen => model.toggle_fullscreen(),
            Self::ToggleFloating => {
                if let Ok(solution) = model.solve(bounds, output_bounds)
                    && let Some(item) = solution
                        .items
                        .iter()
                        .find(|item| Some(item.id) == model.focused())
                {
                    let _ = model.toggle_floating(item.id, item.rectangle);
                }
            }
            _ => return,
        }
        if !matches!(self, Self::ToggleFullscreen)
            && let Some(id) = model.focused()
        {
            let _ = model.focus(id, bounds);
        }
        let resized = matches!(self, Self::ResizeItem(_))
            .then(|| {
                model.focused().and_then(|id| {
                    model
                        .items()
                        .iter()
                        .find(|item| item.id == id)
                        .map(|item| (id, item.rules))
                })
            })
            .flatten();
        let navigation = matches!(self, Self::Focus(_) | Self::Move(_) | Self::Stack(_));
        let free_focus = (navigation && model.mode() == WorkspaceLayout::Free)
            .then(|| {
                model.focused().and_then(|id| {
                    model
                        .items()
                        .iter()
                        .find(|item| item.id == id)
                        .map(|item| item.free)
                })
            })
            .flatten();
        drop(spaces);
        if !matches!(
            self,
            Self::Focus(_) | Self::ToggleFullscreen | Self::OpenOperation
        ) && let Some(mut runtime) = world.get_resource_mut::<Runtime>()
        {
            runtime.influences.remove(&(root, active));
        }
        if navigation && let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
            runtime.views.remove(&(root, active));
        }
        if let Some(geometry) = free_focus
            && let Some(mut view) = world.get_mut::<CanvasView>(root)
        {
            view.zoom = 1.0;
            for axis in 0..2 {
                let padding = 8.0_f64.min(bounds.size()[axis] / 4.0);
                let low = view.center[axis] - bounds.size()[axis] * 0.5 + padding;
                let high = view.center[axis] + bounds.size()[axis] * 0.5 - padding;
                let origin = geometry.origin[axis];
                let end = origin + geometry.size[axis];
                if origin < low {
                    view.center[axis] += origin - low;
                } else if end > high {
                    view.center[axis] += (end - high).min((origin - low).max(0.0));
                }
            }
        }
        if let Some((id, rules)) = resized {
            for mut layout in world.query::<&mut LayoutBox>().iter_mut(world) {
                if layout.id == id.0 {
                    layout.rules = rules;
                }
            }
        }
        if matches!(self, Self::SetLayout(_)) {
            crate::native::edit_mode::render_panel(world, root);
        }
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
            wake.ring();
        }
    }
}

fn focused_entity(world: &mut World, root: Entity) -> Option<Entity> {
    let spaces = world.get::<Workspaces>(root)?;
    let id = spaces
        .entries
        .iter()
        .find(|entry| entry.id == spaces.active)?
        .desktop
        .focused()?;
    world
        .query::<(Entity, &Managed, &ChildOf)>()
        .iter(world)
        .find(|(_, managed, parent)| managed.id == id && parent.parent() == root)
        .map(|(entity, _, _)| entity)
}

#[cfg(any(test, feature = "behavior-checks"))]
pub(crate) mod tests {
    use super::*;
    use crate::native::actions::Action;

    #[test]
    fn minimizing_a_parent_hides_dialogs_and_activating_a_dialog_restores_its_parent() {
        let (mut world, root, parent, dialog) = fixture();
        world.entity_mut(dialog).insert(TransientParent(parent));
        resolve(&mut world);
        focus_item(&mut world, parent);
        let parent_id = world.get::<Managed>(parent).unwrap().id;
        world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_minimized(parent_id, true);
        resolve(&mut world);
        assert!(world.get::<Suppressed>(parent).is_some());
        assert!(world.get::<Suppressed>(dialog).is_some());
        assert!(world.resource::<PublishedGeometry>().0.as_ref().unwrap().items.is_empty());
        focus_item(&mut world, dialog);
        resolve(&mut world);
        assert!(world.get::<Suppressed>(parent).is_none());
        assert!(world.get::<Suppressed>(dialog).is_none());
        assert!(!world.get::<Workspaces>(root).unwrap().entries[0].desktop.minimized(parent_id));
        assert!(world.resource::<PublishedGeometry>().0.as_ref().unwrap().items.iter().any(|item| item.id == ItemId::Application(SurfaceId(7))));
    }

    #[cfg_attr(test, test)]
    fn dialog_stays_above_its_parent_and_preserves_parent_fullscreen() {
        let (mut world, root, parent, dialog) = fixture();
        world.entity_mut(dialog).insert(TransientParent(parent));
        resolve(&mut world);
        assert!(world.get::<Managed>(dialog).unwrap().floating);
        let parent_position = world.get::<CanvasItem>(parent).unwrap().position;
        let dialog_position = world.get::<CanvasItem>(dialog).unwrap().position;
        assert!((parent_position.x - dialog_position.x).abs() < 0.01);
        focus_item(&mut world, parent);
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        let parent_id = world.get::<Managed>(parent).unwrap().id;
        focus_item(&mut world, dialog);
        resolve(&mut world);
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .fullscreen(),
            Some(parent_id)
        );
        assert!(world.get::<Suppressed>(dialog).is_none());
        let geometry = world.resource::<PublishedGeometry>().0.as_ref().unwrap();
        assert!(
            geometry
                .items
                .iter()
                .any(|item| item.id == ItemId::Application(SurfaceId(7)))
        );
        world.despawn(parent);
        resolve(&mut world);
        assert!(world.get::<TransientParent>(dialog).is_none());
        assert!(world.get_entity(dialog).is_ok());
    }

    #[test]
    fn two_outputs_resolve_one_canonical_root_without_reparenting_items() {
        let (mut world, root, native, application) = fixture();
        let mut other = world.get::<Workspaces>(root).unwrap().entries[0].clone();
        other.id = 2;
        world
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .push(other);
        world.entity_mut(application).insert(WorkspaceMember(2));
        let viewport = |id, workspace, width| {
            let bounds = Rectangle::new([0.0; 2], [width, 600.0]).unwrap();
            OutputViewport {
                id: OutputId(id),
                bounds,
                usable: Some(bounds),
                physical_size: [width as u32, 600],
                workspace,
            }
        };
        world.insert_resource(
            crate::native::output_views::OutputViews::new(
                vec![viewport(1, 1, 800.0), viewport(2, 2, 1200.0)],
                OutputId(1),
            )
            .unwrap(),
        );
        resolve(&mut world);
        let snapshot = world.resource::<PublishedGeometry>().0.as_ref().unwrap();
        assert_eq!(snapshot.outputs.len(), 2);
        assert_eq!(
            snapshot
                .items
                .iter()
                .find(|item| item.id == ItemId::Application(SurfaceId(7)))
                .unwrap()
                .output,
            OutputId(2)
        );
        let identity = world.get::<LayoutBox>(native).unwrap().id;
        let first_position = world.get::<CanvasItem>(native).unwrap().position;
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
        world
            .resource_mut::<crate::native::output_views::OutputViews>()
            .focused = OutputId(2);
        resolve(&mut world);
        assert_eq!(world.get::<ChildOf>(native).unwrap().parent(), root);
        assert_eq!(world.get::<ChildOf>(application).unwrap().parent(), root);
        assert_eq!(world.get::<LayoutBox>(native).unwrap().id, identity);
        assert_eq!(
            world.get::<CanvasItem>(native).unwrap().position,
            first_position
        );
        assert_eq!(
            world
                .resource::<PublishedGeometry>()
                .0
                .as_ref()
                .unwrap()
                .outputs
                .len(),
            2
        );
    }

    #[test]
    fn close_requests_the_application_and_keeps_its_live_castle() {
        let (mut world, root, _, application) = fixture();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        world.insert_resource(HostRequests(sender));
        resolve(&mut world);
        focus_item(&mut world, application);
        DesktopAction::Close.apply(&mut world, root);
        assert_eq!(
            receiver.try_recv().unwrap(),
            crate::host::HostRequest::Close(SurfaceId(7))
        );
        assert!(world.get_entity(application).is_ok());
        assert!(request_close(&world, application));
        assert_eq!(
            receiver.try_recv().unwrap(),
            crate::host::HostRequest::Close(SurfaceId(7))
        );
        assert!(world.get_entity(application).is_ok());
    }

    #[test]
    fn bars_limit_tiling_and_maximize_while_fullscreen_uses_the_output() {
        let (mut world, root, _, application) = fixture();
        let bounds = Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
        let usable = Rectangle::new([0.0, 32.0], [1000.0, 640.0]).unwrap();
        world.entity_mut(root).insert(HostViewport(OutputViewport {
            id: OutputId(1),
            bounds,
            usable: Some(usable),
            physical_size: [1000, 700],
            workspace: 1,
        }));
        resolve(&mut world);
        focus_item(&mut world, application);
        DesktopAction::ToggleMaximized.apply(&mut world, root);
        resolve(&mut world);
        let item = world
            .resource::<PublishedGeometry>()
            .0
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(SurfaceId(7)))
            .unwrap();
        assert_eq!(item.mapping.canvas.origin(), [8.0, 40.0]);
        assert_eq!(item.mapping.canvas.size(), [984.0, 624.0]);
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        let item = world
            .resource::<PublishedGeometry>()
            .0
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(SurfaceId(7)))
            .unwrap();
        assert_eq!(item.mapping.canvas, bounds);
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        let output = &world
            .resource::<PublishedGeometry>()
            .0
            .as_ref()
            .unwrap()
            .outputs[0];
        assert_eq!(output.bounds, bounds);
        assert_eq!(output.usable, Some(usable));
    }

    fn fixture() -> (World, Entity, Entity, Entity) {
        let mut world = World::new();
        let mut spaces = Workspaces::default();
        spaces.entries[0]
            .desktop
            .set_mode(WorkspaceLayout::ScrollingColumns);
        let root = world
            .spawn((
                spaces,
                CanvasView::default(),
                ComputedNode {
                    size: Vec2::new(1000.0, 700.0),
                    inverse_scale_factor: 1.0,
                    ..default()
                },
            ))
            .id();
        let mut spawn = || {
            world
                .spawn((
                    CanvasItem {
                        position: bevy::math::DVec2::ZERO,
                        size: Vec2::new(300.0, 200.0),
                    },
                    LayoutBox::new(Vec2::new(300.0, 200.0)),
                    WorkspaceMember(1),
                    ChildOf(root),
                ))
                .id()
        };
        let native = spawn();
        let application = spawn();
        world.entity_mut(application).insert(ApplicationWindow {
            surface: SurfaceId(7),
            content_size: [320, 240],
        });
        (world, root, native, application)
    }

    #[test]
    fn native_and_application_items_share_geometry_and_input_mapping() {
        let (mut world, root, native, application) = fixture();
        super::super::engine::resolve(&mut world);
        assert!(tiled(&world, native));
        assert!(tiled(&world, application));
        assert_eq!(
            world.get::<CanvasItem>(native).unwrap().size,
            world.get::<CanvasItem>(application).unwrap().size
        );
        let snapshot = world.resource::<PublishedGeometry>().0.as_ref().unwrap();
        assert_eq!(snapshot.items.len(), 2);
        let application = snapshot
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(SurfaceId(7)))
            .unwrap();
        let point = std::array::from_fn(|axis| {
            application.mapping.canvas.origin()[axis]
                + application.mapping.canvas.size()[axis] * 0.5
        });
        assert_eq!(
            application.mapping.surface_point(point),
            Some([160.0, 120.0])
        );
        focus_item(&mut world, native);
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .focused(),
            Some(world.get::<Managed>(native).unwrap().id)
        );
    }

    #[test]
    fn floating_does_not_accumulate_scroll_and_closing_removes_the_tile() {
        let (mut world, root, native, application) = fixture();
        resolve(&mut world);
        focus_item(&mut world, application);
        DesktopAction::ToggleFloating.apply(&mut world, root);
        resolve(&mut world);
        let before = *world.get::<CanvasItem>(application).unwrap();
        for _ in 0..12 {
            resolve(&mut world);
        }
        let after = world.get::<CanvasItem>(application).unwrap();
        assert_eq!(before.position, after.position);
        assert_eq!(before.size, after.size);
        assert!(!tiled(&world, application));
        world.despawn(application);
        resolve(&mut world);
        assert_eq!(
            world
                .resource::<PublishedGeometry>()
                .0
                .as_ref()
                .unwrap()
                .items
                .len(),
            1
        );
        assert_eq!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .items()
                .len(),
            1
        );
        assert!(tiled(&world, native));
    }

    #[test]
    fn restarting_keeps_native_layout_and_discards_closed_application_tiles() {
        let (mut world, root, native, _) = fixture();
        resolve(&mut world);
        let saved = serde_json::to_vec(&world.get::<Workspaces>(root).unwrap().entries[0]).unwrap();
        let native_layout = *world.get::<LayoutBox>(native).unwrap();
        let restored: crate::native::workspace::Workspace = serde_json::from_slice(&saved).unwrap();
        let (mut next, root, native, application) = fixture();
        next.despawn(application);
        next.entity_mut(native).insert(native_layout);
        next.get_mut::<Workspaces>(root).unwrap().entries[0] = restored;
        resolve(&mut next);
        let model = &next.get::<Workspaces>(root).unwrap().entries[0].desktop;
        assert!(model.valid());
        assert_eq!(model.items().len(), 1);
        assert_eq!(model.items()[0].id, PlacementId(native_layout.id));
    }

    #[test]
    fn overview_changes_mapping_without_resizing_client_content() {
        let (mut world, root, _, application) = fixture();
        resolve(&mut world);
        let before = world.get::<CanvasItem>(application).unwrap().size;
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        resolve(&mut world);
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size, before);
        let geometry = world
            .resource::<PublishedGeometry>()
            .0
            .as_ref()
            .unwrap()
            .items
            .iter()
            .find(|item| item.id == ItemId::Application(SurfaceId(7)))
            .unwrap();
        assert_eq!(
            geometry.mapping.canvas.size(),
            before.as_dvec2().to_array().map(|size| size * 0.5)
        );
        let center = std::array::from_fn(|axis| {
            geometry.mapping.canvas.origin()[axis] + geometry.mapping.canvas.size()[axis] * 0.5
        });
        assert_eq!(geometry.mapping.surface_point(center), Some([160.0, 120.0]));
    }

    #[test]
    fn fullscreen_suppresses_other_canvas_content_and_restores_overview() {
        let (mut world, root, native, application) = fixture();
        let area = world
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::splat(100.0),
                },
                crate::native::area::InfluenceArea::new(
                    crate::native::area::AreaShape::Circle,
                    bevy::math::DVec2::ZERO,
                    bevy::math::DVec2::splat(100.0),
                ),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        resolve(&mut world);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        super::super::engine::resolve(&mut world);
        assert!(world.get::<Suppressed>(native).is_some());
        assert!(world.get::<Suppressed>(area).is_some());
        assert!(world.get::<Suppressed>(application).is_none());
        assert_eq!(
            world.get::<CanvasItem>(application).unwrap().size,
            Vec2::new(1000.0, 700.0)
        );
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.0);
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        assert!(world.get::<Suppressed>(native).is_none());
        assert!(world.get::<Suppressed>(area).is_none());
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.5);
    }

    #[test]
    fn area_size_changes_layout_once_and_restores_without_scaling_twice() {
        let (mut world, root, native, application) = fixture();
        let mut area = crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            bevy::math::DVec2::new(500.0, 350.0),
            bevy::math::DVec2::splat(4000.0),
        );
        area.scale = 1.5;
        let area = world.spawn((area, WorkspaceMember(1), ChildOf(root))).id();
        resolve(&mut world);
        assert_eq!(world.get::<CanvasItem>(native).unwrap().size.x, 738.0);
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 738.0);
        assert!(
            world
                .get::<crate::native::area_effects::AreaScale>(application)
                .is_none()
        );
        let revision = world
            .resource::<PublishedGeometry>()
            .0
            .as_ref()
            .unwrap()
            .revision;
        for _ in 0..12 {
            resolve(&mut world);
        }
        assert_eq!(
            world
                .resource::<PublishedGeometry>()
                .0
                .as_ref()
                .unwrap()
                .revision,
            revision
        );
        world.increment_change_tick();
        world
            .get_mut::<crate::native::area::InfluenceArea>(area)
            .unwrap()
            .center = [10000.0; 2];
        resolve(&mut world);
        assert_eq!(world.get::<CanvasItem>(native).unwrap().size.x, 492.0);
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 492.0);
    }
    crate::native::laboratory_cases! { dialog_stays_above_its_parent_and_preserves_parent_fullscreen, }
}

pub(crate) fn controls(world: &mut World, root: Entity, panel: Entity) {
    let Some(spaces) = world.get::<Workspaces>(root) else {
        return;
    };
    let Some(workspace) = spaces
        .entries
        .iter()
        .find(|workspace| workspace.id == spaces.active)
    else {
        return;
    };
    let active = workspace.desktop.mode();
    crate::native::edit_mode::label(world, panel, "Workspace layout", 18.0);
    for (mode, name) in [
        (WorkspaceLayout::Free, "Free"),
        (WorkspaceLayout::Row, "Row"),
        (WorkspaceLayout::Column, "Column"),
        (WorkspaceLayout::Grid, "Grid"),
        (WorkspaceLayout::ScrollingColumns, "Scrolling columns"),
    ] {
        let name = if mode == active {
            format!("{name} (selected)")
        } else {
            name.into()
        };
        crate::native::sand_panel::button(
            world,
            panel,
            root,
            &name,
            DesktopAction::SetLayout(mode),
        );
    }
}
