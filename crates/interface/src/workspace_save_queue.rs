//! Reserve one save before collecting its payload, and release the reservation at handoff.

use std::{
    fmt::{self, Display, Formatter},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc},
};

pub(crate) enum ReservationMode {
    Wait,
    Defer,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum QueueError {
    Closed,
    ReservationLost,
}

impl Display for QueueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Closed => "Workspace saving stopped",
            Self::ReservationLost => "Workspace saving lost its queue reservation",
        })
    }
}

impl std::error::Error for QueueError {}

struct State<T> {
    sender: Option<mpsc::SyncSender<Queued<T>>>,
    reserved: bool,
}

pub(crate) struct SaveQueue<T> {
    state: Mutex<State<T>>,
    available: Condvar,
}

impl<T> SaveQueue<T> {
    pub(crate) fn channel() -> (Arc<Self>, mpsc::Receiver<Queued<T>>) {
        let (sender, receiver) = mpsc::sync_channel(1);
        let queue = Arc::new(Self {
            state: Mutex::new(State {
                sender: Some(sender),
                reserved: false,
            }),
            available: Condvar::new(),
        });
        (queue, receiver)
    }

    fn lock(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn reserve(
        self: &Arc<Self>,
        mode: ReservationMode,
    ) -> Result<Option<Slot<T>>, QueueError> {
        let mut state = self.lock();
        loop {
            if state.sender.is_none() {
                return Err(QueueError::Closed);
            }
            if !state.reserved {
                state.reserved = true;
                return Ok(Some(Slot(Arc::clone(self))));
            }
            if matches!(mode, ReservationMode::Defer) {
                return Ok(None);
            }
            state = self
                .available
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub(crate) fn close(&self) {
        let sender = self.lock().sender.take();
        self.available.notify_all();
        drop(sender);
    }
}

pub(crate) struct Slot<T>(Arc<SaveQueue<T>>);

impl<T> Slot<T> {
    pub(crate) fn send(self, request: T) -> Result<(), QueueError> {
        let queue = Arc::clone(&self.0);
        let queued = Queued {
            request,
            _slot: self,
        };
        // Sending can return the slot. Release the mutex before that slot drops.
        let sent = {
            let state = queue.lock();
            match &state.sender {
                Some(sender) => sender.try_send(queued),
                None => Err(mpsc::TrySendError::Disconnected(queued)),
            }
        };
        sent.map_err(|error| match error {
            mpsc::TrySendError::Disconnected(_) => QueueError::Closed,
            mpsc::TrySendError::Full(_) => QueueError::ReservationLost,
        })
    }
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        self.0.lock().reserved = false;
        self.0.available.notify_one();
    }
}

pub(crate) struct Queued<T> {
    request: T,
    _slot: Slot<T>,
}

impl<T> Queued<T> {
    /// Releases the slot so a later save can prepare while this request runs.
    pub(crate) fn start(self) -> T {
        self.request
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    const DEADLINE: Duration = Duration::from_secs(5);

    fn wait_for_slot(
        queue: &Arc<SaveQueue<u8>>,
    ) -> mpsc::Receiver<Result<Option<Slot<u8>>, QueueError>> {
        let (ready, started) = mpsc::channel();
        let (send, receive) = mpsc::channel();
        let queue = Arc::clone(queue);
        thread::spawn(move || {
            ready.send(()).expect("waiting test remains active");
            let _ = send.send(queue.reserve(ReservationMode::Wait));
        });
        started.recv_timeout(DEADLINE).expect("waiter started");
        receive
    }

    fn reserve(queue: &Arc<SaveQueue<u8>>) -> Slot<u8> {
        queue
            .reserve(ReservationMode::Defer)
            .expect("queue open")
            .expect("slot available")
    }

    #[test]
    fn queue_releases_the_slot_at_worker_handoff() {
        let (queue, worker) = SaveQueue::channel();
        let slot = reserve(&queue);
        assert!(
            queue
                .reserve(ReservationMode::Defer)
                .expect("open")
                .is_none()
        );
        slot.send(7).expect("admitted request sent");
        let request = worker.recv_timeout(DEADLINE).expect("request queued");
        assert!(
            queue
                .reserve(ReservationMode::Defer)
                .expect("open")
                .is_none()
        );

        let waiting = wait_for_slot(&queue);
        assert!(matches!(waiting.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert_eq!(request.start(), 7);
        let next = waiting
            .recv_timeout(DEADLINE)
            .expect("slot released")
            .expect("queue open")
            .expect("slot reserved");
        drop(next);
        reserve(&queue).send(3).expect("cancelled slot reusable");
        assert_eq!(
            worker
                .recv_timeout(DEADLINE)
                .expect("second request queued")
                .start(),
            3
        );
    }

    #[test]
    fn closing_wakes_waiters_and_disconnected_sends_release_slots() {
        let (queue, worker) = SaveQueue::channel();
        let slot = reserve(&queue);
        let waiting = wait_for_slot(&queue);
        queue.close();
        assert!(matches!(
            waiting.recv_timeout(DEADLINE),
            Ok(Err(QueueError::Closed))
        ));
        assert_eq!(slot.send(7), Err(QueueError::Closed));
        assert_eq!(
            worker.recv_timeout(DEADLINE).err(),
            Some(mpsc::RecvTimeoutError::Disconnected)
        );

        let (queue, worker) = SaveQueue::channel();
        let slot = reserve(&queue);
        drop(worker);
        assert_eq!(slot.send(7), Err(QueueError::Closed));
        drop(reserve(&queue));
    }
}
