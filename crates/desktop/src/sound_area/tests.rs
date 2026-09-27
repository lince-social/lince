use super::*;
use crate::area::{AreaShape, InfluenceArea};
use bevy::math::DVec2;

#[test]
fn sound_area_crossings_are_silent_on_start_and_play_only_on_edges() {
    let mut world = World::new();
    let area = world.spawn_empty().id();
    let sand = world.spawn_empty().id();
    let mut crossings = Crossings::default();
    let sound = SoundArea {
        enter: "recordings/enter.wav".into(),
        leave: "recordings/leave.wav".into(),
        volume: 0.5,
    };
    assert!(
        crossings
            .sample(area, &sound, HashMap::from([(sand, true)]))
            .is_empty()
    );
    assert!(
        crossings
            .sample(area, &sound, HashMap::from([(sand, true)]))
            .is_empty()
    );
    let plays = crossings.sample(area, &sound, HashMap::from([(sand, false)]));
    assert_eq!(plays.len(), 1);
    assert_eq!(plays[0].path, sound.leave);
    assert_eq!(plays[0].volume, 0.5);
    assert!(
        crossings
            .sample(area, &sound, HashMap::from([(sand, false)]))
            .is_empty()
    );
    assert_eq!(
        crossings.sample(area, &sound, HashMap::from([(sand, true)]))[0].path,
        sound.enter
    );
    assert!(crossings.sample(area, &sound, HashMap::new()).is_empty());
    let changed = SoundArea {
        volume: 1.0,
        ..sound
    };
    assert!(
        crossings
            .sample(area, &changed, HashMap::from([(sand, true)]))
            .is_empty()
    );
}

#[test]
fn sound_area_settings_round_trip_and_validate_paths() {
    let mut area = InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(100.0));
    area.sound = Some(SoundArea {
        enter: "recordings/bell.wav".into(),
        ..Default::default()
    });
    assert!(area.validate());
    let decoded: InfluenceArea =
        serde_json::from_slice(&serde_json::to_vec(&area).unwrap()).unwrap();
    assert_eq!(decoded, area);
    area.sound.as_mut().unwrap().leave = "../outside.wav".into();
    assert!(!area.validate());
}

#[test]
fn sound_area_membership_respects_workspace_and_shape() {
    let mut world = World::new();
    world.init_resource::<Runtime>();
    let root = world.spawn(Workspaces::default()).id();
    let mut area = InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(100.0));
    area.sound = Some(SoundArea::default());
    let area = world.spawn((area, ChildOf(root), WorkspaceMember(1))).id();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(10.0),
            },
            ChildOf(root),
            WorkspaceMember(1),
        ))
        .id();
    let foreign = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(10.0),
            },
            ChildOf(root),
            WorkspaceMember(2),
        ))
        .id();
    update(&mut world);
    let members = &world.resource::<Runtime>().0.areas[&area].1;
    assert_eq!(members.get(&sand), Some(&true));
    assert!(!members.contains_key(&foreign));
    world.get_mut::<CanvasItem>(sand).unwrap().position = DVec2::splat(49.0);
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().0.areas[&area].1.get(&sand),
        Some(&false)
    );
    world.get_mut::<Workspaces>(root).unwrap().active = 2;
    update(&mut world);
    assert!(world.resource::<Runtime>().0.areas.is_empty());
}

#[test]
fn sound_area_many_sands_share_bounded_playback_requests() {
    let mut world = World::new();
    let area = world.spawn_empty().id();
    let sands: Vec<_> = (0..1000).map(|_| world.spawn_empty().id()).collect();
    let sound = SoundArea {
        enter: "recordings/enter.wav".into(),
        ..Default::default()
    };
    let mut crossings = Crossings::default();
    crossings.sample(area, &sound, sands.iter().map(|e| (*e, false)).collect());
    assert_eq!(
        crossings
            .sample(area, &sound, sands.iter().map(|e| (*e, true)).collect())
            .len(),
        1
    );
}

#[test]
fn sound_area_spatial_membership_respects_depth() {
    let mut world = World::new();
    world.init_resource::<Runtime>();
    let root = world
        .spawn((
            Workspaces::default(),
            crate::topology::presentation::SpatialRoot,
        ))
        .id();
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(100.0));
    area.depth = 40.0;
    area.sound = Some(SoundArea::default());
    let owner = world.spawn((area, ChildOf(root), WorkspaceMember(1))).id();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::ZERO,
                size: Vec2::splat(10.0),
            },
            ChildOf(root),
            WorkspaceMember(1),
            crate::topology::Spatial {
                elevation: -20.0,
                ..Default::default()
            },
        ))
        .id();
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().0.areas[&owner].1.get(&sand),
        Some(&true)
    );
    world
        .get_mut::<crate::topology::Spatial>(sand)
        .unwrap()
        .elevation = -50.0;
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().0.areas[&owner].1.get(&sand),
        Some(&false)
    );
}
