use super::*;
use bevy_app::HierarchyPropagatePlugin;
use bevy_ecs::system::SystemState;
use bevy_ui::{UiScale, UiTargetCamera};

#[test]
fn inactive_camera_keeps_its_identity_but_does_not_admit_ui_extraction() {
    let mut app = App::new();
    app.init_resource::<UiScale>()
        .add_plugins(HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(
            PostUpdate,
        ))
        .add_systems(Update, bevy_ui::update::propagate_ui_target_cameras);
    let rendered = app.world_mut().spawn_empty().id();
    let other_rendered = app.world_mut().spawn_empty().id();
    let camera = app
        .world_mut()
        .spawn((Camera::default(), RenderEntity::from(rendered)))
        .id();
    let other = app
        .world_mut()
        .spawn((Camera::default(), RenderEntity::from(other_rendered)))
        .id();
    let node = app
        .world_mut()
        .spawn((Node::default(), UiTargetCamera(camera)))
        .id();
    let other_node = app
        .world_mut()
        .spawn((Node::default(), UiTargetCamera(other)))
        .id();
    app.update();
    let target = *app.world().get::<ComputedUiTargetCamera>(node).unwrap();
    let other_target = *app
        .world()
        .get::<ComputedUiTargetCamera>(other_node)
        .unwrap();
    assert_eq!(target.get(), Some(camera));
    assert_eq!(other_target.get(), Some(other));
    let mut state = SystemState::<UiCameraMap>::new(app.world_mut());
    for active in [true, false, true, false] {
        app.world_mut().get_mut::<Camera>(camera).unwrap().is_active = active;
        let mapping = state.get(app.world()).unwrap();
        let mut mapper = mapping.get_mapper();
        assert_eq!(mapper.map(&other_target), Some(other_rendered));
        for _ in 0..3 {
            assert_eq!(mapper.map(&target), active.then_some(rendered));
        }
        assert_eq!(mapper.map(&other_target), Some(other_rendered));
        assert_eq!(mapper.map(&ComputedUiTargetCamera::default()), None);
        assert_eq!(
            app.world().get::<RenderEntity>(camera).unwrap().id(),
            rendered
        );
    }
    app.world_mut().get_mut::<Camera>(camera).unwrap().is_active = true;
    app.world_mut().entity_mut(camera).remove::<RenderEntity>();
    {
        let mapping = state.get(app.world()).unwrap();
        let mut mapper = mapping.get_mapper();
        assert_eq!(mapper.map(&target), None);
        assert_eq!(mapper.map(&other_target), Some(other_rendered));
    }
    app.world_mut()
        .entity_mut(camera)
        .insert(RenderEntity::from(other_rendered));
    {
        let mapping = state.get(app.world()).unwrap();
        assert_eq!(mapping.get_mapper().map(&target), Some(other_rendered));
    }
    app.world_mut().despawn(camera);
    let mapping = state.get(app.world()).unwrap();
    assert_eq!(mapping.get_mapper().map(&target), None);
}
