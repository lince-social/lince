#![cfg(feature = "ui")]

use bevy::{input_focus::tab_navigation::TabIndex, prelude::*};
use lince_interface::{
    controls,
    style::{CursorToken, TextToken},
    theme::Typography,
    tokens::Token::{Accent, Ink},
    wake::WakeSignal,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    thread,
};

#[test]
fn editor_and_container_primitives_work_without_a_desktop_host() {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<Typography>();
    let world = app.world_mut();
    let root = world.spawn(Node::default()).id();
    let column = controls::column(world, root);
    let row = controls::row(world, column);
    let bundle = controls::text_editor("ação 🐈", world.resource::<Typography>(), 3);
    let field = world.spawn((bundle, ChildOf(row))).id();
    assert_eq!(controls::value(world, field).unwrap(), "ação 🐈");
    assert_eq!(world.get::<TextToken>(field).unwrap().0, Ink);
    assert_eq!(world.get::<CursorToken>(field).unwrap().0, Accent);
    assert_eq!(world.get::<TabIndex>(field).unwrap().0, 3);
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
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let signal = WakeSignal::new(move || {
        observed.fetch_add(1, SeqCst);
    });
    thread::spawn(move || signal.ring()).join().unwrap();
    assert_eq!(count.load(SeqCst), 1);
}
