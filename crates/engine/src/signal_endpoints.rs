use chrono::{DateTime, Utc};
use nucleus::{Cause, DecimalValue, NewFact, RecordKind};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use store::signal_endpoints::{self, Endpoint};

use crate::{Engine, EngineError};

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Request {
    ConfigureStream {
        signal: String,
        value_type: Option<String>,
        #[serde(default)]
        source_url: String,
        expected_revision: Option<i64>,
    },
    Inspect,
    Save {
        endpoint: Option<String>,
        expected_revision: Option<i64>,
        name: String,
        #[serde(default)]
        public_key: String,
        signal_slug: String,
        enabled: bool,
        cooldown_seconds: i64,
        minimum: String,
        maximum: String,
        expires_at: String,
    },
    SetEnabled {
        endpoint: String,
        expected_revision: i64,
        enabled: bool,
    },
    IssueToken {
        endpoint: String,
        expected_revision: i64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    pub sequence: i64,
    pub issued_at: i64,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    pub audience: String,
    pub endpoint: String,
    pub credential_epoch: i64,
    pub signal: String,
    pub sequence: i64,
    pub issued_at: i64,
    pub value: String,
    pub signature: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Receipt {
    pub endpoint: String,
    pub credential_epoch: i64,
    pub sequence: i64,
    pub fact: String,
    pub received_at: i64,
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "signal_endpoint_invalid",
        message: message.to_string(),
    }
}

fn refused() -> EngineError {
    EngineError::Forbidden("Signal endpoint access refused".into())
}

pub(crate) fn token_digest(token: &str) -> Result<String, EngineError> {
    unhex(token, 32).map_err(|_| refused())?;
    Ok(hex(&Sha256::digest(format!(
        "lince.signal-token.v1\0{token}"
    ))))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str, length: usize) -> Result<Vec<u8>, EngineError> {
    if text.len() != length * 2
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("Use lowercase hexadecimal key and signature bytes"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| {
                if byte <= b'9' {
                    byte - b'0'
                } else {
                    byte - b'a' + 10
                }
            };
            Ok(digit(pair[0]) * 16 + digit(pair[1]))
        })
        .collect()
}

fn key(text: &str) -> Result<VerifyingKey, EngineError> {
    VerifyingKey::from_sec1_bytes(&unhex(text, 33)?)
        .map_err(|_| invalid("Use a compressed SEC1 P-256 public key (33 bytes)"))
}

fn decimal(text: &str) -> Result<DecimalValue, EngineError> {
    if text.len() > 64 {
        return Err(invalid("Reading is too long"));
    }
    let value = DecimalValue::parse_inferred(text).map_err(invalid)?;
    if value.to_string() != text {
        return Err(invalid("Use a canonical decimal string"));
    }
    Ok(value)
}

impl Reading {
    pub fn signing_payload(&self) -> Result<Vec<u8>, EngineError> {
        unhex(&self.audience, 32)?;
        if !nucleus::valid_uid(&self.endpoint, "r")
            || !nucleus::valid_uid(&self.signal, "r")
            || self.credential_epoch <= 0
            || self.sequence <= 0
        {
            return Err(invalid("Invalid endpoint, Signal, key epoch or sequence"));
        }
        decimal(&self.value)?;
        Ok(format!(
            "lince-signal-endpoint-v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
            self.audience,
            self.endpoint,
            self.credential_epoch,
            self.signal,
            self.sequence,
            self.issued_at,
            self.value,
        )
        .into_bytes())
    }
}

impl Engine {
    async fn require_signal_endpoint_admin(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        for permission in [
            "permission:assign",
            "configuration:update",
            "organ:update",
            "record:create",
            "record:update",
            "record:read",
        ] {
            self.require_permission_on(tx, actor, permission).await?;
        }
        Ok(())
    }

