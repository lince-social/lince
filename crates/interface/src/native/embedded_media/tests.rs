use super::*;
use bevy::picking::{
    backend::HitData,
    pointer::{Location, PointerId},
};

fn world() -> World {
    let mut world = World::new();
    world.init_resource::<Assets<Font>>();
    world.init_resource::<crate::native::theme::Typography>();
    world.init_resource::<crate::native::tokens::ThemeSettings>();
    world.init_resource::<InputFocus>();
    world.register_component::<Window>();
    world
}

#[test]
fn controls_open_only_on_explicit_input_and_hiding_restores_focus() {
    let mut world = world();
    let parent = world.spawn(Node::default()).id();
    let frame = frame(&mut world, parent, "Model");
    assert_eq!(
        world.get::<Node>(frame.controls).unwrap().display,
        Display::None
    );
    assert_eq!(world.get::<Node>(frame.owner).unwrap().border, UiRect::ZERO);
    assert_eq!(
        world
            .get::<BackgroundColor>(frame.owner)
            .unwrap()
            .0
            .to_srgba()
            .alpha,
        0.0
    );
    world.trigger(Pointer::new(
        PointerId::Mouse,
        Location {
            target: bevy::camera::NormalizedRenderTarget::Image(Handle::<Image>::default().into()),
            position: Vec2::ZERO,
        },
        Over {
            hit: HitData::new(frame.content, 0.0, None, None),
        },
        frame.content,
    ));
    assert_eq!(
        world.get::<Node>(frame.controls).unwrap().display,
        Display::None
    );
    for (button, expected) in [
        (PointerButton::Primary, Display::None),
        (PointerButton::Secondary, Display::Flex),
        (PointerButton::Secondary, Display::None),
    ] {
        world.trigger(Pointer::new(
            PointerId::Mouse,
            Location {
                target: bevy::camera::NormalizedRenderTarget::Image(
                    Handle::<Image>::default().into(),
                ),
                position: Vec2::ZERO,
            },
            Click {
                button,
                hit: HitData::new(frame.content, 0.0, None, None),
                duration: std::time::Duration::ZERO,
                count: 1,
            },
            frame.content,
        ));
        world.flush();
        assert_eq!(world.get::<Node>(frame.controls).unwrap().display, expected);
    }
    set_controls(&mut world, frame.owner, true);
    let child = world.spawn((Node::default(), ChildOf(frame.controls))).id();
    world
        .resource_mut::<InputFocus>()
        .set(child, bevy::input_focus::FocusCause::Navigated);
    set_controls(&mut world, frame.owner, false);
    assert_eq!(
        world.get::<Node>(frame.controls).unwrap().display,
        Display::None
    );
    assert_eq!(world.resource::<InputFocus>().get(), Some(frame.owner));
    assert_eq!(
        world.get::<Node>(frame.content).unwrap().display,
        Display::Flex
    );
}

#[test]
fn keyboard_controls_bubble_from_media_and_escape_returns_focus() {
    use bevy::input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    };
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::native::theme::Typography>()
        .init_resource::<crate::native::tokens::ThemeSettings>()
        .add_plugins((
            bevy::input::InputPlugin,
            bevy::input_focus::InputFocusPlugin,
            bevy::input_focus::InputDispatchPlugin,
        ));
    let window = app
        .world_mut()
        .spawn((Window::default(), bevy::window::PrimaryWindow))
        .id();
    let parent = app.world_mut().spawn(Node::default()).id();
    let frame = frame(app.world_mut(), parent, "Book");
    let child = app
        .world_mut()
        .spawn((Node::default(), ChildOf(frame.controls)))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(frame.owner, bevy::input_focus::FocusCause::Navigated);
    for (key_code, focus, expected) in [
        (KeyCode::F10, frame.owner, Display::Flex),
        (KeyCode::Escape, child, Display::None),
    ] {
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(focus, bevy::input_focus::FocusCause::Navigated);
        app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        app.update();
        assert_eq!(
            app.world().get::<Node>(frame.controls).unwrap().display,
            expected
        );
    }
    assert_eq!(
        app.world().resource::<InputFocus>().get(),
        Some(frame.owner)
    );
}

#[test]
fn books_and_video_embed_actual_viewers_with_hidden_controls() {
    let mut world = world();
    let parent = world.spawn(Node::default()).id();
    for media_type in [
        "application/pdf",
        "application/epub+zip",
        "video/mp4",
        "audio/ogg",
    ] {
        let frame = resource(
            &mut world,
            parent,
            &lince_resources::Resource {
                uri: "https://example.invalid/source".into(),
                title: media_type.into(),
                media_type: media_type.into(),
                ..default()
            },
            false,
        );
        assert_eq!(
            world.get::<Node>(frame.controls).unwrap().display,
            Display::None
        );
        assert_eq!(world.get::<Node>(frame.owner).unwrap().border, UiRect::ZERO);
        if media_type.starts_with("application/") {
            let reader = world.get::<Children>(frame.content).unwrap()[0];
            assert!(
                world
                    .get::<crate::native::document_viewer::DocumentViewer>(reader)
                    .is_some()
            );
        } else {
            assert!(
                world
                    .get::<crate::native::video_castle::Settings>(frame.content)
                    .unwrap()
                    .paused
            );
        }
    }
}
