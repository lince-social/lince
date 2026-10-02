use super::*;
use nucleus::social::{MAX_PRIVATE_FRAME_BYTES, PublicRequest};
use serde_json::{Value, json};

#[async_trait::async_trait]
impl crate::social::Network for Wire {
    async fn request(
        &self,
        destination: &str,
        request: PublicRequest,
    ) -> Result<Value, EngineError> {
        let id = destination
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("Invalid selected service endpoint".into()))?;
        let body = serde_json::to_vec(&request)?;
        let frame_limit = request.frame_limit();
        if body.len() > frame_limit {
            return Err(EngineError::Consequence(
                "The social request is too large".into(),
            ));
        }
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let connection = self
                .endpoint
                .connect(EndpointAddr::new(id), ALPN_SOCIAL)
                .await
                .map_err(|_| {
                    EngineError::Consequence("The selected service is unavailable".into())
                })?;
            let result = async {
                let (mut send, mut recv) = connection
                    .open_bi()
                    .await
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                send.write_all(&body)
                    .await
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                send.finish()
                    .map_err(|e| EngineError::Consequence(e.to_string()))?;
                let bytes = recv.read_to_end(frame_limit).await.map_err(|_| {
                    EngineError::Consequence("The social reply is too large or incomplete".into())
                })?;
                let data: Value = serde_json::from_slice(&bytes)?;
                if let Some(error) = data["error"].as_str() {
                    return Err(EngineError::Consequence(error.into()));
                }
                Ok(data["data"].clone())
            }
            .await;
            connection.close(0u32.into(), b"social request complete");
            result
        })
        .await;
        result.map_err(|_| {
            EngineError::Consequence("The selected service did not reply in time".into())
        })?
    }
}

impl Wire {
    pub(super) async fn serve_social_connection(
        &self,
        connection: Connection,
    ) -> Result<(), EngineError> {
        let work = async {
            let (mut send, mut recv) = connection
                .accept_bi()
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            let bytes = recv
                .read_to_end(MAX_PRIVATE_FRAME_BYTES)
                .await
                .map_err(|_| EngineError::Consequence("The social request is too large".into()))?;
            let response = match self
                .engine
                .social_public_frame(
                    &connection.remote_id().to_string(),
                    &self.node_id().to_string(),
                    &bytes,
                    nucleus::execution::now().timestamp(),
                )
                .await
            {
                Ok(data) => json!({"data":data}),
                Err(_) => {
                    json!({"error":"This service refused the request. Check its roles, limits and the publication authority."})
                }
            };
            let body = serde_json::to_vec(&response)?;
            if body.len() > MAX_PRIVATE_FRAME_BYTES {
                return Err(EngineError::Consequence(
                    "The social reply is too large".into(),
                ));
            }
            send.write_all(&body)
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.finish()
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            send.stopped()
                .await
                .map_err(|e| EngineError::Consequence(e.to_string()))?;
            Ok(())
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), work).await;
        connection.close(0u32.into(), b"social request complete");
        result.map_err(|_| EngineError::Consequence("Social request timed out".into()))?
    }
}