    pub(crate) async fn signal_endpoint_request(
        &self,
        request: Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Value, EngineError> {
        if let Request::ConfigureStream { signal, value_type, source_url, expected_revision } = &request {
            let config = self.configure_stream(signal, value_type.as_deref(), source_url, *expected_revision, actor).await?;
            return Ok(json!({"stream": config}));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Create the local Organ first"))?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_signal_endpoint_admin(&mut tx, actor).await?;
        let audience = signal_endpoints::audience_on(&mut tx).await?;
        let mut saved = None;
        let mut issued_token = None;
        match request {
            Request::ConfigureStream { .. } => unreachable!(),
            Request::Inspect => {}
            Request::IssueToken {
                endpoint,
                expected_revision,
            } => {
                let mut row = signal_endpoints::get_on(&mut tx, &endpoint)
                    .await?
                    .ok_or_else(refused)?;
                if row.revision != expected_revision {
                    return Err(invalid("Endpoint changed; refresh before issuing a token"));
                }
                self.require_signal_endpoint_signal(&mut tx, actor, &row.signal_uid)
                    .await?;
                if row.expires_at <= now.timestamp() {
                    return Err(invalid("Renew access expiry before issuing a token"));
                }
                let bytes = if let Some(execution) = nucleus::execution::current() {
                    execution.entropy()
                } else {
                    let mut bytes = [0; 32];
                    getrandom::fill(&mut bytes)
                        .map_err(|_| invalid("Secure randomness is unavailable"))?;
                    bytes
                };
                let token = hex(&bytes);
                row.credential_epoch = row.credential_epoch.checked_add(1).ok_or_else(refused)?;
                row.revision = row.revision.checked_add(1).ok_or_else(refused)?;
                row.last_sequence = 0;
                row.last_fingerprint = None;
                signal_endpoints::save_on(&mut tx, &row, actor, now.timestamp()).await?;
                signal_endpoints::save_token_on(&mut tx, &endpoint, &token_digest(&token)?).await?;
                issued_token = Some(
                    json!({"endpoint": endpoint, "token": token, "path": format!("/signals/{}/readings", row.signal_uid)}),
                );
                saved = Some(row.uid);
            }
            Request::SetEnabled {
                endpoint,
                expected_revision,
                enabled,
            } => {
                let mut row = signal_endpoints::get_on(&mut tx, &endpoint)
                    .await?
                    .ok_or_else(refused)?;
                if row.revision != expected_revision {
                    return Err(invalid("Endpoint changed; refresh before saving"));
                }
                self.require_signal_endpoint_signal(&mut tx, actor, &row.signal_uid)
                    .await?;
                if enabled && row.expires_at <= now.timestamp() {
                    return Err(invalid("Renew access expiry before enabling this endpoint"));
                }
                row.enabled = enabled;
                row.revision = row.revision.checked_add(1).ok_or_else(refused)?;
                signal_endpoints::save_on(&mut tx, &row, actor, now.timestamp()).await?;
                saved = Some(row.uid);
            }
            Request::Save {
                endpoint,
                expected_revision,
                name,
                public_key,
                signal_slug,
                enabled,
                cooldown_seconds,
                minimum,
                maximum,
                expires_at,
            } => {
                let enrolling = endpoint.is_none();
                if !public_key.is_empty() {
                    key(&public_key)?;
                }
                let low = decimal(&minimum)?;
                let high = decimal(&maximum)?;
                let expires = DateTime::parse_from_rfc3339(&expires_at)
                    .map_err(invalid)?
                    .timestamp();
                if name.trim().is_empty()
                    || name.len() > 128
                    || !(1..=86400).contains(&cooldown_seconds)
                    || low.exact_numeric_cmp(high).is_gt()
                    || expires <= now.timestamp()
                    || expires > now.timestamp().saturating_add(366 * 86400)
                {
                    return Err(invalid(
                        "Choose a name, a 1–86400 second cooldown, an ordered value range and an expiry within one year",
                    ));
                }
                let mut row = if let Some(uid) = endpoint {
                    let row = signal_endpoints::get_on(&mut tx, &uid)
                        .await?
                        .ok_or_else(refused)?;
                    if expected_revision != Some(row.revision) {
                        return Err(invalid("Endpoint changed; refresh before saving"));
                    }
                    self.require_signal_endpoint_signal(&mut tx, actor, &row.signal_uid)
                        .await?;
                    row
                } else {
                    if expected_revision.is_some()
                        || !(nucleus::valid_slug(&signal_slug)
                            || nucleus::valid_uid(&signal_slug, "r"))
                        || signal_slug.len() > 128
                    {
                        return Err(invalid("Choose a valid new Signal slug"));
                    }
                    if signal_endpoints::list_on(&mut tx).await?.len() >= 256 {
                        return Err(invalid("Signal endpoint enrollment limit reached"));
                    }
                    let existing: Option<String> = store::sqlx::query_scalar("SELECT uid FROM record WHERE (uid = ? OR slug = ?) AND deleted_at IS NULL LIMIT 1")
                        .bind(&signal_slug).bind(&signal_slug).fetch_optional(&mut *tx).await?;
                    let signal = if let Some(uid) = existing {
                        if !signal_endpoints::signal_available_on(&mut tx, &uid).await? {
                            return Err(invalid("Choose a local Signal"));
                        }
                        self.require_signal_endpoint_signal(&mut tx, actor, &uid)
                            .await?;
                        uid
                    } else {
                        let signal = nucleus::new_uid("r");
                        let checkpoint = self
                            .record_checkpoint_on(
                                &mut tx,
                                actor,
                                vec![signal.clone()],
                                protein::authority::Operation::Create,
                            )
                            .await?;
                        store::records::create_with_uid_on(
                            &mut tx,
                            &signal,
                            store::records::NewRecord {
                                slug: Some(&signal_slug),
                                kind: RecordKind::Signal,
                                head: name.trim(),
                                body: "External Signal input",
                                quantity: store::exact::zero(),
                            },
                            &organ.uid,
                            None,
                        )
                        .await?;
                        if let Some(checkpoint) = checkpoint {
                            checkpoint.finish(&mut tx, Default::default()).await?;
                        }
                        signal
                    };
                    Endpoint {
                        uid: nucleus::new_uid("r"),
                        name: String::new(),
                        public_key: public_key.clone(),
                        signal_uid: signal,
                        enabled: false,
                        revision: 0,
                        credential_epoch: 1,
                        cooldown_seconds,
                        minimum: minimum.clone(),
                        maximum: maximum.clone(),
                        expires_at: expires,
                        last_sequence: 0,
                        last_fingerprint: None,
                        last_fact_uid: None,
                        last_received_at: None,
                        created_at: now.timestamp(),
                    }
                };
                if row.public_key != public_key {
                    if row.enabled {
                        return Err(invalid(
                            "Disable the endpoint before replacing its identity",
                        ));
                    }
                    row.credential_epoch =
                        row.credential_epoch.checked_add(1).ok_or_else(refused)?;
                    row.last_sequence = 0;
                    row.last_fingerprint = None;
                    signal_endpoints::revoke_token_on(&mut tx, &row.uid).await?;
                }
                row.name = name.trim().into();
                row.public_key = public_key;
                row.enabled = enabled && !enrolling;
                row.revision = row.revision.checked_add(1).ok_or_else(refused)?;
                row.cooldown_seconds = cooldown_seconds;
                row.minimum = minimum;
                row.maximum = maximum;
                row.expires_at = expires;
                signal_endpoints::save_on(&mut tx, &row, actor, now.timestamp()).await?;
                saved = Some(row.uid);
            }
        }
        let mut endpoints = signal_endpoints::list_on(&mut tx).await?;
        if actor.is_some() {
            let targets = endpoints
                .iter()
                .map(|endpoint| endpoint.signal_uid.clone())
                .collect::<Vec<_>>();
            let graph = crate::access::policy_graph_on(&mut tx, &targets)
                .await
                .map_err(|_| refused())?;
            let readable = self.readable_on(&mut tx, actor, &graph).await?;
            endpoints.retain(|endpoint| readable.contains(&endpoint.signal_uid));
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"audience": audience, "endpoints": endpoints, "saved": saved, "issued_token": issued_token}),
        )
    }

