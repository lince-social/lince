use crate::input_settings::{self, PointerControl};
use lince_interface::host::input::{
    PointerCapabilities, PointerDevice, PointerRequest, PointerSettings, PointerSnapshot,
};
use smithay::reexports::input::{Device, DeviceCapability};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

struct Entry {
    device: Device,
    error: Option<String>,
    generation: u64,
    key: Option<String>,
    absolute_error: Option<String>,
    tools: Vec<tablet::Tool>,
}

pub(super) mod absolute;
pub(super) mod tablet;

impl Entry {
    fn capabilities(&self) -> PointerCapabilities {
        PointerCapabilities {
            tap: self.device.config_tap_finger_count() > 0,
            natural_scroll: self.device.config_scroll_has_natural_scroll(),
            left_handed: self.device.config_left_handed_is_available(),
            acceleration: self.device.config_accel_is_available(),
        }
    }

    fn defaults(&self, capabilities: &PointerCapabilities) -> PointerSettings {
        PointerSettings {
            tap: capabilities
                .tap
                .then(|| self.device.config_tap_default_enabled()),
            natural_scroll: capabilities
                .natural_scroll
                .then(|| self.device.config_scroll_default_natural_scroll_enabled()),
            left_handed: capabilities
                .left_handed
                .then(|| self.device.config_left_handed_default()),
            acceleration: capabilities
                .acceleration
                .then(|| self.device.config_accel_default_speed()),
        }
    }

    fn snapshot(&self, id: &str) -> PointerDevice {
        let capabilities = self.capabilities();
        let defaults = self.defaults(&capabilities);
        let effective = PointerSettings {
            tap: capabilities.tap.then(|| self.device.config_tap_enabled()),
            natural_scroll: capabilities
                .natural_scroll
                .then(|| self.device.config_scroll_natural_scroll_enabled()),
            left_handed: capabilities
                .left_handed
                .then(|| self.device.config_left_handed()),
            acceleration: capabilities
                .acceleration
                .then(|| self.device.config_accel_speed()),
        };
        PointerDevice {
            id: id.into(),
            name: self.device.name().into(),
            capabilities,
            defaults,
            effective,
            error: self.error.clone(),
        }
    }

    fn apply(&mut self, settings: &PointerSettings) -> io::Result<()> {
        settings.validate().map_err(io::Error::other)?;
        let capabilities = self.capabilities();
        let defaults = self.defaults(&capabilities);
        let settings = settings.resolve(&capabilities, &defaults);
        if let Some(value) = settings.tap {
            self.device.config_tap_set_enabled(value).map_err(|error| {
                io::Error::other(format!("Tap configuration failed: {error:?}"))
            })?;
        }
        if let Some(value) = settings.natural_scroll {
            self.device
                .config_scroll_set_natural_scroll_enabled(value)
                .map_err(|error| {
                    io::Error::other(format!("Natural scroll configuration failed: {error:?}"))
                })?;
        }
        if let Some(value) = settings.left_handed {
            self.device.config_left_handed_set(value).map_err(|error| {
                io::Error::other(format!("Button handedness configuration failed: {error:?}"))
            })?;
        }
        if let Some(value) = settings.acceleration {
            self.device.config_accel_set_speed(value).map_err(|error| {
                io::Error::other(format!(
                    "Pointer acceleration configuration failed: {error:?}"
                ))
            })?;
        }
        Ok(())
    }
}

pub(super) struct Devices {
    control: PointerControl,
    entries: BTreeMap<String, Entry>,
    scrolling: BTreeSet<(String, u8)>,
    gestures: BTreeSet<(String, u8)>,
    absolute: absolute::Control,
    contacts: BTreeSet<String>,
}

impl Devices {
    pub fn new(settings: PointerSettings) -> Self {
        Self {
            control: PointerControl::new(settings, true),
            entries: BTreeMap::new(),
            scrolling: BTreeSet::new(),
            gestures: BTreeSet::new(),
            absolute: absolute::Control::default(),
            contacts: BTreeSet::new(),
        }
    }

    fn refresh(&mut self) {
        self.control.devices(
            self.entries
                .iter()
                .filter(|(_, entry)| entry.device.has_capability(DeviceCapability::Pointer))
                .map(|(id, entry)| entry.snapshot(id))
                .collect(),
        );
        self.refresh_absolute();
    }

    pub fn added(&mut self, device: Device) {
        if ![DeviceCapability::Pointer, DeviceCapability::Touch, DeviceCapability::TabletTool].into_iter().any(|capability| device.has_capability(capability)) {
            return;
        }
        let Some(generation) = self.absolute.next_generation() else { return; };
        let id = device.sysname().to_owned();
        let mut entry = Entry {
            key: absolute::identity(&device),
            device,
            error: None,
            generation,
            absolute_error: None,
            tools: Vec::new(),
        };
        if entry.device.has_capability(DeviceCapability::Pointer) && let Err(error) = entry.apply(&self.control.snapshot.settings) {
            entry.error = Some(format!(
                "Saved pointer preferences could not be fully applied: {error}"
            ));
        }
        self.entries.insert(id, entry);
        self.restore_absolute_admission();
        self.refresh();
    }

