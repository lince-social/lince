use super::*;
use fiote::provider::TextOutput;

pub(super) struct Output<'a> {
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
    async fn update(&self, text: &str) -> Result<(), String> {
        *self.text.lock().await = text.into();
        self.write("update", text).await
    }
}