    pub async fn accept_signal_endpoint_reading(
        &self,
        reading: Reading,
    ) -> Result<Receipt, EngineError> {
        self.access_scope(
            true,
            self.accept_signal_endpoint_reading_inner(reading, None),
        )
        .await
    }

    pub async fn accept_signal_token(
        &self,
        signal: &str,
        token: &str,
        submission: Submission,
    ) -> Result<Receipt, EngineError> {
        let digest = token_digest(token)?;
        let endpoint = signal_endpoints::token_endpoint(&self.store.pool, &digest)
            .await?
            .ok_or_else(refused)?;
        if endpoint.signal_uid != signal {
            return Err(refused());
        }
        let mut connection = self.store.pool.acquire().await?;
        let audience = signal_endpoints::audience_on(&mut connection).await?;
        drop(connection);
        let reading = Reading {
            audience,
            endpoint: endpoint.uid,
            credential_epoch: endpoint.credential_epoch,
            signal: signal.into(),
            sequence: submission.sequence,
            issued_at: submission.issued_at,
            value: submission.value,
            signature: String::new(),
        };
        self.access_scope(
            true,
            self.accept_signal_endpoint_reading_inner(reading, Some(&digest)),
        )
        .await
    }

    async fn require_signal_endpoint_signal(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        actor: Option<&str>,
        signal: &str,
    ) -> Result<(), EngineError> {
        if actor.is_some() {
            let graph = crate::access::policy_graph_on(tx, &[signal.to_string()])
                .await
                .map_err(|_| refused())?;
            if !self.readable_on(tx, actor, &graph).await?.contains(signal) {
                return Err(refused());
            }
        }
        Ok(())
    }

