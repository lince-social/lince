use super::*;

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
    let mut workspace = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    workspace.id = 2;
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(workspace);
    world.entity_mut(owner).insert(WorkspaceMember(2));
    world.spawn((Text::new("Secondary output"), ChildOf(owner)));
    let output = |id, workspace, scale: f64| {
        let bounds = crate::host::Rectangle::new(
            [(id - 1) as f64 * 1920.0, 0.0],
            [1920.0 / scale, 1080.0 / scale],
        )
        .unwrap();
        crate::host::OutputViewport {
            id: crate::host::OutputId(id),
            workspace,
            bounds,
            usable: Some(bounds),
            physical_size: [1920, 1080],
            rotation: default(),
            raster: crate::host::raster::RasterMapping {
                origin: bounds.origin(),
                scale: [scale; 2],
            },
        }
    };
    let mut views = crate::native::output_views::OutputViews::new(
        vec![output(1, 1, 1.0), output(2, 2, 2.0)],
        crate::host::OutputId(1),
    )
    .unwrap();
    views.canvas = Some(root);
    world.insert_resource(views);
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
