// non-lean-yet
// Scope: struct Output, mod tests.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 54, 64, 70.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites finish: String
//   (31).
// - [Clippy probe: clippy::indexing_slicing] Verify bounds or use checked access on fallible
//   inputs. Sites 21, 24, 177.
// - [Clippy probe: clippy::wildcard_imports] Use explicit imports while preserving framework and
//   test conventions. Sites 1.
// - [rustc probe: unreachable_pub] unreachable `pub` item. Sites 31.

use super::*;
use fiote::provider::{TextOutput, ToolDefinition};
use serde_json::Value;

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
    pub timeline: Option<timeline::Timeline<'a>>,
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
        if let Some(timeline) = &self.timeline {
            timeline.interrupt_tools().await;
        }
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
        if let Some(timeline) = &self.timeline {
            let input = match call.name.as_str() {
                "create_file" | "write_file" | "edit_file" | "read_file" | "list_directory" => {
                    serde_json::json!({"path": call.arguments["path"]})
                }
                "run_command" => serde_json::json!({"command": call.arguments["command"]}),
                _ => Value::Null,
            };
            timeline
                .native_activity(
                    &call.id,
                    &serde_json::json!({
                        "title": call.name,
                        "status": "in_progress",
                        "rawInput": input,
                    }),
                )
                .await;
        }
        Ok(())
    }

    async fn tool_finished(&self, id: &str, result: &Value, diff: Option<&str>) {
        if let Some(timeline) = &self.timeline {
            let output = &result["result"];
            let detail = if result["ok"] != true {
                result.get("error").cloned().unwrap_or_default()
            } else if output.get("exit_code").is_some() {
                serde_json::json!({
                    "exit_code": output["exit_code"],
                    "stdout": output["stdout"],
                    "stderr": output["stderr"],
                    "timed_out": output["timed_out"],
                })
            } else {
                Value::Null
            };
            timeline.native_activity(id, &serde_json::json!({
                "status": if result["ok"] == true && output["success"] != false { "completed" } else { "failed" },
                "rawOutput": detail,
                "diff": diff,
            })).await;
        }
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
        Ok(())
    }
    async fn model_context(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<(), String> {
        if let Some(path) = &self.context_path {
            super::context::supplied(path, messages, tools, self.source_message.as_deref())?;
        }
        Ok(())
    }
    fn tool_output_directory(&self) -> Option<PathBuf> {
        self.summary_path.as_ref().map(|path| path.with_extension("receipts"))
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
            timeline: None,
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