    async fn accept_signal_endpoint_reading_inner(
        &self,
        reading: Reading,
        token_digest: Option<&str>,
    ) -> Result<Receipt, EngineError> {
        let payload = reading.signing_payload()?;
        let fingerprint = hex(&Sha256::digest(&payload));
        let mut tx = store::write_tx(&self.store.pool).await?;
        let now = nucleus::execution::now();
        if store::signal_streams::get_on(&mut tx, &reading.signal).await?.is_some() {
            return Err(invalid("Memory-only Signals use the stream endpoint"));
        }
        let endpoint = signal_endpoints::get_on(&mut tx, &reading.endpoint)
            .await?
            .ok_or_else(refused)?;
        if !endpoint.enabled
            || now.timestamp() >= endpoint.expires_at
            || endpoint.signal_uid != reading.signal
            || endpoint.credential_epoch != reading.credential_epoch
            || signal_endpoints::audience_on(&mut tx).await? != reading.audience
        {
            return Err(refused());
        }
        let proof = if let Some(digest) = token_digest {
            if !signal_endpoints::token_matches_on(&mut tx, &endpoint.uid, digest).await? {
                return Err(refused());
            }
            json!({"algorithm": "bearer-token-sha256", "audience": reading.audience})
        } else {
            let signature =
                Signature::from_slice(&unhex(&reading.signature, 64)?).map_err(|_| refused())?;
            key(&endpoint.public_key)?
                .verify(&payload, &signature)
                .map_err(|_| refused())?;
            json!({"algorithm": "ecdsa-p256-sha256", "public_key": endpoint.public_key, "audience": reading.audience, "signature": reading.signature})
        };
        if !signal_endpoints::signal_available_on(&mut tx, &reading.signal).await? {
            return Err(refused());
        }
        let signal = store::records::quantity_in_transaction(&mut tx, &reading.signal)
            .await?
            .ok_or_else(refused)?;
        if reading.sequence == endpoint.last_sequence
            && endpoint.last_fingerprint.as_deref() == Some(&fingerprint)
        {
            return Ok(Receipt {
                endpoint: endpoint.uid,
                credential_epoch: endpoint.credential_epoch,
                sequence: reading.sequence,
                fact: endpoint.last_fact_uid.ok_or_else(refused)?,
                received_at: endpoint.last_received_at.ok_or_else(refused)?,
            });
        }
        if reading.sequence <= endpoint.last_sequence {
            return Err(EngineError::Conflict {
                code: "signal_endpoint_replay",
                message: "Sequence already used; do not change a pending reading".into(),
            });
        }
        if reading.issued_at > now.timestamp().saturating_add(30)
            || reading.issued_at < now.timestamp().saturating_sub(120)
        {
            return Err(invalid("Reading expired or clock is incorrect"));
        }
        if endpoint.last_received_at.is_some_and(|last| {
            now.timestamp_millis() < last.saturating_add(endpoint.cooldown_seconds * 1000)
        }) {
            return Err(EngineError::Conflict {
                code: "signal_endpoint_cooldown",
                message: "Wait for the endpoint cooldown before sending another reading".into(),
            });
        }
        let value = decimal(&reading.value)?;
        if value.exact_numeric_cmp(decimal(&endpoint.minimum)?).is_lt()
            || value.exact_numeric_cmp(decimal(&endpoint.maximum)?).is_gt()
        {
            return Err(invalid("Reading is outside the allowed value range"));
        }
        let mut new = NewFact::quantity(
            &reading.signal,
            store::exact::difference(value, signal)?,
            Cause::signal(&reading.signal),
        );
        new.payload = Some(json!({"sample": reading.value, "signal_endpoint": reading.endpoint, "credential_epoch": reading.credential_epoch, "sequence": reading.sequence, "issued_at": reading.issued_at, "fingerprint": fingerprint,
            "proof": proof}).to_string());
        let signer = self.signer.lock().await.clone();
        let fact = crate::append::append_one_in_transaction(&mut tx, new, now, signer.as_ref())
            .await?
            .ok_or_else(refused)?;
        signal_endpoints::accept_on(
            &mut tx,
            &endpoint.uid,
            reading.sequence,
            &fingerprint,
            &fact.uid,
            now.timestamp_millis(),
        )
        .await?;
        tx.commit().await?;
        let receipt = Receipt {
            endpoint: endpoint.uid,
            credential_epoch: endpoint.credential_epoch,
            sequence: reading.sequence,
            fact: fact.uid.clone(),
            received_at: now.timestamp_millis(),
        };
        self.notify_query_changed();
        self.observe_committed_fact(fact, now).await?;
        Ok(receipt)
    }
}
