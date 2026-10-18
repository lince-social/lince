use super::*;
use fiote::{
    assistant::{Conversation, EntryReceipt},
    communication::decision::{Admission, Dialogue},
};

impl Host {
    pub(crate) async fn voice_context(&self, previous: &EntryReceipt) -> Result<Dialogue, String> {
        let receipt = self.assistant_receipt(&previous.request_id).await?;
        if receipt.target != previous.target
            || receipt.thread != previous.thread
            || receipt.message != previous.message
            || receipt.reply != previous.reply
            || receipt.state != "completed"
        {
            return Err("The previous voice reply is incomplete or has changed.".into());
        }
        self.record(&receipt.target).await?;
        let request = self
            .record(
                receipt
                    .message
                    .as_deref()
                    .ok_or("The previous request is unavailable.")?,
            )
            .await?;
        let reply = self
            .record(
                receipt
                    .reply
                    .as_deref()
                    .ok_or("The previous reply is unavailable.")?,
            )
            .await?;
        Ok(Dialogue::bounded(&request.body, &reply.body))
    }

    pub(crate) async fn classify_voice_followup(
        &self,
        conversation: &Conversation,
        dialogue: &Dialogue,
        text: &str,
    ) -> Result<Admission, String> {
        self.record(&conversation.target).await?;
        if self.thread_fiote(&conversation.thread).await? != conversation.target {
            return Err("The voice conversation no longer belongs to its Fiote.".into());
        }
        let mut configuration = self
            .load(&conversation.target)?
            .ok_or("Configure the Fiote connection first.")?;
        if !configuration.settings.enabled || configuration.agent.is_some() {
            return Err("Continuous voice needs a local decision model or an enabled native provider connection.".into());
        }
        self.catalog.validate(&mut configuration.settings)?;
        let slot = vault::slot(&configuration.settings);
        let key = if self.catalog.method(&configuration.settings)?.kind
            == fiote::adapters::AuthKind::None
        {
            Secret::default()
        } else {
            self.vault
                .key(&slot)
                .await?
                .ok_or("Sign in to the Fiote connection first.")?
        };
        let adapter = self
            .catalog
            .driver(&configuration.settings.provider)
            .map(|driver| {
                Arc::new(fiote::driver::DriverProvider::new(
                    driver.clone(),
                    configuration.settings.clone(),
                    key.clone(),
                ))
            });
        let provider: Arc<dyn Provider> = match (&self.provider, &adapter) {
            (Some(provider), _) => provider.clone(),
            (_, Some(provider)) => provider.clone(),
            _ => {
                let lock = self
                    .refresh_locks
                    .lock()
                    .await
                    .entry(slot.clone())
                    .or_default()
                    .clone();
                fiote::communication::native::provider(
                    &configuration.settings,
                    &key,
                    Some(Arc::new(vault::Credential {
                        vault: self.vault.clone(),
                        directory: self.directory.clone(),
                        slot: slot.clone(),
                    })),
                    Some(lock),
                )?
            }
        };
        let result =
            fiote::communication::decision::dialogue::classify(provider.as_ref(), dialogue, text)
                .await;
        if let Some(adapter) = adapter {
            self.vault
                .refreshed(slot, adapter.credential().await, key)
                .await?;
        }
        result
    }
}
