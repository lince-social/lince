use cell::ClientMessage;
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct Sender(Kind);

#[derive(Clone)]
enum Kind {
    Ordered(mpsc::Sender<Request>),
    Direct(mpsc::Sender<ClientMessage>),
}

struct Request {
    message: ClientMessage,
    durable: Option<oneshot::Receiver<bool>>,
}

pub struct Permits<'a>(Reservations<'a>);

enum Reservations<'a> {
    Ordered(mpsc::PermitIterator<'a, Request>),
    Direct(mpsc::PermitIterator<'a, ClientMessage>),
}

pub struct Permit<'a>(Reservation<'a>);

enum Reservation<'a> {
    Ordered(mpsc::Permit<'a, Request>),
    Direct(mpsc::Permit<'a, ClientMessage>),
}

impl<'a> Iterator for Permits<'a> {
    type Item = Permit<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            Reservations::Ordered(permits) => permits
                .next()
                .map(|permit| Permit(Reservation::Ordered(permit))),
            Reservations::Direct(permits) => permits
                .next()
                .map(|permit| Permit(Reservation::Direct(permit))),
        }
    }
}

impl Permit<'_> {
    pub fn send(self, message: ClientMessage) {
        admitted(&message);
        match self.0 {
            Reservation::Ordered(permit) => permit.send(Request {
                message,
                durable: None,
            }),
            Reservation::Direct(permit) => permit.send(message),
        }
    }
}

pub(super) struct Receiver {
    incoming: mpsc::Receiver<Request>,
    pending: Option<Request>,
}

pub(super) fn channel(capacity: usize) -> (Sender, Receiver, impl Fn() -> u64 + Send + Sync) {
    let (sender, incoming) = mpsc::channel(capacity);
    let weak = sender.downgrade();
    let queued = move || {
        weak.upgrade().map_or(0, |sender| {
            (sender.max_capacity() - sender.capacity()) as u64
        })
    };
    (
        Sender(Kind::Ordered(sender)),
        Receiver {
            incoming,
            pending: None,
        },
        queued,
    )
}

fn admitted(message: &ClientMessage) {
    if let ClientMessage::Act {
        action: engine::actions::Action::ChangeRecord { request },
        ..
    } = message
    {
        nucleus::performance::Operation::new(&request.id).mark("record-edit-dispatched");
    }
}

impl From<mpsc::Sender<ClientMessage>> for Sender {
    fn from(sender: mpsc::Sender<ClientMessage>) -> Self {
        Self(Kind::Direct(sender))
    }
}

impl Sender {
    pub fn capacity(&self) -> usize {
        match &self.0 {
            Kind::Ordered(sender) => sender.capacity(),
            Kind::Direct(sender) => sender.capacity(),
        }
    }

    pub async fn closed(&self) {
        match &self.0 {
            Kind::Ordered(sender) => sender.closed().await,
            Kind::Direct(sender) => sender.closed().await,
        }
    }

    pub fn try_reserve_many(
        &self,
        count: usize,
    ) -> Result<Permits<'_>, mpsc::error::TrySendError<()>> {
        match &self.0 {
            Kind::Ordered(sender) => sender
                .try_reserve_many(count)
                .map(|permits| Permits(Reservations::Ordered(permits))),
            Kind::Direct(sender) => sender
                .try_reserve_many(count)
                .map(|permits| Permits(Reservations::Direct(permits))),
        }
    }

    pub fn is_closed(&self) -> bool {
        match &self.0 {
            Kind::Ordered(sender) => sender.is_closed(),
            Kind::Direct(sender) => sender.is_closed(),
        }
    }

    pub fn same_channel(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Kind::Ordered(first), Kind::Ordered(second)) => first.same_channel(second),
            (Kind::Direct(first), Kind::Direct(second)) => first.same_channel(second),
            _ => false,
        }
    }

    pub fn try_send(
        &self,
        message: ClientMessage,
    ) -> Result<(), mpsc::error::TrySendError<ClientMessage>> {
        let permit = match &self.0 {
            Kind::Ordered(sender) => sender.try_reserve().map(Reservation::Ordered),
            Kind::Direct(sender) => sender.try_reserve().map(Reservation::Direct),
        };
        match permit {
            Ok(permit) => {
                Permit(permit).send(message);
                Ok(())
            }
            Err(mpsc::error::TrySendError::Full(())) => {
                Err(mpsc::error::TrySendError::Full(message))
            }
            Err(mpsc::error::TrySendError::Closed(())) => {
                Err(mpsc::error::TrySendError::Closed(message))
            }
        }
    }

    pub async fn send(
        &self,
        message: ClientMessage,
    ) -> Result<(), mpsc::error::SendError<ClientMessage>> {
        let permit = match &self.0 {
            Kind::Ordered(sender) => sender.reserve().await.map(Reservation::Ordered),
            Kind::Direct(sender) => sender.reserve().await.map(Reservation::Direct),
        };
        match permit {
            Ok(permit) => {
                Permit(permit).send(message);
                Ok(())
            }
            Err(_) => Err(mpsc::error::SendError(message)),
        }
    }

    pub(crate) fn try_send_after(
        &self,
        message: ClientMessage,
        durable: oneshot::Receiver<bool>,
    ) -> bool {
        let Kind::Ordered(sender) = &self.0 else {
            return false;
        };
        let Ok(permit) = sender.try_reserve() else {
            return false;
        };
        admitted(&message);
        permit.send(Request {
            message,
            durable: Some(durable),
        });
        true
    }
}

