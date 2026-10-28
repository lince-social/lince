use super::*;
use smithay::{
    delegate_layer_shell,
    desktop::{LayerSurface, layer_map_for_output},
    reexports::wayland_server::protocol::wl_output::WlOutput,
    wayland::shell::wlr_layer::{
        self, ExclusiveZone, Layer, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState,
    },
};

pub(super) struct Layers {
    shell: WlrLayerShellState,
    pub(super) surfaces: HashMap<WlSurface, LayerSurface>,
    pub(super) outputs: HashMap<WlSurface, OutputId>,
}

impl Layers {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        Self {
            shell: WlrLayerShellState::new::<State>(handle),
            surfaces: HashMap::new(),
            outputs: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct LayerSnapshot {
    pub output: OutputId,
    pub surface: WlSurface,
    pub origin: [i32; 2],
    pub layer: Layer,
    pub namespace: String,
    pub keyboard: wlr_layer::KeyboardInteractivity,
    pub nodes: Vec<SurfaceNode>,
    pub popups: Vec<PopupSnapshot>,
}

impl Server {
    pub fn usable(&self) -> Rectangle<i32, Logical> {
        let bounds =
            Rectangle::from_size((self.state.initial_size[0], self.state.initial_size[1]).into());
        bounds
            .intersection(layer_map_for_output(&self.state.output).non_exclusive_zone())
            .unwrap_or_else(|| Rectangle::from_size((0, 0).into()))
    }

    pub fn layers(&self) -> Vec<LayerSnapshot> {
        self.state
            .outputs
            .iter()
            .flat_map(|(id, registered)| {
                let layers = {
                    let map = layer_map_for_output(&registered.output);
                    map.layers()
                        .filter(|layer| self.state.buffers.contains_key(layer.wl_surface()))
                        .map(|layer| {
                            let location = map
                                .layer_geometry(layer)
                                .map(|geometry| geometry.loc)
                                .unwrap_or_default();
                            (layer.clone(), location)
                        })
                        .collect::<Vec<_>>()
                };
                layers
                    .into_iter()
                    .map(|(layer, location)| {
                        LayerSnapshot {
                            output: *id,
                            surface: layer.wl_surface().clone(),
                            origin: [
                                location
                                    .x
                                    .saturating_add(registered.output.current_location().x),
                                location
                                    .y
                                    .saturating_add(registered.output.current_location().y),
                            ],
                            layer: layer.layer(),
                            namespace: layer.namespace().into(),
                            keyboard: layer.cached_state().keyboard_interactivity,
                            nodes: self.state.nodes(layer.wl_surface()),
                            popups: self.state.popup_snapshots(layer.wl_surface()),
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn resize(&mut self, size: [i32; 2]) -> io::Result<()> {
        validate_size(size)?;
        let mode = Mode {
            size: (size[0], size[1]).into(),
            refresh: 60_000,
        };
        self.state
            .output
            .change_current_state(Some(mode), None, None, None);
        self.state.output.set_preferred(mode);
        self.state.initial_size = size;
        layer_map_for_output(&self.state.output).arrange();
        for layer in self.state.layers.surfaces.values() {
            layer.layer_surface().send_pending_configure();
        }
        self.state.changed();
        Ok(())
    }
}

impl State {
    pub(super) fn layer_output(&self, surface: &WlSurface) -> &Output {
        self.layers
            .outputs
            .get(surface)
            .and_then(|id| self.outputs.get(id))
            .map_or(&self.output, |registered| &registered.output)
    }
    pub(super) fn layer_configured(&self, surface: &WlSurface) -> bool {
        self.layers
            .surfaces
            .get(surface)
            .is_none_or(|layer| layer.layer_surface().ensure_configured())
    }

    pub(super) fn remove_layer(&mut self, surface: &WlSurface) {
        self.dismiss_children(surface);
        if let Some(layer) = self.layers.surfaces.remove(surface) {
            layer_map_for_output(self.layer_output(surface)).unmap_layer(&layer);
        }
        self.layers.outputs.remove(surface);
    }

    pub(super) fn layer_commit(&mut self, surface: &WlSurface, removed: bool) {
        let Some(layer) = self.layers.surfaces.get(surface) else {
            return;
        };
        let data = layer.cached_state();
        if [data.size.w, data.size.h]
            .iter()
            .any(|size| !(0..=16384).contains(size))
            || [
                data.margin.left,
                data.margin.right,
                data.margin.top,
                data.margin.bottom,
            ]
            .iter()
            .any(|margin| !(-16384..=16384).contains(margin))
            || matches!(data.exclusive_zone, ExclusiveZone::Exclusive(amount) if amount > 32768)
        {
            layer_map_for_output(self.layer_output(surface)).unmap_layer(layer);
            layer.layer_surface().send_close();
            return;
        }
        let mut map = layer_map_for_output(self.layer_output(surface));
        if removed {
            map.unmap_layer(layer);
            with_states(surface, |states| {
                let mut state = states
                    .data_map
                    .get::<LayerSurfaceData>()
                    .unwrap()
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                state.configured = false;
                state.configure_serial = None;
                state.initial_configure_sent = false;
                state.server_pending = None;
                state.last_acked = None;
                state.current = Default::default();
            });
            return;
        } else {
            if map.map_layer(layer).is_err() {
                layer.layer_surface().send_close();
                return;
            }
            map.arrange();
        }
        layer.layer_surface().send_pending_configure();
    }
}

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layers.shell
    }

    fn new_layer_surface(
        &mut self,
        surface: wlr_layer::LayerSurface,
        output: Option<WlOutput>,
        _: Layer,
        namespace: String,
    ) {
        if self.layers.surfaces.len() >= 128 || namespace.len() > 4096 {
            surface.send_close();
            return;
        }
        let requested = output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.output.clone());
        let Some(id) = self
            .outputs
            .iter()
            .find(|(_, registered)| registered.output == requested)
            .map(|(id, _)| *id)
        else {
            surface.send_close();
            return;
        };
        self.layers.outputs.insert(surface.wl_surface().clone(), id);
        self.layers.surfaces.insert(
            surface.wl_surface().clone(),
            LayerSurface::new(surface, namespace),
        );
        self.presentation_changed();
    }

    fn layer_destroyed(&mut self, surface: wlr_layer::LayerSurface) {
        self.remove_layer(surface.wl_surface());
        self.buffers.remove(surface.wl_surface());
        self.presentation_changed();
    }

    fn new_popup(&mut self, parent: wlr_layer::LayerSurface, popup: PopupSurface) {
        if !self.layers.surfaces.contains_key(parent.wl_surface())
            || parent.wl_surface().client().map(|client| client.id())
                != popup.wl_surface().client().map(|client| client.id())
        {
            popup.send_popup_done();
            return;
        }
        self.popups.manager.commit(popup.wl_surface());
        self.presentation_changed();
    }
}

delegate_layer_shell!(State);
