use super::*;
use lince_interface::host::input::{AbsoluteCommand, AbsoluteDevice, AbsolutePreferences, AbsoluteRequest, AbsoluteReload, AbsoluteSnapshot, InputOutcome, InputResponse, validate_absolute};

pub(super) struct Control {
    snapshot: AbsoluteSnapshot,
    pending: Option<AbsoluteCommand>,
    generation: u64,
    pub(super) next_tool: u64,
}

#[cfg(all(test, feature = "server-tests"))]
pub(crate) fn reject_removed_calibration(request: AbsoluteRequest, observed: AbsoluteSnapshot) -> AbsoluteSnapshot {
    let mut devices = Devices::new(PointerSettings::default());
    devices.absolute.snapshot = observed;
    devices.absolute_request(request, false);
    devices.advance_absolute(false, |_, _| panic!("A removed calibration device cannot reach persistence"), || panic!("A save cannot read reload state"));
    devices.absolute_snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lince_interface::host::input::AbsolutePreference;

    #[test]
    fn settings_wait_for_contact_then_save_once_and_cancel_on_device_or_seat_change() {
        let mut devices = Devices::new(PointerSettings::default());
        let settings = BTreeMap::from([("stable".into(), AbsolutePreference { output: Some("DP-1".into()), ..Default::default() })]);
        devices.absolute_contacts(BTreeSet::from(["touch".into()]));
        let revision = devices.absolute_snapshot().revision;
        devices.absolute_request(AbsoluteRequest { id: 1, revision, settings: settings.clone() }, false);
        devices.advance_absolute(false, |_, _| panic!("A held contact must defer saving"), || panic!("Saving must not load a separate file"));
        assert_eq!(devices.absolute_snapshot().response.unwrap().outcome, InputOutcome::Pending);
        devices.absolute_contacts(BTreeSet::new());
        let mut saves = 0;
        devices.advance_absolute(false, |requested, previous| { assert_eq!(requested, &settings); assert!(previous.is_empty()); saves += 1; Ok(None) }, || panic!("Saving must not load a separate file"));
        assert_eq!(saves, 1);
        devices.advance_absolute(false, |_, _| panic!("The completed request cannot save twice"), || panic!("No request remains"));
        assert_eq!(devices.absolute_snapshot().settings, settings);
        let revision = devices.absolute_snapshot().revision;
        devices.absolute_contacts(BTreeSet::from(["touch".into()]));
        devices.absolute_request(AbsoluteRequest { id: 2, revision, settings: BTreeMap::new() }, false);
        devices.active(false);
        assert!(matches!(devices.absolute_snapshot().response.unwrap().outcome, InputOutcome::Failed(_)));
        assert_eq!(devices.absolute_snapshot().settings, settings);
        devices.active(true);
        let revision = devices.absolute_snapshot().revision;
        devices.absolute_request(AbsoluteRequest { id: 3, revision, settings: BTreeMap::new() }, false);
        devices.absolute.snapshot.revision += 1;
        devices.advance_absolute(false, |_, _| panic!("A changed device revision cannot save"), || panic!("A stale request cannot read the file"));
        assert!(matches!(devices.absolute_snapshot().response.unwrap().outcome, InputOutcome::Failed(_)));
    }

    #[test]
    fn file_reload_waits_for_input_never_writes_and_rejects_invalid_or_changed_candidates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("park.json");
        std::fs::write(&path, br#"{"keyboard":{"layout":"br"},"devices":{"stable":{"output":"DP-1"}}}"#).unwrap();
        let mut devices = Devices::new(PointerSettings::default());
        let requested = crate::commands::Commands::load(&path).unwrap().devices;
        let bytes = std::fs::read(&path).unwrap();
        devices.absolute_contacts(BTreeSet::from(["finger".into()]));
        devices.absolute_reload(AbsoluteReload { id: 1, revision: devices.absolute_snapshot().revision, settings: () }, false);
        devices.advance_absolute(false, |_, _| panic!("Reload must never write"), || panic!("Held input must defer reading"));
        devices.absolute_contacts(BTreeSet::new());
        devices.advance_absolute(false, |_, _| panic!("Reload must never write"), || Ok(crate::commands::Commands::load(&path)?.devices));
        assert_eq!(devices.absolute_snapshot().settings, requested);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        devices.absolute_reload(AbsoluteReload { id: 2, revision: devices.absolute_snapshot().revision, settings: () }, false);
        std::fs::write(&path, br#"{"devices":{"stable":{"calibration":[0,0,0,0,0,0]}}}"#).unwrap();
        devices.advance_absolute(false, |_, _| panic!("Reload must never write"), || Ok(crate::commands::Commands::load(&path)?.devices));
        assert_eq!(devices.absolute_snapshot().settings, requested);
        assert!(matches!(devices.absolute_snapshot().response.unwrap().outcome, InputOutcome::Failed(_)));
        devices.absolute_reload(AbsoluteReload { id: 3, revision: devices.absolute_snapshot().revision, settings: () }, false);
        let mut reads = 0;
        devices.advance_absolute(false, |_, _| panic!("Reload must never write"), || {
            reads += 1;
            Ok(BTreeMap::from([("stable".into(), AbsolutePreference { output: Some(if reads == 1 { "DP-2" } else { "DP-3" }.into()), ..Default::default() })]))
        });
        assert_eq!(reads, 2);
        assert_eq!(devices.absolute_snapshot().settings, requested);
        assert!(matches!(devices.absolute_snapshot().response.unwrap().outcome, InputOutcome::Failed(_)));
    }
}

impl Default for Control {
    fn default() -> Self {
        Self { snapshot: AbsoluteSnapshot { available: true, revision: 1, ..Default::default() }, pending: None, generation: 0, next_tool: 0 }
    }
}

impl Control {
    pub(super) fn next_generation(&mut self) -> Option<u64> {
        self.generation = self.generation.checked_add(1)?;
        Some(self.generation)
    }

    pub(super) fn available(&mut self, active: bool) {
        if self.snapshot.available != active {
            self.snapshot.available = active;
            self.snapshot.revision = self.snapshot.revision.saturating_add(1);
        }
        if !active && let Some(request) = self.pending.take() {
            self.reply(request.id(), InputOutcome::Failed("The input seat became inactive; your draft is preserved.".into()));
        }
    }

    fn reply(&mut self, request: u64, outcome: InputOutcome) {
        self.snapshot.response = Some(InputResponse { request, outcome });
    }
}

pub(super) fn identity(device: &Device) -> Option<String> {
    let metadata = unsafe { device.udev_device() }?;
    let property = |name| metadata.property_value(name).and_then(|value| value.to_str()).filter(|value| !value.is_empty()).map(str::to_owned);
    let serial = property("ID_SERIAL");
    let path = property("ID_PATH");
    if serial.is_none() && path.is_none() { return None; }
    Some(format!("{:04x}:{:04x}:{}:{}:{}{}", device.id_vendor(), device.id_product(), serial.unwrap_or_default(), path.unwrap_or_default(),
        if device.has_capability(DeviceCapability::Touch) { "touch" } else { "" },
        if device.has_capability(DeviceCapability::TabletTool) { "tablet" } else { "" }))
}

impl Entry {
    fn absolute(&self, id: &str, unique: bool) -> AbsoluteDevice {
        AbsoluteDevice {
            id: id.into(), key: unique.then(|| self.key.clone()).flatten(), generation: self.generation, name: self.device.name().into(),
            touch: self.device.has_capability(DeviceCapability::Touch), tablet: self.device.has_capability(DeviceCapability::TabletTool),
            calibration: self.device.config_calibration_has_matrix(), default_calibration: self.device.config_calibration_default_matrix(),
            effective_calibration: self.device.config_calibration_matrix(), associated_output: self.device.output_name().map(str::to_owned),
            buttons: self.tools.iter().flat_map(|tool| tool.buttons.iter().copied()).collect::<BTreeSet<_>>().into_iter().collect(),
            error: if !unique { Some("This device has no unambiguous persistent identity; per-device settings are unavailable.".into()) } else { self.absolute_error.clone() },
        }
    }

    fn calibration(&mut self, value: Option<[f32; 6]>) -> io::Result<()> {
        if !self.device.config_calibration_has_matrix() {
            return if value.is_none() { Ok(()) } else { Err(io::Error::other("This device does not support backend calibration.")) };
        }
        let matrix = value.or_else(|| self.device.config_calibration_default_matrix()).ok_or_else(|| io::Error::other("The device calibration default is unavailable."))?;
        self.device.config_calibration_set_matrix(matrix).map_err(|failure| io::Error::other(format!("Device calibration failed: {failure:?}")))?;
        if self.device.config_calibration_matrix().is_none_or(|effective| effective.iter().zip(matrix).any(|(a, b)| (*a - b).abs() > 1e-5)) {
            return Err(io::Error::other("The device did not confirm the requested calibration."));
        }
        Ok(())
    }
}

impl Devices {
    fn absolute_keys(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for entry in self.entries.values().filter(|entry| entry.device.has_capability(DeviceCapability::Touch) || entry.device.has_capability(DeviceCapability::TabletTool)) {
            if let Some(key) = &entry.key { *counts.entry(key.clone()).or_default() += 1; }
        }
        counts
    }

    pub(super) fn refresh_absolute(&mut self) {
        let counts = self.absolute_keys();
        let devices = self.entries.iter().filter(|(_, entry)| entry.device.has_capability(DeviceCapability::Touch) || entry.device.has_capability(DeviceCapability::TabletTool))
            .map(|(id, entry)| entry.absolute(id, entry.key.as_ref().is_some_and(|key| counts.get(key) == Some(&1)))).collect::<Vec<_>>();
        if devices != self.absolute.snapshot.devices {
            self.absolute.snapshot.devices = devices;
            self.absolute.snapshot.revision = self.absolute.snapshot.revision.saturating_add(1);
        }
    }

    pub(super) fn restore_absolute_admission(&mut self) {
        let counts = self.absolute_keys();
        for (id, entry) in &mut self.entries {
            if self.contacts.contains(id) || entry.tools.iter().any(|tool| tool.proximity)
                || !(entry.device.has_capability(DeviceCapability::Touch) || entry.device.has_capability(DeviceCapability::TabletTool)) { continue; }
            let requested = entry.key.as_ref().filter(|key| counts.get(*key) == Some(&1)).and_then(|key| self.absolute.snapshot.settings.get(key)).and_then(|settings| settings.calibration);
            let desired = requested.or_else(|| entry.device.config_calibration_default_matrix());
            if desired == entry.device.config_calibration_matrix() { continue; }
            entry.absolute_error = entry.calibration(requested).err().map(|failure| format!("Saved calibration could not be applied: {failure}"));
        }
    }

    pub fn absolute_preferences(&mut self, settings: AbsolutePreferences) {
        self.absolute.snapshot.settings = settings;
        self.restore_absolute_admission();
        self.refresh_absolute();
    }

    pub fn absolute_snapshot(&self) -> AbsoluteSnapshot { self.absolute.snapshot.clone() }

    pub fn absolute_device(&self, device: &Device) -> Option<AbsoluteDevice> {
        if !self.registered(device) { return None; }
        let id = device.sysname();
        let entry = self.entries.get(id)?;
        let counts = self.absolute_keys();
        Some(entry.absolute(id, entry.key.as_ref().is_some_and(|key| counts.get(key) == Some(&1))))
    }

    pub fn absolute_contacts(&mut self, contacts: BTreeSet<String>) {
        if self.contacts != contacts {
            self.contacts = contacts;
            self.restore_absolute_admission();
            self.refresh_absolute();
        }
    }

    pub fn absolute_request(&mut self, request: AbsoluteRequest, protected: bool) {
        self.enqueue_absolute(AbsoluteCommand::Save(request), protected);
    }

    pub fn absolute_reload(&mut self, request: AbsoluteReload, protected: bool) {
        self.enqueue_absolute(AbsoluteCommand::Reload(request), protected);
    }

    fn enqueue_absolute(&mut self, request: AbsoluteCommand, protected: bool) {
        let reason = if request.id() == 0 || request.id() <= self.absolute.snapshot.last_request { Some("This device request has already been used.".into()) }
            else if protected { Some("Device settings cannot change while the session is locked or inactive.".into()) }
            else if !self.absolute.snapshot.available { Some("Physical device settings require a standalone Park session.".into()) }
            else if self.absolute.pending.is_some() { Some("Another device change is waiting for the current interaction to finish.".into()) }
            else if request.revision() != self.absolute.snapshot.revision { Some("Devices changed while editing; reload before saving.".into()) }
            else { match &request { AbsoluteCommand::Save(request) => validate_absolute(&request.settings).err(), AbsoluteCommand::Reload(_) => None } };
        self.absolute.snapshot.last_request = self.absolute.snapshot.last_request.max(request.id());
        if let Some(reason) = reason { self.absolute.reply(request.id(), InputOutcome::Failed(reason)); }
        else { self.absolute.reply(request.id(), InputOutcome::Pending); self.absolute.pending = Some(request); }
    }

    pub fn advance_absolute(&mut self, protected: bool, save: impl FnOnce(&AbsolutePreferences, &AbsolutePreferences) -> io::Result<Option<String>>, mut read: impl FnMut() -> io::Result<AbsolutePreferences>) {
        let Some(request) = self.absolute.pending.as_ref() else { return; };
        if protected || !self.absolute.snapshot.available || request.revision() != self.absolute.snapshot.revision {
            let request = self.absolute.pending.take().unwrap();
            self.absolute.reply(request.id(), InputOutcome::Failed("Device or seat state changed before applying settings; reload the preserved draft.".into()));
            return;
        }
        if !self.contacts.is_empty() || self.entries.values().any(|entry| entry.tools.iter().any(|tool| tool.proximity)) { return; }
        let request = self.absolute.pending.take().unwrap();
        let id = request.id();
        let reload = matches!(request, AbsoluteCommand::Reload(_));
        let before = self.absolute.snapshot.devices.clone();
        let result = (|| {
            let settings = match request { AbsoluteCommand::Save(request) => request.settings, AbsoluteCommand::Reload(_) => read()? };
            validate_absolute(&settings).map_err(io::Error::other)?;
            for device in &before {
                let Some(key) = &device.key else { continue; };
                let desired = settings.get(key).cloned().unwrap_or_default();
                let previous = self.absolute.snapshot.settings.get(key).cloned().unwrap_or_default();
                if desired == previous { continue; }
                if desired.buttons != previous.buttons && desired.buttons.keys().any(|source| !device.buttons.contains(source)) {
                    return Err(io::Error::other("A button mapping needs a tool that reports that button; bring the tool into proximity and reload."));
                }
                if desired.calibration != previous.calibration {
                    self.entries.get_mut(&device.id).filter(|entry| entry.generation == device.generation).ok_or_else(|| io::Error::other("The device was replaced."))?.calibration(desired.calibration)?;
                }
            }
            let warning = if reload {
                if read()? != settings { return Err(io::Error::other("Device settings changed again while reloading; the previous active state was restored.")); }
                None
            } else { save(&settings, &self.absolute.snapshot.settings)? };
            Ok((settings, warning))
        })();
        match result {
            Ok((settings, warning)) => {
                self.absolute.snapshot.settings = settings;
                self.absolute.snapshot.revision = self.absolute.snapshot.revision.saturating_add(1);
                for entry in self.entries.values_mut() { entry.absolute_error = None; }
                self.absolute.reply(id, InputOutcome::Saved { warning });
            }
            Err(failure) => {
                let mut reason = failure.to_string();
                for device in before {
                    if let Some(entry) = self.entries.get_mut(&device.id).filter(|entry| entry.generation == device.generation)
                        && entry.device.config_calibration_matrix() != device.effective_calibration
                        && let Err(failure) = entry.calibration(device.effective_calibration) {
                        reason.push_str(&format!(" Previous calibration for {} could not be restored: {failure}.", device.name));
                        entry.absolute_error = Some(failure.to_string());
                    }
                }
                self.absolute.reply(id, InputOutcome::Failed(reason));
            }
        }
        self.refresh_absolute();
    }
}
