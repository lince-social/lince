use super::*;

#[derive(Component)]
pub(super) struct Locate {
    key: String,
    receiver: tokio::sync::oneshot::Receiver<Result<nucleus::location::Fix, String>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Locate {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn start(world: &mut World, owner: Entity) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let key = paging::key(world, owner);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned();
    let task = runtime.spawn(async move {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            crate::native::location::locate_once(),
        )
        .await
        .unwrap_or_else(|_| Err("Device location request timed out".into()));
        let _ = sender.send(result);
        if let Some(wake) = wake {
            wake.ring();
        }
    });
    world.entity_mut(owner).insert(Locate {
        key,
        receiver,
        task,
    });
    world.get_mut::<MapSand>(owner).unwrap().notice =
        "Waiting for one device position. This only centers the map.".into();
}

pub(super) fn maintain(world: &mut World) {
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<Locate>>()
        .iter(world)
        .collect();
    for owner in owners {
        if !visible(world, owner)
            || world.get::<Locate>(owner).unwrap().key != paging::key(world, owner)
        {
            world.entity_mut(owner).remove::<Locate>();
            continue;
        }
        let result = world.get_mut::<Locate>(owner).unwrap().receiver.try_recv();
        let result = match result {
            Ok(result) => result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => continue,
            Err(_) => Err("Device location request ended".into()),
        };
        world.entity_mut(owner).remove::<Locate>();
        let mut sand = world.get_mut::<MapSand>(owner).unwrap();
        match result {
            Ok(fix)
                if fix.capture_age_ms < nucleus::location::MAX_FIX_AGE_MS as u64
                    && (Coordinate {
                        latitude: fix.latitude,
                        longitude: fix.longitude,
                    })
                    .valid() =>
            {
                sand.camera.center = Coordinate {
                    latitude: fix.latitude,
                    longitude: fix.longitude,
                };
                sand.camera.zoom = 15.0;
                sand.auto_center = false;
                sand.device_centered = true;
                sand.notice = "Centered on this device. Use Save point or sharing controls separately if wanted.".into();
            }
            Ok(_) => sand.notice = "No current valid device position is available".into(),
            Err(error) => sand.notice = error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn one_shot_centers_without_saving_and_rejects_denied_or_changed_scope_results() {
        let (mut app, _, owner) = super::super::tests::fixture();
        let initial = app.world().get::<MapSand>(owner).unwrap().camera;
        let result = |latitude| nucleus::location::Fix {
            latitude,
            longitude: -46.6,
            accuracy_metres: Some(1000.0),
            captured_at_ms: 0,
            capture_age_ms: 0,
            sequence: 1,
        };
        for response in [Err("OS permission denied".to_string()), Ok(result(-23.5))] {
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let key = paging::key(app.world(), owner);
            let task = tokio::spawn(std::future::pending());
            app.world_mut().entity_mut(owner).insert(Locate {
                key,
                receiver,
                task,
            });
            sender.send(response).unwrap();
            maintain(app.world_mut());
        }
        let sand = app.world().get::<MapSand>(owner).unwrap();
        assert_ne!(sand.camera, initial);
        assert_eq!(sand.camera.center.latitude, -23.5);
        assert!(sand.picked.is_none());
        assert!(sand.markers.is_empty());
        assert!(sand.device_centered);
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let key = paging::key(app.world(), owner);
        app.world_mut().entity_mut(owner).insert(Locate {
            key,
            receiver,
            task: tokio::spawn(std::future::pending()),
        });
        app.world_mut().get_mut::<MapSand>(owner).unwrap().person = nucleus::new_uid("r");
        sender.send(Ok(result(40.0))).unwrap();
        maintain(app.world_mut());
        assert_eq!(
            app.world()
                .get::<MapSand>(owner)
                .unwrap()
                .camera
                .center
                .latitude,
            -23.5
        );
        assert!(app.world().get::<Locate>(owner).is_none());
    }
}
