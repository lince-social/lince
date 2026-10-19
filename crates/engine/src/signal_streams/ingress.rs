use super::{MAX_PAYLOAD, Metadata, Publisher, Snapshot, invalid};
use crate::{Engine, EngineError, ailuros};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use store::signal_endpoints::Endpoint;
use tokio::sync::watch;

pub enum Credential {
    Token { value: String, epoch: i64 },
    Device(ailuros::StreamIdentity),
}

impl Credential {
    pub async fn authorize(&self, engine: &Engine, signal: &str) -> Result<Endpoint, EngineError> {
        match self {
            Self::Token { value, epoch } => {
                let endpoint = engine.stream_token(signal, value).await?;
                if endpoint.credential_epoch != *epoch {
                    return Err(invalid("Credential changed"));
                }
                Ok(endpoint)
            }
            Self::Device(identity) => {
                if identity.signal() != signal {
                    return Err(invalid("Device identity belongs to a different Signal"));
                }
                engine.authorize_ailuros_stream(identity).await
            }
        }
    }

    pub fn max_payload(&self) -> usize {
        match self {
            Self::Token { .. } => MAX_PAYLOAD,
            Self::Device(_) => ailuros::STREAM_MAX_PAYLOAD,
        }
    }
}

pub struct PublisherSession {
    signal: String,
    credential: Credential,
    producer: Publisher,
    metadata: Option<Metadata>,
    budget: ailuros::StreamBudget,
    demand_key: Option<(String, usize)>,
    received: Instant,
    window: Instant,
    messages: u32,
}

impl PublisherSession {
    pub async fn open(
        engine: &Engine,
        signal: &str,
        credential: Credential,
    ) -> Result<Self, EngineError> {
        credential.authorize(engine, signal).await?;
        let channel = engine.stream_channel(signal, None).await?;
        if !channel.config.source_url.is_empty() {
            return Err(EngineError::Forbidden(
                "Signal uses an external source".into(),
            ));
        }
        let producer = channel.publisher().map_err(|_| EngineError::Conflict {
            code: "signal_producer_busy",
            message: "Signal already has a producer".into(),
        })?;
        Ok(Self::new(credential, producer))
    }

    fn new(credential: Credential, producer: Publisher) -> Self {
        let now = Instant::now();
        Self {
            signal: producer.channel.config.signal_uid.clone(),
            credential,
            producer,
            metadata: None,
            budget: ailuros::StreamBudget::new(now),
            demand_key: None,
            received: now,
            window: now,
            messages: 0,
        }
    }

    pub fn demand_changes(&self) -> watch::Receiver<Snapshot> {
        self.producer.demand.clone()
    }

    pub fn demand(&mut self) -> Result<Option<Value>, EngineError> {
        let snapshot = self.producer.snapshot();
        if snapshot.status == "Configuration changed" {
            return Err(invalid("Stream configuration changed"));
        }
        let key = (snapshot.epoch.clone(), snapshot.consumers);
        if self.demand_key.as_ref() == Some(&key) {
            return Ok(None);
        }
        if self.demand_key.as_ref().map(|key| &key.0) != Some(&key.0) {
            self.metadata = None;
        }
        self.demand_key = Some(key);
        Ok(Some(json!({"type":"demand", "epoch":snapshot.epoch,
            "consumers":snapshot.consumers,"active":snapshot.consumers > 0})))
    }

    pub fn received(&mut self, now: Instant) -> Result<(), EngineError> {
        self.received = now;
        if now.saturating_duration_since(self.window) >= Duration::from_secs(1) {
            self.window = now;
            self.messages = 0;
        }
        self.messages = self.messages.saturating_add(1);
        if self.messages > 150 {
            return Err(invalid("Stream message budget exceeded"));
        }
        Ok(())
    }

    pub fn expired(&self) -> bool {
        self.received.elapsed() > Duration::from_secs(30)
    }

    pub async fn authorize(&self, engine: &Engine) -> Result<Endpoint, EngineError> {
        self.credential.authorize(engine, &self.signal).await
    }

    pub fn metadata(&mut self, text: &str) -> Result<(), EngineError> {
        if text.len() > ailuros_protocol::MAX_CONTROL || self.metadata.is_some() {
            return Err(invalid("Invalid stream metadata order or size"));
        }
        self.metadata =
            Some(serde_json::from_str(text).map_err(|_| invalid("Invalid stream metadata"))?);
        Ok(())
    }

