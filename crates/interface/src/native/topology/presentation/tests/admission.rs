// non-lean-yet
// Scope: fn demand, fn
//   text_demand_tracks_reentry_and_current_children_without_suspending_their_state, fn
//   secondary_outputs_and_overview_previews_keep_their_text_resolution.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use crate::{
    host::{OutputId, Rectangle, SurfaceId, raster::RasterMapping},
    native::layout::{LayoutBox, desktop::ApplicationWindow},
};

fn two_outputs(world: &mut World, root: Entity, scale: f64) -> OutputViewport {
    let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
    let mut workspace = spaces.entries[0].clone();
    workspace.id = 2;
    spaces.entries.push(workspace);
    let output = |id, scale: f64| {
        let bounds = Rectangle::new(
            [(id - 1) as f64 * 1920.0, 0.0],
            [1920.0 / scale, 1080.0 / scale],
        )
        .unwrap();
        OutputViewport {
            id: OutputId(id),
            workspace: id,
            bounds,
            usable: Some(bounds),
            physical_size: [1920, 1080],
            rotation: default(),
            raster: RasterMapping {
                origin: bounds.origin(),
                scale: [scale; 2],
            },
        }
    };
    let secondary = output(2, scale);
    let mut views = OutputViews::new(vec![output(1, 1.0), secondary.clone()], OutputId(1)).unwrap();
    views.canvas = Some(root);
    world.insert_resource(views);
    world.init_resource::<Assets<crate::native::canvas_background::PatternMaterial>>();
    let target = world.resource_mut::<Assets<Image>>().add(image(
        UVec2::from_array(secondary.physical_size),
        TextureFormat::Rgba8UnormSrgb,
    ));
    world.insert_resource(super::super::super::output_views::Targets(HashMap::from([
        (secondary.id, target),
    ])));
    secondary
}

fn demand(world: &World, root: Entity, entity: Entity) -> Request {
    request(
        world,
        root,
        entity,
        world.get::<CanvasItem>(entity).unwrap(),
        UVec2::new(1920, 1080),
        1.0,
    )
}

#[test]
fn text_demand_tracks_reentry_and_current_children_without_suspending_their_state() {
    let (mut world, root, owners) = scene(1);
    let owner = owners[0];
    let label = world.spawn((Text::new("Initial"), ChildOf(owner))).id();
    let visible = demand(&world, root, owner);
    assert!(visible.visible && visible.text);
    assert_eq!(visible.density, 2.0);
    world.get_mut::<CanvasItem>(owner).unwrap().position.x = 10000.0;
    let hidden = demand(&world, root, owner);
    assert!(!hidden.visible && !hidden.text);
    world.get_mut::<Text>(label).unwrap().0 = "Updated while off screen".into();
    world.get_mut::<CanvasItem>(owner).unwrap().position.x = 0.0;
    assert_eq!(demand(&world, root, owner).density, 2.0);
    assert_eq!(
        world.get::<Text>(label).unwrap().0,
        "Updated while off screen"
    );
    world.get_mut::<Node>(label).unwrap().display = Display::None;
    let no_text = demand(&world, root, owner);
    assert!(no_text.visible && !no_text.text);
    assert_eq!(no_text.density, 1.0);
    world.get_mut::<Node>(label).unwrap().display = Display::Flex;
    assert!(demand(&world, root, owner).text);
    let size = world.get::<CanvasItem>(owner).unwrap().size;
    world.get_mut::<CanvasItem>(owner).unwrap().size = Vec2::ZERO;
    assert!(!demand(&world, root, owner).visible);
    world.get_mut::<CanvasItem>(owner).unwrap().size = size;
    world.despawn(label);
    assert!(!demand(&world, root, owner).text);
    let replacement = world.spawn((Text::new("Replacement"), ChildOf(owner))).id();
    assert_eq!(demand(&world, root, owner).density, 2.0);
    let mut view = world
        .get_mut::<super::super::super::view::View>(root)
        .unwrap();
    view.spatial = true;
    view.position = [0.0, 500.0, 500.0];
    view.pitch = -std::f32::consts::FRAC_PI_4;
    drop(view);
    let spatial_text = demand(&world, root, owner);
    world.despawn(replacement);
    let spatial_plain = demand(&world, root, owner);
    assert!(
        spatial_text.visible && spatial_text.text && spatial_plain.visible && !spatial_plain.text
    );
    assert_eq!(spatial_text.density, spatial_plain.density * 2.0);
}

