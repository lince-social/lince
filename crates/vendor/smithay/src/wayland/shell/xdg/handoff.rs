use super::{PopupSurface, ToplevelSurface};
use crate::utils::{CallbackIdentity, HookId};
use crate::wayland::compositor;
use wayland_server::protocol::wl_surface::WlSurface;

pub fn toplevel_commit_identity<D: 'static>() -> CallbackIdentity {
    compositor::handoff::commit_callback_identity::<D, _>(ToplevelSurface::commit_hook)
}

pub fn restore_toplevel_commit_hook<D: 'static>(surface: &WlSurface) -> HookId {
    compositor::add_post_commit_hook::<D, _>(surface, ToplevelSurface::commit_hook)
}

pub fn popup_pre_commit_identity<D: 'static>() -> CallbackIdentity {
    compositor::handoff::commit_callback_identity::<D, _>(PopupSurface::pre_commit_hook)
}

pub fn popup_post_commit_identity<D: 'static>() -> CallbackIdentity {
    compositor::handoff::commit_callback_identity::<D, _>(PopupSurface::post_commit_hook)
}

pub fn restore_popup_commit_hooks<D: 'static>(surface: &WlSurface) -> (HookId, HookId) {
    (
        compositor::add_pre_commit_hook::<D, _>(surface, PopupSurface::pre_commit_hook),
        compositor::add_post_commit_hook::<D, _>(surface, PopupSurface::post_commit_hook),
    )
}

use super::{XdgShellState, XdgShellSurfaceUserData, XdgSurfaceUserData, XdgWmBaseUserData};
use crate::utils::Serial;
use indexmap::IndexSet;
use std::{
    any::Any,
    io,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use wayland_protocols::xdg::{
    decoration::zv1::server::zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1,
    dialog::v1::server::xdg_dialog_v1::XdgDialogV1,
    shell::server::{xdg_surface::XdgSurface, xdg_toplevel::XdgToplevel, xdg_wm_base::XdgWmBase},
};
use wayland_server::{Resource, Weak};

#[derive(Debug)]
pub struct WmBaseParts {
    pub pending_ping: Option<Serial>,
    pub known_surfaces: Vec<Weak<XdgSurface>>,
    pub known_capacity: usize,
}

pub fn wm_base_parts(
    data: &XdgWmBaseUserData,
    encode_user_data: impl FnMut(&dyn Any) -> io::Result<()>,
) -> io::Result<WmBaseParts> {
    let client = data
        .client_data
        .lock()
        .map_err(|_| io::Error::other("Poisoned shell client checkpoint lock"))?;
    client.data.handoff_visit(encode_user_data)?;
    let surfaces = data
        .known_surfaces
        .lock()
        .map_err(|_| io::Error::other("Poisoned shell surface checkpoint lock"))?;
    Ok(WmBaseParts {
        pending_ping: client.pending_ping,
        known_surfaces: surfaces.iter().cloned().collect(),
        known_capacity: surfaces.capacity(),
    })
}

pub fn wm_base_data(
    pending_ping: Option<Serial>,
    restore_user_data: impl FnOnce(&crate::utils::user_data::UserDataMap) -> io::Result<()>,
) -> io::Result<XdgWmBaseUserData> {
    let data = XdgWmBaseUserData::default();
    {
        let mut client = data
            .client_data
            .lock()
            .map_err(|_| io::Error::other("Poisoned shell client restore lock"))?;
        client.pending_ping = pending_ping;
        restore_user_data(&client.data)?;
    }
    Ok(data)
}

pub fn restore_known_surfaces(
    base: &XdgWmBase,
    capacity: usize,
    surfaces: Vec<Weak<XdgSurface>>,
) -> io::Result<()> {
    if capacity > 1048576 || surfaces.len() > capacity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Shell surface list exceeds checkpoint limits",
        ));
    }
    let data = base.data::<XdgWmBaseUserData>().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Shell base has no restored data",
        )
    })?;
    let mut restored = IndexSet::with_capacity(capacity);
    for surface in surfaces {
        let object = surface.upgrade().map_err(io::Error::other)?;
        if object
            .data::<XdgSurfaceUserData>()
            .is_none_or(|data| data.wm_base != *base)
            || !restored.insert(surface)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Duplicate shell surface or a different shell owner",
            ));
        }
    }
    let mut known = data
        .known_surfaces
        .lock()
        .map_err(|_| io::Error::other("Poisoned shell surface restore lock"))?;
    if !known.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Shell surface list is already initialized",
        ));
    }
    *known = restored;
    Ok(())
}

#[derive(Debug)]
pub struct SurfaceLinks {
    pub wl_surface: WlSurface,
    pub wm_base: XdgWmBase,
    pub active_role: bool,
}

pub fn surface_links(data: &XdgSurfaceUserData) -> SurfaceLinks {
    SurfaceLinks {
        wl_surface: data.wl_surface.clone(),
        wm_base: data.wm_base.clone(),
        active_role: data.has_active_role.load(Ordering::Acquire),
    }
}

pub fn surface_data(links: SurfaceLinks) -> io::Result<XdgSurfaceUserData> {
    let data = links.wm_base.data::<XdgWmBaseUserData>().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "XDG surface has no restored shell base",
        )
    })?;
    if !links.wl_surface.is_alive()
        || !links.wm_base.is_alive()
        || !links.wl_surface.id().same_client_as(&links.wm_base.id())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "XDG surface refers to dead resources or different clients",
        ));
    }
    let known_surfaces = data.known_surfaces.clone();
    Ok(XdgSurfaceUserData {
        known_surfaces,
        wl_surface: links.wl_surface,
        wm_base: links.wm_base,
        has_active_role: AtomicBool::new(links.active_role),
    })
}

