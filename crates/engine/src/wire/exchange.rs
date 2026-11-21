// non-lean-yet

//! Outbound request exchange and Transfer messages.

use std::sync::Arc;

use iroh::{EndpointAddr, endpoint::Connection};

use super::{
    ALPN_SYNC, MAX_FRAME_BYTES, TransferPeer, TransferVerb, Wire, WireRequest, WireResponse,
};
use crate::error::EngineError;

impl Wire {
    pub fn set_transfer_handler(&self, handler: Arc<dyn TransferPeer>) {
        *self.transfer.lock().expect("transfer handler") = Some(handler);
    }

    pub async fn transfer_post(
        &self,
        contact_organ: &str,
        verb: TransferVerb,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, contact_organ)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence(format!("{contact_organ} is not introduced"))
            })?;
        if contact.trust != "known" {
            return Err(EngineError::Consequence(format!(
                "{contact_organ} is not a known contact, so nothing is delivered to it"
            )));
        }
        let node_id: iroh::EndpointId = contact
            .node_id
            .as_deref()
            .unwrap_or_default()
            .parse()
            .map_err(|_| {
                EngineError::Consequence(format!("{contact_organ} has no dialable NodeId"))
            })?;
        match self
            .request(
                EndpointAddr::new(node_id),
                ALPN_SYNC,
                &WireRequest::TransferPost { verb, body },
            )
            .await?
        {
            WireResponse::Transfer { body } => Ok(body),
            WireResponse::Error { message } | WireResponse::Refused { message, .. } => {
                Err(EngineError::Consequence(message))
            }
            _ => Err(EngineError::Consequence(
                "peer answered a Transfer exchange with something else".into(),
            )),
        }
    }

    /// Opens a connection for a single request and closes it after the exchange.
    ///
    /// # Errors
    /// Returns transport and encoding failures. A peer refusal is a valid reply.
    pub async fn request(
        &self,
        addr: impl Into<EndpointAddr>,
        alpn: &[u8],
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let connection =
            self.endpoint.connect(addr, alpn).await.map_err(|error| {
                EngineError::Consequence(format!("iroh connect failed: {error}"))
            })?;
        let response = self.exchange(&connection, request).await;
        connection.close(0u32.into(), b"done");
        response
    }

    pub(super) async fn exchange(
        &self,
        connection: &Connection,
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| EngineError::Consequence(format!("peer open: {error}")))?;
        let bytes = serde_json::to_vec(request)
            .map_err(|error| EngineError::Consequence(error.to_string()))?;
        send.write_all(&bytes)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
        send.finish()
            .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        let raw = recv
            .read_to_end(MAX_FRAME_BYTES)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer read: {error}")))?;
        serde_json::from_slice(&raw)
            .map_err(|error| EngineError::Consequence(format!("unreadable response: {error}")))
    }
}
