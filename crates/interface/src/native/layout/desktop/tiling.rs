use super::*;

pub(super) struct InitialFocus {
    focused: Option<PlacementId>,
    selected: Option<Entity>,
    center: [f64; 2],
}

pub(super) fn shift_camera(
    world: &mut World,
    root: Entity,
    workspace: u64,
    before: bevy::math::DVec2,
    after: bevy::math::DVec2,
) {
    let delta = after - before;
    if delta == bevy::math::DVec2::ZERO || !delta.is_finite() {
        return;
    }
    if world
        .get::<Workspaces>(root)
        .is_some_and(|spaces| spaces.active == workspace)
    {
        if let Some(mut view) = world.get_mut::<crate::native::topology::view::View>(root) {
            view.position[0] += delta.x;
            view.position[2] += delta.y;
        }
    } else if let Some(mut spaces) = world.get_mut::<Workspaces>(root)
        && let Some(space) = spaces
            .entries
            .iter_mut()
            .find(|space| space.id == workspace)
    {
        space.topology.position[0] += delta.x;
        space.topology.position[2] += delta.y;
    }
}

pub(super) fn transition(world: &mut World, root: Entity, action: DesktopAction) -> bool {
    let Some((active, mode, focused, saved, normalized)) =
        world.get::<Workspaces>(root).and_then(|spaces| {
            let workspace = spaces
                .entries
                .iter()
                .find(|space| space.id == spaces.active)?;
            Some((
                spaces.active,
                workspace.desktop.mode(),
                workspace.desktop.focused(),
                workspace.desktop.free_view(),
                workspace.desktop.fullscreen().is_some()
                    || workspace.desktop.items().iter().any(|item| {
                        workspace.desktop.maximized(item.id)
                            && !workspace.desktop.minimized(item.id)
                    }),
            ))
        })
    else {
        return false;
    };
    let next = match action {
        DesktopAction::ToggleTiling => {
            if mode == WorkspaceLayout::Free {
                WorkspaceLayout::Tiled
            } else {
                WorkspaceLayout::Free
            }
        }
        DesktopAction::SetLayout(WorkspaceLayout::Tiled) => WorkspaceLayout::Tiled,
        DesktopAction::SetLayout(WorkspaceLayout::Free) if mode == WorkspaceLayout::Tiled => {
            WorkspaceLayout::Free
        }
        _ => return false,
    };
    if next == mode {
        return true;
    }
    super::focus::clear_mode(world, root);
    let canvas = crate::native::output_views::canvas(world, root, active);
    let camera_canvas = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.fullscreen_views.get(&(root, active)))
        .map_or(canvas, |saved| saved.canvas);
    let free_canvas = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.fullscreen_views.get(&(root, active)))
        .filter(|saved| saved.mode == WorkspaceLayout::Free)
        .map_or(canvas, |saved| saved.canvas);
    let latest: Vec<_> = if mode == WorkspaceLayout::Free {
        world
            .query::<(&CanvasItem, &Managed, &WorkspaceMember, &ChildOf)>()
            .iter(world)
            .filter(|(_, _, member, parent)| member.0 == active && parent.parent() == root)
            .map(|(item, managed, _, _)| {
                (
                    managed.id,
                    Geometry {
                        origin: (item.position - item.size.as_dvec2() * 0.5).to_array(),
                        size: item.size.as_dvec2().to_array(),
                    },
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let selected = crate::native::canvas_selection::selected(world, root)
        .into_iter()
        .find(|entity| {
            candidate(world, *entity)
                && world
                    .get::<WorkspaceMember>(*entity)
                    .is_some_and(|member| member.0 == active)
        });
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        let workspace = spaces
            .entries
            .iter_mut()
            .find(|space| space.id == active)
            .unwrap();
        if mode == WorkspaceLayout::Free {
            if workspace.desktop.fullscreen().is_none() {
                for (id, geometry) in latest {
                    if !workspace.desktop.maximized(id) {
                        let _ = workspace.desktop.set_free_geometry(id, geometry);
                    }
                }
            }
            let _ = workspace
                .desktop
                .remember_free_view(free_canvas.center.to_array(), free_canvas.zoom);
        }
        workspace.desktop.set_mode(next);
    }
    world.init_resource::<Runtime>();
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.views.remove(&(root, active));
    runtime.influences.remove(&(root, active));
    if next == WorkspaceLayout::Tiled {
        runtime.entering.insert(
            (root, active),
            InitialFocus {
                focused,
                selected,
                center: canvas.center.to_array(),
            },
        );
    } else {
        runtime.entering.remove(&(root, active));
        runtime.restoring_free.insert((root, active));
    }
    drop(runtime);
    if next == WorkspaceLayout::Free
        && let Some((center, zoom)) = saved
    {
        let mut restored = CanvasView {
            center: bevy::math::DVec2::from_array(center),
            ..canvas
        };
        restored.set_zoom(zoom);
        shift_camera(world, root, active, camera_canvas.center, restored.center);
        if normalized {
            let size = viewport(world, root).map_or([0.0; 2], |bounds| bounds.size());
            world.resource_mut::<Runtime>().fullscreen_views.insert(
                (root, active),
                FullscreenView {
                    mode: WorkspaceLayout::Free,
                    canvas: restored,
                    scroll: [0.0; 2],
                    size,
                },
            );
        } else {
            world
                .resource_mut::<Runtime>()
                .fullscreen_views
                .remove(&(root, active));
        }
        if let Some(mut view) = world.get_mut::<CanvasView>(root) {
            *view = restored;
        }
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    true
}

pub(super) fn resolve_focus(
    world: &mut World,
    root: Entity,
    active: u64,
    model: &mut crate::layout::desktop::Workspace,
    entities: &BTreeMap<PlacementId, Entity>,
    usable: Rectangle,
) {
    let Some(initial) = world
        .resource_mut::<Runtime>()
        .entering
        .remove(&(root, active))
    else {
        return;
    };
    if model.mode() != WorkspaceLayout::Tiled {
        return;
    }
    let eligible =
        |id: PlacementId| model.items().iter().any(|item| item.id == id) && !model.minimized(id) && !model.compact_hidden(id);
    let selected = initial
        .selected
        .and_then(|selected| {
            entities
                .iter()
                .find(|(_, entity)| **entity == selected)
                .map(|(id, _)| *id)
        })
        .map(|id| model.compact_group(id).map_or(id, |group| group.active))
        .filter(|id| eligible(*id));
    let target = initial
        .focused
        .map(|id| model.compact_group(id).map_or(id, |group| group.active))
        .filter(|id| eligible(*id))
        .or(selected)
        .or_else(|| {
            model
                .items()
                .iter()
                .filter(|item| !model.minimized(item.id) && !model.compact_hidden(item.id))
                .map(|item| {
                    let distance: f64 = (0..2)
                        .map(|axis| {
                            (item.free.origin[axis] + item.free.size[axis] * 0.5
                                - initial.center[axis])
                                .powi(2)
                        })
                        .sum();
                    (item.id, distance)
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
                .map(|(id, _)| id)
        });
    if let Some(id) = target {
        let _ = model.focus(id, usable);
    }
}

#[cfg(test)]
mod tests;
