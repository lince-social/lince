use super::*;
use bevy::ecs::system::RunSystemOnce;

fn fixture() -> (World, Entity, Entity, Context) {
    let (mut world, entity) = super::super::tests::editing_world("aé界");
    let bounds = Rectangle::new([1000.0, 50.0], [800.0, 600.0]).unwrap();
    let viewport = OutputViewport { rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1000, 750],
        raster: RasterMapping { origin: bounds.origin(), scale: [1.25; 2] }, workspace: 1 };
    let root = world.spawn((native::workspace::Workspaces::default(), native::layout::desktop::HostViewport(viewport.clone()), Node::default())).id();
    world.entity_mut(entity).insert((ChildOf(root), Node::default(), InheritedVisibility::VISIBLE));
    let camera = world.spawn_empty().id();
    let image = Handle::<Image>::default().id();
    let view = View { root: Some(root), output: viewport.id, image, raster: viewport.raster, origin: bounds.origin(), viewport: Some(viewport.clone()) };
    let owner = native::accessibility::context(&world, entity, true).unwrap();
    let context = Context { entity, owner: Some(owner), camera, image, view, local: Some(viewport), workspace: Some(1), sensitive: false,
        transform: UiGlobalTransform::default().to_cols_array(), size: Vec2::new(200.0, 80.0), content_origin: Vec2::ZERO, clip: None,
        caret: [1100, 100, 1, 20], source: Source::Final };
    (world, root, entity, context)
}

#[test]
fn semantic_editor_session_survives_geometry_but_not_another_owner_or_unproven_view() {
    let (mut world, root, entity, original) = fixture();
    let mut moved = original.clone();
    moved.camera = world.spawn_empty().id();
    moved.transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(80.0, 40.0))).to_cols_array();
    moved.size *= 2.0;
    moved.caret = [1200, 300, 2, 40];
    moved.view.raster.scale = [1.5; 2];
    assert!(original.same_session(&moved));
    world.insert_resource(lince_interface::host::display::DisplaySnapshot { revision: 8, ..default() });
    moved.owner = native::accessibility::context(&world, entity, true);
    assert!(original.same_session(&moved));
    assert!(!original.owner.as_ref().unwrap().describes(&world, entity));
    let unknown = Context { owner: None, ..original.clone() };
    assert!(!unknown.same_session(&Context { owner: None, ..moved.clone() }));
    assert!(!original.same_session(&unknown));
    world.entity_mut(entity).insert(native::protein_area::RecordBinding { area: root, uid: "record-replacement".into(), source: Default::default() });
    moved.owner = native::accessibility::context(&world, entity, true);
    assert!(!original.same_session(&moved));
    world.entity_mut(entity).remove::<native::protein_area::RecordBinding>();
    world.entity_mut(root).insert(native::workspace::Workspaces::default());
    moved.owner = native::accessibility::context(&world, entity, true);
    assert!(!original.same_session(&moved));
    let before_protection = moved.clone();
    native::accessibility::secret(&mut world, entity);
    moved.owner = native::accessibility::context(&world, entity, true);
    moved.sensitive = true;
    assert!(!before_protection.same_session(&moved));
}

#[test]
fn same_output_geometry_preparation_preserves_preedit_but_output_loss_cancels_once() {
    let (mut world, _, entity, context) = fixture();
    let initial = super::super::text_state(&world, entity, context.caret).unwrap();
    super::super::enqueue(&mut world, Edit { context: initial, commit: None, preedit: Some(("界".into(), Some((0, 3)))), delete: None });
    super::super::tests::process(&mut world);
    assert!(world.get::<EditableText>(entity).unwrap().is_composing());
    world.insert_resource(CaptureState { view: Some(context.view.clone()), context: Some(context.clone()), generation: 7 });
    let mut view = context.view.clone();
    view.raster.scale = [1.5; 2];
    view.origin = [1200.0, 80.0];
    prepare(&mut world, Some(view));
    assert_eq!(world.resource::<CaptureState>().generation, 7);
    assert!(world.get::<EditableText>(entity).unwrap().is_composing());
    assert!(world.get::<EditableText>(entity).unwrap().pending_edits.is_empty());
    prepare(&mut world, None);
    assert_eq!(world.resource::<CaptureState>().generation, 8);
    world.run_system_once(bevy::text::apply_text_edits).unwrap();
    assert!(!world.get::<EditableText>(entity).unwrap().is_composing());
    prepare(&mut world, None);
    assert_eq!(world.resource::<CaptureState>().generation, 8);
    assert!(world.get::<EditableText>(entity).unwrap().pending_edits.is_empty());
}
