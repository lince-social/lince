// non-lean-yet
// Scope: struct Sender, enum Kind, struct Request, struct EditSequence.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 91, 93, 100, 276.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (305).

use cell::{ClientMessage, ServerMessage};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
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
    edit: Option<(String, EditSequence)>,
}

#[derive(Clone, Default)]
pub(crate) struct EditSequence(Arc<AtomicBool>);

impl EditSequence {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub struct Permits<'a>(Reservations<'a>);

enum Reservations<'a> {
    Ordered(mpsc::PermitIterator<'a, Request>),
    Direct(mpsc::PermitIterator<'a, ClientMessage>),
}

pub struct Permit<'a>(Reservation<'a>);

pub(crate) struct EditPermit<'a> {
    permit: mpsc::Permit<'a, Request>,
    sequence: &'a EditSequence,
}

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
                edit: None,
            }),
            Reservation::Direct(permit) => permit.send(message),
        }
    }
}

impl EditPermit<'_> {
    pub(crate) fn send(
        self,
        request: &engine::record_change::Request,
        durable: Option<oneshot::Receiver<bool>>,
    ) {
        let message = ClientMessage::Act {
            id: request.id.clone(),
            action: engine::actions::Action::ChangeRecord {
                request: request.clone(),
            },
        };
        admitted(&message);
        self.permit.send(Request {
            message,
            durable,
            edit: Some((request.id.clone(), self.sequence.clone())),
        });
    }
}

pub(super) struct Receiver {
    incoming: mpsc::Receiver<Request>,
    pending: Option<Request>,
    active_edit: Option<(String, EditSequence)>,
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
            active_edit: None,
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
        match self.try_reserve() {
            Ok(permit) => {
                permit.send(message);
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

    pub fn try_reserve(&self) -> Result<Permit<'_>, mpsc::error::TrySendError<()>> {
        match &self.0 {
            Kind::Ordered(sender) => sender.try_reserve().map(Reservation::Ordered),
            Kind::Direct(sender) => sender.try_reserve().map(Reservation::Direct),
        }
        .map(Permit)
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

    pub(crate) fn ordered(&self) -> bool {
        matches!(self.0, Kind::Ordered(_))
    }

    pub(crate) fn try_send_edit(
        &self,
        request: &engine::record_change::Request,
        sequence: &EditSequence,
        durable: Option<oneshot::Receiver<bool>>,
    ) -> bool {
        let Some(permit) = self.try_reserve_edit(sequence) else {
            return false;
        };
        permit.send(request, durable);
        true
    }

    pub(crate) fn try_reserve_edit<'a>(
        &'a self,
        sequence: &'a EditSequence,
    ) -> Option<EditPermit<'a>> {
        if sequence.cancelled() {
            return None;
        }
        let Kind::Ordered(sender) = &self.0 else {
            return None;
        };
        Some(EditPermit {
            permit: sender.try_reserve().ok()?,
            sequence,
        })
    }
}

impl Receiver {
    pub(super) fn complete(&mut self, messages: &mut Vec<ServerMessage>) {
        if let Some((id, sequence)) = self.active_edit.take()
            && (!messages.iter().any(|message| matches!(message, ServerMessage::ActionOk { id: reply, .. } if reply == &id))
                || messages.iter().any(|message| matches!(message, ServerMessage::Error { id: reply, .. } if reply == &id)))
        {
            sequence.cancel();
            if !messages.iter().any(|message| matches!(message, ServerMessage::Error { id: reply, .. } if reply == &id)) {
                let (message, code) = messages.iter().find_map(|message| match message {
                    ServerMessage::Error { message, code, .. } => Some((message.clone(), code.clone())),
                    _ => None,
                }).unwrap_or_else(|| ("The local save did not return confirmation".into(), None));
                messages.push(ServerMessage::Error { id, message, code });
            }
        }
    }

