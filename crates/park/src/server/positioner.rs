use super::*;
use smithay::{
    reexports::{
        wayland_protocols::xdg::shell::server::{
            xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
        },
        wayland_server::{DataInit, Dispatch, delegate_dispatch, delegate_global_dispatch},
    },
    wayland::shell::xdg::{
        XdgPositionerUserData, XdgShellSurfaceUserData, XdgSurfaceUserData, XdgWmBaseUserData,
    },
};

delegate_global_dispatch!(State: [xdg_wm_base::XdgWmBase: ()] => XdgShellState);
delegate_dispatch!(State: [xdg_wm_base::XdgWmBase: XdgWmBaseUserData] => XdgShellState);
delegate_dispatch!(State: [xdg_popup::XdgPopup: XdgShellSurfaceUserData] => XdgShellState);
delegate_dispatch!(State: [xdg_toplevel::XdgToplevel: XdgShellSurfaceUserData] => XdgShellState);

impl Dispatch<xdg_positioner::XdgPositioner, XdgPositionerUserData> for State {
    fn request(
        state: &mut Self,
        client: &Client,
        positioner: &xdg_positioner::XdgPositioner,
        request: xdg_positioner::Request,
        data: &XdgPositionerUserData,
        display: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        let dimension = |value| (1..=16384).contains(&value);
        let coordinate = |value| (-32768..=32768).contains(&value);
        let valid = match &request {
            xdg_positioner::Request::SetSize { width, height } => {
                dimension(*width) && dimension(*height)
            }
            xdg_positioner::Request::SetAnchorRect {
                x,
                y,
                width,
                height,
            } => coordinate(*x) && coordinate(*y) && dimension(*width) && dimension(*height),
            xdg_positioner::Request::SetOffset { x, y } => coordinate(*x) && coordinate(*y),
            xdg_positioner::Request::SetParentSize {
                parent_width,
                parent_height,
            } => dimension(*parent_width) && dimension(*parent_height),
            _ => true,
        };
        if !valid {
            positioner.post_error(
                xdg_positioner::Error::InvalidInput,
                "Positioner geometry exceeds Park's supported bounds",
            );
            return;
        }
        <XdgShellState as Dispatch<xdg_positioner::XdgPositioner, XdgPositionerUserData, Self>>::request(state, client, positioner, request, data, display, init);
    }
}

impl Dispatch<xdg_surface::XdgSurface, XdgSurfaceUserData> for State {
    fn request(
        state: &mut Self,
        client: &Client,
        surface: &xdg_surface::XdgSurface,
        request: xdg_surface::Request,
        data: &XdgSurfaceUserData,
        display: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let xdg_surface::Request::SetWindowGeometry {
            x,
            y,
            width,
            height,
        } = &request
            && (!(-32768..=32768).contains(x)
                || !(-32768..=32768).contains(y)
                || !(1..=16384).contains(width)
                || !(1..=16384).contains(height))
        {
            surface.post_error(
                xdg_surface::Error::InvalidSize,
                "Window geometry exceeds Park's supported bounds",
            );
            return;
        }
        <XdgShellState as Dispatch<xdg_surface::XdgSurface, XdgSurfaceUserData, Self>>::request(
            state, client, surface, request, data, display, init,
        );
    }

    fn destroyed(
        state: &mut Self,
        client: ClientId,
        surface: &xdg_surface::XdgSurface,
        data: &XdgSurfaceUserData,
    ) {
        <XdgShellState as Dispatch<xdg_surface::XdgSurface, XdgSurfaceUserData, Self>>::destroyed(
            state, client, surface, data,
        );
    }
}
