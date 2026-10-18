use super::*;
use smithay::wayland::{
    input_method::{InputMethodHandler, InputMethodManagerState, PopupSurface as InputPopup},
    text_input::TextInputManagerState,
    virtual_keyboard::VirtualKeyboardManagerState,
};
#[cfg(feature = "nested")]
use smithay::wayland::input_method::InputMethodSeat;

pub(super) struct Ime {
    _text: TextInputManagerState,
    _method: InputMethodManagerState,
    _keyboard: VirtualKeyboardManagerState,
    pub(super) popups: Vec<InputPopup>,
}

fn authorized(client: &Client) -> bool {
    client.get_data::<ClientState>().is_some_and(|data|
        data.capabilities.contains(&crate::launch::Capability::InputMethod))
}

impl Ime {
    pub(super) fn new(handle: &DisplayHandle) -> Self {
        Self {
            _text: TextInputManagerState::new::<State>(handle),
            _method: InputMethodManagerState::new::<State, _>(handle, authorized),
            _keyboard: VirtualKeyboardManagerState::new::<State, _>(handle, authorized),
            popups: Vec::new(),
        }
    }
}

impl InputMethodHandler for State {
    fn new_popup(&mut self, surface: InputPopup) {
        self.ime.popups.retain(InputPopup::alive);
        if self.ime.popups.len() >= 16 {
            self.reject(surface.wl_surface(), "Park input method popup limit reached");
            return;
        }
        self.ime.popups.retain(|popup| popup.wl_surface() != surface.wl_surface());
        self.ime.popups.push(surface.clone());
        self.popup_repositioned(surface);
    }

    fn dismiss_popup(&mut self, surface: InputPopup) {
        self.ime.popups.retain(|popup| popup.wl_surface() != surface.wl_surface());
        self.presentation_changed();
    }

    fn popup_repositioned(&mut self, surface: InputPopup) {
        let area = surface.text_input_rectangle();
        let origin = [area.loc.x, area.loc.y.saturating_add(area.size.h)];
        surface.set_location(origin.into());
        surface.surface_role.text_input_rectangle(area.loc.x.saturating_sub(origin[0]), area.loc.y.saturating_sub(origin[1]), area.size.w, area.size.h);
        self.presentation_changed();
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        if self.native_surface.as_ref() == Some(parent) {
            return Rectangle::from_size((self.initial_size[0], self.initial_size[1]).into());
        }
        if let Some(layer) = self.layers.surfaces.get(parent) {
            return smithay::desktop::layer_map_for_output(self.layer_output(parent))
                .layer_geometry(layer).unwrap_or_default();
        }
        self.id(parent).and_then(|id| self.windows.get(&id))
            .map(|window| Rectangle::from_size(window.configured_size.into()))
            .unwrap_or_default()
    }
}

impl State {
    pub(super) fn ime_snapshots(&self, root: &WlSurface) -> Vec<PopupSnapshot> {
        if self.lock.owner.is_some() { return Vec::new(); }
        self.ime.popups.iter().filter(|popup| popup.alive()
            && popup.get_parent().is_some_and(|parent| &parent.surface == root)
            && self.buffers.contains_key(popup.wl_surface()))
            .map(|popup| {
                let nodes = self.nodes(popup.wl_surface());
                let size = nodes.iter().fold([1i32, 1i32], |size, node| [
                    size[0].max(node.origin[0].saturating_add(node.layout.size[0])),
                    size[1].max(node.origin[1].saturating_add(node.layout.size[1])),
                ]);
                let caret = popup.text_input_rectangle();
                let bounds = if self.native_surface.as_ref() == Some(root) {
                    self.presented_geometry.as_ref().and_then(|geometry| geometry.outputs.iter().find(|output|
                        output.bounds.contains([f64::from(caret.loc.x), f64::from(caret.loc.y)])))
                        .map(|output| output.usable.unwrap_or(output.bounds))
                        .map(|rectangle| [rectangle.origin()[0] as i32, rectangle.origin()[1] as i32,
                            rectangle.size()[0] as i32, rectangle.size()[1] as i32])
                } else if let Some(layer) = self.layers.surfaces.get(root) {
                    let output = self.layer_output(root);
                    smithay::desktop::layer_map_for_output(output).layer_geometry(layer)
                        .zip(super::outputs::logical_size(output))
                        .map(|(layer, size)| [-layer.loc.x, -layer.loc.y, size.w, size.h])
                } else {
                    self.id(root).and_then(|id| self.windows.get(&id)).map(|window|
                        [0, 0, window.configured_size[0], window.configured_size[1]])
                };
                let origin = bounds.map_or([popup.location().x, popup.location().y], |bounds|
                    placement([caret.loc.x, caret.loc.y, caret.size.w, caret.size.h], size, bounds));
                if [popup.location().x, popup.location().y] != origin {
                    popup.set_location(origin.into());
                    popup.surface_role.text_input_rectangle(caret.loc.x.saturating_sub(origin[0]), caret.loc.y.saturating_sub(origin[1]), caret.size.w, caret.size.h);
                }
                PopupSnapshot { surface: popup.wl_surface().clone(), origin, nodes }
            })
            .collect()
    }
}

