use super::*;
use fiote::assistant::{Conversation, EntryReceipt, EntryRequest};
use sha2::{Digest, Sha256};

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err("Use a stable assistant request ID with at most 128 letters/digits.".into());
    }
    Ok(())
}

impl Host {
    fn entry_path(&self, id: &str) -> Result<PathBuf, String> {
        valid_id(id)?;
        Ok(self
            .directory
            .join(format!("assistant-request-{}.json", digest(id.as_bytes()))))
    }

    async fn entry_state(&self, mut receipt: EntryReceipt) -> Result<EntryReceipt, String> {
        if let Some(reply) = &receipt.reply {
            let metadata =
                store::records::get_extension(&self.engine.store.pool, reply, "lince.message")
                    .await
                    .map_err(|error| error.to_string())?;
            let running = match &receipt.thread {
                Some(thread) => self.running.lock().await.contains_key(thread),
                None => false,
            };
            receipt.state = match metadata
                .as_ref()
                .and_then(|metadata| metadata["state"].as_str())
            {
                Some("finished") => "completed",
                Some("interrupted") => "interrupted",
                Some("writing") if running => "running",
                _ => "uncertain",
            }
            .into();
        } else if receipt.state == "accepted" {
            receipt.state = "uncertain".into();
            receipt.detail =
                "Inspect the saved conversation before retrying this interrupted request.".into();
        }
        Ok(receipt)
    }

    pub(super) async fn assistant_receipt(&self, id: &str) -> Result<EntryReceipt, String> {
        let path = self.entry_path(id)?;
        if std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len()
            > 16_384
        {
            return Err("Assistant receipt exceeds its size limit.".into());
        }
        let receipt =
            serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        self.entry_state(receipt).await
    }

