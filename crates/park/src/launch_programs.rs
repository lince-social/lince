use lince_interface::host::{
    input::{InputOutcome, InputResponse},
    launch::{Programs, ProgramsSnapshot, ProgramsRequest},
};
use std::io;

#[derive(Default)]
pub(crate) struct LaunchPrograms {
    pub snapshot: ProgramsSnapshot,
}

impl LaunchPrograms {
    pub fn configure(&mut self, settings: Programs) {
        if self.snapshot.settings != settings {
            self.snapshot.revision = self.snapshot.revision.saturating_add(1);
            self.snapshot.settings = settings;
        }
        self.snapshot.available = true;
    }

    pub fn request(&mut self, request: ProgramsRequest, locked: bool,
        save: impl FnOnce(&Programs, &Programs) -> io::Result<Option<String>>) {
        if request.id <= self.snapshot.last_request { return; }
        self.snapshot.last_request = request.id;
        let result = (|| {
            if locked { return Err("Unlock Park before changing session programs.".to_owned()); }
            if !self.snapshot.available { return Err("Session program settings are unavailable in this desktop.".to_owned()); }
            if request.revision != self.snapshot.revision { return Err("Session programs changed; reload before saving this draft.".to_owned()); }
            request.settings.validate()?;
            save(&request.settings, &self.snapshot.settings).map_err(|error| error.to_string())
        })();
        let outcome = match result {
            Ok(warning) => { self.configure(request.settings); InputOutcome::Saved { warning } },
            Err(error) => InputOutcome::Failed(error),
        };
        self.snapshot.response = Some(InputResponse { request: request.id, outcome });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::launch::Description;

    #[test]
    fn session_program_saves_are_bounded_once_only_and_reject_locked_stale_or_failed_changes() {
        let mut service = LaunchPrograms::default();
        service.configure(Programs::default());
        let settings = Programs { terminal: Some(Description { program: "fixture-terminal".into(), arguments: vec!["literal".into()], directory: None, capabilities: vec![] }), ..Default::default() };
        service.request(ProgramsRequest { id: 1, revision: 0, settings: settings.clone() }, true, |_, _| panic!("Locked settings must not reach persistence"));
        assert!(service.snapshot.settings.terminal.is_none());
        service.request(ProgramsRequest { id: 2, revision: 0, settings: settings.clone() }, false, |_, _| Err(io::Error::other("Disk unavailable")));
        assert!(service.snapshot.settings.terminal.is_none());
        service.request(ProgramsRequest { id: 3, revision: 0, settings: settings.clone() }, false, |candidate, expected| {
            assert_eq!(candidate, &settings);
            assert_eq!(expected, &Programs::default());
            Ok(None)
        });
        assert_eq!(service.snapshot.settings, settings);
        assert_eq!(service.snapshot.revision, 1);
        service.request(ProgramsRequest { id: 4, revision: 0, settings: Programs::default() }, false, |_, _| panic!("Stale changes must not reach persistence"));
        assert_eq!(service.snapshot.settings, settings);
        service.request(ProgramsRequest { id: 3, revision: 1, settings: Programs::default() }, false, |_, _| panic!("Replayed changes must not reach persistence"));
        assert_eq!(service.snapshot.settings, settings);
    }
}
