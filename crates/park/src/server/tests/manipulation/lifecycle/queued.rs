use super::*;

fn workspaces(fixture: &mut Fixture) -> (u64, u64, u64, Arc<crate::runtime::Completed>) {
    let workspaces = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .workspaces
        .clone();
    let original = workspaces
        .entries
        .iter()
        .find(|workspace| workspace.active)
        .unwrap()
        .id;
    let serial = fixture
        .desktop
        .worker
        .input(Input::CreateWorkspace {
            owner: workspaces.owner,
        })
        .unwrap();
    advance(&mut fixture.desktop, serial);
    let other = fixture
        .desktop
        .current
        .as_ref()
        .unwrap()
        .frame
        .input
        .workspaces
        .entries
        .iter()
        .find(|workspace| workspace.active)
        .unwrap()
        .id;
    assert_ne!(original, other);
    let previous = fixture.desktop.current.clone().unwrap();
    fixture.workspace(workspaces.owner, original);
    fixture.configure();
    (workspaces.owner, original, other, previous)
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_queued_client_resize_cannot_revive_after_leaving_and_returning_to_its_workspace() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&runtime, directory.path());
    let (owner, original, other, _) = workspaces(&mut fixture);
    let initial = mapping(&fixture.desktop, fixture.id);
    let button = fixture.begin(true, true);
    fixture.workspace(owner, other);
    fixture.workspace(owner, original);
    fixture.desktop.dispatch().unwrap();
    fixture.cancelled(initial);
    fixture.release_and_check_normal_input(button, true);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn an_older_frame_from_another_workspace_cannot_retire_a_valid_new_client_press() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&runtime, directory.path());
    let (_, _, _, previous) = workspaces(&mut fixture);
    let initial = mapping(&fixture.desktop, fixture.id);
    let button = fixture.begin(true, true);
    let current = fixture.desktop.current.clone().unwrap();
    assert!(previous.input_serial < current.input_serial);
    fixture.desktop.adopt(previous);
    fixture.desktop.adopt(current);
    fixture.desktop.dispatch().unwrap();
    fixture.flush();
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .input
            .manipulation
            .is_some()
    );
    assert!(
        fixture
            .desktop
            .current
            .as_ref()
            .unwrap()
            .frame
            .windows
            .iter()
            .find(|window| window.surface == fixture.id)
            .unwrap()
            .state
            .resizing
    );
    fixture.desktop.key(1, true, 15).unwrap();
    fixture.desktop.key(1, false, 16).unwrap();
    fixture.cancelled(initial);
    fixture.release_and_check_normal_input(button, true);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn retiring_an_unclaimed_press_context_keeps_an_ordinary_client_press_and_release_paired() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&runtime, directory.path());
    let (owner, original, other, _) = workspaces(&mut fixture);
    let initial = mapping(&fixture.desktop, fixture.id);
    fixture.desktop.move_pointer(center(initial), 1).unwrap();
    fixture.desktop.button(272, true, 2).unwrap();
    fixture.sync();
    assert!(!fixture.desktop.server.manipulation_grabbed());
    assert_eq!(fixture.harness.client.pointer_buttons, vec![(272, true)]);
    fixture.workspace(owner, other);
    fixture.workspace(owner, original);
    fixture.desktop.button(272, false, 3).unwrap();
    fixture.sync();
    assert_eq!(
        fixture.harness.client.pointer_buttons,
        vec![(272, true), (272, false)]
    );
    assert!(fixture.desktop.buttons.is_empty());
    assert!(fixture.desktop.server.state.buttons.is_empty());
    fixture.cancelled(initial);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn a_delayed_client_claim_cannot_revive_after_restoring_the_original_output_geometry() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    let mut fixture = Fixture::new(&runtime, directory.path());
    let initial = mapping(&fixture.desktop, fixture.id);
    let button = fixture.begin(true, true);
    let mut claims = fixture.desktop.server.manipulations();
    assert_eq!(claims.len(), 1);
    let claim = claims.pop().unwrap();
    fixture.output_size([800, 600]);
    fixture.output_size([640, 480]);
    fixture
        .desktop
        .begin_application_manipulation(claim)
        .unwrap();
    fixture.cancelled(initial);
    fixture.release_and_check_normal_input(button, true);
    fixture.desktop.worker.stop().unwrap();
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn delayed_claims_for_unmapped_or_destroyed_targets_do_not_bind_to_a_returning_window() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    for destroy in [false, true] {
        let mut fixture = Fixture::connected(
            &runtime,
            &directory.path().join(format!("target-{destroy}")),
        );
        let initial = mapping(&fixture.desktop, fixture.id);
        let old_id = fixture.id;
        let button = fixture.begin(true, true);
        let mut claims = fixture.desktop.server.manipulations();
        assert_eq!(claims.len(), 1);
        let claim = claims.pop().unwrap();
        if destroy {
            fixture.window.role.destroy();
            fixture.window.xdg.destroy();
            fixture.window.surface.destroy();
        } else {
            fixture.window.surface.attach(None, 0, 0);
            fixture.window.surface.commit();
        }
        fixture.sync();
        fixture.scene();
        if destroy {
            fixture.replace_window();
            assert_ne!(fixture.id, old_id);
        } else {
            fixture.window.surface.commit();
            fixture.sync();
            let buffer = opaque_buffer(&fixture.harness, [320, 240]);
            fixture.window.surface.attach(Some(&buffer), 0, 0);
            fixture.window.surface.commit();
            fixture.sync();
        }
        fixture.scene();
        let serial = fixture
            .desktop
            .worker
            .input(Input::Window(
                fixture.id,
                WindowRequest {
                    maximized: Some(false),
                    ..Default::default()
                },
            ))
            .unwrap();
        advance(&mut fixture.desktop, serial);
        fixture.configure();
        let restored = mapping(&fixture.desktop, fixture.id);
        if !destroy {
            assert_eq!(restored, initial);
        }
        fixture
            .desktop
            .begin_application_manipulation(claim)
            .unwrap();
        fixture.cancelled(restored);
        fixture.release_and_check_normal_input(button, true);
        fixture.desktop.worker.stop().unwrap();
    }
}
