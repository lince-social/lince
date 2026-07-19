//! Per-session verification for client-held Person keys.
//!
//! A verified Action intent proves who requested an Action. It is deliberately
//! distinct from a Fact signature: Fact hashes are produced later, inside the
//! semantic transaction, and must never be signed or attributed by the server
//! on the Person's behalf.

use std::collections::HashSet;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use nucleus::action_intent::{ActionIntentSessionProof, SignedActionIntent};

use crate::Engine;
use crate::actions::{Action, ActionOutcome, VerifiedActionAuthorship};
use crate::error::EngineError;

const MAX_IDENTIFIER_BYTES: usize = 512;
const MAX_CHALLENGE_BYTES: usize = 512;
const MAX_ACTION_BYTES: usize = 1_048_576;
const MAX_ACTION_BASE64_BYTES: usize = 1_398_104;

/// Server-owned state for one authenticated transport session.
///
/// Fields that establish authority are private so callers cannot construct a
/// session for an arbitrary Person. Use `Engine::begin_action_intent_session`.
pub struct ActionIntentSession {
    authenticated_actor: String,
    person_uid: String,
    session_id: String,
    challenge: String,
    bound_key_id: Option<String>,
    next_sequence: u64,
    used_message_ids: HashSet<String>,
}

impl ActionIntentSession {
    pub fn authenticated_actor(&self) -> &str {
        &self.authenticated_actor
    }

    pub fn person_uid(&self) -> &str {
        &self.person_uid
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn bound_key_id(&self) -> Option<&str> {
        self.bound_key_id.as_deref()
    }
}

/// An Action whose session, Person identity, key and signature were verified.
/// The actor is copied from server-owned session state, never from the client.
pub struct VerifiedActionIntent {
    intent_uid: String,
    pub message_id: String,
    action: Action,
    authenticated_actor: String,
    person_uid: String,
    key_id: String,
    session_id: String,
    session_challenge: String,
    sequence: u64,
    action_base64: String,
    signature: String,
}

impl VerifiedActionIntent {
    pub fn intent_uid(&self) -> &str {
        &self.intent_uid
    }
    pub fn authenticated_actor(&self) -> &str {
        &self.authenticated_actor
    }

    pub fn person_uid(&self) -> &str {
        &self.person_uid
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn message_id(&self) -> &str {
        &self.message_id
    }
}

impl Engine {
    /// Start signed-intent verification for an authenticated app user.
    ///
    /// The challenge is generated here rather than accepted from the client.
    /// Reconnecting creates a new challenge and invalidates old envelopes.
    pub async fn begin_action_intent_session(
        &self,
        authenticated_actor: &str,
    ) -> Result<ActionIntentSession, EngineError> {
        let user_id: i64 = authenticated_actor
            .parse()
            .map_err(|_| EngineError::Forbidden("unrecognized actor".into()))?;
        let user = store::auth::user_by_id(&self.store.pool, user_id)
            .await?
            .ok_or_else(|| EngineError::Forbidden("unrecognized actor".into()))?;
        let person_uid = store::auth::person_for_user(&self.store.pool, user.id)
            .await?
            .ok_or_else(|| {
                EngineError::Forbidden("authenticated user has no assigned person identity".into())
            })?;

        Ok(ActionIntentSession {
            authenticated_actor: authenticated_actor.to_owned(),
            person_uid,
            session_id: uuid::Uuid::new_v4().to_string(),
            challenge: format!("v1:{}", uuid::Uuid::new_v4()),
            bound_key_id: None,
            next_sequence: 1,
            used_message_ids: HashSet::new(),
        })
    }