    pub(crate) async fn inspect_assistant_entry(
        &self,
        id: &str,
    ) -> Result<Option<EntryReceipt>, String> {
        match std::fs::metadata(self.entry_path(id)?) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
            Ok(_) => {}
        }
        let receipt = self.assistant_receipt(id).await?;
        self.record(&receipt.target).await?;
        Ok(Some(receipt))
    }

    pub(super) async fn assistant_send(
        &self,
        request: EntryRequest,
    ) -> Result<EntryReceipt, String> {
        self.assistant_submit(request, None).await
    }

    pub(super) async fn assistant_continue(
        &self,
        request: EntryRequest,
        conversation: Conversation,
    ) -> Result<EntryReceipt, String> {
        self.assistant_submit(request, Some(conversation)).await
    }

    async fn assistant_submit(
        &self,
        request: EntryRequest,
        conversation: Option<Conversation>,
    ) -> Result<EntryReceipt, String> {
        valid_id(&request.request_id)?;
        if request.text.trim().is_empty() && request.content.is_empty() {
            return Err("Write or attach an assistant request.".into());
        }
        if request.text.len() > 65_536 {
            return Err("Assistant requests are limited to 64 KiB of text.".into());
        }
        if request.text.trim_start().starts_with("/login") {
            return Err("Use Configure recipient or the local /login picker; credentials must never be sent as assistant messages.".into());
        }
        nucleus::message::validate(&request.content)?;
        if let Some(conversation) = &conversation {
            if !nucleus::valid_uid(&conversation.target, "r")
                || !nucleus::valid_uid(&conversation.thread, "r")
                || request
                    .target
                    .as_ref()
                    .is_some_and(|target| target != &conversation.target)
            {
                return Err("The continued conversation must keep its Fiote and thread.".into());
            }
            let thread = self.record(&conversation.thread).await?;
            let predicate = store::concepts::resolve(&self.engine.store.pool, "thread-of")
                .await
                .map_err(|error| error.to_string())?
                .ok_or("The conversation has no owner.")?;
            let parents = store::assertions::objects_from_subject(
                &self.engine.store.pool,
                &conversation.thread,
                &predicate,
            )
            .await
            .map_err(|error| error.to_string())?;
            if thread.kind != RecordKind::Thread.as_str()
                || parents.len() != 1
                || parents[0].uid != conversation.target
                || self.thread_fiote(&conversation.thread).await? != conversation.target
            {
                return Err("This conversation no longer belongs to the chosen Fiote. End it and choose again.".into());
            }
        }
        let fingerprint = match &conversation {
            Some(conversation) => serde_json::to_vec(&(&request, conversation)),
            None => serde_json::to_vec(&request),
        }
        .map_err(|error| error.to_string())
        .map(|bytes| digest(&bytes))?;
        let path = self.entry_path(&request.request_id)?;
        let _guard = self.handoff_dispatch.lock().await;
        if path.exists() {
            let receipt = self.assistant_receipt(&request.request_id).await?;
            if receipt.fingerprint != fingerprint {
                return Err("This request ID belongs to a different assistant submission. Inspect its receipt before submitting new work.".into());
            }
            return Ok(receipt);
        }
        let target = match conversation
            .as_ref()
            .map(|conversation| &conversation.target)
            .or(request.target.as_ref())
        {
            Some(target) if nucleus::valid_uid(target, "r") => target.clone(),
            Some(_) => return Err("Choose a valid Fiote Record UID.".into()),
            None => {
                self.intake_selection()
                    .await?
                    .ok_or("Create and configure a Fiote before requesting assistance.")?
                    .record
            }
        };
        self.record(&target).await?;
        let Some(mut configuration) = self.load(&target)? else {
            return Err("Choose a configured Fiote. Your request remains unsent.".into());
        };
        if !configuration.settings.enabled {
            return Err(
                "The chosen Fiote needs a connection. Your request remains unsent; open its setup."
                    .into(),
            );
        }
        if configuration
            .agent
            .as_ref()
            .is_some_and(|agent| agent.requires_node())
        {
            return Err("Node.js agent launchers are not supported. Configure the recipient with a native Fiote connection; your request remains unsent.".into());
        }
        if configuration.agent.is_none() {
            self.catalog.validate(&mut configuration.settings)?;
            let key = if self.catalog.method(&configuration.settings)?.kind
                == fiote::adapters::AuthKind::None
            {
                Secret::default()
            } else {
                self
                    .vault
                    .key(&vault::slot(&configuration.settings))
                    .await?
                    .ok_or("Sign in to the chosen Fiote's connection before sending. Your request remains unsent.")?
            };
            let provider: Arc<dyn Provider> = if let Some(provider) = &self.provider {
                provider.clone()
            } else if let Some(driver) = self.catalog.driver(&configuration.settings.provider) {
                Arc::new(fiote::driver::DriverProvider::new(
                    driver.clone(),
                    configuration.settings.clone(),
                    key,
                ))
            } else {
                fiote::communication::native::provider(&configuration.settings, &key, None, None)?
            };
            provider.validate_content(&request.content)?;
        }
        if self.running.lock().await.len() >= 8 {
            return Err("Eight Fiote conversations are running. Keep this request and retry when one finishes.".into());
        }
        if let Some(conversation) = &conversation
            && self.running.lock().await.contains_key(&conversation.thread)
        {
            return Err(
                "Fiote is still replying in this conversation. Wait for its reply or Stop the run."
                    .into(),
            );
        }
        let mut receipt = EntryReceipt {
            request_id: request.request_id,
            fingerprint,
            target: target.clone(),
            thread: None,
            message: None,
            reply: None,
            state: "accepted".into(),
            detail: String::new(),
        };
        save(&path, &receipt)?;
        let started = async {
            let thread = if let Some(conversation) = conversation {
                conversation.thread
            } else {
                self.engine
                    .act(
                        Action::CreateThread {
                            target: target.clone(),
                            head: "Assistant request".into(),
                        },
                        None,
                    )
                    .await
                    .map_err(|error| error.to_string())?
                    .created
                    .ok_or("Assistant conversation was not created.")?
            };
            receipt.thread = Some(thread.clone());
            save(&path, &receipt)?;
            let outcome = Box::pin(self.start_input_turn(
                &thread,
                &request.text,
                &request.content,
                None,
                Some(&target),
            ))
            .await?
            .ok_or("The chosen Fiote did not accept the assistant request.")?;
            receipt.message = outcome.created;
            receipt.detail = outcome.warnings.join("\n");
            if let Some(message) = &receipt.message {
                let predicate = store::concepts::resolve(&self.engine.store.pool, "reply-to")
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or("Reply relation is unavailable.")?;
                let replies = store::assertions::subjects_pointing_to(
                    &self.engine.store.pool,
                    &predicate,
                    message,
                )
                .await
                .map_err(|error| error.to_string())?;
                receipt.reply = replies.first().map(|reply| reply.uid.clone());
            }
            receipt.state = if receipt.reply.is_some() {
                "running"
            } else {
                "failed"
            }
            .into();
            Ok::<_, String>(())
        }
        .await;
        if let Err(error) = started {
            receipt.state = "failed".into();
            receipt.detail = error;
        }
        save(&path, &receipt)?;
        self.entry_state(receipt).await
    }
}
