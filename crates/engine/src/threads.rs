//! Threads (Ontology §11): reaching someone, as Records synced with exactly
//! one peer.
//!
//! **A thread is not a subsystem.** Sharing a conversation IS granting one
//! contact sync access to it, and the same act shares any Record with any
//! contact. There is no message-delivery path, no message table, and no ACL
//! table — a message is a Record, and it travels because its root is granted.
//!
//! **Messaging is not collab.** Two people typing into one string is collab;
//! a conversation is not that. So messages are ordinary Records synced as
//! ordinary `set` ops, ordered by HLC, with no Loro doc involved. Concurrent
//! sends do not conflict — they are two different Records, both arrive, both
//! display. Collab's role here is exactly this small: if both parties happen
//! to OPEN the same message Record, its body behaves like any other
//! collab-edited body. Nothing about a conversation is special-cased.
//!
//! Three levels, one grant:
//!   `Conversation` (the root, shared with one contact)
//!     → `Thread` (a topic; a new one needs no new grant)
//!       → `Message`
//! Each level is a Record, joined by Assertions, and every one of them is
//! born with `replica_root` already pointing at the conversation. That is what
//! makes the grant cascade without anything having to walk the graph.

use nucleus::RecordKind;

use crate::Engine;
use crate::error::EngineError;

/// These are the established Record/Protein thread relations. Keeping the two
/// levels distinct lets a surface project threads and messages without
/// guessing from record kinds.
pub const THREAD_OF_PREDICATE: &str = "thread-of";
pub const MESSAGE_IN_PREDICATE: &str = "message-in";

impl Engine {
    /// Start a conversation with `contact_organ` and offer it to them.
    ///
    /// The Conversation Record is its OWN root, so everything created inside
    /// it inherits that uid and rides the grant. Returns
    /// `(conversation_uid, first_thread_uid)` — clicking "chat" on a
    /// discovered stranger creates both and opens the Record sand on the
    /// thread, which is why they are made together.
    pub async fn start_conversation(
        &self,
        contact_organ: &str,
        title: &str,
    ) -> Result<(String, String), EngineError> {
        let pool = &self.store.pool;
        // Created with no root, then it becomes its own: the root column must
        // hold a real uid and the uid does not exist until the row does. This
        // is the ONE record for which a post-create stamp is correct, because
        // nothing is inside it yet and its own ops carry no conversation
        // content — the head is the title the local user just typed.
        let conversation = store::records::create(
            pool,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Conversation,
                head: title,
                body: "",
                quantity: store::exact::one(),
            },
        )
        .await?;
        store::replica::make_own_root(pool, &conversation.uid).await?;

        // A `with` Assertion binds the conversation to the contact's Organ
        // Record, as §1/§3 already allow — still no new ACL table. It is
        // deliberately NOT the grant: the grant is the `replica_grant` row,
        // and conflating "who this is about" with "who may receive it" is how
        // an ACL sneaks back in.
        store::replica::offer(pool, &conversation.uid, contact_organ).await?;

