use super::*;
use smithay::reexports::wayland_server::backend::GlobalId;

pub(super) struct Registered {
    pub output: Output,
    pub global: GlobalId,
}

#[derive(Clone, Debug)]
pub struct Monitor {
    pub id: OutputId,
    pub name: String,
    pub origin: [i32; 2],
    pub size: [i32; 2],
}

impl Server {
    #[cfg(feature = "session")]
    pub(crate) fn disconnect_outputs(&mut self) {
        self.clear_input(0);
        let layers: Vec<_> = self.state.layers.surfaces.keys().cloned().collect();
        for surface in layers {
            if let Some(layer) = self.state.layers.surfaces.get(&surface) { layer.layer_surface().send_close(); }
            self.state.remove_layer(&surface);
        }
        for (_, registered) in std::mem::take(&mut self.state.outputs) {
            for surface in &self.state.surfaces { registered.output.leave(surface); }
            self.state.handle.remove_global::<State>(registered.global);
        }
        self.state.configure_locks();
        self.state.presented_geometry = None;
        self.state.presented_focus = None;
        self.state.presentation_changed();
    }

    pub fn configure_outputs(&mut self, monitors: &[Monitor]) -> io::Result<()> {
        self.configure_scaled_outputs(monitors, &BTreeMap::new())
    }

    pub fn configure_scaled_outputs(&mut self, monitors: &[Monitor], scales: &BTreeMap<OutputId, f64>) -> io::Result<()> {
        self.configure_prepared_outputs(monitors, scales, &BTreeMap::new())
    }

    pub(crate) fn configure_prepared_outputs(&mut self, monitors: &[Monitor], scales: &BTreeMap<OutputId, f64>, prepared: &BTreeMap<OutputId, (Output, Mode)>) -> io::Result<()> {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut pixels = 0_u64;
        let mut logical_pixels = 0_u64;
        if monitors.is_empty() || monitors.len() > 16 {
            return Err(io::Error::other(
                "Park needs between one and sixteen outputs",
            ));
        }
        if scales.keys().any(|id| !monitors.iter().any(|monitor| monitor.id == *id)) {
            return Err(io::Error::other("Scale requested for an unknown output"));
        }
        for monitor in monitors {
            validate_size(monitor.size)?;
            let scale = scales.get(&monitor.id).copied().unwrap_or(1.0);
            if !scale.is_finite() || !(0.5..=4.0).contains(&scale)
                || (scale * 120.0 - (scale * 120.0).round()).abs() > 0.000001 {
                return Err(io::Error::other("Unsupported fractional output scale"));
            }
            let logical = monitor.size.map(|size| (f64::from(size) / scale).round() as i32);
            validate_size(logical)?;
            logical_pixels += logical[0] as u64 * logical[1] as u64;
            if logical_pixels > 64 * 1024 * 1024 {
                return Err(io::Error::other("Logical output area exceeds Park's resource limit"));
            }
            pixels += monitor.size[0] as u64 * monitor.size[1] as u64;
            if monitor.name.is_empty()
                || monitor.name.len() > 512
                || !ids.insert(monitor.id)
                || !names.insert(&monitor.name)
                || monitor
                    .origin
                    .iter()
                    .zip(monitor.size)
                    .any(|(origin, size)| origin.checked_add(size).is_none())
                || pixels > 64 * 1024 * 1024
            {
                return Err(io::Error::other("invalid Park output configuration"));
            }
        }
        let removed: Vec<_> = self
            .state
            .outputs
            .iter()
            .filter(|(id, output)| {
                !monitors
                    .iter()
                    .any(|monitor| monitor.id == **id && monitor.name == output.output.name())
            })
            .map(|(id, _)| *id)
            .collect();
        for id in removed {
            let surfaces: Vec<_> = self
                .state
                .layers
                .outputs
                .iter()
                .filter(|(_, output)| **output == id)
                .map(|(surface, _)| surface.clone())
                .collect();
            for surface in surfaces {
                if let Some(layer) = self.state.layers.surfaces.get(&surface) {
                    layer.layer_surface().send_close();
                }
                self.state.remove_layer(&surface);
            }
            if let Some(removed) = self.state.outputs.remove(&id) {
                for window in self.state.windows.values() {
                    removed.output.leave(window.role.wl_surface());
                }
                self.state.handle.remove_global::<State>(removed.global);
            }
        }
        for monitor in monitors {
            if !self.state.outputs.contains_key(&monitor.id) {
                let output = prepared.get(&monitor.id).map(|(output, _)| output.clone()).unwrap_or_else(|| Output::new(
                    monitor.name.clone(),
                    PhysicalProperties {
                        size: (0, 0).into(),
                        subpixel: Subpixel::Unknown,
                        make: "Lince".into(),
                        model: "Park".into(),
                    },
                ));
                let global = output.create_global::<State>(&self.state.handle);
                self.state
                    .outputs
                    .insert(monitor.id, Registered { output, global });
            }
            let registered = &self.state.outputs[&monitor.id];
            let mode = prepared.get(&monitor.id).map(|(_, mode)| *mode).unwrap_or(Mode {
                size: (monitor.size[0], monitor.size[1]).into(),
                refresh: 60_000,
            });
            registered.output.change_current_state(
                Some(mode),
                Some(Transform::Normal),
                Some(smithay::output::Scale::Fractional(scales.get(&monitor.id).copied().unwrap_or(1.0))),
                Some((monitor.origin[0], monitor.origin[1]).into()),
            );
            registered.output.set_preferred(mode);
            smithay::desktop::layer_map_for_output(&registered.output).arrange();
        }
        self.state.output = self.state.outputs[&monitors[0].id].output.clone();
        self.state.initial_size = monitors[0].size;
        self.state.configure_locks();
        self.state.update_surface_scales();
        for layer in self.state.layers.surfaces.values() {
            layer.layer_surface().send_pending_configure();
        }
        self.state.presentation_changed();
        Ok(())
    }

