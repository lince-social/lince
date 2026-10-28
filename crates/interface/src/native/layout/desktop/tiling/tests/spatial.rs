use super::*;
use crate::{host::Direction, layout::desktop::Width, native::topology::view::View};
use bevy::math::DVec2;

fn tiled(spatial: bool) -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = fixture();
    world.entity_mut(root).insert((
        View {
            spatial,
            position: [1420.0, 900.0, 250.0],
            yaw: 0.125,
            pitch: -0.75,
            ..default()
        },
        crate::native::canvas_selection::SandSelection(vec![native]),
    ));
    DesktopAction::ToggleTiling.apply(&mut world, root);
    resolve(&mut world);
    let ids = [native, application].map(|entity| world.get::<Managed>(entity).unwrap().id);
    let model = &mut world.get_mut::<Workspaces>(root).unwrap().entries[0].desktop;
    for id in ids {
        model.set_item_width(id, Width::Full).unwrap();
    }
    super::super::super::focus_item(&mut world, native);
    resolve(&mut world);
    (world, root, native, application)
}

fn step(world: &mut World, root: Entity, action: DesktopAction) -> DVec2 {
    let canvas = *world.get::<CanvasView>(root).unwrap();
    let view = *world.get::<View>(root).unwrap();
    action.apply(world, root);
    resolve(world);
    let next = *world.get::<CanvasView>(root).unwrap();
    let current = *world.get::<View>(root).unwrap();
    let delta = next.center - canvas.center;
    assert!((current.position[0] - view.position[0] - delta.x).abs() < 0.000_001);
    assert!((current.position[2] - view.position[2] - delta.y).abs() < 0.000_001);
    assert_eq!(current.position[1], view.position[1]);
    assert_eq!(current.yaw, view.yaw);
    assert_eq!(current.pitch, view.pitch);
    assert_eq!(current.plane, view.plane);
    assert_eq!(current.selection_depth, view.selection_depth);
    for _ in 0..4 {
        resolve(world);
        assert_eq!(*world.get::<View>(root).unwrap(), current);
        assert_eq!(world.get::<CanvasView>(root).unwrap().center, next.center);
    }
    delta
}

#[test]
fn tiled_focus_tracks_horizontal_and_vertical_scroll_in_both_projections_without_drift() {
    for spatial in [false, true] {
        let (mut world, root, native, application) = tiled(spatial);
        let sizes =
            [native, application].map(|entity| world.get::<CanvasItem>(entity).unwrap().size);
        let horizontal = step(&mut world, root, DesktopAction::Focus(Direction::Right));
        assert!(horizontal.x > 100.0);
        for (entity, size) in [native, application].into_iter().zip(sizes) {
            assert_eq!(world.get::<CanvasItem>(entity).unwrap().size, size);
        }
        let vertical = step(&mut world, root, DesktopAction::Move(Direction::Down));
        assert!(vertical.y > 100.0);
        assert!(step(&mut world, root, DesktopAction::Focus(Direction::Up)).y < -100.0);
        assert!(step(&mut world, root, DesktopAction::Focus(Direction::Down)).y > 100.0);
        let current = *world.get::<View>(root).unwrap();
        world.get_mut::<View>(root).unwrap().spatial = true;
        resolve(&mut world);
        assert_eq!(world.get::<View>(root).unwrap().position, current.position);
        step(&mut world, root, DesktopAction::Unfocus);
        step(&mut world, root, DesktopAction::CycleItemWidth);
    }
}

