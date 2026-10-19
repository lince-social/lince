use crate::{Engine, EngineError, signal_endpoints, signal_streams::invalid};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use store::signal_endpoints::Endpoint;
pub mod receiver;

pub const STREAM_MAX_PAYLOAD: usize = ailuros_protocol::MAX_DATA;
pub const AUTH_MAX_PAYLOAD: usize = ailuros_protocol::MAX_AUTH_CONTROL;
pub const STREAM_MAX_FPS: u32 = 15;
pub const AUTH_TIMEOUT_SECONDS: u64 = 5;
pub const STREAM_MAX_BYTES_PER_SECOND: usize = 2 * 1024 * 1024;

pub struct StreamBudget {
    window: Instant,
    frames: u32,
    bytes: usize,
}

impl StreamBudget {
    pub fn new(now: Instant) -> Self {
        Self {
            window: now,
            frames: 0,
            bytes: 0,
        }
    }

    pub fn accept(&mut self, bytes: usize, now: Instant) -> Result<(), EngineError> {
        if now.saturating_duration_since(self.window) >= Duration::from_secs(1) {
            self.window = now;
            self.frames = 0;
            self.bytes = 0;
        }
        self.frames = self.frames.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        if bytes > STREAM_MAX_PAYLOAD
            || self.frames > STREAM_MAX_FPS
            || self.bytes > STREAM_MAX_BYTES_PER_SECOND
        {
            return Err(invalid("Ailuros stream budget exceeded"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamProof {
    pub audience: String,
    pub device: String,
    pub key_epoch: i64,
    pub signal: String,
    pub challenge: String,
    pub signature: String,
}

#[derive(Clone, Debug)]
pub struct StreamIdentity {
    device: String,
    signal: String,
    key_epoch: i64,
    public_key: String,
}

impl StreamIdentity {
    pub(crate) fn signal(&self) -> &str {
        &self.signal
    }
}

fn hexadecimal(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl StreamProof {
    pub fn signing_payload(&self) -> Result<Vec<u8>, EngineError> {
        if !hexadecimal(&self.audience)
            || !hexadecimal(&self.challenge)
            || !nucleus::valid_uid(&self.device, "r")
            || !nucleus::valid_uid(&self.signal, "r")
            || self.key_epoch <= 0
        {
            return Err(invalid("Invalid ailuros stream proof"));
        }
        Ok(format!(
            "lince-ailuros-stream-v1\n{}\n{}\n{}\n{}\n{}\npublish\n",
            self.audience, self.device, self.key_epoch, self.signal, self.challenge
        )
        .into_bytes())
    }
}

impl Engine {
    pub async fn ailuros_stream_challenge(&self) -> Result<(String, String), EngineError> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|_| invalid("Secure randomness unavailable"))?;
        let mut connection = self.store.pool.acquire().await?;
        let audience = store::signal_endpoints::audience_on(&mut connection).await?;
        Ok((audience, signal_endpoints::hex(&nonce)))
    }

    pub async fn authenticate_ailuros_stream(
        &self,
        proof: &StreamProof,
        challenge: &str,
        signal: &str,
    ) -> Result<StreamIdentity, EngineError> {
        let payload = proof.signing_payload()?;
        if proof.challenge != challenge || proof.signal != signal {
            return Err(EngineError::Forbidden(
                "Ailuros stream proof refused".into(),
            ));
        }
        let mut connection = self.store.pool.acquire().await?;
        if store::signal_endpoints::audience_on(&mut connection).await? != proof.audience {
            return Err(EngineError::Forbidden(
                "Ailuros stream audience refused".into(),
            ));
        }
        let endpoint = store::signal_endpoints::get_on(&mut connection, &proof.device)
            .await?
            .ok_or_else(|| invalid("Ailuros device unavailable"))?;
        signal_endpoints::verify_proof(&endpoint.public_key, &payload, &proof.signature)?;
        let identity = StreamIdentity {
            device: proof.device.clone(),
            signal: proof.signal.clone(),
            key_epoch: proof.key_epoch,
            public_key: endpoint.public_key,
        };
        drop(connection);
        self.authorize_ailuros_stream(&identity).await?;
        Ok(identity)
    }

    pub async fn authorize_ailuros_stream(
        &self,
        identity: &StreamIdentity,
    ) -> Result<Endpoint, EngineError> {
        let mut connection = self.store.pool.acquire().await?;
        let endpoint = store::signal_endpoints::get_on(&mut connection, &identity.device)
            .await?
            .ok_or_else(|| invalid("Ailuros device unavailable"))?;
        if !endpoint.enabled
            || endpoint.signal_uid != identity.signal
            || endpoint.credential_epoch != identity.key_epoch
            || endpoint.public_key != identity.public_key
            || endpoint.expires_at <= nucleus::execution::now().timestamp()
            || !store::signal_endpoints::signal_available_on(&mut connection, &identity.signal)
                .await?
        {
            return Err(EngineError::Forbidden(
                "Ailuros stream access refused".into(),
            ));
        }
        Ok(endpoint)
    }
}
