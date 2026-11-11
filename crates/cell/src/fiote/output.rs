use super::*;
use fiote::provider::TextOutput;

pub(super) struct Output<'a> {
    pub context_path: Option<PathBuf>,
    pub journal_path: Option<PathBuf>,
    pub summary_path: Option<PathBuf>,
    pub original_messages: Vec<Message>,
    pub source_message: Option<String>,
    pub usage_path: Option<PathBuf>,
    pub tools: &'a Registry,
    pub message: &'a str,
    pub text: Mutex<String>,
}

impl Output<'_> {
    async fn write(&self, operation: &str, text: &str) -> Result<(), String> {
        let result = self.tools.run("lince_message", serde_json::json!({
            "operation":operation,"request_id":nucleus::new_uid("stream"),"message_uid":self.message,"text":text,
        })).await;
        if result["ok"] == true {
            Ok(())
        } else {
            Err(result["error"]
                .as_str()
                .unwrap_or("Could not save Fiote's message.")
                .into())
        }
    }

    pub async fn finish(&self, body: &str, state: MessageState) -> Result<(), String> {
        if let Some(path) = &self.journal_path {
            let mut journal = history::load(path)?.unwrap_or_default();
            journal.body = Some(body.into());
            save(path, &journal)?;
        }
        self.write(
            if state == MessageState::Finished {
                "finish"
            } else {
                "interrupt"
            },
            body,
        )
        .await
    }
}

#[async_trait::async_trait]
impl TextOutput for Output<'_> {
    async fn tool_pending(&self, call: &fiote::provider::ToolCall) -> Result<(), String> {
        if let Some(path) = &self.journal_path {
            let mut journal = history::load(path)?.unwrap_or_default();
            journal.pending = Some(call.clone());
            save(path, &journal)?;
        }
        Ok(())
    }
    async fn executed_calls(&self) -> Result<Vec<String>, String> {
        Ok(self
            .original_messages
            .iter()
            .filter_map(|message| match message {
                Message::Tool { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect())
    }
    async fn compacted(&self, before: &[Message], after: &[Message]) -> Result<(), String> {
        let mut original = self.original_messages.clone();
        let journal = if let Some(path) = &self.journal_path {
            history::checkpoint(path, before)?;
            history::load(path)?
        } else {
            None
        };
        if let (Some(path), Some(journal)) = (&self.journal_path, &journal) {
            original.extend(history::frames(path, journal)?);
        }
        if let Some(path) = &self.summary_path {
            save(
                path,
                &fiote::conversation::Checkpoint {
                    covered: original.len(),
                    fingerprint: fiote::conversation::fingerprint(&original)?,
                    replacement: after.to_vec(),
                },
            )?;
        }
        if let Some(path) = &self.journal_path {
            history::replace_context(path, before, after)?;
        }
        Ok(())
    }
    async fn context(&self, messages: &[Message]) -> Result<(), String> {
        if let Some(path) = &self.journal_path {
            history::checkpoint(path, messages)?;
        }
        if let Some(path) = &self.context_path {
            super::context::supplied(path, messages, self.source_message.as_deref())?;
        }
        Ok(())
    }
    async fn usage(&self, report: nucleus::operation::Usage) -> Result<(), String> {
        if let Some(path) = &self.usage_path {
            usage::record(path, report)?;
        }
        Ok(())
    }
    async fn update(&self, text: &str) -> Result<(), String> {
        *self.text.lock().await = text.into();
        self.write("update", text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn active_turn_checkpoint_restores_archived_context_without_losing_receipts() {
        let root = tempfile::tempdir().unwrap();
        let journal_path = history::path(root.path(), "fixture");
        let summary_path = root.path().join("summary.json");
        let original = vec![Message::User("Complete work without reboot".into())];
        let mut before = original.clone();
        before.push(Message::Assistant {
            text: String::new(),
            calls: vec![fiote::provider::ToolCall {
                id: "saved-effect".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({}),
                signatures: None,
            }],
        });
        before.push(Message::Tool {
            id: "saved-effect".into(),
            name: "write_file".into(),
            result: serde_json::json!({"ok":true,"receipt":"saved"}),
        });
        let tools = Registry::default();
        let output = Output {
            context_path: None,
            journal_path: Some(journal_path.clone()),
            summary_path: Some(summary_path.clone()),
            original_messages: original.clone(),
            source_message: Some("fixture-user".into()),
            usage_path: None,
            tools: &tools,
            message: "fixture",
            text: Default::default(),
        };
        output.context(&before).await.unwrap();
        let after = vec![Message::Replay {
            provider: "chatgpt".into(),
            items: vec![serde_json::json!({"type":"compaction","encrypted_content":"opaque"})],
        }];
        output.compacted(&before, &after).await.unwrap();
        let mut continuing = after.clone();
        continuing.push(Message::Assistant {
            text: "Finished".into(),
            calls: vec![],
        });
        output.context(&continuing).await.unwrap();
        let journal = history::load(&journal_path).unwrap().unwrap();
        assert_eq!(journal.archives.len(), 1);
        let mut raw = original;
        raw.extend(history::frames(&journal_path, &journal).unwrap());
        let checkpoint: fiote::conversation::Checkpoint =
            serde_json::from_slice(&std::fs::read(summary_path).unwrap()).unwrap();
        assert_eq!(checkpoint.covered, 3);
        let restored = checkpoint.apply(&raw).unwrap().unwrap();
        assert_eq!(
            fiote::conversation::fingerprint(&restored).unwrap(),
            fiote::conversation::fingerprint(&continuing).unwrap()
        );
        assert!(raw.iter().any(|m| matches!(m, Message::Tool { id, result, .. } if id == "saved-effect" && result["receipt"] == "saved")));
        raw[0] = Message::User("Edited goal".into());
        assert!(checkpoint.apply(&raw).unwrap().is_none());
    }
}
