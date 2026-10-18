use serde::{Deserialize, Serialize};
use std::{
    num::NonZeroUsize,
    sync::{
        Arc, Mutex, Weak,
        mpsc::{Receiver, SendError, SyncSender, sync_channel},
    },
};

#[cfg_attr(feature = "ui", derive(bevy::prelude::Resource))]
#[derive(Clone)]
pub struct WakeSignal(Arc<dyn Fn() + Send + Sync>);

impl WakeSignal {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn ring(&self) {
        (self.0)();
    }

    #[cfg(any(feature = "ui", feature = "activation"))]
    pub fn after(&self, delay: std::time::Duration) {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let wake = self.clone();
            runtime.spawn(async move {
                tokio::time::sleep(delay).await;
                wake.ring();
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities {
    pub application_windows: bool,
    pub output_management: bool,
    pub session_control: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SurfaceId(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowState {
    pub minimized: bool,
    pub fullscreen: bool,
    pub maximized: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WindowRequest {
    pub minimized: Option<bool>,
    pub fullscreen: Option<bool>,
    pub maximized: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutputId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemId {
    Native([u8; 16]),
    Application(SurfaceId),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceLayout {
    Free,
    Row,
    Column,
    Grid,
    ScrollingColumns,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopAction {
    Focus(Direction),
    Move(Direction),
    Stack(Direction),
    ResizeColumn(i32),
    ResizeItem(i32),
    CycleColumnWidth,
    SetLayout(WorkspaceLayout),
    ToggleFloating,
    ToggleFullscreen,
    ToggleMaximized,
    Close,
    OpenOperation,
    OpenTerminal,
    OpenLauncher,
    Workspace(u32),
    MoveToWorkspace(u32),
    FocusOutput(Direction),
    MoveToOutput(Direction),
    MoveWorkspaceToOutput(Direction),
    InspectWindowRecord,
    Lock,
    Logout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangle {
    origin: [f64; 2],
    size: [f64; 2],
}

impl Rectangle {
    pub fn new(origin: [f64; 2], size: [f64; 2]) -> Option<Self> {
        origin
            .iter()
            .chain(size.iter())
            .all(|value| value.is_finite())
            .then_some(())?;
        (size[0] > 0.0 && size[1] > 0.0).then_some(())?;
        origin
            .iter()
            .zip(size)
            .all(|(origin, size)| (origin + size).is_finite())
            .then_some(Self { origin, size })
    }

    pub fn origin(self) -> [f64; 2] {
        self.origin
    }

    pub fn size(self) -> [f64; 2] {
        self.size
    }

    pub fn contains(self, point: [f64; 2]) -> bool {
        (0..2).all(|axis| {
            point[axis] >= self.origin[axis] && point[axis] < self.origin[axis] + self.size[axis]
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentMapping {
    pub canvas: Rectangle,
    pub surface: Rectangle,
}

impl ContentMapping {
    pub fn surface_point(self, canvas_point: [f64; 2]) -> Option<[f64; 2]> {
        self.canvas.contains(canvas_point).then(|| {
            std::array::from_fn(|axis| {
                self.surface.origin[axis]
                    + (canvas_point[axis] - self.canvas.origin[axis]) / self.canvas.size[axis]
                        * self.surface.size[axis]
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutputViewport {
    pub id: OutputId,
    pub bounds: Rectangle,
    pub usable: Option<Rectangle>,
    pub physical_size: [u32; 2],
    pub workspace: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemGeometry {
    pub id: ItemId,
    pub output: OutputId,
    pub mapping: ContentMapping,
    pub floating: bool,
    pub clip: Option<Vec<Vec<[f64; 2]>>>,
}

impl ItemGeometry {
    pub fn surface_point(&self, point: [f64; 2]) -> Option<[f64; 2]> {
        if self.clip.as_ref().is_some_and(|polygons| {
            !polygons.iter().any(|polygon| {
                polygon.len() >= 3
                    && polygon
                        .iter()
                        .zip(polygon.iter().cycle().skip(1))
                        .take(polygon.len())
                        .all(|(a, b)| {
                            (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0])
                                >= -1e-6
                        })
            })
        }) {
            return None;
        }
        self.mapping.surface_point(point)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometrySnapshot {
    pub revision: u64,
    pub outputs: Vec<OutputViewport>,
    pub items: Vec<ItemGeometry>,
    pub focused: Option<ItemId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostEvent {
    Capabilities(Capabilities),
    WindowOpened {
        surface: SurfaceId,
        title: String,
        app_id: String,
    },
    WindowMetadata {
        surface: SurfaceId,
        title: String,
        app_id: String,
    },
    WindowClosed(SurfaceId),
    ActionFailed {
        request: u64,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionRequest {
    pub id: u64,
    pub action: DesktopAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRequest {
    DropFinished { token: u64, accepted: bool },
    Close(SurfaceId),
    Action(DesktopAction),
    Exit,
}

pub struct PresentedFrame<T> {
    pub output: OutputId,
    pub geometry_revision: u64,
    pub image: T,
}

pub struct LatestSender<T> {
    slot: Weak<Mutex<Option<T>>>,
}

pub struct LatestReceiver<T> {
    slot: Arc<Mutex<Option<T>>>,
}

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
    pub fn publish(&self, value: T) -> Result<Option<T>, SendError<T>> {
        let Some(slot) = self.slot.upgrade() else {
            return Err(SendError(value));
        };
        let mut pending = slot.lock().unwrap_or_else(|error| error.into_inner());
        Ok(pending.replace(value))
    }
}

impl<T> LatestReceiver<T> {
    pub fn take(&self) -> Option<T> {
        self.slot
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

pub struct HostBridge {
    pub events: SyncSender<HostEvent>,
    pub actions: Receiver<ActionRequest>,
    pub geometry: LatestSender<GeometrySnapshot>,
}

pub struct PresentationBridge {
    pub events: Receiver<HostEvent>,
    pub actions: SyncSender<ActionRequest>,
    pub geometry: LatestReceiver<GeometrySnapshot>,
}

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
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::TrySendError,
    };

    #[test]
    fn mapped_input_uses_the_published_content_rectangle() {
        let mapping = ContentMapping {
            canvas: Rectangle::new([-400.0, 20.0], [800.0, 600.0]).unwrap(),
            surface: Rectangle::new([12.0, 8.0], [400.0, 300.0]).unwrap(),
        };
        assert_eq!(mapping.surface_point([0.0, 320.0]), Some([212.0, 158.0]));
        assert_eq!(mapping.surface_point([-400.0, 20.0]), Some([12.0, 8.0]));
        assert_eq!(mapping.surface_point([400.0, 320.0]), None);
        assert_eq!(mapping.surface_point([0.0, f64::NAN]), None);
    }

    #[test]
    fn invalid_geometry_cannot_enter_the_mapping_contract() {
        for (origin, size) in [
            ([f64::NAN, 0.0], [100.0, 100.0]),
            ([0.0, 0.0], [0.0, 100.0]),
            ([0.0, 0.0], [-1.0, 100.0]),
            ([f64::MAX, 0.0], [f64::MAX, 100.0]),
        ] {
            assert!(Rectangle::new(origin, size).is_none());
        }
    }

    #[test]
    fn clipped_application_geometry_does_not_receive_input_outside_a_hole() {
        let rectangle = Rectangle::new([0.0; 2], [200.0; 2]).unwrap();
        let item = ItemGeometry {
            id: ItemId::Application(SurfaceId(1)),
            output: OutputId(1),
            mapping: ContentMapping {
                canvas: rectangle,
                surface: rectangle,
            },
            floating: false,
            clip: Some(vec![vec![
                [50.0, 50.0],
                [150.0, 50.0],
                [150.0, 150.0],
                [50.0, 150.0],
            ]]),
        };
        assert_eq!(item.surface_point([100.0; 2]), Some([100.0; 2]));
        assert_eq!(item.surface_point([25.0; 2]), None);
        assert_eq!(item.surface_point([f64::NAN, 100.0]), None);
    }

    #[test]
    fn lifecycle_and_action_queues_report_full_without_blocking() {
        let (host, presentation) = bridge(NonZeroUsize::new(1).unwrap());
        host.events
            .try_send(HostEvent::WindowClosed(SurfaceId(1)))
            .unwrap();
        assert!(matches!(
            host.events.try_send(HostEvent::WindowClosed(SurfaceId(2))),
            Err(TrySendError::Full(_))
        ));
        assert_eq!(
            presentation.events.try_recv().unwrap(),
            HostEvent::WindowClosed(SurfaceId(1))
        );
        let action = ActionRequest {
            id: 1,
            action: DesktopAction::Close,
        };
        presentation.actions.try_send(action).unwrap();
        assert!(matches!(
            presentation.actions.try_send(action),
            Err(TrySendError::Full(_))
        ));
        assert_eq!(host.actions.try_recv().unwrap(), action);
    }

    #[test]
    fn obsolete_resources_are_returned_to_the_publisher() {
        struct Image(Arc<AtomicUsize>);
        impl Drop for Image {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = latest_channel();
        assert!(sender.publish(Image(drops.clone())).is_ok());
        let old = sender.publish(Image(drops.clone())).ok().unwrap();
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        drop(old);
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(receiver.take());
        assert!(receiver.take().is_none());
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        drop(receiver);
        assert!(sender.publish(Image(drops.clone())).is_err());
        assert_eq!(drops.load(Ordering::Relaxed), 3);
    }
}