impl Receiver {
    pub(super) async fn recv(&mut self) -> Option<ClientMessage> {
        loop {
            if self.pending.is_none() {
                self.pending = self.incoming.recv().await;
            }
            let pending = self.pending.as_mut()?;
            let allowed = match &mut pending.durable {
                Some(durable) => durable.await.unwrap_or(false),
                None => true,
            };
            let request = self.pending.take().unwrap();
            if allowed {
                return Some(request.message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };

    fn message(id: &str) -> ClientMessage {
        ClientMessage::CollabLeave {
            record_uid: id.into(),
        }
    }

    fn id(message: ClientMessage) -> String {
        match message {
            ClientMessage::CollabLeave { record_uid } => record_uid,
            _ => panic!("Unexpected command"),
        }
    }

    fn pending(receiver: &mut Receiver) {
        let mut receive = std::pin::pin!(receiver.recv());
        assert!(matches!(
            receive
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }

    #[tokio::test]
    async fn cancelled_receive_keeps_durable_command_before_later_control_commands() {
        let (sender, mut receiver, queued) = channel(2);
        let (durable, ready) = oneshot::channel();
        assert!(sender.try_send_after(message("edit"), ready));
        sender
            .try_send(ClientMessage::LiveLogin {
                username: "synthetic-account".into(),
                password: "synthetic-password".into(),
            })
            .unwrap();
        assert_eq!(queued(), 2);
        for _ in 0..4 {
            pending(&mut receiver);
        }
        assert_eq!(queued(), 1);
        durable.send(true).unwrap();
        assert_eq!(id(receiver.recv().await.unwrap()), "edit");
        assert!(
            matches!(receiver.recv().await, Some(ClientMessage::LiveLogin { username, .. }) if username == "synthetic-account")
        );
        assert_eq!(queued(), 0);
        drop(sender);
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn failed_or_abandoned_durability_never_executes_the_command() {
        let (sender, mut receiver, _) = channel(3);
        let (failed, first) = oneshot::channel();
        let (abandoned, second) = oneshot::channel();
        assert!(sender.try_send_after(message("failed"), first));
        assert!(sender.try_send_after(message("abandoned"), second));
        sender.send(message("following")).await.unwrap();
        pending(&mut receiver);
        failed.send(false).unwrap();
        pending(&mut receiver);
        drop(abandoned);
        assert_eq!(id(receiver.recv().await.unwrap()), "following");
        drop(sender);
        assert!(receiver.recv().await.is_none());
    }

    #[tokio::test]
    async fn full_or_closed_queue_refuses_a_fence_and_preserves_normal_sender_errors() {
        let (sender, mut receiver, queued) = channel(1);
        assert!(sender.same_channel(&sender.clone()));
        let (other, _, _) = channel(1);
        assert!(!sender.same_channel(&other));
        sender.try_send(message("occupied")).unwrap();
        let (durable, ready) = oneshot::channel();
        assert!(!sender.try_send_after(message("refused"), ready));
        assert!(durable.send(true).is_err());
        assert!(matches!(
            sender.try_send(message("full")),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        assert_eq!(queued(), 1);
        assert_eq!(id(receiver.recv().await.unwrap()), "occupied");
        drop(receiver);
        assert!(sender.is_closed());
        assert!(matches!(
            sender.try_send(message("closed")),
            Err(mpsc::error::TrySendError::Closed(_))
        ));
        assert!(sender.send(message("closed")).await.is_err());
        let (durable, ready) = oneshot::channel();
        assert!(!sender.try_send_after(message("refused"), ready));
        assert!(durable.send(true).is_err());
    }

    #[tokio::test]
    async fn batch_reservations_are_atomic_and_release_unused_capacity() {
        let (sender, mut receiver, _) = channel(3);
        let mut permits = sender.try_reserve_many(2).unwrap();
        assert_eq!(sender.capacity(), 1);
        assert!(matches!(
            sender.try_reserve_many(2),
            Err(mpsc::error::TrySendError::Full(()))
        ));
        assert_eq!(sender.capacity(), 1);
        permits.next().unwrap().send(message("first"));
        drop(permits);
        assert_eq!(sender.capacity(), 2);
        sender.try_send(message("second")).unwrap();
        assert_eq!(id(receiver.recv().await.unwrap()), "first");
        assert_eq!(id(receiver.recv().await.unwrap()), "second");
        assert_eq!(sender.capacity(), 3);
        drop(receiver);
        sender.closed().await;
        assert!(matches!(
            sender.try_reserve_many(2),
            Err(mpsc::error::TrySendError::Closed(()))
        ));
    }

    #[tokio::test]
    async fn direct_connections_keep_their_original_queue_and_require_prior_durability() {
        let (direct, mut receiver) = mpsc::channel(1);
        let sender = Sender::from(direct.clone());
        assert!(sender.same_channel(&direct.into()));
        let (durable, ready) = oneshot::channel();
        assert!(!sender.try_send_after(message("not-durable"), ready));
        assert!(durable.send(true).is_err());
        assert!(receiver.try_recv().is_err());
        sender.try_send(message("durable")).unwrap();
        assert!(matches!(
            sender.try_send(message("full")),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        assert_eq!(id(receiver.recv().await.unwrap()), "durable");
        sender.send(message("next")).await.unwrap();
        assert_eq!(id(receiver.recv().await.unwrap()), "next");
        drop(receiver);
        assert!(sender.is_closed());
        assert!(sender.send(message("closed")).await.is_err());
    }
}
