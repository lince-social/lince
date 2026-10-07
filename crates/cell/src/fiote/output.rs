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
    async fn summary(&self, covered: usize, text: &str) -> Result<(), String> {
        if let Some(path) = &self.summary_path {
            let prefix = self
                .original_messages
                .get(..covered)
                .ok_or("Invalid conversation summary coverage.")?;
            save(
                path,
                &fiote::conversation::Summary {
                    covered,
                    fingerprint: fiote::conversation::fingerprint(prefix)?,
                    text: text.into(),
                },
            )?;
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
