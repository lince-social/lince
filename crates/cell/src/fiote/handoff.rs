use super::*;
use fiote::assistant::{Candidate, Delegate, Directory, Handoff, Source};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Clone)]
pub(super) struct Forwarded {
    pub author: String,
    pub message: String,
}

impl Forwarded {
    pub(super) fn content(
        &self,
        parts: &[nucleus::message::MessagePart],
    ) -> Vec<nucleus::message::MessagePart> {
        let mut result = parts.to_vec();
        result.push(nucleus::message::MessagePart::Reference {
            name: "Original delegated request".into(),
            uri: format!("record:{}", self.message),
        });
        if nucleus::message::validate(&result).is_err() {
            result.pop();
        }
        result
    }
}

impl Host {
    pub(super) async fn assistant_directory(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Directory, String> {
        if !(1..=30).contains(&limit) {
            return Err("Discover between one and thirty Fiotes per page.".into());
        }
        let choices = self.fiote_choices().await?;
        let total = choices.len();
        let mut fiotes = Vec::new();
        for choice in choices.into_iter().skip(offset).take(limit) {
            let row = self.record(&choice.record).await?;
            fiotes.push(Candidate {
                enabled: self
                    .load(&choice.record)?
                    .is_some_and(|config| config.settings.enabled),
                record: choice.record,
                title: choice.title,
                description: row.body.chars().take(1600).collect(),
                description_truncated: row.body.chars().count() > 1600,
            });
        }
        Ok(Directory {
            fiotes,
            next_offset: offset.checked_add(limit).filter(|end| *end < total),
            intake: self.intake_selection().await?,
        })
    }

    fn handoff_path(&self, source: &Source, request: &Delegate) -> PathBuf {
        let identity = format!("{}\0{}", source.thread, request.request_id);
        self.directory
            .join(format!("handoff-{}.json", digest(identity.as_bytes())))
    }

    async fn handoff_state(&self, mut receipt: Handoff) -> Result<Handoff, String> {
        if let Some(reply) = &receipt.reply {
            let message =
                store::records::get_extension(&self.engine.store.pool, reply, "lince.message")
                    .await
                    .map_err(|error| error.to_string())?;
            let active = match &receipt.thread {
                Some(thread) => self.running.lock().await.contains_key(thread),
                None => false,
            };
            receipt.state = match message.as_ref().and_then(|value| value["state"].as_str()) {
                Some("finished") => "completed",
                Some("writing") if active => "running",
                Some("interrupted") => "interrupted",
                _ => "uncertain",
            }
            .into();
        } else if receipt.state == "accepted" {
            receipt.state = "uncertain".into();
            receipt.detail = "The request was accepted without a confirmed child run. Inspect its thread and Messages before retrying.".into();
        }
        Ok(receipt)
    }

    pub(super) async fn handoff_receipts(
        &self,
        thread: Option<&str>,
        fiote: Option<&str>,
    ) -> Result<Vec<Handoff>, String> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&self.directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.file_name().to_string_lossy().starts_with("handoff-") {
                paths.push(entry.path());
            }
        }
        if paths.len() > 4096 {
            return Err("Fiote handoff history exceeds its supported 4,096-receipt limit.".into());
        }
        paths.sort();
        let mut result = Vec::new();
        for path in paths {
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16_384 {
                return Err("A handoff receipt exceeds its size limit.".into());
            }
            let receipt: Handoff =
                serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if thread.is_some_and(|thread| receipt.source.thread != thread)
                || fiote.is_some_and(|fiote| receipt.source.fiote != fiote)
            {
                continue;
            }
            result.push(self.handoff_state(receipt).await?);
        }
        Ok(result)
    }

    pub(super) async fn delegate_request(
        &self,
        source: Source,
        request: Delegate,
    ) -> Result<Handoff, String> {
        if request.request_id.is_empty()
            || request.request_id.len() > 128
            || !request
                .request_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            || !nucleus::valid_uid(&request.target, "r")
            || request.request.len() > 65_536
        {
            return Err("Use a stable request ID of at most 128 letters/digits, a Fiote Record UID, and a request of at most 64 KiB.".into());
        }
        if self.thread_fiote(&source.thread).await? != source.fiote
            || self
                .load(&source.fiote)?
                .is_none_or(|config| config.author != source.author)
        {
            return Err("Handoff source does not match the current Fiote session.".into());
        }
        let predicate = store::concepts::resolve(&self.engine.store.pool, "message-in")
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Message ownership is unavailable.")?;
        if !store::assertions::object_uids_from_subject(
            &self.engine.store.pool,
            &source.message,
            &predicate,
        )
        .await
        .map_err(|e| e.to_string())?
        .contains(&source.thread)
        {
            return Err("The handoff source Message belongs to another conversation.".into());
        }
        let source_row = self.record(&source.message).await?;
        let body = if request.request.trim().is_empty() {
            source_row.body
        } else {
            request.request.clone()
        };
        if body.trim().is_empty() {
            return Err("The delegated request is empty.".into());
        }
        let content = store::message_content::load(&self.engine.store.pool, &source.message)
            .await
            .map_err(|e| e.to_string())?;
        let mut chain: Vec<String> = match store::records::get_extension(
            &self.engine.store.pool,
            &source.thread,
            "lince.fiote-handoff",
        )
        .await
        .map_err(|e| e.to_string())?
        {
            Some(value) => serde_json::from_value(value["chain"].clone())
                .map_err(|_| "Invalid saved delegation chain.")?,
            None => vec![source.fiote.clone()],
        };
        if chain.last() != Some(&source.fiote)
            || chain.iter().collect::<BTreeSet<_>>().len() != chain.len()
            || chain.len() >= 5
            || chain.contains(&request.target)
        {
            return Err(
                "Fiote handoff refuses self-handoff, cycles, and more than four delegation hops."
                    .into(),
            );
        }
        let fingerprint =
            digest(&serde_json::to_vec(&(&source, &request)).map_err(|e| e.to_string())?);
        let path = self.handoff_path(&source, &request);
        let _guard = self.handoff_dispatch.lock().await;
        match std::fs::read(&path) {
            Ok(bytes) => {
                let receipt: Handoff = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                if receipt.fingerprint != fingerprint {
                    return Err("This request ID already belongs to a different handoff. Inspect its receipt.".into());
                }
                return self.handoff_state(receipt).await;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
        let target = self.record(&request.target).await?;
        if store::records::get_extension(&self.engine.store.pool, &request.target, "lince.fiote")
            .await
            .map_err(|e| e.to_string())?
            .is_none()
            || self.load(&request.target)?.is_none_or(|config| !config.settings.enabled)
        {
            return Err(format!("{} needs an enabled Fiote connection before handoff.", target.head));
        }
        if self.running.lock().await.len() >= 8 {
            return Err("Eight Fiote conversations are already running. The handoff has not started; preserve this request and retry after a run finishes.".into());
        }
        chain.push(request.target.clone());
        let mut receipt = Handoff {
            source: source.clone(),
            request_id: request.request_id.clone(),
            target: request.target.clone(),
            fingerprint,
            chain: chain.clone(),
            thread: None,
            message: None,
            reply: None,
            state: "accepted".into(),
            detail: String::new(),
        };
        save(&path, &receipt)?;
        let started = async {
            let thread = self.engine.act(Action::CreateThread {
                target: request.target.clone(), head: format!("Delegated from {}", self.record(&source.fiote).await?.head),
            }, None).await.map_err(|e| e.to_string())?.created.ok_or("The child conversation was not created.")?;
            receipt.thread = Some(thread.clone());
            save(&path, &receipt)?;
            self.engine.act(Action::SetExtension {
                target: thread.clone(), namespace: "lince.fiote-handoff".into(),
                fds: serde_json::json!({"chain":chain,"source":source,"request_id":request.request_id}),
            }, None).await.map_err(|e| e.to_string())?;
            self.engine.act(Action::ReportFioteChild {
                parent: source.thread.clone(), thread: thread.clone(), task: source.message.clone(), state: "working".into(),
            }, None).await.map_err(|e| e.to_string())?;
            let forwarded = Forwarded { author: source.author.clone(), message: source.message.clone() };
            let outcome = Box::pin(self.start_input_turn(&thread, &body, &content, Some(forwarded), Some(&receipt.target))).await?
                .ok_or("The recipient did not accept the delegated request.")?;
            receipt.message = outcome.created;
            if let Some(message) = &receipt.message {
                let reply_to = store::concepts::resolve(&self.engine.store.pool, "reply-to").await.map_err(|e| e.to_string())?.ok_or("Reply relationship is unavailable.")?;
                let replies = store::assertions::subjects_pointing_to(&self.engine.store.pool, &reply_to, message).await.map_err(|e| e.to_string())?;
                receipt.reply = replies.first().map(|reply| reply.uid.clone());
            }
            receipt.state = if receipt.reply.is_some() { "running" } else { "failed" }.into();
            receipt.detail = outcome.warnings.join("\n");
            Ok::<(), String>(())
        }.await;
        if let Err(error) = started {
            receipt.state = "failed".into();
            receipt.detail = error;
        }
        save(&path, &receipt)?;
        self.handoff_state(receipt).await
    }
}
