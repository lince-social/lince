use super::{
    CompositorClientState, CompositorState, SurfaceAttributes, SurfaceUserData, erase_commit_hook,
    erase_destruction_hook,
};

pub use super::tree::SuggestedSurfaceState;

#[derive(Clone, Debug)]
pub struct EmptyQueueCheckpoint {
    pub transaction_capacity: usize,
    pub seen_capacity: usize,
    pub seen_surfaces: Vec<u32>,
}

#[derive(Clone, Debug)]
pub struct ClientCheckpoint {
    pub scale: f64,
    pub queue: Option<EmptyQueueCheckpoint>,
}

#[derive(Clone, Debug)]
pub struct IsolatedSurfaceCheckpoint {
    pub role: Option<String>,
    pub next_transaction: u32,
}

impl CompositorClientState {
    #[doc(hidden)]
    pub fn handoff_state(&self) -> io::Result<ClientCheckpoint> {
        let queue = self
            .queue
            .lock()
            .map_err(|_| io::Error::other("Poisoned compositor client checkpoint lock"))?;
        Ok(ClientCheckpoint {
            scale: self.client_scale(),
            queue: queue
                .as_ref()
                .map(|queue| queue.handoff_empty())
                .transpose()?,
        })
    }

    #[doc(hidden)]
    pub fn from_handoff(checkpoint: ClientCheckpoint) -> io::Result<Self> {
        if !checkpoint.scale.is_finite() || checkpoint.scale <= 0.0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid restored client scale",
            ));
        }
        let restored = Self::default();
        restored.set_client_scale(checkpoint.scale);
        *restored
            .queue
            .lock()
            .map_err(|_| io::Error::other("Poisoned compositor client restore lock"))? = checkpoint
            .queue
            .map(super::TransactionQueue::restore_empty)
            .transpose()?;
        Ok(restored)
    }
}

pub fn isolated_surface(surface: &WlSurface) -> io::Result<IsolatedSurfaceCheckpoint> {
    let data = surface.data::<SurfaceUserData>().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Surface has no compositor data")
    })?;
    data.inner
        .lock()
        .map_err(|_| io::Error::other("Poisoned surface checkpoint lock"))?
        .handoff_isolated(surface)
}

pub fn restore_isolated_surface(
    surface: &WlSurface,
    checkpoint: IsolatedSurfaceCheckpoint,
    role: impl FnOnce(&str) -> Option<&'static str>,
) -> io::Result<()> {
    let data = surface.data::<SurfaceUserData>().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Surface has no restored compositor data",
        )
    })?;
    data.inner
        .lock()
        .map_err(|_| io::Error::other("Poisoned surface restore lock"))?
        .restore_isolated(surface, checkpoint, role)
}
use crate::utils::{CallbackIdentity, hook::HandoffCallback};
use std::io;
use wayland_server::{DisplayHandle, Resource, protocol::wl_surface::WlSurface};

impl CompositorState {
    #[doc(hidden)]
    pub fn register_handoff_surface<D: 'static>(&mut self, surface: &WlSurface) -> io::Result<()> {
        let data = surface.data::<SurfaceUserData>().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Surface has no restored compositor data",
            )
        })?;
        if !surface.is_alive() || data.user_state_type.0 != std::any::TypeId::of::<D>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Restored surface is dead or has the wrong dispatch state",
            ));
        }
        if self.surfaces.contains(surface) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Surface is already registered",
            ));
        }
        data.inner
            .lock()
            .map_err(|_| io::Error::other("Poisoned surface restore lock"))?
            .initialize_handoff(surface)?;
        self.surfaces.push(surface.clone());
        Ok(())
    }
}

impl SurfaceAttributes {
    #[doc(hidden)]
    pub fn handoff_client_scale(&self) -> f64 {
        self.client_scale
    }

    #[doc(hidden)]
    pub fn restore_client_scale(&mut self, scale: f64) -> io::Result<()> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid restored client scale",
            ));
        }
        self.client_scale = scale;
        Ok(())
    }
}

#[derive(Debug)]
pub struct SurfaceHooks {
    pub pre_commit: Vec<CallbackIdentity>,
    pub post_commit: Vec<CallbackIdentity>,
    pub destruction: Vec<CallbackIdentity>,
    pub capacities: [usize; 3],
}

pub fn reserve_hooks(surface: &WlSurface, capacities: [usize; 3]) -> io::Result<()> {
    let data = surface.data::<SurfaceUserData>().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Surface has no restored compositor data",
        )
    })?;
    data.inner
        .lock()
        .map_err(|_| io::Error::other("Poisoned surface hook restore lock"))?
        .reserve_handoff_hooks(capacities)
}

pub fn surface_hooks(surface: &WlSurface) -> io::Result<SurfaceHooks> {
    let data = surface.data::<SurfaceUserData>().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Surface has no compositor user data",
        )
    })?;
    let data = data
        .inner
        .lock()
        .map_err(|_| io::Error::other("Poisoned surface checkpoint lock"))?;
    Ok(data.handoff_hooks())
}

pub fn commit_callback_identity<D, F>(callback: F) -> CallbackIdentity
where
    F: Fn(&mut D, &DisplayHandle, &WlSurface) + Send + Sync + 'static,
    D: 'static,
{
    erase_commit_hook::<D, F>(callback).handoff_identity()
}

pub fn destruction_callback_identity<D, F>(callback: F) -> CallbackIdentity
where
    F: Fn(&mut D, &WlSurface) + Send + Sync + 'static,
    D: 'static,
{
    erase_destruction_hook::<D, F>(callback).handoff_identity()
}
