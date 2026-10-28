use lince_interface::host::{
    external::{Request, Settings, Snapshot},
    input::{InputOutcome, InputResponse},
};
use std::io;

#[derive(Default)]
pub(crate) struct ExternalCommands {
    pub snapshot: Snapshot,
}

impl ExternalCommands {
    pub fn binding_allowed(
        &self,
        action: lince_interface::host::external::Action,
        locked: bool,
        inhibited: bool,
        modal: bool,
    ) -> bool {
        if locked {
            self.snapshot.settings.command(action, true).is_ok()
        } else {
            !inhibited && !modal
        }
    }
    pub fn configure(&mut self, settings: Settings) -> bool {
        let changed = self.snapshot.settings != settings;
        if changed {
            self.snapshot.revision = self.snapshot.revision.saturating_add(1);
            self.snapshot.settings = settings;
        }
        self.snapshot.available = true;
        self.snapshot.message = None;
        changed
    }

    pub fn request(
        &mut self,
        request: Request,
        locked: bool,
        save: impl FnOnce(&Settings, &Settings) -> io::Result<Option<String>>,
    ) -> bool {
        if request.id <= self.snapshot.last_request {
            return false;
        }
        self.snapshot.last_request = request.id;
        let result = (|| {
            if locked {
                return Err("Unlock Park before changing external commands.".to_owned());
            }
            if !self.snapshot.available {
                return Err("The local Park command configuration is unavailable.".to_owned());
            }
            if request.revision != self.snapshot.revision {
                return Err("External commands changed; cancel to reload before saving.".to_owned());
            }
            request.settings.validate()?;
            save(&request.settings, &self.snapshot.settings).map_err(|error| error.to_string())
        })();
        let (outcome, changed) = match result {
            Ok(warning) => {
                let changed = self.configure(request.settings);
                (InputOutcome::Saved { warning }, changed)
            }
            Err(error) => (InputOutcome::Failed(error), false),
        };
        self.snapshot.response = Some(InputResponse {
            request: request.id,
            outcome,
        });
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::{
        external::{Action, Command},
        launch::Description,
    };

    fn settings() -> Settings {
        Settings(std::collections::BTreeMap::from([(
            Action::VolumeUp,
            Command {
                command: Description {
                    program: "volume-tool".into(),
                    arguments: vec!["0.05+".into()],
                    directory: None,
                    capabilities: Vec::new(),
                },
                allow_when_locked: true,
            },
        )]))
    }

    #[test]
    fn settings_requests_commit_once_and_reject_locked_stale_and_failed_saves() {
        let mut state = ExternalCommands::default();
        state.configure(Settings::default());
        let request = Request {
            id: 1,
            revision: 0,
            settings: settings(),
        };
        assert!(!state.request(request.clone(), true, |_, _| panic!("locked save")));
        assert!(state.snapshot.settings.0.is_empty());
        assert!(matches!(
            state.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Failed(_)
        ));
        assert!(!state.request(
            Request {
                id: 2,
                ..request.clone()
            },
            false,
            |_, _| Err(io::Error::other("disk failed"))
        ));
        assert!(state.snapshot.settings.0.is_empty());
        assert!(state.request(
            Request {
                id: 3,
                ..request.clone()
            },
            false,
            |candidate, expected| {
                assert!(expected.0.is_empty());
                assert_eq!(candidate, &settings());
                Ok(Some("directory sync warning".into()))
            }
        ));
        assert_eq!(state.snapshot.revision, 1);
        assert_eq!(state.snapshot.settings, settings());
        assert!(matches!(
            state.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Saved { warning: Some(_) }
        ));
        assert!(
            !state.request(Request { id: 4, ..request }, false, |_, _| panic!(
                "stale save"
            ))
        );
        assert_eq!(state.snapshot.settings, settings());
        let response = state.snapshot.response.clone();
        assert!(!state.request(
            Request {
                id: 2,
                revision: 1,
                settings: Settings::default()
            },
            false,
            |_, _| panic!("replayed save")
        ));
        assert_eq!(state.snapshot.response, response);
    }

    #[test]
    fn file_transactions_merge_other_settings_and_preserve_literal_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        let original = serde_json::json!({"keyboard":{"layout":"br"}, "pointer":{"tap":true}, "terminal":{"program":"a terminal","arguments":[]}, "outputs":{}, "external":{}});
        std::fs::write(&path, serde_json::to_vec(&original).unwrap()).unwrap();
        let mut saved = settings();
        saved
            .0
            .get_mut(&Action::VolumeUp)
            .unwrap()
            .command
            .arguments = vec![String::new(), "$(printf nope)".into(), "two words".into()];
        crate::commands::Commands::save_external(&path, &saved, &Settings::default()).unwrap();
        let loaded = crate::commands::Commands::load(&path).unwrap();
        assert_eq!(loaded.external, saved);
        assert_eq!(loaded.keyboard.layout, "br");
        assert_eq!(loaded.pointer.tap, Some(true));
        assert_eq!(loaded.terminal.unwrap().program, "a terminal");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let bytes = std::fs::read(&path).unwrap();
        assert!(
            crate::commands::Commands::save_external(
                &path,
                &Settings::default(),
                &Settings::default()
            )
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        saved
            .0
            .get_mut(&Action::VolumeUp)
            .unwrap()
            .command
            .capabilities
            .push(lince_interface::host::launch::Capability::Capture);
        assert!(crate::commands::Commands::save_external(&path, &saved, &loaded.external).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn binding_policy_respects_capture_inhibition_and_explicit_lock_permission() {
        let mut state = ExternalCommands::default();
        for action in Action::ALL {
            assert!(state.binding_allowed(action, false, false, false));
            assert!(!state.binding_allowed(action, false, true, false));
            assert!(!state.binding_allowed(action, false, false, true));
            assert!(!state.binding_allowed(action, true, false, false));
            let mut command = settings().0[&Action::VolumeUp].clone();
            command.allow_when_locked = action.lockable();
            state.snapshot.settings.0.insert(action, command);
            assert_eq!(
                state.binding_allowed(action, true, false, false),
                action.lockable()
            );
            assert_eq!(
                state.binding_allowed(action, true, true, true),
                action.lockable()
            );
        }
        state
            .snapshot
            .settings
            .0
            .get_mut(&Action::VolumeUp)
            .unwrap()
            .command
            .capabilities
            .push(lince_interface::host::launch::Capability::Capture);
        assert!(!state.binding_allowed(Action::VolumeUp, true, false, false));
    }

    #[test]
    fn the_existing_spawn_path_receives_literal_argv_and_reports_missing_programs() {
        let mut server = crate::server::Server::new([640, 480]).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("wayland-test");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let description = Description {
            program: "bash".into(),
            arguments: vec![
                "-c".into(),
                "test \"$1\" = '$(printf literal)' && test \"$2\" = ''".into(),
                "fixture".into(),
                "$(printf literal)".into(),
                String::new(),
            ],
            directory: Some(directory.path().into()),
            capabilities: Vec::new(),
        };
        assert!(
            server
                .spawn_helper(&description, &socket)
                .unwrap()
                .wait()
                .unwrap()
                .success()
        );
        let missing = Description {
            program: "/missing/lince-test-provider".into(),
            ..description
        };
        assert!(server.spawn_helper(&missing, &socket).is_err());
    }
}
