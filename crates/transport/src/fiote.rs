use async_trait::async_trait;

#[async_trait]
pub trait Service: Send + Sync {
    async fn handle(
        &self,
        request: fiote::config::Request,
    ) -> Result<fiote::config::Status, String>;
    async fn send(
        &self,
        thread: &str,
        body: &str,
    ) -> Result<Option<engine::actions::ActionOutcome>, String>;
}
