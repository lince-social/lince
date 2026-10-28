use super::super::super::recovery::inspect;
use super::*;
use bevy::ecs::entity_disabling::Disabled;
use ui::actions::Action;
use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_v2::ZwpInputMethodV2;

#[path = "handoff/controls.rs"]
mod controls;

const TEXT: &str = "Replacement native editoré";

fn keys(fixture: &mut Fixture, events: &[(u32, bool)]) {
    for event in events {
        super::keys(fixture, std::slice::from_ref(event));
        controls::frame(fixture, "key");
    }
}

fn preedit(
    fixture: &mut Fixture,
    provider: &mut Harness,
    method: &ZwpInputMethodV2,
    owner: u64,
    text: &str,
) -> crate::native_input::Editor {
    let serial = provider.client.input_serial;
    method.set_preedit_string("界".into(), 0, 3);
    method.commit(serial);
    wait(fixture, provider, owner, text, true, serial);
    endpoint(fixture)
}

fn retired(fixture: &mut Fixture, provider: &mut Harness, owner: u64) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        fixture.desktop.dispatch().unwrap();
        fixture.flush();
        let actual = value(fixture, owner);
        client_sync(&mut fixture.desktop, provider);
        client_sync(&mut fixture.desktop, provider);
        if actual == (TEXT.into(), false)
            && fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .editor
                .is_none()
            && !provider.client.input_active
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "The old editor remained active: {actual:?}"
        );
    }
}

fn restore(fixture: &mut Fixture, provider: &mut Harness, id: [u8; 16], owner: u64) {
    assert_eq!(focus(fixture, id).owner, owner);
    keys(
        fixture,
        &[(29, true), (107, true), (107, false), (29, false)],
    );
    let serial = provider.client.input_serial;
    wait(fixture, provider, owner, TEXT, false, serial);
}

fn protocol_stale(
    fixture: &mut Fixture,
    provider: &mut Harness,
    method: &ZwpInputMethodV2,
    serial: u32,
) {
    method.commit_string("stale".into());
    method.commit(serial);
    client_sync(&mut fixture.desktop, provider);
    fixture.flush();
    client_sync(&mut fixture.desktop, provider);
    fixture.flush();
}

