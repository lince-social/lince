use crate::{Engine, EngineError};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::OnceLock;

const MAX_BYTES: usize = 2048;
const MAX_AGE: i64 = 900;

fn invalid() -> EngineError {
    EngineError::Consequence("This page expired or changed. Refresh the newest results.".into())
}

fn cipher() -> XChaCha20Poly1305 {
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    XChaCha20Poly1305::new(
        KEY.get_or_init(|| iroh::SecretKey::generate().to_bytes())
            .into(),
    )
}

fn seal<T: Serialize>(aad: &[u8], now: i64, position: &T) -> Result<String, EngineError> {
    let bytes = serde_json::to_vec(&(now, position))?;
    if bytes.len() > 1024 {
        return Err(invalid());
    }
    let random = iroh::SecretKey::generate().to_bytes();
    let nonce: [u8; 24] = random[..24].try_into().map_err(|_| invalid())?;
    let encrypted = cipher()
        .encrypt((&nonce).into(), Payload { msg: &bytes, aad })
        .map_err(|_| invalid())?;
    let mut token = nonce.to_vec();
    token.extend(encrypted);
    Ok(URL_SAFE_NO_PAD.encode(token))
}

fn open<T: DeserializeOwned>(aad: &[u8], now: i64, token: &str) -> Result<T, EngineError> {
    if token.len() > MAX_BYTES {
        return Err(invalid());
    }
    let bytes = URL_SAFE_NO_PAD.decode(token).map_err(|_| invalid())?;
    if bytes.len() < 40 {
        return Err(invalid());
    }
    let nonce: [u8; 24] = bytes[..24].try_into().map_err(|_| invalid())?;
    let plain = cipher()
        .decrypt(
            (&nonce).into(),
            Payload {
                msg: &bytes[24..],
                aad,
            },
        )
        .map_err(|_| invalid())?;
    let (issued, position): (i64, T) = serde_json::from_slice(&plain).map_err(|_| invalid())?;
    if issued > now || now.saturating_sub(issued) > MAX_AGE {
        return Err(invalid());
    }
    Ok(position)
}

impl Engine {
    async fn read_cursor_aad(
        &self,
        purpose: &str,
        principal: &str,
    ) -> Result<Vec<u8>, EngineError> {
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(invalid)?;
        Ok(serde_json::to_vec(&(
            "lince.read-cursor.v1",
            cell.uid,
            purpose,
            principal,
        ))?)
    }

    pub(crate) async fn encode_read_cursor<T: Serialize>(
        &self,
        purpose: &str,
        principal: &str,
        position: &T,
        now: i64,
    ) -> Result<String, EngineError> {
        seal(
            &self.read_cursor_aad(purpose, principal).await?,
            now,
            position,
        )
    }

    pub(crate) async fn decode_read_cursor<T: DeserializeOwned>(
        &self,
        purpose: &str,
        principal: &str,
        token: &str,
        now: i64,
    ) -> Result<T, EngineError> {
        open(&self.read_cursor_aad(purpose, principal).await?, now, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuation_hides_positions_and_binds_context_and_age() {
        let position = ("private-record-identity", "private-time");
        let token = seal(b"purpose/person/cell", 1000, &position).unwrap();
        assert!(!token.contains(position.0));
        assert_eq!(
            open::<(String, String)>(b"purpose/person/cell", 1001, &token).unwrap(),
            (position.0.into(), position.1.into())
        );
        for context in [
            b"other-purpose/person/cell".as_slice(),
            b"purpose/other-person/cell",
            b"purpose/person/other-cell",
        ] {
            assert!(open::<(String, String)>(context, 1001, &token).is_err());
        }
        assert!(open::<(String, String)>(b"purpose/person/cell", 1901, &token).is_err());
        assert!(open::<(String, String)>(b"purpose/person/cell", 999, &token).is_err());
        let mut bytes = URL_SAFE_NO_PAD.decode(&token).unwrap();
        bytes[25] ^= 1;
        assert!(
            open::<(String, String)>(b"purpose/person/cell", 1001, &URL_SAFE_NO_PAD.encode(bytes))
                .is_err()
        );
    }

    #[test]
    fn continuation_rejects_oversized_or_malformed_input() {
        assert!(seal(b"context", 1000, &"x".repeat(1024)).is_err());
        for token in [
            "x".repeat(MAX_BYTES + 1),
            "!invalid".into(),
            URL_SAFE_NO_PAD.encode([0u8; 39]),
        ] {
            assert!(open::<String>(b"context", 1000, &token).is_err());
        }
    }
}
