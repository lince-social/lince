//! Bounded events and actions, plus a latest-value slot for geometry and frames.

use super::{ActionRequest, GeometrySnapshot, HostEvent, OutputId};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, Weak,
        mpsc::{Receiver, SendError, SyncSender, sync_channel},
    },
};

/// An output image tied to the geometry revision used to render it.
pub struct PresentedFrame<T> {
    pub output: OutputId,
    pub geometry_revision: u64,
    pub image: T,
}

/// Publishes values while a receiver owns the slot.
pub struct LatestSender<T> {
    slot: Weak<Mutex<Option<T>>>,
}

/// Owns the pending value and takes it at most once.
pub struct LatestReceiver<T> {
    slot: Arc<Mutex<Option<T>>>,
}

/// Creates a slot that retains a single value, replacing it on publication.
pub fn latest_channel<T>() -> (LatestSender<T>, LatestReceiver<T>) {
    let slot = Arc::new(Mutex::new(None));
    (
        LatestSender {
            slot: Arc::downgrade(&slot),
        },
        LatestReceiver { slot },
    )
}

impl<T> LatestSender<T> {
    /// Publishes a value and returns the previous one for disposal by the publisher.
    ///
    /// # Errors
    ///
    /// Returns the submitted value in a `SendError` if the receiver is gone.
    pub fn publish(&self, value: T) -> Result<Option<T>, SendError<T>> {
        let Some(slot) = self.slot.upgrade() else {
            return Err(SendError(value));
        };
        Ok(slot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .replace(value))
    }
}

impl<T> LatestReceiver<T> {
    /// Removes the pending value, leaving the slot ready for the next publication.
    pub fn take(&self) -> Option<T> {
        self.slot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

/// The desktop host's end of the presentation exchange.
pub struct HostBridge {
    pub events: SyncSender<HostEvent>,
    pub actions: Receiver<ActionRequest>,
    pub geometry: LatestSender<GeometrySnapshot>,
}

/// The presentation's end of the desktop host exchange.
pub struct PresentationBridge {
    pub events: Receiver<HostEvent>,
    pub actions: SyncSender<ActionRequest>,
    pub geometry: LatestReceiver<GeometrySnapshot>,
}

/// Connects the host and presentation with the given capacity in each direction.
pub fn bridge(capacity: NonZeroUsize) -> (HostBridge, PresentationBridge) {
    let (event_tx, event_rx) = sync_channel(capacity.get());
    let (action_tx, action_rx) = sync_channel(capacity.get());
    let (geometry_tx, geometry_rx) = latest_channel();
    (
        HostBridge {
            events: event_tx,
            actions: action_rx,
            geometry: geometry_tx,
        },
        PresentationBridge {
            events: event_rx,
            actions: action_tx,
            geometry: geometry_rx,
        },
    )
}

#[cfg(test)]
mod tests;
