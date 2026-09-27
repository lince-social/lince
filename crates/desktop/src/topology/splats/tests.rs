use super::*;
use bevy_gaussian_splatting::{Gaussian3d, PlanarGaussian3d, io::codec::CloudCodec};
use std::io::Cursor;

fn ply(rest: usize, binary: bool) -> Vec<u8> {
    let mut properties = vec![
        ("x".to_owned(), 2.0),
        ("y".into(), 3.0),
        ("z".into(), 4.0),
        ("rot_0".into(), 2.0),
        ("rot_1".into(), 0.0),
        ("rot_2".into(), 0.0),
        ("rot_3".into(), 0.0),
        ("scale_0".into(), -10.0),
        ("scale_1".into(), 0.0),
        ("scale_2".into(), 2.0),
        ("opacity".into(), 0.0),
        ("f_dc_0".into(), 0.1),
        ("f_dc_1".into(), 0.2),
        ("f_dc_2".into(), 0.3),
    ];
    for i in 0..rest {
        properties.push((format!("f_rest_{i}"), i as f32 + 1.0));
    }
    properties.reverse();
    let encoding = if binary { "binary_big_endian" } else { "ascii" };
    let mut result = format!("ply\nformat {encoding} 1.0\nelement vertex 1\n").into_bytes();
    for (name, _) in &properties {
        result.extend_from_slice(format!("property float {name}\n").as_bytes());
    }
    result.extend_from_slice(b"end_header\n");
    if binary {
        for (_, value) in properties {
            result.extend_from_slice(&value.to_be_bytes());
        }
    } else {
        result.extend_from_slice(
            properties
                .iter()
                .map(|(_, v)| v.to_string())
                .collect::<Vec<_>>()
                .join(" ")
                .as_bytes(),
        );
        result.push(b'\n');
    }
    result
}

#[test]
fn converts_all_supported_degrees_and_property_orders_without_clamping() {
    for rest in [0, 9, 24, 45] {
        for binary in [false, true] {
            let cloud = codec::read_ply(&mut Cursor::new(ply(rest, binary))).unwrap();
            assert_eq!(cloud.position_visibility.len(), 1);
            let g = cloud.iter().next().unwrap();
            assert_eq!(g.position_visibility.position, [2.0, 3.0, 4.0]);
            assert_eq!(g.rotation.rotation, [1.0, 0.0, 0.0, 0.0]);
            assert_eq!(
                g.scale_opacity.scale,
                [(-10.0_f32).exp(), 1.0, 2.0_f32.exp()]
            );
            assert_eq!(g.scale_opacity.opacity, 0.5);
            assert_eq!(&g.spherical_harmonic.coefficients[..3], &[0.1, 0.2, 0.3]);
            for channel in 0..3 {
                for coefficient in 0..rest / 3 {
                    assert_eq!(
                        g.spherical_harmonic.coefficients[(coefficient + 1) * 3 + channel],
                        (channel * (rest / 3) + coefficient + 1) as f32
                    );
                }
            }
            let mut bytes = Vec::new();
            codec::encode(&cloud, &mut bytes).unwrap();
            assert_eq!(codec::decode(&bytes).unwrap().0, cloud);
            assert_eq!(PlanarGaussian3d::decode(&bytes), cloud);
        }
    }
}

#[test]
fn invalid_clouds_and_ply_fail_without_silent_reduction() {
    assert!(codec::decode(b"not a cloud").is_err());
    assert!(
        codec::read_ply(&mut Cursor::new(
            b"ply\nformat ascii 1.0\nelement vertex 1000000000\nend_header\n"
        ))
        .is_err()
    );
    let mut truncated = ply(45, true);
    truncated.pop();
    assert!(codec::read_ply(&mut Cursor::new(truncated)).is_err());
    assert!(codec::read_ply(&mut Cursor::new(ply(46, false))).is_err());
    let mut cloud = codec::read_ply(&mut Cursor::new(ply(0, false))).unwrap();
    cloud.rotation.clear();
    assert!(codec::decode(&cloud.encode()).is_err());
    let mut cloud = codec::read_ply(&mut Cursor::new(ply(0, false))).unwrap();
    cloud.scale_opacity[0].opacity = f32::NAN;
    assert!(codec::decode(&cloud.encode()).is_err());
}