#[test]
fn fullscreen_does_not_move_the_spatial_camera_and_exit_applies_only_new_tiled_scroll() {
    for resized in [false, true] {
        let (mut world, root, _, _) = tiled(true);
        step(&mut world, root, DesktopAction::Focus(Direction::Right));
        let view = *world.get::<View>(root).unwrap();
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        assert_eq!(*world.get::<View>(root).unwrap(), view);
        if resized {
            let bounds = Rectangle::new([0.0; 2], [820.0, 600.0]).unwrap();
            world.get_mut::<ComputedNode>(root).unwrap().size = Vec2::new(820.0, 600.0);
            world.entity_mut(root).insert(HostViewport(OutputViewport { rotation: Default::default(),
                id: OutputId(1),
                bounds,
                usable: Rectangle::new([24.0, 32.0], [796.0, 568.0]),
                physical_size: [820, 600],
                raster: crate::host::raster::RasterMapping {
                    origin: bounds.origin(),
                    scale: [1.0; 2],
                },
                workspace: 1,
            }));
            resolve(&mut world);
            assert_eq!(*world.get::<View>(root).unwrap(), view);
        }
        let saved = world.resource::<Runtime>().fullscreen_views[&(root, 1)].canvas;
        DesktopAction::ToggleFullscreen.apply(&mut world, root);
        resolve(&mut world);
        let scroll = world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .scroll();
        let bounds = super::super::super::viewport(&world, root).unwrap();
        let expected = DVec2::from_array(std::array::from_fn(|axis| {
            bounds.size()[axis] * 0.5 + scroll[axis]
        }));
        let center = world.get::<CanvasView>(root).unwrap().center;
        assert_eq!(center, expected);
        let delta = center - saved.center;
        if resized {
            assert!(delta.length() > 1.0);
        } else {
            assert_eq!(delta, DVec2::ZERO);
        }
        let current = *world.get::<View>(root).unwrap();
        assert!((current.position[0] - view.position[0] - delta.x).abs() < 0.000_001);
        assert!((current.position[2] - view.position[2] - delta.y).abs() < 0.000_001);
        assert_eq!(current.position[1], view.position[1]);
        assert_eq!(current.yaw, view.yaw);
        assert_eq!(current.pitch, view.pitch);
        for _ in 0..4 {
            resolve(&mut world);
            assert_eq!(*world.get::<View>(root).unwrap(), current);
            assert_eq!(world.get::<CanvasView>(root).unwrap().center, center);
        }
    }
}

#[test]
fn untile_restores_the_free_spatial_anchor_even_while_fullscreen() {
    for fullscreen in [false, true] {
        let (mut world, root, native, _) = fixture();
        let original = View {
            spatial: true,
            position: [1410.0, 900.0, 250.0],
            yaw: 0.125,
            pitch: -0.75,
            ..default()
        };
        world.entity_mut(root).insert((
            original,
            crate::native::canvas_selection::SandSelection(vec![native]),
        ));
        DesktopAction::ToggleTiling.apply(&mut world, root);
        resolve(&mut world);
        step(&mut world, root, DesktopAction::Move(Direction::Down));
        if fullscreen {
            DesktopAction::ToggleFullscreen.apply(&mut world, root);
            resolve(&mut world);
        }
        DesktopAction::ToggleTiling.apply(&mut world, root);
        resolve(&mut world);
        if fullscreen {
            DesktopAction::ToggleFullscreen.apply(&mut world, root);
            resolve(&mut world);
        }
        assert_eq!(*world.get::<View>(root).unwrap(), original);
        assert_eq!(
            world.get::<CanvasView>(root).unwrap().center,
            DVec2::new(1200.0, -450.0)
        );
        let center = DVec2::new(1900.0, -750.0);
        world.get_mut::<CanvasView>(root).unwrap().center = center;
        resolve(&mut world);
        assert_eq!(*world.get::<View>(root).unwrap(), original);
    }
}

#[test]
fn background_output_scroll_updates_only_its_saved_spatial_anchor() {
    let (mut world, root, _, _) = tiled(true);
    let mut other = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    other.id = 2;
    other.center = [1250.0, 500.0];
    other.topology.position = [1400.0, 700.0, 800.0];
    let initial_other = other.topology;
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(other);
    let bounds = Rectangle::new([0.0; 2], [800.0, 600.0]).unwrap();
    world.insert_resource(
        crate::native::output_views::OutputViews::new(
            [1, 2]
                .map(|id| OutputViewport { rotation: Default::default(),
                    id: OutputId(id),
                    bounds,
                    usable: Some(bounds),
                    physical_size: [800, 600],
                    raster: crate::host::raster::RasterMapping {
                        origin: [0.0; 2],
                        scale: [1.0; 2],
                    },
                    workspace: id,
                })
                .to_vec(),
            OutputId(1),
        )
        .unwrap(),
    );
    resolve(&mut world);
    let other = &world.get::<Workspaces>(root).unwrap().entries[1];
    let delta = DVec2::from_array(other.center) - DVec2::new(1250.0, 500.0);
    assert_eq!(
        other.topology.position,
        [
            initial_other.position[0] + delta.x,
            700.0,
            initial_other.position[2] + delta.y
        ]
    );
    let companion = other.topology;
    step(&mut world, root, DesktopAction::Focus(Direction::Right));
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[1].topology,
        companion
    );
}
