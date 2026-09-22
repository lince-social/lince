use super::{
    dsp::{Distortion, Effects},
    library::{Clip, Library, suggestions, valid_path},
};

#[test]
fn sound_effects_are_bounded_and_bypass_preserves_original() {
    let samples: Vec<_> = (0..4800).map(|i| (i as f32 * 0.07).sin() * 0.3).collect();
    assert_eq!(Effects::default().process(&samples, 48000), samples);
    for distortion in [
        Distortion::Overdrive,
        Distortion::Fuzz,
        Distortion::BitCrush,
    ] {
        let effects = Effects {
            enabled: true,
            distortion,
            drive: 1.0,
            ..Default::default()
        };
        let processed = effects.process(&samples, 48000);
        assert_ne!(processed, samples);
        assert_eq!(processed.len(), samples.len());
        assert!(
            processed
                .iter()
                .all(|sample| sample.is_finite() && sample.abs() <= 1.0)
        );
        let dry = Effects {
            mix: 0.0,
            level: 1.0,
            ..effects
        }
        .process(&samples, 48000);
        assert_eq!(dry, samples);
    }
    let invalid = Effects {
        drive: f32::NAN,
        ..Default::default()
    };
    assert!(!invalid.valid());
    assert_eq!(
        invalid.process(&[f32::NAN, f32::INFINITY, 0.5], 0),
        [0.0, 0.0, 0.5]
    );
}

#[test]
fn sound_library_saves_non_destructive_effects_and_searches_paths() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::open(directory.path()).unwrap();
    let path = "recordings/door.wav";
    let clip = Clip {
        samples: vec![0.0, 0.1, -0.2, 0.4, -0.5],
        rate: 48000,
    };
    library.save_recording(path, &clip).unwrap();
    assert!(directory.path().join(path).is_file());
    assert!(library.save_recording(path, &clip).is_err());
    let effects = Effects {
        enabled: true,
        ..Default::default()
    };
    library.apply(path, effects).unwrap();
    let first = library.read(path, false).unwrap().samples;
    assert_ne!(first, clip.samples);
    library.apply(path, effects).unwrap();
    assert_eq!(first, library.read(path, false).unwrap().samples);
    assert_eq!(clip.samples, library.read(path, true).unwrap().samples);
    library.apply(path, Effects::default()).unwrap();
    assert_eq!(clip.samples, library.read(path, false).unwrap().samples);
    let paths = library.list().unwrap();
    assert_eq!(suggestions(&paths, "DOOR"), vec![path]);
    assert!(suggestions(&paths, "missing").is_empty());
}

#[test]
fn sound_paths_reject_traversal_and_non_audio_files() {
    for path in [
        "/tmp/a.wav",
        "recordings/../a.wav",
        "recordings/sub/a.wav",
        "recordings/.hidden.wav",
        "recordings/a.mp3",
        "recordings/a\\b.wav",
        "recordings/a\0.wav",
    ] {
        assert!(!valid_path(path), "{path}");
    }
    assert!(valid_path("recordings/take-1.wav"));
    assert!(valid_path(&super::library::recording_path(
        "../hello/world"
    )));
}

#[cfg(unix)]
#[test]
fn sound_library_rejects_symlinks_outside_lince() {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let library = Library::open(directory.path()).unwrap();
    std::fs::write(other.path().join("secret.wav"), "secret").unwrap();
    std::os::unix::fs::symlink(
        other.path().join("secret.wav"),
        directory.path().join("recordings/link.wav"),
    )
    .unwrap();
    assert!(library.read("recordings/link.wav", false).is_err());
    assert!(library.list().unwrap().is_empty());
    let escape = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(other.path(), escape.path().join("recordings")).unwrap();
    assert!(Library::open(escape.path()).is_err());
}

#[test]
fn sound_library_rejects_empty_and_non_finite_audio() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::open(directory.path()).unwrap();
    assert!(
        library
            .save_recording(
                "recordings/empty.wav",
                &Clip {
                    samples: vec![],
                    rate: 48000
                }
            )
            .is_err()
    );
    let target = directory.path().join("recordings/invalid.wav");
    let mut writer = hound::WavWriter::create(
        target,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .unwrap();
    writer.write_sample(f32::NAN).unwrap();
    writer.finalize().unwrap();
    assert!(library.read("recordings/invalid.wav", false).is_err());
}

#[test]
fn sound_path_autocomplete_updates_area_enter_and_leave_independently() {
    use crate::{
        area::{AreaShape, InfluenceArea},
        area_panel::AreaEditor,
        edit_mode::EditMode,
        workspace::WorkspaceMember,
    };
    use bevy::{prelude::*, text::EditableText};
    let (mut app, root) = crate::edit_mode::tests::fixture();
    app.add_plugins(crate::sound_area::SoundAreaPlugin);
    let world = app.world_mut();
    let (sender, _receiver) = std::sync::mpsc::sync_channel(64);
    let (_events, incoming) = std::sync::mpsc::channel();
    world.insert_resource(super::Audio {
        sender,
        events: std::sync::Mutex::new(incoming),
        paths: vec!["recordings/bell.wav".into(), "recordings/door.wav".into()],
        error: None,
        revision: 1,
    });
    world.get_mut::<EditMode>(root).unwrap().enabled = true;
    world.get_mut::<EditMode>(root).unwrap().areas = true;
    let mut area = InfluenceArea::new(
        AreaShape::Square,
        bevy::math::DVec2::ZERO,
        bevy::math::DVec2::splat(100.0),
    );
    area.sound = Some(crate::sound_area::SoundArea::default());
    let owner = world.spawn((area, ChildOf(root), WorkspaceMember(1))).id();
    world.entity_mut(root).insert(AreaEditor {
        selected: Some(owner),
        ..Default::default()
    });
    let panel = world.spawn(Node::default()).id();
    crate::sound_area::controls(world, root, panel, owner);
    let fields: Vec<_> = world
        .query_filtered::<(Entity, &ChildOf), With<EditableText>>()
        .iter(world)
        .filter(|(_, parent)| parent.parent() == panel)
        .map(|(e, _)| e)
        .collect();
    assert_eq!(fields.len(), 2);
    world
        .get_mut::<EditableText>(fields[0])
        .unwrap()
        .editor
        .set_text("recordings/bell.wav");
    world
        .get_mut::<EditableText>(fields[1])
        .unwrap()
        .editor
        .set_text("recordings/door.wav");
    app.update();
    let sound = app
        .world()
        .get::<InfluenceArea>(owner)
        .unwrap()
        .sound
        .as_ref()
        .unwrap();
    assert_eq!(sound.enter, "recordings/bell.wav");
    assert_eq!(sound.leave, "recordings/door.wav");
    let labels: Vec<_> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(labels.contains(&"recordings/bell.wav".into()));
    assert!(labels.contains(&"recordings/door.wav".into()));
}

#[test]
fn sound_library_rejects_non_regular_files_before_decoding() {
    let directory = tempfile::tempdir().unwrap();
    let library = Library::open(directory.path()).unwrap();
    std::fs::create_dir(directory.path().join("recordings/directory.wav")).unwrap();
    assert!(library.read("recordings/directory.wav", false).is_err());
    assert!(
        library
            .save_recording(
                "recordings/invalid.wav",
                &Clip {
                    samples: vec![f32::NAN],
                    rate: 48000
                }
            )
            .is_err()
    );
}
