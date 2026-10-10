use super::*;
use nucleus::location::PeerRequest;

#[async_trait::async_trait]
impl crate::location::Network for Wire {
    fn node_id(&self) -> String {
        self.node_id().to_string()
    }

    async fn location_request(
        &self,
        node: &str,
        request: PeerRequest,
    ) -> Result<serde_json::Value, EngineError> {
        let node = node
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("Invalid location device endpoint".into()))?;
        if serde_json::to_vec(&request)?.len() > 16 * 1024 {
            return Err(EngineError::Consequence(
                "Location request exceeds its limit".into(),
            ));
        }
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.request(
                EndpointAddr::new(node),
                ALPN_LOCATION,
                &WireRequest::Location { request },
            ),
        )
        .await
        .map_err(|_| EngineError::Consequence("Location device did not reply".into()))??;
        match response {
            WireResponse::Location { data } => Ok(data),
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                Err(EngineError::Forbidden(message))
            }
            _ => Err(EngineError::Consequence("Unexpected location reply".into())),
        }
    }
}