#[test]
fn secondary_outputs_and_overview_previews_keep_their_text_resolution() {
    let (mut world, root, owners) = scene(1);
    let owner = owners[0];
    two_outputs(&mut world, root, 2.0);
    world.entity_mut(owner).insert(WorkspaceMember(2));
    world.spawn((Text::new("Secondary output"), ChildOf(owner)));
    let secondary = demand(&world, root, owner);
    assert!(secondary.visible && secondary.text);
    assert_eq!(secondary.density, 4.0);
    world.insert_resource(super::super::super::previews::Previews(vec![
        super::super::super::previews::Preview {
            id: owner,
            root,
            workspace: 2,
            view: CanvasView {
                center: bevy::math::DVec2::ZERO,
                zoom: 3.0,
            },
            pixels: UVec2::new(1920, 1080),
            image: Handle::default(),
        },
    ]));
    assert_eq!(demand(&world, root, owner).density, 6.0);
    world.remove_resource::<super::super::super::previews::Previews>();
    assert_eq!(demand(&world, root, owner).density, 4.0);
    world.get_mut::<CanvasItem>(owner).unwrap().position.x = 10000.0;
    assert!(!demand(&world, root, owner).visible);
    world.insert_resource(super::super::super::previews::Previews(vec![
        super::super::super::previews::Preview {
            id: owner,
            root,
            workspace: 2,
            view: CanvasView {
                center: bevy::math::DVec2::new(10000.0, 0.0),
                zoom: 0.5,
            },
            pixels: UVec2::splat(256),
            image: Handle::default(),
        },
    ]));
    let preview = demand(&world, root, owner);
    assert!(preview.visible && preview.text);
    assert_eq!(preview.density, 1.0);
    world.remove_resource::<super::super::super::previews::Previews>();
    let retired = demand(&world, root, owner);
    assert!(!retired.visible && !retired.text);
}

#[cfg_attr(test, test)]
pub(super) fn zoom_pressure_keeps_other_output_captures_at_their_own_resolution() {
    let (mut world, root, owners) = scene(6);
    let output = two_outputs(&mut world, root, 1.25);
    for owner in &owners[..4] {
        *world.get_mut::<CanvasItem>(*owner).unwrap() = CanvasItem {
            position: default(),
            size: Vec2::new(1000.0, 800.0),
        };
        world.spawn((Text::new("Record title"), ChildOf(*owner)));
    }
    let (client, record) = (owners[4], owners[5]);
    for (owner, position, size) in [
        (client, 100.0, Vec2::new(640.0, 480.0)),
        (record, -480.0, Vec2::new(240.0, 100.0)),
    ] {
        world.entity_mut(owner).insert((
            WorkspaceMember(2),
            LayoutBox::new(size),
            CanvasItem {
                position: bevy::math::DVec2::new(position, 0.0),
                size,
            },
        ));
    }
    world.entity_mut(client).insert(ApplicationWindow {
        surface: SurfaceId(1),
        content_size: [640, 480],
    });
    world.spawn((Text::new("Relation title"), ChildOf(record)));

    synchronize(&mut world);
    let captures = [client, record].map(|owner| {
        let surface = world.get::<Surface>(owner).unwrap();
        (surface.pixels, surface.image.id())
    });
    assert_eq!(captures[0].0, UVec2::new(800, 600));
    assert!(captures[1].0.x >= 600);
    for zoom in [1.2, 2.1, 2.99, 2.1, 1.2, 1.0] {
        world.get_mut::<CanvasView>(root).unwrap().zoom = zoom;
        synchronize(&mut world);
        let diagnostics = world.resource::<surface_budget::Diagnostics>();
        assert!(diagnostics.granted_bytes <= surface_budget::BYTE_BUDGET);
        if zoom >= 2.1 {
            assert!(owners[..4].iter().all(|owner| {
                world
                    .get::<surface_budget::NativeRaster>(*owner)
                    .is_some_and(|raster| raster.budget_limited)
            }));
        }
        for (owner, expected) in [client, record].into_iter().zip(captures) {
            let surface = world.get::<Surface>(owner).unwrap();
            assert!(surface.visible);
            assert_eq!((surface.pixels, surface.image.id()), expected);
            assert!(world.get::<Camera>(surface.camera).unwrap().is_active);
        }
        let geometry = super::super::super::output_views::geometry(&mut world, root, &output);
        assert_eq!(geometry.len(), 2);
    }
}

#[cfg_attr(test, test)]
pub(super) fn focused_3d_camera_turns_leave_secondary_visibility_and_density_stable() {
    let (mut world, root, owners) = scene(3);
    let output = two_outputs(&mut world, root, 2.0);
    let (focused, record, offscreen) = (owners[0], owners[1], owners[2]);
    let center = [1200.0, 800.0];
    {
        let mut spaces = world.get_mut::<Workspaces>(root).unwrap();
        spaces.entries[1].center = center;
        spaces.entries[1].zoom = 1.5;
    }
    world.entity_mut(record).insert((
        WorkspaceMember(2),
        LayoutBox::new(Vec2::splat(100.0)),
        CanvasItem {
            position: bevy::math::DVec2::from_array(center),
            size: Vec2::splat(100.0),
        },
    ));
    world.entity_mut(offscreen).insert(WorkspaceMember(2));
    world.spawn((Text::new("Relation title"), ChildOf(record)));

    for (yaw, focused_visible) in [(0.0, true), (std::f32::consts::PI, false), (0.0, true)] {
        *world
            .get_mut::<super::super::super::view::View>(root)
            .unwrap() = super::super::super::view::View {
            spatial: true,
            position: [0.0, 500.0, 500.0],
            yaw,
            pitch: -std::f32::consts::FRAC_PI_4,
            ..default()
        };
        let request = demand(&world, root, record);
        assert!(request.visible);
        assert_eq!(request.density, 6.0);
        synchronize(&mut world);
        assert_eq!(
            world.get::<Surface>(focused).unwrap().visible,
            focused_visible
        );
        assert!(world.get::<Surface>(record).unwrap().visible);
        assert!(!world.get::<Surface>(offscreen).unwrap().visible);
        let geometry = super::super::super::output_views::geometry(&mut world, root, &output);
        assert_eq!(geometry.len(), 1);
    }
}
