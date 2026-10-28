use super::*;

#[test]
fn an_inactive_tiled_destination_reprojects_the_committed_row_without_changing_authored_homes_or_focus()
 {
    let (mut world, root, native, application) = fixture();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let anchor = world.get::<Managed>(native).unwrap().id;
    let id = world.get::<Managed>(application).unwrap().id;
    prepare_captured(&mut world, root, 1, anchor, 2, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let original = model(&world, root, 1)
        .items()
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .clone();
    let neighbour = model(&world, root, 2).items()[0].clone();
    let view = *world.get::<CanvasView>(root).unwrap();
    let input = world.resource::<bevy::input_focus::InputFocus>().get();
    let window = *world.get::<ApplicationWindow>(application).unwrap();
    let layout = *world.get::<LayoutBox>(application).unwrap();
    world.get_mut::<CanvasItem>(application).unwrap().position.x = -8000.0;
    prepare_captured(
        &mut world,
        root,
        1,
        id,
        2,
        Some(DropSlot {
            anchor,
            side: crate::host::Direction::Right,
        }),
    )
    .unwrap()
    .commit(&mut world)
    .unwrap();
    assert_eq!(model(&world, root, 2).tiled_rows(), vec![vec![anchor, id]]);
    let a = world.get::<CanvasItem>(application).unwrap();
    let b = world.get::<CanvasItem>(native).unwrap();
    assert!(a.position.x - f64::from(a.size.x) * 0.5 >= b.position.x + f64::from(b.size.x) * 0.5);
    assert_eq!(
        model(&world, root, 2)
            .items()
            .iter()
            .find(|item| item.id == id),
        Some(&original)
    );
    assert_eq!(
        model(&world, root, 2)
            .items()
            .iter()
            .find(|item| item.id == anchor),
        Some(&neighbour)
    );
    assert_eq!(world.get::<Workspaces>(root).unwrap().active, 1);
    assert!(*world.get::<CanvasView>(root).unwrap() == view);
    assert_eq!(
        world.resource::<bevy::input_focus::InputFocus>().get(),
        input
    );
    assert_eq!(
        world.get::<ApplicationWindow>(application).unwrap().surface,
        window.surface
    );
    assert_eq!(
        world
            .get::<ApplicationWindow>(application)
            .unwrap()
            .content_size,
        window.content_size
    );
    assert_eq!(*world.get::<LayoutBox>(application).unwrap(), layout);
}

#[test]
fn a_hidden_transient_family_reprojects_together_without_rewriting_its_authored_home() {
    let (mut world, root, native, application) = fixture();
    world
        .entity_mut(native)
        .insert(TransientParent(application));
    crate::native::layout::engine::resolve(&mut world);
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let id = world.get::<Managed>(native).unwrap().id;
    let original = model(&world, root, 1)
        .items()
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .free;
    world.get_mut::<CanvasItem>(native).unwrap().position.x = -6000.0;
    prepare_captured(&mut world, root, 1, id, 2, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let child = world.get::<CanvasItem>(native).unwrap();
    let parent = world.get::<CanvasItem>(application).unwrap();
    assert!(child.position.distance(parent.position) < 0.000001);
    assert!(world.get::<Managed>(native).unwrap().floating);
    assert_eq!(world.get::<TransientParent>(native).unwrap().0, application);
    assert_eq!(
        model(&world, root, 2)
            .items()
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .free,
        original
    );
}

#[test]
fn free_destinations_keep_the_existing_live_geometry() {
    let (mut world, root, _, application) = fixture();
    destination(&mut world, root, WorkspaceLayout::Free);
    let original = *world.get::<CanvasItem>(application).unwrap();
    let id = world.get::<Managed>(application).unwrap().id;
    prepare_captured(&mut world, root, 1, id, 2, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let current = world.get::<CanvasItem>(application).unwrap();
    assert_eq!(current.position, original.position);
    assert_eq!(current.size, original.size);
}

#[test]
fn a_workspace_on_a_companion_output_keeps_its_normal_output_geometry_owner() {
    use crate::host::{OutputId, OutputViewport};
    let (mut world, root, native, _) = fixture();
    destination(&mut world, root, WorkspaceLayout::Tiled);
    let outputs = [1, 2].map(|id| {
        let bounds = Rectangle::new([(id - 1) as f64 * 1000.0, 0.0], [800.0, 600.0]).unwrap();
        OutputViewport { rotation: Default::default(),
            id: OutputId(id),
            bounds,
            usable: Some(bounds),
            physical_size: [800, 600],
            raster: crate::host::raster::RasterMapping {
                origin: bounds.origin(),
                scale: [1.0; 2],
            },
            workspace: id,
        }
    });
    let mut views =
        crate::native::output_views::OutputViews::new(outputs.into(), OutputId(1)).unwrap();
    views.canvas = Some(root);
    world.insert_resource(views);
    let original = *world.get::<CanvasItem>(native).unwrap();
    let id = world.get::<Managed>(native).unwrap().id;
    prepare_captured(&mut world, root, 1, id, 2, None)
        .unwrap()
        .commit(&mut world)
        .unwrap();
    let current = world.get::<CanvasItem>(native).unwrap();
    assert_eq!(current.position, original.position);
    assert_eq!(current.size, original.size);
}
