use super::*;
use crate::actions::Action;

#[test]
fn recorder_selection_effects_and_placement_restore() {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::theme::Typography>();
    world.init_resource::<crate::tokens::ThemeSettings>();
    let root = world.spawn(crate::workspace::Workspaces::default()).id();
    let owner = spawn(
        &mut world,
        root,
        1,
        DVec2::new(30.0, 40.0),
        RecorderCastle::default(),
    );
    ui::Control::Select("recordings/one.wav".into()).apply(&mut world, owner);
    ui::Control::Mode(crate::sound::dsp::Distortion::Fuzz).apply(&mut world, owner);
    assert!(world.get::<RecorderCastle>(owner).unwrap().effect().enabled);
    ui::Control::Select("recordings/two.wav".into()).apply(&mut world, owner);
    assert_eq!(
        world.get::<RecorderCastle>(owner).unwrap().effect(),
        Effects::default()
    );
    ui::Control::Select("recordings/one.wav".into()).apply(&mut world, owner);
    let settings = world.get::<RecorderCastle>(owner).unwrap().effect();
    assert_eq!(settings.distortion, crate::sound::dsp::Distortion::Fuzz);
    let saved = snapshot(&mut world, root).remove(0);
    assert!(saved.valid());
    let encoded = serde_json::to_vec(&saved).unwrap();
    world.despawn(owner);
    let saved: SavedRecorder = serde_json::from_slice(&encoded).unwrap();
    saved.restore(&mut world, root);
    let (castle, item) = world
        .query::<(&RecorderCastle, &crate::canvas::CanvasItem)>()
        .single(&world)
        .unwrap();
    assert_eq!(castle.selected, "recordings/one.wav");
    assert_eq!(castle.effect(), settings);
    assert_eq!(item.position, DVec2::new(30.0, 40.0));
}

#[test]
fn recorder_rejects_untrusted_saved_paths_and_effects() {
    let mut castle = RecorderCastle {
        selected: "../secret.wav".into(),
        ..Default::default()
    };
    assert!(!castle.valid());
    castle.selected = "recordings/test.wav".into();
    assert!(castle.valid());
    castle.effects.insert(
        castle.selected.clone(),
        Effects {
            level: f32::INFINITY,
            ..Default::default()
        },
    );
    assert!(!castle.valid());
}