#[derive(Debug)]
pub struct ShellSurfaceLinks {
    pub wl_surface: WlSurface,
    pub wm_base: XdgWmBase,
    pub xdg_surface: XdgSurface,
    pub decoration: Option<ZxdgToplevelDecorationV1>,
    pub dialog: Option<XdgDialogV1>,
}

pub fn shell_surface_links(data: &XdgShellSurfaceUserData) -> io::Result<ShellSurfaceLinks> {
    Ok(ShellSurfaceLinks {
        wl_surface: data.wl_surface.clone(),
        wm_base: data.wm_base.clone(),
        xdg_surface: data.xdg_surface.clone(),
        decoration: data
            .decoration
            .lock()
            .map_err(|_| io::Error::other("Poisoned decoration checkpoint lock"))?
            .clone(),
        dialog: data
            .dialog
            .lock()
            .map_err(|_| io::Error::other("Poisoned dialog checkpoint lock"))?
            .clone(),
    })
}

pub fn shell_surface_data(links: ShellSurfaceLinks) -> io::Result<XdgShellSurfaceUserData> {
    let data = links
        .xdg_surface
        .data::<XdgSurfaceUserData>()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Shell role has no restored XDG surface",
            )
        })?;
    if data.wl_surface != links.wl_surface
        || data.wm_base != links.wm_base
        || !links.xdg_surface.is_alive()
        || !data.has_active_role.load(Ordering::Acquire)
        || links.decoration.as_ref().is_some_and(|object| {
            !object.is_alive() || !object.id().same_client_as(&links.xdg_surface.id())
        })
        || links.dialog.as_ref().is_some_and(|object| {
            !object.is_alive() || !object.id().same_client_as(&links.xdg_surface.id())
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Shell role links differ from their surface or client",
        ));
    }
    Ok(XdgShellSurfaceUserData {
        wl_surface: links.wl_surface,
        wm_base: links.wm_base,
        xdg_surface: links.xdg_surface,
        decoration: Mutex::new(links.decoration),
        dialog: Mutex::new(links.dialog),
        alive_tracker: Default::default(),
    })
}

impl XdgShellState {
    #[doc(hidden)]
    pub fn register_handoff_toplevel(
        &mut self,
        resource: &XdgToplevel,
    ) -> io::Result<ToplevelSurface> {
        let data = resource.data::<XdgShellSurfaceUserData>().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Toplevel has no restored role data",
            )
        })?;
        if !resource.is_alive()
            || self
                .known_toplevels
                .iter()
                .any(|known| known.shell_surface == *resource)
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Toplevel is dead or already registered",
            ));
        }
        let surface = ToplevelSurface {
            wl_surface: data.wl_surface.clone(),
            shell_surface: resource.clone(),
        };
        self.known_toplevels.push(surface.clone());
        Ok(surface)
    }
}

pub fn toplevel_states_parts(
    states: &super::ToplevelStateSet,
) -> (
    &[wayland_protocols::xdg::shell::server::xdg_toplevel::State],
    usize,
) {
    (&states.states, states.states.capacity())
}

pub fn toplevel_states_data(
    values: Vec<wayland_protocols::xdg::shell::server::xdg_toplevel::State>,
    capacity: usize,
) -> io::Result<super::ToplevelStateSet> {
    if capacity > 4096 || values.len() > capacity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Toplevel state allocation exceeds checkpoint limit",
        ));
    }
    let mut states = Vec::with_capacity(capacity);
    for state in values {
        if states.contains(&state) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Duplicate restored toplevel state",
            ));
        }
        states.push(state);
    }
    Ok(super::ToplevelStateSet { states })
}

pub fn capabilities_parts(
    capabilities: &super::WmCapabilitySet,
) -> (
    Vec<wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities>,
    usize,
) {
    (
        capabilities.capabilities.iter().copied().collect(),
        capabilities.capabilities.capacity(),
    )
}

pub fn capabilities_data(
    values: Vec<wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities>,
    capacity: usize,
) -> io::Result<super::WmCapabilitySet> {
    if capacity > 4096 || values.len() > capacity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Toplevel capability allocation exceeds checkpoint limit",
        ));
    }
    let mut capabilities = std::collections::HashSet::with_capacity(capacity);
    for capability in values {
        if !capabilities.insert(capability) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Duplicate restored toplevel capability",
            ));
        }
    }
    Ok(super::WmCapabilitySet { capabilities })
}

impl XdgShellState {
    #[doc(hidden)]
    pub fn handoff_manager_layout(
        &self,
    ) -> (
        [usize; 2],
        Vec<wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities>,
        usize,
    ) {
        let (capabilities, capacity) = capabilities_parts(&self.default_capabilities);
        (
            [
                self.known_toplevels.capacity(),
                self.known_popups.capacity(),
            ],
            capabilities,
            capacity,
        )
    }

    #[doc(hidden)]
    pub fn restore_handoff_manager_layout(
        &mut self,
        capacities: [usize; 2],
        capabilities: Vec<wayland_protocols::xdg::shell::server::xdg_toplevel::WmCapabilities>,
        capability_capacity: usize,
    ) -> io::Result<()> {
        if capacities.into_iter().any(|capacity| capacity > 65536) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Shell list allocation exceeds checkpoint limits",
            ));
        }
        if !self.known_toplevels.is_empty() || !self.known_popups.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Shell lists are already initialized",
            ));
        }
        let capabilities = capabilities_data(capabilities, capability_capacity)?;
        self.known_toplevels = Vec::with_capacity(capacities[0]);
        self.known_popups = Vec::with_capacity(capacities[1]);
        self.default_capabilities = capabilities;
        Ok(())
    }
}
