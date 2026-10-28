use super::*;

#[derive(Resource, Default)]
pub(super) struct Suspension {
    pub active: bool,
    cameras: Vec<Entity>,
}

pub(in crate::presentation) fn install(app: &mut App) {
    app.init_resource::<Suspension>()
        .add_systems(First, restore)
        .add_systems(Last, suspend);
}

fn restore(mut suspension: ResMut<Suspension>, mut cameras: Query<&mut Camera>) {
    for entity in suspension.cameras.drain(..) {
        if let Ok(mut camera) = cameras.get_mut(entity) {
            camera.is_active = true;
        }
    }
}

fn suspend(mut suspension: ResMut<Suspension>, mut cameras: Query<(Entity, &mut Camera)>) {
    if !suspension.active {
        return;
    }
    for (entity, mut camera) in &mut cameras {
        if camera.is_active {
            suspension.cameras.push(entity);
            camera.is_active = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suspension_restores_activity_before_layout_and_retains_disabled_and_new_cameras() {
        #[derive(Resource, Default)]
        struct Observed(Vec<usize>);
        let mut app = App::new();
        install(&mut app);
        app.init_resource::<Observed>().add_systems(
            Update,
            |cameras: Query<&Camera>, mut observed: ResMut<Observed>| {
                observed
                    .0
                    .push(cameras.iter().filter(|camera| camera.is_active).count());
            },
        );
        let active = app.world_mut().spawn(Camera::default()).id();
        let disabled = app
            .world_mut()
            .spawn(Camera {
                is_active: false,
                ..default()
            })
            .id();
        app.world_mut().resource_mut::<Suspension>().active = true;
        app.update();
        assert!(!app.world().get::<Camera>(active).unwrap().is_active);
        assert!(!app.world().get::<Camera>(disabled).unwrap().is_active);
        let created = app.world_mut().spawn(Camera::default()).id();
        app.world_mut().despawn(active);
        app.update();
        assert_eq!(app.world().resource::<Suspension>().cameras, vec![created]);
        app.update();
        app.world_mut().resource_mut::<Suspension>().active = false;
        app.update();
        assert!(app.world().get::<Camera>(created).unwrap().is_active);
        assert!(!app.world().get::<Camera>(disabled).unwrap().is_active);
        assert!(app.world().resource::<Suspension>().cameras.is_empty());
        assert_eq!(app.world().resource::<Observed>().0, vec![1, 1, 1, 1]);
    }
}
