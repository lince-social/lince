//! Speech capabilities supplied by the host to a local Transport session.

use async_trait::async_trait;

pub use fiote::speech::{Job, Request, Status};

/// Receives speech control requests from a local Cell.
#[async_trait]
pub trait Service: Send + Sync {
    /// Handles a speech request and returns the resulting status.
    ///
    /// # Errors
    /// Returns the diagnostic to send in the speech response if the request fails.
    async fn handle(&self, request: Request) -> Result<Status, String>;
}
