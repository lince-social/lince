use super::*;

pub(super) fn apply(
    world: &mut World,
    root: Entity,
    action: DesktopAction,
    bounds: Rectangle,
) -> bool {
    if !matches!(
        action,
        DesktopAction::CycleItemWidth | DesktopAction::CycleRowHeight | DesktopAction::Resize(_)
    ) {
        return false;
    }
    if let Some(reason) = super::CommandContext::new(world, root).unavailable(action) {
        crate::native::notifications::report(world, "Window layout", reason);
        if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
            wake.ring();
        }
        return true;
    }
    let settings = world
        .get_resource::<crate::native::shortcuts::configuration::LayoutSettings>()
        .cloned()
        .unwrap_or_default();
    let active = world.get::<Workspaces>(root).map(|spaces| spaces.active);
    let scale = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.fullscreen_views.get(&(root, active?)))
        .map(|saved| saved.canvas.zoom)
        .unwrap_or_else(|| world.get::<CanvasView>(root).map_or(1.0, |view| view.zoom));
    let result = world.get_mut::<Workspaces>(root).and_then(|mut spaces| {
        let active = spaces.active;
        let workspace = spaces.entries.iter_mut().find(|space| space.id == active)?;
        if workspace.desktop.fullscreen().is_some() {
            return Some(Err("Leave fullscreen before changing the window width."));
        }
        if workspace.desktop.focused().is_none() {
            return Some(Err("Focus an item before changing its width."));
        }
        let result = match action {
            DesktopAction::CycleItemWidth => workspace.desktop.cycle_item_width(&settings.width_presets, bounds, scale),
            DesktopAction::CycleRowHeight => workspace.desktop.cycle_row_height(&settings.height_presets),
            DesktopAction::Resize(direction) => {
                use crate::host::Direction;
                let axis = usize::from(matches!(direction, Direction::Up | Direction::Down));
                let sign = if matches!(direction, Direction::Left | Direction::Up) { -1.0 } else { 1.0 };
                workspace.desktop.resize_focused(axis, sign * f64::from(settings.resize_step), bounds, scale)
            }
            _ => unreachable!(),
        }.map_err(|_| "Size commands require a rectangular item in a free or tiled workspace; row-height presets require a tiled row.");
        let tiled = workspace.desktop.mode() == WorkspaceLayout::Tiled && workspace.desktop.focused().is_some_and(|id| workspace.desktop.items().iter().any(|item| item.id == id && !item.floating));
        if result.is_ok() && tiled && let Some(id) = workspace.desktop.focused() { let _ = workspace.desktop.focus(id, bounds); }
        let rules = (!tiled && matches!(action, DesktopAction::Resize(_)))
            .then(|| workspace.desktop.items().iter().find(|item| Some(item.id) == workspace.desktop.focused()).map(|item| (item.id, item.rules)))
            .flatten();
        Some(result.map(|_| (tiled, rules)))
    });
    if let Some(Err(error)) = result {
        crate::native::notifications::report(world, "Window layout", error);
    }
    if let Some(Ok((_, Some((id, rules))))) = result {
        let entities: Vec<_> = world
            .get::<Children>(root)
            .map(|children| children.iter().collect())
            .unwrap_or_default();
        for entity in entities {
            if world
                .get::<Managed>(entity)
                .is_some_and(|managed| managed.id == id)
                && let Some(mut layout) = world.get_mut::<LayoutBox>(entity)
                && let DesktopAction::Resize(direction) = action
            {
                let axis = usize::from(matches!(
                    direction,
                    crate::host::Direction::Up | crate::host::Direction::Down
                ));
                layout.rules.axes[axis] = rules.axes[axis];
            }
        }
    }
    if let Some(Ok((tiled, _))) = result
        && let Some(active) = active
        && let Some(mut runtime) = world.get_resource_mut::<Runtime>()
    {
        if tiled {
            runtime.views.remove(&(root, active));
        }
        runtime.influences.remove(&(root, active));
    }
    if let Some(wake) = world.get_resource::<crate::host::WakeSignal>() {
        wake.ring();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{layout::desktop::Width, native::actions::Action};

    #[test]
    fn native_resize_commands_survive_layout_and_preserve_other_axis_and_neighbor() {
        let (mut world, root, native, application) = super::super::tests::fixture();
        world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        super::super::super::engine::resolve(&mut world);
        focus_item(&mut world, native);
        DesktopAction::CycleItemWidth.apply(&mut world, root);
        super::super::super::engine::resolve(&mut world);
        let before = *world.get::<CanvasItem>(native).unwrap();
        let neighbor = *world.get::<CanvasItem>(application).unwrap();
        DesktopAction::Resize(crate::host::Direction::Down).apply(&mut world, root);
        for _ in 0..4 {
            super::super::super::engine::resolve(&mut world);
        }
        let item = world.get::<CanvasItem>(native).unwrap();
        assert_eq!(item.size, before.size + Vec2::new(0.0, 128.0));
        assert_eq!(item.position, before.position);
        let id = world.get::<Managed>(native).unwrap().id;
        let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
        assert!(
            model
                .items()
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .free_width
                .is_some()
        );
        DesktopAction::Resize(crate::host::Direction::Left).apply(&mut world, root);
        for _ in 0..4 {
            super::super::super::engine::resolve(&mut world);
        }
        let item = world.get::<CanvasItem>(native).unwrap();
        assert_eq!(item.size, before.size + Vec2::new(-128.0, 128.0));
        assert_eq!(item.position, before.position);
        assert_eq!(
            world.get::<CanvasItem>(application).unwrap().size,
            neighbor.size
        );
        assert_eq!(
            world.get::<CanvasItem>(application).unwrap().position,
            neighbor.position
        );
        DesktopAction::Unfocus.apply(&mut world, root);
        let before =
            serde_json::to_vec(&world.get::<Workspaces>(root).unwrap().entries[0].desktop).unwrap();
        DesktopAction::Resize(crate::host::Direction::Right).apply(&mut world, root);
        DesktopAction::CycleRowHeight.apply(&mut world, root);
        assert_eq!(
            serde_json::to_vec(&world.get::<Workspaces>(root).unwrap().entries[0].desktop).unwrap(),
            before
        );
    }

    #[test]
    fn floating_resize_does_not_replace_the_authored_rectangle_after_retile_and_untile() {
        let (mut world, root, native, _) = super::super::tests::fixture();
        world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        super::super::super::engine::resolve(&mut world);
        let before = *world.get::<CanvasItem>(native).unwrap();
        focus_item(&mut world, native);
        DesktopAction::ToggleTiling.apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        DesktopAction::ToggleFloating.apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        DesktopAction::Resize(crate::host::Direction::Down).apply(&mut world, root);
        DesktopAction::Resize(crate::host::Direction::Right).apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        assert_ne!(world.get::<CanvasItem>(native).unwrap().size, before.size);
        DesktopAction::ToggleFloating.apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        DesktopAction::ToggleTiling.apply(&mut world, root);
        for _ in 0..4 {
            super::super::super::engine::resolve(&mut world);
        }
        assert_eq!(world.get::<CanvasItem>(native).unwrap().size, before.size);
        assert_eq!(
            world.get::<CanvasItem>(native).unwrap().position,
            before.position
        );
    }

    #[test]
    fn width_after_explicit_unfocus_preserves_existing_items_and_camera() {
        let (mut world, root, native, application) = super::super::tests::fixture();
        world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        super::super::super::engine::resolve(&mut world);
        focus_item(&mut world, native);
        DesktopAction::Unfocus.apply(&mut world, root);
        let before: Vec<_> = [native, application]
            .into_iter()
            .map(|entity| {
                let item = *world.get::<CanvasItem>(entity).unwrap();
                let id = world.get::<Managed>(entity).unwrap().id;
                let width = world.get::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .item_width(id);
                (entity, item, id, width)
            })
            .collect();
        let view = *world.get::<CanvasView>(root).unwrap();
        assert!(
            super::super::CommandContext::new(&world, root)
                .unavailable(DesktopAction::CycleItemWidth)
                .is_some()
        );
        DesktopAction::CycleItemWidth.apply(&mut world, root);
        assert!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .focused()
                .is_none()
        );
        assert_eq!(world.get::<CanvasView>(root).unwrap().center, view.center);
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, view.zoom);
        for (entity, before, id, width) in before {
            let item = world.get::<CanvasItem>(entity).unwrap();
            assert_eq!(item.position, before.position);
            assert_eq!(item.size, before.size);
            assert_eq!(
                world.get::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .item_width(id),
                width
            );
        }
    }

    #[test]
    fn opening_uses_the_configured_width_without_resizing_existing_items() {
        let (mut world, root, _, _) = super::super::tests::fixture();
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Tiled);
        world.insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
            default_width: [2, 3],
            ..default()
        });
        super::super::super::engine::resolve(&mut world);
        assert!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .items()
                .iter()
                .all(|item| world.get::<Workspaces>(root).unwrap().entries[0]
                    .desktop
                    .item_width(item.id)
                    == Some(Width::Fraction(2, 3)))
        );
        world
            .resource_mut::<crate::native::shortcuts::configuration::LayoutSettings>()
            .default_width = [1, 3];
        let new = world
            .spawn((
                CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::new(300.0, 200.0),
                },
                LayoutBox::new(Vec2::new(300.0, 200.0)),
                WorkspaceMember(1),
                ChildOf(root),
            ))
            .id();
        super::super::super::engine::resolve(&mut world);
        let id = world.get::<Managed>(new).unwrap().id;
        let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
        for item in model.items() {
            assert_eq!(
                model.item_width(item.id),
                Some(if item.id == id {
                    Width::Fraction(1, 3)
                } else {
                    Width::Fraction(2, 3)
                })
            );
        }
    }

    #[test]
    fn free_width_survives_native_layout_output_resize_and_maximize_restoration() {
        let (mut world, root, _, application) = super::super::tests::fixture();
        world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        world.get_mut::<CanvasView>(root).unwrap().zoom = 0.5;
        super::super::super::engine::resolve(&mut world);
        focus_item(&mut world, application);
        DesktopAction::CycleItemWidth.apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 1968.0);
        world.get_mut::<ComputedNode>(root).unwrap().size.x = 800.0;
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 1568.0);
        DesktopAction::ToggleMaximized.apply(&mut world, root);
        super::super::super::engine::resolve(&mut world);
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 1.0);
        DesktopAction::CycleItemWidth.apply(&mut world, root);
        for _ in 0..3 {
            super::super::super::engine::resolve(&mut world);
        }
        assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 776.0);
        assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, 0.5);
    }

    #[test]
    fn position_only_observations_keep_a_pending_width_but_manual_resizing_replaces_it() {
        let (mut world, root, native, _) = super::super::tests::fixture();
        world.insert_resource(DefaultLayout(WorkspaceLayout::Free));
        world.get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(WorkspaceLayout::Free);
        super::super::super::engine::resolve(&mut world);
        focus_item(&mut world, native);
        DesktopAction::CycleItemWidth.apply(&mut world, root);
        let moved = bevy::math::DVec2::new(200.0, -40.0);
        world.get_mut::<CanvasItem>(native).unwrap().position = moved;
        super::super::super::engine::resolve(&mut world);
        let item = world.get::<CanvasItem>(native).unwrap();
        assert_eq!(item.position, moved);
        assert_eq!(item.size.x, 984.0);
        let id = world.get::<Managed>(native).unwrap().id;
        assert!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .items()
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .free_width
                .is_some()
        );
        world.get_mut::<CanvasItem>(native).unwrap().size.x = 500.0;
        super::super::super::engine::resolve(&mut world);
        assert!(
            world.get::<Workspaces>(root).unwrap().entries[0]
                .desktop
                .items()
                .iter()
                .find(|item| item.id == id)
                .unwrap()
                .free_width
                .is_none()
        );
    }
}