    pub async fn publish(&mut self, engine: &Engine, bytes: &[u8]) -> Result<Value, EngineError> {
        let metadata = self
            .metadata
            .take()
            .ok_or_else(|| invalid("Missing stream metadata"))?;
        if bytes.len() > self.credential.max_payload() {
            return Err(invalid("Stream payload exceeds limit"));
        }
        if matches!(&self.credential, Credential::Device(_)) {
            self.budget.accept(bytes.len(), Instant::now())?;
            let now = nucleus::execution::now().timestamp_millis();
            if metadata.captured_at < now.saturating_sub(120_000)
                || metadata.captured_at > now.saturating_add(30_000)
            {
                return Err(invalid("Stream capture time outside accepted range"));
            }
        }
        let epoch = metadata.epoch.clone();
        let sequence = metadata.sequence;
        let retained = engine
            .access_scope(false, async {
                let current = self.authorize(engine).await?;
                if metadata.content_type == "text/decimal" {
                    super::validate_scalar_endpoint(&current, bytes)?;
                }
                if self.producer.snapshot().consumers == 0 {
                    return Ok(false);
                }
                self.producer.publish(metadata, bytes.to_vec()).await
            })
            .await?;
        Ok(json!({"type":"published","epoch":epoch,"sequence":sequence,"retained":retained}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn session() -> (Arc<super::super::Channel>, PublisherSession) {
        let channel = Arc::new(super::super::Channel::new(
            store::signal_streams::Configuration {
                signal_uid: nucleus::new_uid("r"),
                value_type: "video".into(),
                source_url: String::new(),
                revision: 1,
            },
        ));
        let producer = channel.publisher().unwrap();
        let session = PublisherSession::new(
            Credential::Token {
                value: String::new(),
                epoch: 1,
            },
            producer,
        );
        (channel, session)
    }

    fn metadata(epoch: &str) -> String {
        serde_json::to_string(&Metadata {
            epoch: epoch.into(),
            sequence: 1,
            captured_at: 0,
            content_type: "image/jpeg".into(),
        })
        .unwrap()
    }

    #[test]
    fn demand_tracks_viewer_changes_and_discards_metadata_when_the_epoch_changes() {
        let (channel, mut session) = session();
        let idle = session.demand().unwrap().unwrap();
        assert_eq!(idle["active"], false);
        assert!(session.demand().unwrap().is_none());
        let first = channel.subscribe().unwrap();
        let active = session.demand().unwrap().unwrap();
        assert_eq!(active["consumers"], 1);
        let second = channel.subscribe().unwrap();
        assert_eq!(session.demand().unwrap().unwrap()["consumers"], 2);
        drop(first);
        let remaining = session.demand().unwrap().unwrap();
        assert_eq!(remaining["epoch"], active["epoch"]);
        assert_eq!(remaining["consumers"], 1);
        session
            .metadata(&metadata(active["epoch"].as_str().unwrap()))
            .unwrap();
        drop(second);
        let stopped = session.demand().unwrap().unwrap();
        assert_ne!(stopped["epoch"], active["epoch"]);
        assert_eq!(stopped["active"], false);
        let _reopened = channel.subscribe().unwrap();
        let reopened = session.demand().unwrap().unwrap();
        assert_eq!(reopened["active"], true);
        session
            .metadata(&metadata(reopened["epoch"].as_str().unwrap()))
            .unwrap();
    }

    #[test]
    fn control_messages_are_bounded_and_metadata_cannot_accumulate() {
        let (channel, mut session) = session();
        assert!(session.metadata(&" ".repeat(4097)).is_err());
        assert!(session.metadata("{}").is_err());
        let text = metadata(&channel.snapshot().epoch);
        session.metadata(&text).unwrap();
        assert!(session.metadata(&text).is_err());
        let now = Instant::now();
        for _ in 0..150 {
            session.received(now).unwrap();
        }
        assert!(session.received(now).is_err());
        session.received(now + Duration::from_secs(1)).unwrap();
        channel.close();
        assert!(session.demand().is_err());
    }
}
