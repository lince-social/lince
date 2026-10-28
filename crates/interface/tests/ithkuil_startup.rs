#![cfg(feature = "ui")]

use bevy::prelude::*;
use lince_interface::{
    i18n::{Language, Token, initialize, native_text},
    ithkuil::IthkuilPlugin,
};

#[test]
fn startup_language_renders_native_translations_and_restores_english_font_on_reuse() {
    initialize(Language::Ithkuil).unwrap();
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .add_plugins(IthkuilPlugin);
    let original = TextFont::from_font_size(18.0);
    let native = app
        .world_mut()
        .spawn((Text::new(Token::Name.text()), original.clone()))
        .id();
    let english = app
        .world_mut()
        .spawn((Text::new(Token::Save.text()), original.clone()))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Text>(native).unwrap().0,
        native_text(Token::Name.text()).unwrap().encoding()
    );
    assert_ne!(
        app.world().get::<TextFont>(native).unwrap().font,
        original.font
    );
    assert_eq!(app.world().get::<Text>(english).unwrap().0, "Save");
    assert_eq!(*app.world().get::<TextFont>(english).unwrap(), original);
    let pointer = app.world().get::<Text>(native).unwrap().0.as_ptr();
    let tick = app
        .world()
        .entity(native)
        .get_ref::<Text>()
        .unwrap()
        .last_changed();
    for _ in 0..1000 {
        lince_interface::controls::status(app.world_mut(), native, Token::Name.text());
    }
    app.update();
    assert_eq!(app.world().get::<Text>(native).unwrap().0.as_ptr(), pointer);
    assert_eq!(
        app.world()
            .entity(native)
            .get_ref::<Text>()
            .unwrap()
            .last_changed(),
        tick
    );
    app.world_mut().get_mut::<Text>(native).unwrap().0 = Token::Save.text().into();
    app.update();
    assert_eq!(app.world().get::<Text>(native).unwrap().0, "Save");
    assert_eq!(*app.world().get::<TextFont>(native).unwrap(), original);
}
