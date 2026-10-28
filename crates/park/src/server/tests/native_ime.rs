use super::*;
use crate::native_input::{Editor, client::Event};

fn synchronize(harness: &mut Harness) { for _ in 0..8 { harness.sync(); } }

#[test]
fn native_caret_geometry_keeps_the_provider_serial_and_moves_existing_and_new_popups() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::InputMethod]));
    let method = harness.client.input_method.as_ref().unwrap().get_input_method(harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), ());
    synchronize(&mut harness);
    let context = Editor { owner: 5, revision: 7, projection: 9, from_ime: false, selection: [1, 1], surrounding: "aé界".into(),
        cursor: 1, anchor: 1, sensitive: false, multiline: false, caret: [20, 20, 2, 20] };
    harness.server.native_editor(Some(context.clone()));
    harness.server.native_focus();
    synchronize(&mut harness);
    assert!(harness.client.input_active);
    let serial = harness.client.input_serial;
    let surrounding = harness.client.input_surrounding.len();
    let causes = harness.client.input_causes.len();
    let mut moved = context.clone();
    for step in 0..120 {
        moved.caret = [40 + step, 60 + step, 2, 20];
        harness.server.native_editor(Some(moved.clone()));
        synchronize(&mut harness);
        assert_eq!(harness.client.input_serial, serial);
        assert_eq!(harness.client.input_surrounding.len(), surrounding);
        assert_eq!(harness.client.input_causes.len(), causes);
    }
    let surface = harness.client.compositor.as_ref().unwrap().create_surface(&harness.queue.handle(), ());
    let popup = method.get_input_popup_surface(&surface, &harness.queue.handle(), ());
    let buffer = harness.buffer([64, 32]);
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    synchronize(&mut harness);
    let snapshots = harness.server.native_popups();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].origin, [moved.caret[0], moved.caret[1] + moved.caret[3]]);
    moved.caret = [310, 240, 3, 28];
    harness.server.native_editor(Some(moved));
    synchronize(&mut harness);
    assert_eq!(harness.server.native_popups()[0].origin, [310, 268]);
    assert_eq!(harness.client.input_serial, serial);
    method.commit_string("é".into());
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    synchronize(&mut harness);
    let edits = harness.server.native_events().into_iter().filter_map(|event| match event { Event::Edit(edit) => Some(edit), _ => None }).collect::<Vec<_>>();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].context.same_text(&context));
    assert_eq!(edits[0].commit.as_deref(), Some("é"));
    assert_eq!(edits[0].preedit, Some(("界".into(), Some((0, 3)))));
    let replacement = Editor { projection: context.projection + 1, ..context };
    harness.server.native_editor(Some(replacement.clone()));
    synchronize(&mut harness);
    assert_ne!(harness.client.input_serial, serial);
    method.commit_string("stale".into());
    method.commit(serial);
    synchronize(&mut harness);
    assert!(harness.server.native_events().is_empty());
    method.commit_string("fresh".into());
    method.commit(harness.client.input_serial);
    synchronize(&mut harness);
    let events = harness.server.native_events();
    assert!(matches!(events.as_slice(), [Event::Edit(edit)] if edit.context.same_text(&replacement) && edit.commit.as_deref() == Some("fresh")));
    harness.server.native_editor(None);
    assert!(harness.server.native_popups().is_empty());
    synchronize(&mut harness);
    assert!(!harness.client.input_active);
    popup.destroy();
    surface.destroy();
    method.destroy();
    synchronize(&mut harness);
}
