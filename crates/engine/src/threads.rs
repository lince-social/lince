use nucleus::RecordKind;

use crate::Engine;
use crate::error::EngineError;

pub const THREAD_OF_PREDICATE: &str = "thread-of";
pub const MESSAGE_IN_PREDICATE: &str = "message-in";

impl Engine {
    pub async fn start_conversation(
        &self,
        contact_organ: &str,
        title: &str,
    ) -> Result<(String, String), EngineError> {
        let pool = &self.store.pool;
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

        store::replica::offer(pool, &conversation.uid, contact_organ).await?;

        let thread = self.open_thread(&conversation.uid, title).await?;
        Ok((conversation.uid, thread))
    }

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

    pub(crate) async fn link_in(
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

    pub async fn accept_conversation(
        &self,
        root: &str,
        contact_organ: &str,
    ) -> Result<(), EngineError> {
        store::replica::accept(&self.store.pool, root, contact_organ).await?;
        Ok(())
    }

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

    pub async fn decline_invite(&self, invite_uid: &str) -> Result<(), EngineError> {
        let invite = store::invites::get(&self.store.pool, invite_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("no such invite".into()))?;
        store::replica::revoke(&self.store.pool, &invite.root, &invite.from_organ).await?;
        store::offers::refuse(
            &self.store.pool,
            store::offers::OfferKind::ThreadInvite,
            &invite.root,
            &invite.from_organ,
        )
        .await?;
        store::invites::clear(&self.store.pool, invite_uid).await?;
        self.notify_notifications_changed();
        Ok(())
    }

    pub async fn revoke_conversation(
        &self,
        root: &str,
        contact_organ: &str,
    ) -> Result<(), EngineError> {
        store::replica::revoke(&self.store.pool, root, contact_organ).await?;
        Ok(())
    }
}
