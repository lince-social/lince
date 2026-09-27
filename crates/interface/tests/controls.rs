#![cfg(feature = "ui")]

use bevy::prelude::*;
use lince_interface::{controls, style, theme, tokens, wake::WakeSignal};

#[test]
fn editor_and_container_primitives_work_without_a_desktop_host() {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<theme::Typography>();
    let world = app.world_mut();
    let root = world.spawn(Node::default()).id();
    let column = controls::column(world, root);
    let row = controls::row(world, column);
    let bundle = controls::text_editor("ação 🐈", world.resource::<theme::Typography>(), 3);
    let field = world.spawn((bundle, ChildOf(row))).id();
    assert_eq!(controls::value(world, field).unwrap(), "ação 🐈");
    assert_eq!(
        world.get::<style::TextToken>(field).unwrap().0,
        tokens::Token::Ink
    );
    assert_eq!(
        world.get::<style::CursorToken>(field).unwrap().0,
        tokens::Token::Accent
    );
    assert_eq!(
        world
            .get::<bevy::input_focus::tab_navigation::TabIndex>(field)
            .unwrap()
            .0,
        3
    );
    let label = world.spawn((Text::new(""), ChildOf(row))).id();
    controls::status(world, label, "Saved");
    assert_eq!(world.get::<Text>(label).unwrap().0, "Saved");
    controls::clear(world, column);
    assert!(world.get_entity(field).is_err());
    assert!(world.get_entity(label).is_err());
    assert!(world.get_entity(row).is_err());
    assert!(world.get_entity(column).is_ok());
}

#[test]
fn a_worker_can_wake_the_host_without_owning_its_event_loop() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let signal = WakeSignal::new(move || {
        observed.fetch_add(1, Ordering::SeqCst);
    });
    std::thread::spawn(move || signal.ring()).join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}