        let thread = self.open_thread(&conversation.uid, title).await?;
        Ok((conversation.uid, thread))
    }

    /// Add a Thread to an existing Conversation. Needs no new grant, no new
    /// pairing and no new sync setup — that is the whole reason to nest rather
    /// than make every thread its own Record.
    pub async fn open_thread(
        &self,
        conversation_uid: &str,
        title: &str,
    ) -> Result<String, EngineError> {
        let pool = &self.store.pool;
        let root = store::replica::root_of(pool, conversation_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("not a conversation root".into()))?;
        let thread = store::records::create_in_root(
            pool,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Thread,
                head: title,
                body: "",
                quantity: store::exact::one(),
            },
            Some(&root),
        )
        .await?;
        self.link_in(&thread.uid, conversation_uid, THREAD_OF_PREDICATE)
            .await?;
        Ok(thread.uid)
    }

    /// Append a message to a thread. `body` is the text; `head` is the label
    /// shown in a list.
    ///
    /// Nothing here is append-only or immutable: either party may edit or
    /// delete anything shared, and that is accepted rather than constrained —
    /// a Record you hold a copy of is a Record you can change.
    pub async fn send_message(
        &self,
        thread_uid: &str,
        head: &str,
        body: &str,
    ) -> Result<String, EngineError> {
        let pool = &self.store.pool;
        let root = store::replica::root_of(pool, thread_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("thread is not inside a root".into()))?;
        let message = store::records::create_in_root(
            pool,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Message,
                head,
                body,
                quantity: store::exact::one(),
            },
            Some(&root),
        )
        .await?;
        self.link_in(&message.uid, thread_uid, MESSAGE_IN_PREDICATE)
            .await?;
        Ok(message.uid)
    }

    /// `(child) --in--> (parent)`, both necessarily inside the same root —
    /// `assertions::assert` refuses the link otherwise.
    async fn link_in(
        &self,
        child_uid: &str,
        parent_uid: &str,
        predicate_name: &str,
    ) -> Result<(), EngineError> {
        let pool = &self.store.pool;
        let predicate = store::concepts::ensure(pool, predicate_name).await?;
        store::assertions::assert(
            pool,
            store::assertions::NewAssertion {
                subject_uid: child_uid,
                predicate_uid: &predicate,
                object_uid: Some(parent_uid),
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await?;
        Ok(())
    }

    /// Accept a conversation someone offered. Acceptance is what turns "you
    /// may see this" into "I keep a copy", and it is also what stops an Organ
    /// from pushing unwanted Records into someone's store.
    pub async fn accept_conversation(
        &self,
        root: &str,
        contact_organ: &str,
    ) -> Result<(), EngineError> {
        store::replica::accept(&self.store.pool, root, contact_organ).await?;
        Ok(())
    }

    /// Say yes to an invite: keep a copy of what was offered, and clear it.
    ///
    /// Accepting opens the conversation and NOTHING else. It does not set
    /// `trust`, does not enable sync, and does not adopt a key — agreeing to
    /// read what someone sends is not the same as deciding who they are. That
    /// promotion happens inside the thread, deliberately, as its own act.
    pub async fn accept_invite(&self, invite_uid: &str) -> Result<String, EngineError> {
        let invite = store::invites::get(&self.store.pool, invite_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("no such invite".into()))?;
        self.accept_conversation(&invite.root, &invite.from_organ)
            .await?;
        store::invites::clear(&self.store.pool, invite_uid).await?;
        self.notify_notifications_changed();
        Ok(invite.root)
    }

    /// Say no: revoke the offered grant and clear the invite.
    ///
    /// Revoking is what makes this an ANSWER rather than a dismissal. Leaving
    /// the grant `offered` would keep the sender waiting on a reply that never
    /// comes while the one-pending-per-Organ slot stayed occupied — so they
    /// could not ask again either. Declining frees both.
    pub async fn decline_invite(&self, invite_uid: &str) -> Result<(), EngineError> {
        let invite = store::invites::get(&self.store.pool, invite_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("no such invite".into()))?;
        store::replica::revoke(&self.store.pool, &invite.root, &invite.from_organ).await?;
        store::invites::clear(&self.store.pool, invite_uid).await?;
        self.notify_notifications_changed();
        Ok(())
    }

    /// Stop a conversation reaching us, and us reaching it.
    ///
    /// Deleting the Record and revoking the grant are TWO acts and this is the
    /// second. Deleting alone would leave the peer pushing ops at a uid that
    /// is gone; emitting a `tombstone` would delete THEIR copy too, since
    /// tombstone is a synced op kind. Revoking leaves their copy alone and
    /// simply stops accepting them — §12's split between revoke (hard, local,
    /// guaranteed) and forget (a request the remote may honour).
    pub async fn revoke_conversation(
        &self,
        root: &str,
        contact_organ: &str,
    ) -> Result<(), EngineError> {
        store::replica::revoke(&self.store.pool, root, contact_organ).await?;
        Ok(())
    }
}
