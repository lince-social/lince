use super::*;
use crate::{DesktopAction, SurfaceId};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc::TrySendError,
};

#[test]
fn lifecycle_and_action_queues_report_full_without_blocking() {
    let (host, presentation) = bridge(NonZeroUsize::MIN);
    host.events
        .try_send(HostEvent::WindowClosed(SurfaceId(1)))
        .unwrap();
    assert!(matches!(
        host.events.try_send(HostEvent::WindowClosed(SurfaceId(2))),
        Err(TrySendError::Full(_))
    ));
    assert_eq!(
        presentation.events.try_recv(),
        Ok(HostEvent::WindowClosed(SurfaceId(1)))
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
    assert_eq!(host.actions.try_recv(), Ok(action));
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
