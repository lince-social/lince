// non-lean-yet

//! Engine-facing adapter for the Cell transport interface.

use std::sync::Arc;

use iroh::EndpointAddr;

use super::{ALPN_THREAD, AuditAgreement, CarrierProbe, Wire, WireRequest, WireResponse};
use crate::{error::EngineError, pairing::EnrolmentInvite, roster::SignedRoster};

impl Wire {
    pub fn serve_enrolment(self: &Arc<Self>) {
        if self.private.is_some_and(|config| !config.full_protocols) {
            return;
        }
        let as_trait: Arc<dyn crate::enrolment::CellTransport> = self.clone();
        self.engine.set_enroller(Arc::downgrade(&as_trait));
        let engine = Arc::downgrade(&self.engine);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                let Some(engine) = engine.upgrade() else {
                    break;
                };
                if let Err(error) = engine.sweep_calls().await {
                    tracing::warn!(%error, "call cleanup failed");
                }
            }
        });
    }
}

#[async_trait::async_trait]
impl crate::enrolment::CellTransport for Wire {
    async fn sand_packages(
        &self,
        organ: &str,
        query: nucleus::sand_package::Query,
    ) -> Result<nucleus::sand_package::Response, EngineError> {
        self.engine.require_package_contact(organ).await?;
        let contact = store::organs::contact(&self.engine.store.pool, organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("Organ contact is unavailable".into()))?;
        let connection = self
            .dial(&contact)
            .await
            .ok_or_else(|| EngineError::Consequence("Organ contact is unreachable".into()))?;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.exchange(&connection, &WireRequest::SandPackages { query }),
        )
        .await;
        connection.close(0u32.into(), b"package exchange done");
        match result
            .map_err(|_| EngineError::Consequence("Package exchange timed out".into()))??
        {
            WireResponse::SandPackages { response } => Ok(response),
            WireResponse::Error { message } | WireResponse::Refused { message, .. } => {
                Err(EngineError::Consequence(message))
            }
            _ => Err(EngineError::Consequence("Invalid package response".into())),
        }
    }
    async fn pairing_invite(&self) -> Result<Option<crate::pairing::PairingInvite>, EngineError> {
        Wire::pairing_invite(self).await.map(Some)
    }
    async fn call(
        &self,
        node: &str,
        request: crate::calls::Request,
    ) -> Result<crate::calls::Snapshot, EngineError> {
        let mut target = node.to_owned();
        for _ in 0..2 {
            let id = target
                .parse()
                .map_err(|_| EngineError::Consequence("Invalid call coordinator".into()))?;
            let response = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.request(
                    EndpointAddr::new(id),
                    ALPN_THREAD,
                    &WireRequest::Call {
                        request: request.clone(),
                    },
                ),
            )
            .await
            .map_err(|_| EngineError::Consequence("Call coordinator is unreachable".into()))??;
            match response {
                WireResponse::Call { snapshot } => match &snapshot.redirect {
                    Some(node) => target = node.clone(),
                    None => return Ok(snapshot),
                },
                WireResponse::Refused { message, .. } => {
                    return Err(EngineError::Consequence(message));
                }
                _ => return Err(EngineError::Consequence("Invalid call response".into())),
            }
        }
        Err(EngineError::Consequence(
            "Call coordinator could not be reached".into(),
        ))
    }
    fn local_node_id(&self) -> Option<String> {
        Some(self.node_id().to_string())
    }
    fn peer_network(&self) -> Option<serde_json::Value> {
        Some(self.network_status())
    }
    async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError> {
        Wire::enrol(self, invite).await
    }

    async fn reconnect_contact(&self, organ: &str) -> Result<(), EngineError> {
        Wire::reconnect_contact(self, organ).await
    }

    async fn audit_against(
        &self,
        contact_organ: &str,
    ) -> Result<Option<AuditAgreement>, EngineError> {
        Wire::audit_against(self, contact_organ).await
    }

    async fn carrier_probe(&self, node_id: &str) -> CarrierProbe {
        Wire::carrier_probe(self, node_id).await
    }

    async fn collect_mail_now(&self) -> Result<usize, EngineError> {
        Wire::collect_own_mail(self).await
    }

    async fn sync_now(&self) -> Result<usize, EngineError> {
        Wire::sync_once(self).await
    }

    async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError> {
        Wire::ask_to_be_carried(self, node_id).await
    }

    async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError> {
        Wire::redeem_mailbox_invite(self, node_id, token).await
    }
}