impl Server {
    #[cfg(feature = "nested")]
    pub(crate) fn native_drop_target(&mut self, target: Option<crate::native_input::transfer::Target>) {
        if let Some(client) = &mut self.native_client { client.drop_target(target); }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn suspend_native_drop(&mut self) {
        if let Some(client) = &mut self.native_client { client.suspend_drop(); }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn finish_native_drop(&mut self, token: u64, accepted: bool) {
        if let Some(client) = &mut self.native_client { client.finish_drop(token, accepted); }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn native_timeout(&self) -> Option<std::time::Duration> {
        self.native_client.as_ref().and_then(|client| client.timeout())
    }

    #[cfg(feature = "nested")]
    pub(crate) fn native_editor(&mut self, editor: Option<crate::native_input::Editor>) {
        if let Some(client) = &mut self.native_client { client.editor(editor); }
    }

    #[cfg(feature = "nested")]
    pub(crate) fn native_events(&mut self) -> Vec<crate::native_input::client::Event> {
        self.native_client.as_mut().map_or_else(Vec::new, |client| client.events())
    }

    #[cfg(feature = "nested")]
    pub(crate) fn native_ready(&self) -> bool { self.native_client.as_ref().is_some_and(|client| client.ready()) }

    #[cfg(feature = "nested")]
    pub(crate) fn native_focus(&mut self) {
        self.focus_surface(self.state.native_surface.clone(), smithay::utils::SERIAL_COUNTER.next_serial());
    }

    #[cfg(feature = "nested")]
    pub(crate) fn native_surface(&self) -> Option<WlSurface> { self.state.native_surface.clone() }

    #[cfg(feature = "nested")]
    pub(crate) fn native_popups(&self) -> Vec<PopupSnapshot> {
        self.state.native_surface.as_ref().filter(|surface|
            self.state.seat.get_keyboard().and_then(|keyboard| keyboard.current_focus()).as_ref() == Some(*surface))
            .map_or_else(Vec::new, |surface| self.state.ime_snapshots(surface))
    }

    #[cfg(feature = "nested")]
    pub(crate) fn input_method_grabbed(&self) -> bool { self.state.seat.input_method().keyboard_grabbed() }
}

smithay::delegate_text_input_manager!(State);
smithay::delegate_input_method_manager!(State);
smithay::delegate_virtual_keyboard_manager!(State);

fn placement(caret: [i32; 4], size: [i32; 2], bounds: [i32; 4]) -> [i32; 2] {
    let below = caret[1].saturating_add(caret[3]);
    let bottom = bounds[1].saturating_add(bounds[3]);
    let y = if below.saturating_add(size[1]) > bottom { caret[1].saturating_sub(size[1]) } else { below };
    [caret[0].clamp(bounds[0], bounds[0].saturating_add(bounds[2].saturating_sub(size[0]).max(0))),
        y.clamp(bounds[1], bounds[1].saturating_add(bounds[3].saturating_sub(size[1]).max(0)))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_fit_the_parent_region_and_flip_above_the_caret() {
        assert_eq!(placement([700, 500, 2, 20], [200, 150], [640, 30, 800, 570]), [700, 350]);
        assert_eq!(placement([1400, 100, 2, 20], [200, 150], [640, 30, 800, 570]), [1240, 120]);
        assert_eq!(placement([650, 40, 2, 20], [900, 700], [640, 30, 800, 570]), [640, 30]);
    }
}
