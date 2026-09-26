pub use fiote::speech::{Job, Model, Provider, Request, Settings, Status};

#[async_trait::async_trait]
pub trait Service: Send + Sync {
    async fn handle(&self, request: Request) -> Result<Status, String>;
}
