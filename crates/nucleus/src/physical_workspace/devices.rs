use std::{
    collections::{BTreeMap, BTreeSet},
    iter::once,
};

use super::{Configuration, Connection, DeviceKind, Runtime, Status, Surface, valid_id};

impl Default for Runtime {
    fn default() -> Self {
        Self {
            configuration: Configuration::default(),
            connections: BTreeMap::new(),
            observations: BTreeMap::new(),
            pointers: BTreeMap::new(),
            requests: BTreeMap::new(),
            blanked: true,
            clock_ms: 0,
        }
    }
}

impl Runtime {
    /// Starts a blanked runtime with a validated installation.
    ///
    /// # Errors
    ///
    /// Returns the first installation validation diagnostic.
    pub fn new(configuration: Configuration) -> Result<Self, String> {
        configuration.validate()?;
        Ok(Self {
            configuration,
            ..Self::default()
        })
    }

    /// Applies a revised configuration and invalidates affected work.
    ///
    /// # Errors
    ///
    /// Returns the first installation or calibration revision diagnostic.
    pub fn configure(&mut self, configuration: Configuration) -> Result<(), String> {
        configuration.validate()?;
        for surface in &configuration.surfaces {
            if let Some(old) = self
                .configuration
                .surfaces
                .iter()
                .find(|s| s.id == surface.id)
                && surface != old
                && surface.calibration_revision <= old.calibration_revision
            {
                return Err("Changed surfaces require a newer calibration revision.".into());
            }
        }
        if configuration != self.configuration {
            let changed: BTreeSet<_> = self
                .configuration
                .surfaces
                .iter()
                .filter(|old| !configuration.surfaces.contains(old))
                .map(|s| s.id.clone())
                .collect();
            let unchanged_devices: BTreeSet<_> = self
                .configuration
                .devices
                .iter()
                .filter(|old| configuration.devices.contains(old))
                .map(|device| device.id.as_str())
                .collect();
            let preserved_devices: BTreeSet<_> = self
                .configuration
                .surfaces
                .iter()
                .filter(|s| !changed.contains(&s.id))
                .flat_map(|surface| {
                    once(surface.projector.as_str()).chain(surface.camera.as_deref())
                })
                .collect();
            let removed_devices: Vec<_> = self
                .connections
                .keys()
                .filter(|id| {
                    !unchanged_devices.contains(id.as_str())
                        || !preserved_devices.contains(id.as_str())
                })
                .cloned()
                .collect();
            for device in removed_devices {
                self.disconnect(&device);
            }
            self.pointers.retain(|_, p| !changed.contains(&p.surface));
            self.observations
                .retain(|_, e| !changed.contains(&e.surface));
            for request in self.requests.values_mut() {
                if changed.contains(&request.surface) && !request.terminal() {
                    request.status = Status::Canceled;
                }
            }
            if self
                .configuration
                .surfaces
                .iter()
                .all(|s| changed.contains(&s.id))
            {
                self.blanked = true;
            }
        }
        self.configuration = configuration;
        Ok(())
    }

    /// Connects a device under a distinct session identity.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic for an unknown device, invalid session, or repeated session.
    pub fn connect(&mut self, device: &str, session: &str) -> Result<(), String> {
        if !valid_id(session) || !self.configuration.devices.iter().any(|d| d.id == device) {
            return Err("Unknown device or invalid session.".into());
        }
        if self
            .connections
            .get(device)
            .is_some_and(|c| c.session == session)
        {
            return Err("A reconnect requires a new session identity.".into());
        }
        self.disconnect(device);
        self.connections.insert(
            device.into(),
            Connection {
                session: session.into(),
                capture_enabled: false,
                sequence: 0,
                time_ms: 0,
            },
        );
        Ok(())
    }

    /// Changes capture permission and releases capture state when disabled.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the camera is unknown or disconnected.
    pub fn capture(&mut self, device: &str, enabled: bool) -> Result<(), String> {
        if !self
            .configuration
            .devices
            .iter()
            .any(|d| d.id == device && d.kind == DeviceKind::Camera)
        {
            return Err("Select a camera.".into());
        }
        self.connections
            .get_mut(device)
            .ok_or("Camera is disconnected.")?
            .capture_enabled = enabled;
        if !enabled {
            self.release_device(device);
        }
        Ok(())
    }

    fn release_device(&mut self, device: &str) {
        let removed: BTreeSet<_> = self
            .observations
            .extract_if(.., |_, evidence| evidence.device == device)
            .map(|(_, evidence)| evidence.id)
            .collect();
        self.pointers.retain(|_, p| p.device != device);
        for request in self.requests.values_mut() {
            if request.observations.iter().any(|id| removed.contains(id)) && !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    /// Disconnects a device and releases state that depends on it.
    pub fn disconnect(&mut self, device: &str) {
        self.connections.remove(device);
        self.release_device(device);
        let affected: BTreeSet<_> = self
            .configuration
            .surfaces
            .iter()
            .filter(|s| s.projector == device || s.camera.as_deref() == Some(device))
            .map(|surface| surface.id.as_str())
            .collect();
        self.pointers
            .retain(|_, p| !affected.contains(p.surface.as_str()));
        for request in self.requests.values_mut() {
            if affected.contains(request.surface.as_str()) && !request.terminal() {
                request.status = Status::Canceled;
            }
        }
    }

    /// Looks up a calibrated surface in the given workspace.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic when the surface is unavailable in that workspace.
    pub fn surface(&self, id: &str, workspace: u64) -> Result<&Surface, String> {
        self.configuration
            .surfaces
            .iter()
            .find(|s| s.id == id && s.workspace == workspace)
            .ok_or_else(|| "The surface is unavailable in this workspace.".into())
    }
}