pub(super) fn exercise(
    fixture: &mut Fixture,
    provider: &mut Harness,
    method: &ZwpInputMethodV2,
    id: [u8; 16],
    owner: u64,
    note: Entity,
) {
    let old = preedit(fixture, provider, method, owner, TEXT);
    let serial = provider.client.input_serial;
    fixture.activate(0);
    fixture.configure();
    stale(fixture, old);
    retired(fixture, provider, owner);
    assert_eq!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .geometry
            .as_ref()
            .unwrap()
            .focused,
        Some(ItemId::Application(fixture.clients[0].1))
    );
    assert_eq!(
        fixture
            .desktop
            .server
            .state
            .seat
            .get_keyboard()
            .unwrap()
            .current_focus(),
        fixture
            .desktop
            .server
            .windows()
            .into_iter()
            .find(|window| window.id == fixture.clients[0].1)
            .map(|window| window.surface)
    );
    let keys_before = fixture.harness.client.keys.len();
    keys(fixture, &[(45, true), (45, false)]);
    fixture.sync();
    assert_eq!(
        &fixture.harness.client.keys[keys_before..],
        &[(45, true), (45, false)]
    );
    protocol_stale(fixture, provider, method, serial);
    assert_eq!(value(fixture, owner), (TEXT.into(), false));
    restore(fixture, provider, id, owner);

    let old = preedit(fixture, provider, method, owner, TEXT);
    let serial = provider.client.input_serial;
    inspect(fixture, move |canvas| {
        canvas.world_mut().entity_mut(note).insert(Disabled);
    });
    stale(fixture, old);
    retired(fixture, provider, owner);
    protocol_stale(fixture, provider, method, serial);
    assert_eq!(value(fixture, owner), (TEXT.into(), false));
    inspect(fixture, move |canvas| {
        canvas.world_mut().entity_mut(note).remove::<Disabled>();
    });
    restore(fixture, provider, id, owner);

    let client_keys = fixture.harness.client.keys.clone();
    let old = preedit(fixture, provider, method, owner, TEXT);
    let old_serial = provider.client.input_serial;
    keys(
        fixture,
        &[(125, true), (25, true), (25, false), (125, false)],
    );
    let modal = endpoint(fixture);
    assert_ne!(modal.owner, owner);
    assert!(fixture.desktop.current.as_ref().unwrap().frame.input.modal);
    let modal_owner = modal.owner;
    inspect(fixture, move |canvas| {
        let world = canvas.world_mut();
        let field = field(world, modal_owner);
        let camera = world
            .get::<bevy::ui::ComputedUiTargetCamera>(field)
            .unwrap()
            .get()
            .unwrap();
        assert!(
            world
                .get::<ui::topology::presentation::VisualOwner>(camera)
                .is_none()
        );
        assert!(matches!(
            world.get::<bevy::camera::RenderTarget>(camera),
            Some(bevy::camera::RenderTarget::Image(_))
        ));
    });
    stale(fixture, old);
    protocol_stale(fixture, provider, method, old_serial);
    assert_eq!(value(fixture, owner), (TEXT.into(), false));
    assert_eq!(value(fixture, modal_owner), (String::new(), false));
    wait(fixture, provider, modal_owner, "", false, old_serial);
    preedit(fixture, provider, method, modal_owner, "");
    let serial = provider.client.input_serial;
    method.commit_string("é".into());
    method.commit(serial);
    wait(fixture, provider, modal_owner, "é", false, serial);
    let modal = endpoint(fixture);
    keys(fixture, &[(1, true), (1, false)]);
    stale(fixture, modal);
    assert!(!fixture.desktop.current.as_ref().unwrap().frame.input.modal);
    assert_eq!(value(fixture, owner), (TEXT.into(), false));
    restore(fixture, provider, id, owner);

    inspect(fixture, |canvas| {
        let world = canvas.world_mut();
        let root = world
            .query_filtered::<Entity, With<ui::workspace::Workspaces>>()
            .single(world)
            .unwrap();
        ui::edit_mode::EditAction::Open.apply(world, root);
        ui::edit_mode::EditAction::Shortcuts.apply(world, root);
    });
    controls::panel_visible(fixture);
    controls::name(fixture, "shortcut.summary.view.overview");
    assert_eq!(endpoint(fixture).owner, owner);
    let old = preedit(fixture, provider, method, owner, TEXT);
    let serial = provider.client.input_serial;
    let point = controls::name(fixture, "shortcut.summary.view.overview");
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_modal_triggers
            .iter()
            .any(|bounds| bounds.contains(point))
    );
    controls::frame(fixture, "before-capture");
    controls::click(fixture, point);
    controls::modal_usable(fixture);
    controls::frame(fixture, "after-capture");
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_capture
    );
    stale(fixture, old);
    retired(fixture, provider, owner);
    protocol_stale(fixture, provider, method, serial);
    controls::filter_unchanged(fixture);
    keys(fixture, &[(60, true), (60, false), (28, true), (28, false)]);
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_capture
    );
    inspect(fixture, |canvas| {
        let world = canvas.world_mut();
        assert!(world.query::<&Text>().iter(world).any(|text| {
            text.0
                .lines()
                .any(|line| line == "Desktop shortcuts need Ctrl, Alt or Super.")
        }));
    });
    keys(
        fixture,
        &[
            (125, true),
            (60, true),
            (60, false),
            (125, false),
            (28, true),
            (28, false),
        ],
    );
    assert!(
        !fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_capture
    );
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_modal
    );
    inspect(fixture, |canvas| {
        let world = canvas.world_mut();
        let command = world
            .query::<(Entity, &Name)>()
            .iter(world)
            .find(|(_, name)| name.as_str() == "view.overview")
            .unwrap()
            .0;
        let changed = world
            .query::<(Entity, &Name, &Children)>()
            .iter(world)
            .filter(|(_, name, _)| name.as_str() == "shortcut.capture")
            .any(|(entity, _, children)| {
                let mut ancestor = Some(entity);
                for _ in 0..ui::layout::MAX_DEPTH {
                    let Some(entity) = ancestor else { break };
                    if entity == command {
                        return children.iter().any(|child| {
                            world
                                .get::<Text>(child)
                                .is_some_and(|text| text.0 == "Mod+F2")
                        });
                    }
                    ancestor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                false
            });
        assert!(
            changed,
            "The actual capture did not stage Mod+F2 for Overview"
        );
    });
    controls::modal_usable(fixture);
    controls::frame(fixture, "validated");
    let point = controls::caption(fixture, "Cancel draft");
    controls::click(fixture, point);
    assert!(
        !fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .keyboard_modal
    );
    keys(
        fixture,
        &[(125, true), (18, true), (18, false), (125, false)],
    );
    restore(fixture, provider, id, owner);
    preedit(fixture, provider, method, owner, TEXT);
    let serial = provider.client.input_serial;
    method.commit_string("é".into());
    method.commit(serial);
    wait(fixture, provider, owner, &format!("{TEXT}é"), false, serial);
    fixture.sync();
    assert_eq!(fixture.harness.client.keys, client_keys);
}
