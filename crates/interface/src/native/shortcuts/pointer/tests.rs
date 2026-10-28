use super::*;
use crate::native::shortcuts::model::{Chord, ModKey, Settings, Trigger};

#[test]
fn pointer_defaults_are_typed_and_share_alias_validation_conflicts_and_configuration() {
    let mut settings = Settings::default();
    settings.validate().unwrap();
    assert!(settings.chord(Shortcut::PointerMove).is_none());
    assert!(matches!(
        settings.trigger(Shortcut::PointerMove),
        Some(Trigger::Pointer(_))
    ));
    assert_eq!(
        settings.pointer(Shortcut::PointerMove).unwrap().button,
        MouseButton::Left
    );
    assert_eq!(
        settings.pointer(Shortcut::PointerResize).unwrap().button,
        MouseButton::Right
    );
    settings
        .stage(Shortcut::PointerMove, Some("control+mouseMiddle"))
        .unwrap();
    settings
        .stage(Shortcut::PointerResize, Some("Ctrl+MouseMiddle"))
        .unwrap();
    assert!(settings.validate().is_err());
    assert_eq!(
        settings.conflicts()[0].1,
        [Shortcut::PointerMove, Shortcut::PointerResize]
    );
    settings
        .stage(Shortcut::PointerResize, Some("Mod+Shift+MouseRight"))
        .unwrap();
    settings.stage_modifier(ModKey::Alt).unwrap();
    settings.validate().unwrap();
    let loaded = Settings::from_overrides(
        settings.modifier(),
        settings
            .overrides()
            .map(|(shortcut, value)| (shortcut, value.map(str::to_owned))),
    )
    .unwrap();
    assert_eq!(loaded.text(Shortcut::PointerMove), "Ctrl+MouseMiddle");
    assert_eq!(
        loaded.pointer(Shortcut::PointerResize).unwrap().modifiers,
        Modifiers {
            alt: true,
            shift: true,
            ..Modifiers::NONE
        }
    );
    assert!(Chord::parse_desktop("Ctrl+MouseMiddle").is_err());
    let mut world = bevy::prelude::World::new();
    world.insert_resource(loaded);
    let bindings = crate::native::shortcuts::pointer_bindings(&world);
    assert_eq!(bindings.len(), 2);
    assert!(
        bindings
            .iter()
            .any(|binding| binding.chord.button == MouseButton::Middle
                && binding.operation == Manipulation::Move)
    );
}

#[test]
fn mouse_bindings_cannot_remove_ordinary_clicks_or_become_keyboard_commands() {
    for invalid in [
        "MouseLeft",
        "Shift+MouseRight",
        "Ctrl+A",
        "Mod",
        "Ctrl+MouseOther",
        "Ctrl+Ctrl+MouseLeft",
    ] {
        let mut settings = Settings::default();
        assert!(
            settings
                .stage(Shortcut::PointerMove, Some(invalid))
                .is_err(),
            "{invalid}"
        );
        assert_eq!(settings.text(Shortcut::PointerMove), "Mod+MouseLeft");
    }
    let mut settings = Settings::default();
    assert!(
        settings
            .stage(Shortcut::Close, Some("Ctrl+MouseLeft"))
            .is_err()
    );
    settings.stage(Shortcut::PointerMove, None).unwrap();
    assert!(settings.pointer(Shortcut::PointerMove).is_none());
    settings.stage_reset(Shortcut::PointerMove);
    assert!(settings.pointer(Shortcut::PointerMove).is_some());
    let mut world = bevy::prelude::World::new();
    let owner = world.spawn_empty().id();
    world.insert_resource(crate::native::shortcuts::KeyboardModalOwner(Some(owner)));
    assert!(crate::native::shortcuts::pointer_bindings(&world).is_empty());
}