    pub fn usable_output(&self, id: OutputId) -> Option<Rectangle<i32, Logical>> {
        let output = &self.state.outputs.get(&id)?.output;
        let bounds = Rectangle::from_size(logical_size(output)?);
        Some(
            bounds
                .intersection(smithay::desktop::layer_map_for_output(output).non_exclusive_zone())
                .unwrap_or_else(|| Rectangle::from_size((0, 0).into())),
        )
    }

    pub fn output_viewports(&self) -> Vec<(String, lince_interface::host::OutputViewport)> {
        self.state
            .outputs
            .iter()
            .filter_map(|(id, registered)| {
                let output = &registered.output;
                let origin = output.current_location();
                let size = output.current_mode()?.size;
                let logical = logical_size(output)?;
                let bounds = lince_interface::host::Rectangle::new(
                    [f64::from(origin.x), f64::from(origin.y)],
                    [f64::from(logical.w), f64::from(logical.h)],
                )?;
                let usable = self.usable_output(*id)?;
                let usable = lince_interface::host::Rectangle::new(
                    [
                        f64::from(origin.x + usable.loc.x),
                        f64::from(origin.y + usable.loc.y),
                    ],
                    [f64::from(usable.size.w), f64::from(usable.size.h)],
                );
                Some((
                    output.name(),
                    lince_interface::host::OutputViewport {
                        id: *id,
                        bounds,
                        usable,
                        physical_size: [size.w as u32, size.h as u32],
                        workspace: 1,
                    },
                ))
            })
            .collect()
    }

    pub fn presented_outputs(
        &mut self,
        geometry: &lince_interface::host::GeometrySnapshot,
        focused: Option<OutputId>,
    ) {
        self.state.presented_geometry = Some(geometry.clone());
        self.state.presented_focus = focused;
        for (id, window) in &self.state.windows {
            for (output, registered) in &self.state.outputs {
                if window.mapped
                    && geometry.items.iter().any(|item| {
                        item.id == lince_interface::host::ItemId::Application(*id)
                            && item.output == *output
                    })
                {
                    registered.output.enter(window.role.wl_surface());
                } else {
                    registered.output.leave(window.role.wl_surface());
                }
            }
        }
        self.state.update_surface_scales();
    }
}

pub(super) fn logical_size(output: &Output) -> Option<smithay::utils::Size<i32, Logical>> {
    Some(output.current_transform().transform_size(output.current_mode()?.size)
        .to_f64().to_logical(output.current_scale().fractional_scale()).to_i32_round())
}