    pub(super) async fn recv(&mut self) -> Option<ClientMessage> {
        if let Some((_, sequence)) = self.active_edit.take() {
            sequence.cancel();
        }
        loop {
            if self.pending.is_none() {
                self.pending = self.incoming.recv().await;
            }
            let pending = self.pending.as_mut()?;
            if pending
                .edit
                .as_ref()
                .is_some_and(|(_, sequence)| sequence.cancelled())
            {
                self.pending = None;
                continue;
            }
            let allowed = match &mut pending.durable {
                Some(durable) => durable.await.unwrap_or(false),
                None => true,
            };
            let request = self.pending.take().unwrap();
            if !allowed {
                if let Some((_, sequence)) = request.edit {
                    sequence.cancel();
                }
            } else if request
                .edit
                .as_ref()
                .is_none_or(|(_, sequence)| !sequence.cancelled())
            {
                self.active_edit = request.edit;
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

    fn edit(id: &str) -> engine::record_change::Request {
        engine::record_change::Request {
            id: id.into(),
            record_uid: "synthetic-record".into(),
            mutation: engine::record_change::Mutation::Quantity { value: "1".into() },
        }
    }

    fn ok(id: &str) -> ServerMessage {
        ServerMessage::ActionOk {
            id: id.into(),
            created: None,
            facts: 1,
            warnings: Vec::new(),
            data: None,
        }
    }

    fn id(message: ClientMessage) -> String {
        match message {
            ClientMessage::CollabLeave { record_uid } => record_uid,
            ClientMessage::Act { id, .. } => id,
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
        assert!(sender.try_send_edit(&edit("edit"), &EditSequence::default(), Some(ready)));
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
        receiver.complete(&mut vec![ok("edit")]);
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
        assert!(sender.try_send_edit(&edit("failed"), &EditSequence::default(), Some(first)));
        assert!(sender.try_send_edit(&edit("abandoned"), &EditSequence::default(), Some(second)));
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
        assert!(!sender.try_send_edit(&edit("refused"), &EditSequence::default(), Some(ready)));
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
        assert!(!sender.try_send_edit(&edit("refused"), &EditSequence::default(), Some(ready)));
        assert!(durable.send(true).is_err());
    }

    #[tokio::test]
    async fn completed_edits_advance_without_acknowledgement_from_the_interface() {
        let (sender, mut receiver, queued) = channel(3);
        let sequence = EditSequence::default();
        for name in ["first", "second", "third"] {
            assert!(sender.try_send_edit(&edit(name), &sequence, None));
        }
        for name in ["first", "second", "third"] {
            assert_eq!(id(receiver.recv().await.unwrap()), name);
            receiver.complete(&mut vec![ok(name)]);
        }
        assert!(!sequence.cancelled());
        assert_eq!(queued(), 0);
    }

    #[tokio::test]
    async fn failed_edit_stops_dependents_but_keeps_independent_edits_and_controls() {
        let (sender, mut receiver, _) = channel(4);
        let sequence = EditSequence::default();
        assert!(sender.try_send_edit(&edit("failed"), &sequence, None));
        assert!(sender.try_send_edit(&edit("dependent"), &sequence, None));
        let independent = EditSequence::default();
        assert!(sender.try_send_edit(&edit("independent"), &independent, None));
        sender.try_send(message("control")).unwrap();
        assert_eq!(id(receiver.recv().await.unwrap()), "failed");
        receiver.complete(&mut vec![ServerMessage::Error {
            id: "failed".into(),
            message: "Synthetic write failure".into(),
            code: None,
        }]);
        assert!(sequence.cancelled());
        assert!(!sender.try_send_edit(&edit("later"), &sequence, None));
        assert_eq!(id(receiver.recv().await.unwrap()), "independent");
        receiver.complete(&mut vec![ok("independent")]);
        assert_eq!(id(receiver.recv().await.unwrap()), "control");
        assert!(!independent.cancelled());
    }

    #[tokio::test]
    async fn missing_or_unrelated_completion_never_releases_dependent_edits() {
        for unrelated_reply in [false, true] {
            let (sender, mut receiver, _) = channel(3);
            let sequence = EditSequence::default();
            assert!(sender.try_send_edit(&edit("first"), &sequence, None));
            assert!(sender.try_send_edit(&edit("dependent"), &sequence, None));
            sender.try_send(message("control")).unwrap();
            assert_eq!(id(receiver.recv().await.unwrap()), "first");
            if unrelated_reply {
                let mut replies = vec![ok("unrelated")];
                receiver.complete(&mut replies);
                assert!(
                    matches!(replies.last(), Some(ServerMessage::Error { id, .. }) if id == "first")
                );
            }
            assert_eq!(id(receiver.recv().await.unwrap()), "control");
            assert!(sequence.cancelled());
        }
    }

    #[tokio::test]
    async fn session_failure_keeps_its_reason_and_identifies_the_unconfirmed_edit() {
        let (sender, mut receiver, _) = channel(1);
        let sequence = EditSequence::default();
        assert!(sender.try_send_edit(&edit("waiting"), &sequence, None));
        assert_eq!(id(receiver.recv().await.unwrap()), "waiting");
        let mut replies = vec![ServerMessage::Error {
            id: "-".into(),
            message: "Please log in again".into(),
            code: Some("session_expired".into()),
        }];
        receiver.complete(&mut replies);
        assert!(sequence.cancelled());
        assert_eq!(replies.len(), 2);
        assert!(matches!(&replies[0], ServerMessage::Error { id, .. } if id == "-"));
        assert!(
            matches!(&replies[1], ServerMessage::Error { id, message, code }
            if id == "waiting" && message == "Please log in again" && code.as_deref() == Some("session_expired"))
        );
    }

    #[tokio::test]
    async fn abandoned_recovery_blocks_the_whole_edit_sequence() {
        let (sender, mut receiver, _) = channel(3);
        let sequence = EditSequence::default();
        let (durable, ready) = oneshot::channel();
        assert!(sender.try_send_edit(&edit("first"), &sequence, Some(ready)));
        assert!(sender.try_send_edit(&edit("dependent"), &sequence, None));
        sender.try_send(message("control")).unwrap();
        pending(&mut receiver);
        drop(durable);
        assert_eq!(id(receiver.recv().await.unwrap()), "control");
        assert!(sequence.cancelled());
    }

    #[tokio::test]
    async fn cancellation_during_recovery_never_executes_the_edit() {
        let (sender, mut receiver, _) = channel(2);
        let sequence = EditSequence::default();
        let (durable, ready) = oneshot::channel();
        assert!(sender.try_send_edit(&edit("first"), &sequence, Some(ready)));
        sender.try_send(message("control")).unwrap();
        let mut receive = std::pin::pin!(receiver.recv());
        assert!(matches!(
            receive
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
        sequence.cancel();
        durable.send(true).unwrap();
        assert_eq!(id(receive.await.unwrap()), "control");
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
        assert!(!sender.try_send_edit(&edit("not-durable"), &EditSequence::default(), Some(ready)));
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
