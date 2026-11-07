use std::{
    io,
    num::NonZeroUsize,
    os::fd::OwnedFd,
    sync::{Arc, Mutex},
};

use super::{BufferData, Pool, ShmBufferUserData, ShmPoolUserData, ShmState, wl_bytes_per_pixel};

#[derive(Clone, Debug)]
pub struct SharedPool(Arc<Pool>);

impl SharedPool {
    pub fn from_pool(data: &ShmPoolUserData) -> Self {
        Self(data.inner.clone())
    }

    pub fn from_buffer(data: &ShmBufferUserData) -> io::Result<(Self, BufferData)> {
        if !data
            .destruction_hooks
            .lock()
            .map_err(|_| io::Error::other("Poisoned buffer hook lock"))?
            .is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Recreate SHM buffer destruction hooks before handoff",
            ));
        }
        Ok((Self(data.pool.clone()), data.data))
    }

    pub fn same_pool(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn checkpoint(&self) -> io::Result<(OwnedFd, NonZeroUsize)> {
        self.0.handoff_descriptor()
    }

    pub fn restore(descriptor: OwnedFd, size: NonZeroUsize) -> io::Result<Self> {
        if size.get() > i32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "SHM pool exceeds protocol size",
            ));
        }
        Pool::new(descriptor, size)
            .map(|pool| Self(Arc::new(pool)))
            .map_err(|_| io::Error::other("Could not remap preserved SHM pool"))
    }

    pub fn pool_data(&self) -> ShmPoolUserData {
        ShmPoolUserData {
            inner: self.0.clone(),
        }
    }

    pub fn buffer_data(&self, owner: &ShmState, data: BufferData) -> io::Result<ShmBufferUserData> {
        let bytes_per_pixel = wl_bytes_per_pixel(wayland_server::WEnum::Value(data.format));
        let extent = data
            .stride
            .checked_mul(data.height)
            .and_then(|length| data.offset.checked_add(length));
        if data.offset < 0
            || data.width <= 0
            || data.height <= 0
            || data.stride <= 0
            || bytes_per_pixel <= 0
            || data.stride / bytes_per_pixel < data.width
            || !owner.formats.contains(&data.format)
            || extent.is_none_or(|end| end < 0 || end as usize > self.0.size())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid restored SHM buffer layout or format",
            ));
        }
        Ok(ShmBufferUserData {
            pool: self.0.clone(),
            data,
            destruction_hooks: Mutex::new(Vec::new()),
        })
    }
}
