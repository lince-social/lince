use super::*;
use smithay::{
    delegate_xdg_decoration,
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode,
    wayland::shell::xdg::decoration::XdgDecorationHandler,
};

impl State {
    fn client_decoration(&self, surface: ToplevelSurface) {
        surface.with_pending_state(|state| state.decoration_mode = Some(Mode::ClientSide));
        if self
            .id(surface.wl_surface())
            .is_some_and(|id| !self.windows[&id].needs_initial)
        {
            surface.send_pending_configure();
        }
    }
}

impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, surface: ToplevelSurface) {
        self.client_decoration(surface);
    }

    fn request_mode(&mut self, surface: ToplevelSurface, _: Mode) {
        self.client_decoration(surface);
    }

    fn unset_mode(&mut self, surface: ToplevelSurface) {
        self.client_decoration(surface);
    }
}

delegate_xdg_decoration!(State);
