use lince_interface::host::input::{
    InputOutcome, InputResponse, PointerDevice, PointerRequest, PointerSettings, PointerSnapshot,
};
use std::io;

pub(crate) struct PointerControl {
    pub snapshot: PointerSnapshot,
    pending: Option<PointerRequest>,
}

impl PointerControl {
    pub fn new(settings: PointerSettings, available: bool) -> Self {
        Self {
            snapshot: PointerSnapshot {
                revision: 1,
                available,
                settings,
                ..Default::default()
            },
            pending: None,
        }
    }

    pub fn devices(&mut self, devices: Vec<PointerDevice>) {
        if self.snapshot.devices != devices {
            self.snapshot.devices = devices;
            self.snapshot.revision = self.snapshot.revision.saturating_add(1);
        }
    }

    pub fn available(&mut self, available: bool) {
        if self.snapshot.available != available {
            self.snapshot.available = available;
            self.snapshot.revision = self.snapshot.revision.saturating_add(1);
        }
        if !available {
            self.cancel("Physical input configuration is unavailable while this seat is inactive.");
        }
    }

    pub fn request(&mut self, request: PointerRequest, protected: bool) {
        let reason = if request.id == 0 || request.id <= self.snapshot.last_request {
            Some("This pointer request has already been used; retry with a new request.".into())
        } else if protected {
            Some("Pointer settings cannot change while the session is locked.".into())
        } else if !self.snapshot.available {
            Some("Physical pointer settings require a standalone Park session; nested Park uses its parent compositor's devices.".into())
        } else if self.pending.is_some() {
            Some("Another pointer change is waiting for the current interaction to finish.".into())
        } else if request.revision != self.snapshot.revision {
            Some("The pointer devices changed while editing; reload the active settings before saving.".into())
        } else {
            request.settings.validate().err()
        };
        self.snapshot.last_request = self.snapshot.last_request.max(request.id);
        if let Some(reason) = reason {
            self.failed(request.id, reason);
        } else {
            self.snapshot.response = Some(InputResponse {
                request: request.id,
                outcome: InputOutcome::Pending,
            });
            self.pending = Some(request);
        }
    }

    pub fn ready(&mut self, protected: bool, idle: bool) -> Option<PointerRequest> {
        let request = self.pending.as_ref()?;
        let reason = if protected {
            Some("The session locked before pointer settings could be applied.")
        } else if !self.snapshot.available {
            Some("The input seat became inactive before pointer settings could be applied.")
        } else if request.revision != self.snapshot.revision {
            Some("The pointer devices changed before settings could be applied; reload the draft.")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.cancel(reason);
            return None;
        }
        if !idle {
            return None;
        }
        self.pending.take()
    }

    pub fn cancel(&mut self, reason: &str) {
        if let Some(request) = self.pending.take() {
            self.failed(request.id, reason.to_owned());
        }
    }

    pub fn failed(&mut self, request: u64, reason: String) {
        self.snapshot.response = Some(InputResponse {
            request,
            outcome: InputOutcome::Failed(reason),
        });
    }

    pub fn saved(&mut self, request: u64, settings: PointerSettings, warning: Option<String>) {
        self.snapshot.settings = settings;
        self.snapshot.revision = self.snapshot.revision.saturating_add(1);
        self.snapshot.response = Some(InputResponse {
            request,
            outcome: InputOutcome::Saved { warning },
        });
    }
}

pub(crate) struct ApplyFailure {
    pub reason: String,
    pub restoration: Vec<(String, String)>,
}

fn restore(
    before: &[PointerDevice],
    write: &mut impl FnMut(&str, &PointerSettings) -> io::Result<()>,
    reason: String,
) -> ApplyFailure {
    let restoration = before
        .iter()
        .filter_map(|device| {
            write(&device.id, &device.effective)
                .err()
                .map(|error| (device.id.clone(), error.to_string()))
        })
        .collect();
    ApplyFailure {
        reason,
        restoration,
    }
}