    /// Bind a client-held key to this authenticated session after proving
    /// possession over the server challenge. A new key may be published for
    /// the mapped Person, but an existing key id is never reassigned.
    pub async fn authenticate_action_intent_session(
        &self,
        session: &mut ActionIntentSession,
        proof: ActionIntentSessionProof,
    ) -> Result<(), EngineError> {
        validate_identifier("session id", &proof.session_id)?;
        validate_identifier("Person uid", &proof.person_uid)?;
        validate_identifier("key id", &proof.key_id)?;
        if proof.session_id != session.session_id {
            return Err(EngineError::Conflict {
                code: "action_intent_session_mismatch",
                message: "key proof names a different session".into(),
            });
        }
        if proof.session_challenge != session.challenge {
            return Err(EngineError::Conflict {
                code: "action_intent_session_mismatch",
                message: "key proof belongs to a different session".into(),
            });
        }
        if proof.person_uid != session.person_uid {
            return Err(EngineError::Conflict {
                code: "action_intent_identity_mismatch",
                message: "key proof does not belong to the authenticated Person".into(),
            });
        }
        if session
            .bound_key_id
            .as_deref()
            .is_some_and(|key_id| key_id != proof.key_id)
        {
            return Err(EngineError::Conflict {
                code: "action_intent_session_key_mismatch",
                message: "session is already bound to a different key".into(),
            });
        }

        let verifying_key = decode_public_key(&proof.public_key_base64)?;
        if B64.encode(verifying_key.as_bytes()) != proof.public_key_base64 {
            return Err(EngineError::Conflict {
                code: "action_intent_key_invalid",
                message: "Action intent key does not use canonical standard base64".into(),
            });
        }
        let signature = decode_signature(&proof.signature)?;
        verifying_key
            .verify(&proof.signing_bytes(), &signature)
            .map_err(|_| EngineError::Conflict {
                code: "action_intent_signature_invalid",
                message: "Action intent session key proof is invalid".into(),
            })?;

        let existing: Option<String> = store::sqlx::query_scalar(
            "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
        )
        .bind(&session.person_uid)
        .bind(&proof.key_id)
        .fetch_optional(&self.store.pool)
        .await?;
        match existing {
            Some(public_key) if public_key != proof.public_key_base64 => {
                return Err(EngineError::Conflict {
                    code: "action_intent_key_id_conflict",
                    message: "published key id already belongs to different key material".into(),
                });
            }
            Some(_) => {}
            None => {
                store::sqlx::query(
                    "INSERT INTO identity_key (actor_uid, key_id, public_key) VALUES (?, ?, ?)",
                )
                .bind(&session.person_uid)
                .bind(&proof.key_id)
                .bind(&proof.public_key_base64)
                .execute(&self.store.pool)
                .await?;
            }
        }
        session.bound_key_id = Some(proof.key_id);
        Ok(())
    }

    /// Verify and consume one signed Action envelope.
    ///
    /// Replay markers are consumed only after successful signature
    /// verification. They stay consumed even if later Action validation fails;
    /// a retry must use the next sequence and a new message id while the Action's own
    /// request id provides semantic idempotency where required.
    pub async fn verify_action_intent(
        &self,
        session: &mut ActionIntentSession,
        intent: SignedActionIntent,
    ) -> Result<VerifiedActionIntent, EngineError> {
        validate_identifier("session id", &intent.session_id)?;
        validate_identifier("message id", &intent.message_id)?;
        if intent.action_base64.is_empty() || intent.action_base64.len() > MAX_ACTION_BASE64_BYTES {
            return Err(invalid_intent("Action payload has an invalid length"));
        }
        if intent.session_challenge.is_empty()
            || intent.session_challenge.len() > MAX_CHALLENGE_BYTES
        {
            return Err(invalid_intent("session challenge has an invalid length"));
        }
        if intent.session_id != session.session_id || intent.session_challenge != session.challenge
        {
            return Err(EngineError::Conflict {
                code: "action_intent_session_mismatch",
                message: "signed Action intent belongs to a different session".into(),
            });
        }
        let bound_key_id = session
            .bound_key_id
            .as_deref()
            .map(str::to_owned)
            .ok_or_else(|| EngineError::Conflict {
                code: "action_intent_session_unauthenticated",
                message: "session must prove possession of a Person key before sending Actions"
                    .into(),
            })?;
        if intent.sequence != session.next_sequence
            || session.used_message_ids.contains(&intent.message_id)
        {
            return Err(EngineError::Conflict {
                code: "action_intent_replay",
                message: "signed Action intent sequence is not next or message id was already used"
                    .into(),
            });
        }

        let public_key: Option<String> = store::sqlx::query_scalar(
            "SELECT public_key FROM identity_key WHERE actor_uid = ? AND key_id = ?",
        )
        .bind(&session.person_uid)
        .bind(&bound_key_id)
        .fetch_optional(&self.store.pool)
        .await?;
        let public_key = public_key.ok_or_else(|| EngineError::Conflict {
            code: "action_intent_key_unknown",
            message: "signed Action intent names no published key for the authenticated Person"
                .into(),
        })?;

        let verifying_key = decode_public_key(&public_key)?;
        let signature = decode_signature(&intent.signature)?;
        let signed_bytes = intent.signing_bytes();
        verifying_key
            .verify(&signed_bytes, &signature)
            .map_err(|_| EngineError::Conflict {
                code: "action_intent_signature_invalid",
                message: "signed Action intent signature is invalid".into(),
            })?;

        let action_bytes = B64
            .decode(&intent.action_base64)
            .map_err(|_| invalid_intent("Action payload is not valid standard base64"))?;
        if action_bytes.len() > MAX_ACTION_BYTES
            || B64.encode(&action_bytes) != intent.action_base64
        {
            return Err(invalid_intent(
                "Action payload is too large or does not use canonical standard base64",
            ));
        }
        let action: Action = serde_json::from_slice(&action_bytes)
            .map_err(|error| invalid_intent(&format!("Action payload is invalid: {error}")))?;

        let stored = store::action_intents::insert_pending(
            &self.store.pool,
            store::action_intents::NewSignedActionIntent {
                session_id: &intent.session_id,
                session_challenge: &intent.session_challenge,
                sequence: intent.sequence,
                message_id: &intent.message_id,
                actor_person_uid: &session.person_uid,
                key_id: &bound_key_id,
                action_base64: &intent.action_base64,
                signature: &intent.signature,
            },
            Utc::now(),
        )
        .await?;

        session.used_message_ids.insert(intent.message_id.clone());
        session.next_sequence =
            session
                .next_sequence
                .checked_add(1)
                .ok_or_else(|| EngineError::Conflict {
                    code: "action_intent_sequence_exhausted",
                    message: "signed Action intent session sequence is exhausted".into(),
                })?;
        Ok(VerifiedActionIntent {
            intent_uid: stored.uid,
            message_id: intent.message_id,
            action,
            authenticated_actor: session.authenticated_actor.clone(),
            person_uid: session.person_uid.clone(),
            key_id: bound_key_id,
            session_id: intent.session_id,
            session_challenge: intent.session_challenge,
            sequence: intent.sequence,
            action_base64: intent.action_base64,
            signature: intent.signature,
        })
    }

