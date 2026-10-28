use super::*;

impl Fixture {
    pub(super) fn scene(&mut self) {
        self.desktop.dispatch().unwrap();
        let expected: Vec<_> = self
            .desktop
            .server
            .windows()
            .iter()
            .map(|window| (window.id, window.mapped))
            .collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.flush();
            let actual: Vec<_> = self
                .desktop
                .current
                .as_ref()
                .unwrap()
                .windows
                .iter()
                .map(|window| (window.id, window.mapped))
                .collect();
            if actual == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "The worker did not adopt the current client lifecycle"
            );
            self.desktop.dispatch().unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    pub(super) fn replace_window(&mut self) {
        std::mem::swap(&mut self.desktop.server, &mut self.harness.server);
        self.window = self.harness.window();
        let buffer = opaque_buffer(&self.harness, [320, 240]);
        self.window.surface.attach(Some(&buffer), 0, 0);
        self.window.surface.commit();
        self.harness.sync();
        std::mem::swap(&mut self.desktop.server, &mut self.harness.server);
        self.id = self
            .desktop
            .server
            .windows()
            .iter()
            .find(|window| window.mapped)
            .unwrap()
            .id;
    }

    pub(super) fn workspace(&mut self, owner: u64, id: u64) {
        let serial = self
            .desktop
            .worker
            .input(Input::Workspace { owner, id })
            .unwrap();
        advance(&mut self.desktop, serial);
        assert!(
            self.desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .workspaces
                .entries
                .iter()
                .any(|workspace| workspace.id == id && workspace.active)
        );
    }

    pub(super) fn output_size(&mut self, size: [i32; 2]) {
        self.desktop.server.resize(size).unwrap();
        self.desktop.dispatch().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.flush();
            if self
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .geometry
                .as_ref()
                .unwrap()
                .outputs[0]
                .physical_size
                == size.map(|value| value as u32)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "The worker did not adopt the current output geometry"
            );
            self.desktop.dispatch().unwrap();
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn unmap_remap_and_destroy_replace_do_not_resume_a_drag_or_redirect_its_release() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    for client in [false, true] {
        for destroy in [false, true] {
            eprintln!("client={client}, destroy={destroy}");
            let mut fixture = Fixture::connected(
                &runtime,
                &directory.path().join(format!("{client}-{destroy}")),
            );
            let initial = mapping(&fixture.desktop, fixture.id);
            let old_id = fixture.id;
            let button = fixture.begin(client, false);
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
            assert!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .manipulation
                    .is_none()
            );
            assert!(
                !fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .geometry
                    .as_ref()
                    .unwrap()
                    .items
                    .iter()
                    .any(|item| item.id == lince_interface::host::ItemId::Application(old_id))
            );
            if destroy {
                fixture.replace_window();
                assert_ne!(fixture.id, old_id);
            } else {
                let configured = fixture.harness.client.configures.len();
                fixture.window.surface.commit();
                fixture.sync();
                assert!(fixture.harness.client.configures.len() > configured);
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
            assert!(
                fixture
                    .desktop
                    .current
                    .as_ref()
                    .unwrap()
                    .frame
                    .input
                    .manipulation
                    .is_none()
            );
            fixture.configure();
            let restored = mapping(&fixture.desktop, fixture.id);
            if !destroy {
                assert_eq!(restored, initial);
            }
            fixture.desktop.move_pointer(center(restored), 19).unwrap();
            fixture.release_and_check_normal_input(button, client);
            fixture.flush();
            assert_eq!(mapping(&fixture.desktop, fixture.id), restored);
            fixture.desktop.worker.stop().unwrap();
        }
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn changing_workspace_cancels_both_resize_entry_paths_and_return_restores_committed_geometry() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    for client in [false, true] {
        let mut fixture = Fixture::new(
            &runtime,
            &directory.path().join(format!("workspace-{client}")),
        );
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
        fixture.workspace(workspaces.owner, original);
        fixture.configure();
        let initial = mapping(&fixture.desktop, fixture.id);
        let button = fixture.begin(client, false);
        fixture.workspace(workspaces.owner, other);
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        fixture.workspace(workspaces.owner, original);
        fixture.cancelled(initial);
        fixture.release_and_check_normal_input(button, client);
        fixture.desktop.worker.stop().unwrap();
    }
}

#[test]
#[ignore = "requires hardware Vulkan and EGL"]
fn output_geometry_changes_cancel_both_resize_entry_paths_without_committing_the_preview() {
    let runtime = runtime();
    let directory = tempfile::tempdir().unwrap();
    for client in [false, true] {
        let mut fixture =
            Fixture::new(&runtime, &directory.path().join(format!("output-{client}")));
        let initial = mapping(&fixture.desktop, fixture.id);
        let button = fixture.begin(client, false);
        fixture.output_size([800, 600]);
        assert!(
            fixture
                .desktop
                .current
                .as_ref()
                .unwrap()
                .frame
                .input
                .manipulation
                .is_none()
        );
        assert!(
            !fixture
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
        fixture.output_size([640, 480]);
        fixture.cancelled(initial);
        fixture.release_and_check_normal_input(button, client);
        fixture.desktop.worker.stop().unwrap();
    }
}