#[test]
fn accepts_upstream_padding_and_accounts_for_splat_extent() {
    let mut g = Gaussian3d::default();
    g.rotation.rotation = [1.0, 0.0, 0.0, 0.0];
    g.scale_opacity.scale = [2.0, 1.0, 1.0];
    g.scale_opacity.opacity = 1.0;
    let cloud = PlanarGaussian3d::from(vec![g, Gaussian3d::default()]);
    let bounds = codec::validate(&cloud).unwrap();
    assert_eq!(bounds.min, Vec3::splat(-8.0));
    assert_eq!(bounds.max, Vec3::splat(8.0));
    assert_eq!(
        ray_distance(&bounds, Vec3::new(0.0, 0.0, 10.0), -Vec3::Z),
        Some(2.0)
    );
    assert_eq!(
        ray_distance(&bounds, Vec3::new(9.0, 0.0, 10.0), -Vec3::Z),
        None
    );
    assert_eq!(ray_distance(&bounds, Vec3::ZERO, Vec3::Z), Some(8.0));
}

#[test]
fn conversion_preserves_sources_and_import_copies_credits() {
    let source = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let input = source.path().join("model.with.dots.ply");
    let output = source.path().join("prepared.gcloud");
    let original = ply(45, true);
    std::fs::write(&input, &original).unwrap();
    std::fs::write(source.path().join("license.txt"), "Example author credit").unwrap();
    assert_eq!(codec::convert_ply(&input, &output).unwrap().0, 1);
    assert!(codec::convert_ply(&input, &output).is_err());
    assert_eq!(std::fs::read(&input).unwrap(), original);
    let asset = super::super::assets::copy_import(&output, destination.path()).unwrap();
    assert!(asset.valid());
    assert!(is_splat(&asset));
    let package = destination.path().join(&asset.id);
    assert_eq!(
        std::fs::read(package.join(&asset.file)).unwrap(),
        std::fs::read(output).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(package.join("license.txt")).unwrap(),
        "Example author credit"
    );
}

#[test]
fn framing_changes_the_view_without_changing_the_asset_pose() {
    use crate::{
        canvas::CanvasView,
        topology::{Spatial, assets, view::View},
    };
    let mut world = World::new();
    let root = world.spawn((CanvasView::default(), View::default())).id();
    let asset = ImportedAsset {
        id: "a".repeat(32),
        name: "Any cloud".into(),
        file: "source.gcloud".into(),
        scale: 100.0,
    };
    let entity = assets::spawn(
        &mut world,
        root,
        1,
        bevy::math::DVec2::new(20.0, 30.0),
        asset,
    );
    world.entity_mut(entity).insert(Bounds {
        min: Vec3::new(2.0, 1.0, 4.0),
        max: Vec3::new(4.0, 3.0, 6.0),
    });
    let before = *world.get::<Spatial>(entity).unwrap();
    assets::frame(&mut world, entity);
    assert_eq!(world.get::<Spatial>(entity).unwrap(), &before);
    assert_eq!(
        world.get::<CanvasView>(root).unwrap().center,
        bevy::math::DVec2::new(320.0, 530.0)
    );
    let copy = assets::duplicate(&mut world, entity).unwrap();
    assert_eq!(
        world.get::<ImportedAsset>(copy).unwrap().file,
        "source.gcloud"
    );
    let saved = assets::snapshot(&mut world, root);
    let restored: Vec<assets::SavedAsset> =
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.iter().all(|item| item.asset.valid()));
}

#[test]
fn loading_wakes_an_idle_window_and_settles_after_completion() {
    let (sender, receiver) = mpsc::channel();
    let mut timer = LoadWake::new(crate::wake::WakeSignal::new(move || {
        let _ = sender.send(());
    }));
    timer.update(true);
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    for _ in 0..4 {
        timer.update(false);
        assert!(timer.active);
    }
    timer.update(false);
    assert!(!timer.active);
    drop(timer);
}