    /// Execute a previously verified and persisted intent with its effective
    /// actor taken exclusively from the server-owned authenticated session.
    pub async fn act_verified_intent(
        &self,
        verified: VerifiedActionIntent,
    ) -> Result<ActionOutcome, EngineError> {
        let now = Utc::now();
        let VerifiedActionIntent {
            intent_uid,
            action,
            authenticated_actor,
            person_uid,
            key_id,
            session_id,
            session_challenge,
            sequence,
            message_id,
            action_base64,
            signature,
        } = verified;
        if let Some(outcome) = self
            .queue_remote_transfer_action(
                &action,
                &person_uid,
                &key_id,
                &session_id,
                &session_challenge,
                sequence,
                &message_id,
                &action_base64,
                &signature,
                now,
            )
            .await?
        {
            store::action_intents::mark_committed(&self.store.pool, &intent_uid, &[], now).await?;
            return Ok(outcome);
        }
        let result = self
            .act_at_with_authorship(
                action,
                Some(authenticated_actor),
                now,
                Some(VerifiedActionAuthorship {
                    person_uid: person_uid.clone(),
                    intent_uid: intent_uid.clone(),
                }),
            )
            .await;
        match result {
            Ok(outcome) => {
                let fact_uids = outcome
                    .facts
                    .iter()
                    .filter(|fact| fact.actor_uid.as_deref() == Some(person_uid.as_str()))
                    .map(|fact| fact.uid.clone())
                    .collect::<Vec<_>>();
                store::action_intents::mark_committed(
                    &self.store.pool,
                    &intent_uid,
                    &fact_uids,
                    Utc::now(),
                )
                .await?;
                Ok(outcome)
            }
            Err(error) => {
                let _ = store::action_intents::mark_failed(
                    &self.store.pool,
                    &intent_uid,
                    error.code(),
                    &error.to_string(),
                    Utc::now(),
                )
                .await;
                Err(error)
            }
        }
    }
}

fn validate_identifier(name: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.contains('\r')
        || value.contains('\n')
    {
        return Err(invalid_intent(&format!("{name} has an invalid length")));
    }
    Ok(())
}

fn decode_public_key(value: &str) -> Result<VerifyingKey, EngineError> {
    let bytes = B64.decode(value).map_err(|_| EngineError::Conflict {
        code: "action_intent_key_invalid",
        message: "published Action intent key is invalid".into(),
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| EngineError::Conflict {
        code: "action_intent_key_invalid",
        message: "published Action intent key is invalid".into(),
    })?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| EngineError::Conflict {
        code: "action_intent_key_invalid",
        message: "published Action intent key is invalid".into(),
    })
}

fn decode_signature(value: &str) -> Result<Signature, EngineError> {
    let bytes = B64.decode(value).map_err(|_| EngineError::Conflict {
        code: "action_intent_signature_invalid",
        message: "signed Action intent signature is invalid".into(),
    })?;
    Signature::from_slice(&bytes).map_err(|_| EngineError::Conflict {
        code: "action_intent_signature_invalid",
        message: "signed Action intent signature is invalid".into(),
    })
}

fn invalid_intent(message: &str) -> EngineError {
    EngineError::Conflict {
        code: "action_intent_invalid",
        message: message.into(),
    }
}
