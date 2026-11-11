use super::Secret;
pub use fiote::credentials::{Store, slot};
use std::sync::Arc;

pub(super) struct Credential {
    pub store: Arc<Store>,
    pub slot: String,
}

#[async_trait::async_trait]
impl fiote::communication::auth::CredentialStore for Credential {
    async fn refresh_guard(&self) -> Result<Option<std::fs::File>, String> {
        self.store.refresh_guard(&self.slot).await.map(Some)
    }
    async fn read(&self) -> Result<Secret, String> {
        self.store
            .key(&self.slot)
            .await?
            .ok_or_else(|| "Sign in to this account first.".into())
    }
    async fn replace(&self, previous: &Secret, next: Secret) -> Result<(), String> {
        self.store
            .refreshed(self.slot.clone(), next, previous.clone())
            .await
    }
}
