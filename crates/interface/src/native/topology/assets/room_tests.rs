use super::*;
use crate::native::{
    canvas::CanvasView,
    canvas_host::Identity,
    topology::{
        self,
        rooms::{Member, Room},
    },
};
use bevy::math::{DQuat, DVec2, DVec3};
use nucleus::canvas::{Pose, room as model};

fn fixture() -> (tempfile::TempDir, World, Entity, Entity, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("room.gcloud");
    let mut point = bevy_gaussian_splatting::Gaussian3d::default();
    point.position_visibility.visibility = 1.0;
    point.rotation.rotation = [1.0, 0.0, 0.0, 0.0];
    point.scale_opacity.scale = [0.1; 3];
    point.scale_opacity.opacity = 1.0;
    let cloud = bevy_gaussian_splatting::PlanarGaussian3d::from(vec![point]);
    let mut bytes = Vec::new();
    topology::splats::codec::encode(&cloud, &mut bytes).unwrap();
    std::fs::write(&source, bytes).unwrap();
    std::fs::write(directory.path().join("CREDITS.txt"), "Room fixture author").unwrap();
    let mut world = World::new();
    world.insert_resource(AssetDirectory(directory.path().join("packages")));
    let root = world
        .spawn((
            crate::native::workspace::Workspaces::default(),
            CanvasView::default(),
        ))
        .id();
    let room = make_room(&mut world, root);
    (directory, world, root, room, source)
}

fn make_room(world: &mut World, root: Entity) -> Entity {
    world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(1000.0),
            },
            Room(default()),
            Identity(nucleus::new_uid("placement")),
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id()
}

fn finish(world: &mut World) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while world
        .get_resource::<Imports>()
        .is_some_and(|imports| !imports.0.is_empty())
    {
        assert!(
            std::time::Instant::now() < deadline,
            "Room import did not finish"
        );
        update(world);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn room_import_follows_its_moved_destination_and_restores_calibration_and_credits() {
    let (_directory, mut world, root, room, source) = fixture();
    import_into_room(&mut world, room, source.clone()).unwrap();
    topology::set_position(&mut world, room, DVec3::new(100.0, 30.0, 200.0));
    let rotation = DQuat::from_rotation_y(0.5).to_array();
    world.entity_mut(room).insert(Spatial {
        elevation: 30.0,
        rotation,
        ..default()
    });
    finish(&mut world);
    let entity = world
        .query_filtered::<Entity, With<ImportedAsset>>()
        .single(&world)
        .unwrap();
    assert_eq!(
        world.get::<Member>(entity).unwrap().0.local,
        model::identity_pose()
    );
    assert_eq!(
        topology::position(&world, entity),
        topology::position(&world, room)
    );
    let local = Pose {
        position: [12.0, 8.0, -16.0],
        rotation: DQuat::from_rotation_z(0.2).to_array(),
    };
    world.get_mut::<Member>(entity).unwrap().0.local = local.clone();
    world.get_mut::<ImportedAsset>(entity).unwrap().scale = 80.0;
    world
        .entity_mut(entity)
        .insert(Identity(nucleus::new_uid("placement")));
    topology::rooms::synchronize(&mut world);
    let expected = topology::position(&world, entity);
    let expected_rotation = topology::spatial(&world, entity).rotation;
    let saved = snapshot(&mut world, root);
    let restored: Vec<SavedAsset> =
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(restored[0].placement.valid());
    assert_eq!(
        restored[0].placement.room_member.as_ref().unwrap().0.local,
        local
    );
    let package = world
        .resource::<AssetDirectory>()
        .0
        .join(&restored[0].asset.id);
    assert_eq!(
        std::fs::read(package.join("source.gcloud")).unwrap(),
        std::fs::read(source).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(package.join("CREDITS.txt")).unwrap(),
        "Room fixture author"
    );
    world.despawn(entity);
    let mut invalid = restored[0].clone();
    invalid.workspace = 99;
    assert!(invalid.restore(&mut world, root).is_err());
    let copy = restored[0].clone().restore(&mut world, root).unwrap();
    assert!(restored[0].clone().restore(&mut world, root).is_err());
    topology::rooms::synchronize(&mut world);
    assert_eq!(world.get::<ImportedAsset>(copy).unwrap().scale, 80.0);
    assert_eq!(topology::position(&world, copy), expected);
    assert_eq!(topology::spatial(&world, copy).rotation, expected_rotation);
    discard(&mut world, copy);
    assert!(!package.exists());
}

#[test]
fn room_import_cancellation_is_scoped_and_deleted_or_hosted_destinations_release_packages() {
    for unavailable in [false, true] {
        let (_directory, mut world, root, first, source) = fixture();
        let second = make_room(&mut world, root);
        import_into_room(&mut world, first, source.clone()).unwrap();
        import_into_room(&mut world, second, source).unwrap();
        cancel_room_imports(&mut world, first);
        finish(&mut world);
        let entities: Vec<_> = world
            .query_filtered::<Entity, With<ImportedAsset>>()
            .iter(&world)
            .collect();
        assert_eq!(entities.len(), 1);
        assert_eq!(
            world.get::<Member>(entities[0]).unwrap().0.room,
            world.get::<Identity>(second).unwrap().0
        );
        discard(&mut world, entities[0]);
        let source = _directory.path().join("room.gcloud");
        import_into_room(&mut world, second, source).unwrap();
        if unavailable {
            world.despawn(second);
        } else {
            world
                .entity_mut(second)
                .insert(crate::native::workspace_sync::HostedReplica);
        }
        finish(&mut world);
        assert_eq!(world.query::<&ImportedAsset>().iter(&world).count(), 0);
        assert_eq!(
            std::fs::read_dir(&world.resource::<AssetDirectory>().0)
                .unwrap()
                .count(),
            0
        );
    }
}
