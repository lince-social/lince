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
    pub fn configure_outputs(&mut self, monitors: &[Monitor]) -> io::Result<()> {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut pixels = 0_u64;
        if monitors.is_empty() || monitors.len() > 16 {
            return Err(io::Error::other(
                "Park needs between one and sixteen outputs",
            ));
        }
        for monitor in monitors {
            validate_size(monitor.size)?;
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
                let output = Output::new(
                    monitor.name.clone(),
                    PhysicalProperties {
                        size: (0, 0).into(),
                        subpixel: Subpixel::Unknown,
                        make: "Lince".into(),
                        model: "Park".into(),
                    },
                );
                let global = output.create_global::<State>(&self.state.handle);
                self.state
                    .outputs
                    .insert(monitor.id, Registered { output, global });
            }
            let registered = &self.state.outputs[&monitor.id];
            let mode = Mode {
                size: (monitor.size[0], monitor.size[1]).into(),
                refresh: 60_000,
            };
            registered.output.change_current_state(
                Some(mode),
                Some(Transform::Normal),
                None,
                Some((monitor.origin[0], monitor.origin[1]).into()),
            );
            registered.output.set_preferred(mode);
            smithay::desktop::layer_map_for_output(&registered.output).arrange();
        }
        self.state.output = self.state.outputs[&monitors[0].id].output.clone();
        self.state.initial_size = monitors[0].size;
        for layer in self.state.layers.surfaces.values() {
            layer.layer_surface().send_pending_configure();
        }
        self.state.presentation_changed();
        Ok(())
    }

    pub fn usable_output(&self, id: OutputId) -> Option<Rectangle<i32, Logical>> {
        let output = &self.state.outputs.get(&id)?.output;
        let bounds = Rectangle::from_size(output.current_mode()?.size.to_logical(1));
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
                let bounds = lince_interface::host::Rectangle::new(
                    [f64::from(origin.x), f64::from(origin.y)],
                    [f64::from(size.w), f64::from(size.h)],
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
    }
}
