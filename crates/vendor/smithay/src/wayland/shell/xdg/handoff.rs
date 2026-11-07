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
