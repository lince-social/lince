//! Fiote capabilities supplied by the host to a local Transport session.

use async_trait::async_trait;
use engine::actions::ActionOutcome;
use fiote::{
    acp::terminal::{TerminalFrame, TerminalRequest},
    assistant::{Delegate, Directory, Handoff, Source},
    config::{Request, Status},
};
use nucleus::message::MessagePart;

/// A host-provided Fiote service shared by Transport sessions.
#[async_trait]
pub trait Service: Send + Sync {
    /// Lists a page of available assistants.
    ///
    /// # Errors
    /// The default implementation reports that discovery is unavailable.
    async fn discover(&self, _offset: usize, _limit: usize) -> Result<Directory, String> {
        Err("Fiote discovery is unavailable for this connection.".into())
    }

    /// Submits work from a scoped source and returns the handoff receipt.
    ///
    /// # Errors
    /// The default implementation reports that handoff is unavailable.
    async fn delegate(&self, _source: Source, _request: Delegate) -> Result<Handoff, String> {
        Err("Fiote handoff is unavailable for this connection.".into())
    }

    /// Reads the handoff receipts visible from a thread.
    ///
    /// # Errors
    /// The default implementation reports that handoff inspection is unavailable.
    async fn handoffs(&self, _thread: &str) -> Result<Vec<Handoff>, String> {
        Err("Fiote handoff inspection is unavailable for this connection.".into())
    }

    /// Handles a terminal interaction for provider login.
    ///
    /// # Errors
    /// The default implementation reports that terminal login is unavailable.
    async fn terminal(&self, _request: TerminalRequest) -> Result<TerminalFrame, String> {
        Err("Terminal login is unavailable.".into())
    }

    /// Handles a Fiote request and returns the resulting status.
    ///
    /// # Errors
    /// Returns the diagnostic to send in the Fiote response if the request fails.
    async fn handle(&self, request: Request) -> Result<Status, String>;

    /// Submits a plain-text message. `None` leaves the message for normal Record handling.
    ///
    /// # Errors
    /// Returns a diagnostic if the service refuses or fails to submit the message.
    async fn send(&self, thread: &str, body: &str) -> Result<Option<ActionOutcome>, String>;

    /// Submits structured content. The default delegates plain text to [`send`](Self::send).
    ///
    /// # Errors
    /// The default rejects attachments and forwards errors from the plain-text submission.
    async fn send_content(
        &self,
        thread: &str,
        body: &str,
        content: &[MessagePart],
    ) -> Result<Option<ActionOutcome>, String> {
        if content.is_empty() {
            self.send(thread, body).await
        } else {
            Err("This Fiote service does not accept attachments.".into())
        }
    }
}
