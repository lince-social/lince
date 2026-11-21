//! Shared desktop contracts, presentation geometry, and host services.

use std::{fmt, sync::Arc};

mod bridge;
mod desktop;
mod geometry;

pub use bridge::{
    HostBridge, LatestReceiver, LatestSender, PresentationBridge, PresentedFrame, bridge,
    latest_channel,
};
pub use desktop::{
    ActionRequest, Capabilities, DesktopAction, Direction, FileDragContext, FileDragSupport,
    HostEvent, HostRequest, ItemId, Manipulation, OpeningRequest, OpeningRevision, OpeningView,
    OutputId, OverviewHoverRequest, SurfaceId, WindowInspection, WindowRequest, WindowState,
    WorkspaceLayout,
};
pub use geometry::{
    CanvasProjection, ContentMapping, GeometrySnapshot, ItemGeometry, OutputViewport, Rectangle,
};

pub mod capture;
pub mod color;
pub mod control;
pub mod display;
pub mod external;
pub mod input;
pub mod launch;
pub mod magnification;
pub mod mapping;
pub mod performance;
pub mod raster;

#[cfg(feature = "layout")]
pub mod layout;

#[cfg(feature = "test-driver")]
pub mod testing;

#[cfg(feature = "wake-timer")]
mod deadline;
#[cfg(feature = "wake-timer")]
pub use deadline::WakeDeadline;

#[cfg_attr(feature = "ui", derive(bevy_ecs::prelude::Resource))]
#[derive(Clone)]
/// Shared callback that wakes the presentation loop.
pub struct WakeSignal(Arc<dyn Fn() + Send + Sync>);

impl fmt::Debug for WakeSignal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad("WakeSignal")
    }
}

impl WakeSignal {
    /// Wraps a callback that can be signaled from other threads.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    /// Invokes the callback on the current thread.
    pub fn ring(&self) {
        (self.0)();
    }

    /// Records the wake reason before invoking the callback.
    pub fn ring_reason(&self, reason: performance::Count) {
        performance::count(reason);
        self.ring();
    }

    #[cfg(feature = "wake-timer")]
    /// Schedules a wake when called within a Tokio runtime.
    pub fn after(&self, delay: std::time::Duration) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            performance::count(performance::Count::TimerScheduled);
            let wake = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                wake.ring_reason(performance::Count::TimerFired);
            });
        }
    }
}