pub(crate) fn apply(
    before: &[PointerDevice],
    requested: &PointerSettings,
    mut write: impl FnMut(&str, &PointerSettings) -> io::Result<()>,
    save: impl FnOnce() -> io::Result<Option<String>>,
) -> Result<Option<String>, ApplyFailure> {
    requested.validate().map_err(|reason| ApplyFailure {
        reason,
        restoration: Vec::new(),
    })?;
    for device in before {
        let desired = requested.resolve(&device.capabilities, &device.defaults);
        if let Err(error) = write(&device.id, &desired) {
            return Err(restore(
                before,
                &mut write,
                format!("Could not configure {}: {error}", device.name),
            ));
        }
    }
    save().map_err(|error| {
        restore(
            before,
            &mut write,
            format!("Could not save pointer settings: {error}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::input::PointerCapabilities;
    use std::{cell::RefCell, collections::BTreeMap};

    fn devices() -> Vec<PointerDevice> {
        ["touchpad", "mouse"]
            .into_iter()
            .map(|id| {
                let capabilities = PointerCapabilities {
                    tap: id == "touchpad",
                    acceleration: true,
                    ..Default::default()
                };
                let effective = PointerSettings {
                    tap: capabilities.tap.then_some(false),
                    acceleration: Some(0.0),
                    ..Default::default()
                };
                PointerDevice {
                    id: id.into(),
                    name: id.into(),
                    capabilities,
                    defaults: effective.clone(),
                    effective,
                    error: None,
                }
            })
            .collect()
    }

    #[test]
    fn pointer_requests_wait_for_interactions_and_reject_hotplug_lock_and_unavailable_seats() {
        let mut control = PointerControl::new(PointerSettings::default(), true);
        control.devices(devices());
        let request = |id, revision| PointerRequest {
            id,
            revision,
            settings: PointerSettings {
                tap: Some(true),
                ..Default::default()
            },
        };
        control.request(request(1, control.snapshot.revision), false);
        assert!(control.ready(false, false).is_none());
        assert!(matches!(
            control.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Pending
        ));
        let ready = control.ready(false, true).unwrap();
        assert_eq!(ready.id, 1);
        assert!(control.ready(false, true).is_none());
        control.saved(ready.id, ready.settings, None);
        control.request(request(2, control.snapshot.revision), false);
        control.devices(Vec::new());
        assert!(control.ready(false, true).is_none());
        assert!(matches!(
            control.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Failed(_)
        ));
        control.request(request(3, control.snapshot.revision), false);
        assert!(control.ready(true, true).is_none());
        assert!(matches!(
            control.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Failed(_)
        ));
        control.available(false);
        control.request(request(4, control.snapshot.revision), false);
        assert!(control.ready(false, true).is_none());
        assert!(matches!(
            control.snapshot.response.as_ref().unwrap().outcome,
            InputOutcome::Failed(_)
        ));
    }

    #[test]
    fn pointer_transaction_applies_only_supported_controls_and_preserves_durability_warnings() {
        let before = devices();
        let values = RefCell::new(BTreeMap::new());
        let settings = PointerSettings {
            tap: Some(true),
            acceleration: Some(0.5),
            ..Default::default()
        };
        let result = apply(
            &before,
            &settings,
            |id, value| {
                values.borrow_mut().insert(id.to_owned(), value.clone());
                Ok(())
            },
            || Ok(Some("fixture durability warning".into())),
        )
        .unwrap_or_else(|_| panic!("Expected a saved transaction"));
        assert_eq!(values.borrow()["touchpad"].tap, Some(true));
        assert_eq!(values.borrow()["mouse"].tap, None);
        assert_eq!(values.borrow()["mouse"].acceleration, Some(0.5));
        assert_eq!(result.as_deref(), Some("fixture durability warning"));
    }

    #[test]
    fn apply_and_write_failures_restore_previous_device_values_without_saving_partial_changes() {
        let before = devices();
        let settings = PointerSettings {
            tap: Some(true),
            acceleration: Some(0.5),
            ..Default::default()
        };
        for apply_failure in [false, true] {
            let values = RefCell::new(BTreeMap::new());
            let failure = apply(
                &before,
                &settings,
                |id, value| {
                    values.borrow_mut().insert(id.to_owned(), value.clone());
                    if apply_failure && id == "mouse" && value.acceleration == Some(0.5) {
                        Err(io::Error::other("fixture apply failure"))
                    } else {
                        Ok(())
                    }
                },
                || {
                    assert!(!apply_failure);
                    Err(io::Error::other("fixture save failure"))
                },
            );
            assert!(failure.is_err());
            assert!(failure.err().unwrap().restoration.is_empty());
            for device in &before {
                assert_eq!(values.borrow()[&device.id], device.effective);
            }
        }
    }

    #[test]
    fn failed_restoration_is_reported_and_invalid_preferences_never_reach_devices() {
        let before = devices();
        let settings = PointerSettings {
            acceleration: Some(0.5),
            ..Default::default()
        };
        let failure = apply(
            &before,
            &settings,
            |id, value| {
                if id == "mouse" && value.acceleration == Some(0.0) {
                    Err(io::Error::other("fixture restore failure"))
                } else {
                    Ok(())
                }
            },
            || Err(io::Error::other("fixture save failure")),
        )
        .err()
        .unwrap();
        assert_eq!(
            failure.restoration,
            vec![("mouse".into(), "fixture restore failure".into())]
        );
        assert!(failure.reason.contains("fixture save failure"));
        let invalid = PointerSettings {
            acceleration: Some(f64::NAN),
            ..Default::default()
        };
        assert!(
            apply(
                &before,
                &invalid,
                |_, _| panic!("Invalid preferences must not reach a device"),
                || panic!("Invalid preferences must not be saved")
            )
            .is_err()
        );
    }
}