    pub fn removed(&mut self, id: &str) {
        self.entries.remove(id);
        self.scrolling.retain(|(device, _)| device != id);
        self.gestures.retain(|(device, _)| device != id);
        self.contacts.remove(id);
        self.refresh();
    }

    pub fn registered(&self, device: &Device) -> bool {
        self.entries.get(device.sysname()).is_some_and(|entry| &entry.device == device)
    }

    pub fn generation(&self, id: &str) -> Option<u64> { self.entries.get(id).map(|entry| entry.generation) }

    pub fn active(&mut self, active: bool) {
        self.control.available(active);
        self.absolute.available(active);
        if !active {
            self.entries.clear();
            self.scrolling.clear();
            self.gestures.clear();
            self.contacts.clear();
            self.refresh();
        }
    }

    pub fn scroll(&mut self, id: &str, axis: u8, active: bool) {
        if active {
            self.scrolling.insert((id.to_owned(), axis));
        } else {
            self.scrolling.remove(&(id.to_owned(), axis));
        }
    }

    pub fn gesture(&mut self, id: &str, kind: u8, active: bool) {
        if active {
            self.gestures.insert((id.into(), kind));
        } else {
            self.gestures.remove(&(id.into(), kind));
        }
    }

    pub fn request(&mut self, request: PointerRequest, protected: bool) {
        self.control.request(request, protected);
    }

    pub fn snapshot(&self) -> PointerSnapshot {
        self.control.snapshot.clone()
    }

    pub fn gestures_available(&self) -> bool {
        self.control.snapshot.available
            && self.entries.values().any(|entry| entry.device.has_capability(DeviceCapability::Gesture))
    }

    pub fn advance(
        &mut self,
        protected: bool,
        pointer_idle: bool,
        save: impl FnOnce(&PointerSettings, &PointerSettings) -> io::Result<Option<String>>,
    ) {
        let idle = pointer_idle && self.scrolling.is_empty() && self.gestures.is_empty();
        let Some(request) = self.control.ready(protected, idle) else {
            return;
        };
        let before = self.control.snapshot.devices.clone();
        let result = input_settings::apply(
            &before,
            &request.settings,
            |id, settings| {
                self.entries
                    .get_mut(id)
                    .ok_or_else(|| io::Error::other("The pointer device disappeared"))?
                    .apply(settings)
            },
            || save(&request.settings, &self.control.snapshot.settings),
        );
        match result {
            Ok(warning) => {
                for entry in self.entries.values_mut() {
                    entry.error = None;
                }
                self.control.saved(request.id, request.settings, warning);
            }
            Err(failure) => {
                let mut reason = failure.reason;
                for (id, error) in failure.restoration {
                    reason.push_str(&format!(
                        " Previous values could not be restored for {id}: {error}."
                    ));
                    if let Some(entry) = self.entries.get_mut(&id) {
                        entry.error = Some(error);
                    }
                }
                self.control.failed(request.id, reason);
            }
        }
        self.refresh();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::input::InputOutcome;

    #[test]
    fn preferences_wait_for_every_scroll_axis_and_gesture_and_cancel_on_seat_departure() {
        let mut devices = Devices::new(PointerSettings::default());
        devices.scroll("pad", 0, true);
        devices.scroll("pad", 1, true);
        devices.gesture("pad", 0, true);
        devices.request(
            PointerRequest {
                id: 1,
                revision: devices.snapshot().revision,
                settings: PointerSettings {
                    tap: Some(true),
                    ..Default::default()
                },
            },
            false,
        );
        devices.scroll("pad", 0, false);
        devices.advance(false, true, |_, _| {
            panic!("The vertical scroll is still active")
        });
        devices.scroll("pad", 1, false);
        devices.advance(false, true, |_, _| panic!("The gesture is still active"));
        devices.gesture("pad", 0, false);
        let mut saved = false;
        devices.advance(false, true, |requested, previous| {
            assert_eq!(requested.tap, Some(true));
            assert_eq!(previous.tap, None);
            saved = true;
            Ok(None)
        });
        assert!(saved);
        assert!(matches!(
            devices.snapshot().response.unwrap().outcome,
            InputOutcome::Saved { .. }
        ));
        devices.request(
            PointerRequest {
                id: 2,
                revision: devices.snapshot().revision,
                settings: PointerSettings::default(),
            },
            false,
        );
        devices.scroll("pad", 1, true);
        devices.active(false);
        assert!(!devices.snapshot().available);
        assert!(devices.scrolling.is_empty());
        assert!(matches!(
            devices.snapshot().response.unwrap().outcome,
            InputOutcome::Failed(_)
        ));
        devices.active(true);
        devices.advance(false, true, |_, _| {
            panic!("A cancelled request cannot resume")
        });
    }
}
