use async_trait::async_trait;

#[async_trait]
pub trait Service: Send + Sync {
    async fn terminal(
        &self,
        _request: fiote::acp::terminal::TerminalRequest,
    ) -> Result<fiote::acp::terminal::TerminalFrame, String> {
        Err("Terminal login is unavailable.".into())
    }
    async fn handle(
        &self,
        request: fiote::config::Request,
    ) -> Result<fiote::config::Status, String>;
    async fn send(
        &self,
        thread: &str,
        body: &str,
    ) -> Result<Option<engine::actions::ActionOutcome>, String>;
    async fn send_content(
        &self,
        thread: &str,
        body: &str,
        content: &[nucleus::message::MessagePart],
    ) -> Result<Option<engine::actions::ActionOutcome>, String> {
        if !content.is_empty() {
            return Err("This Fiote service does not accept attachments.".into());
        }
        self.send(thread, body).await
    }
}
