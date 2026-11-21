use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use nucleus::{action_intent::signing_bytes, performance::Span};
use serde_json::{Value, json};
use store::{
    Store,
    action_intents::{for_fact, published_keys_for_actor},
    facts,
};

use crate::ProteinError;

pub(in crate::transfers) async fn transfer_fact_proof(
    store: &Store,
    fact_uid: &str,
) -> Result<Value, ProteinError> {
    let _proof = Span::new("protein-transfer-proof-await");
    let Some(fact) = facts::get(&store.pool, fact_uid).await? else {
        return Ok(json!({
            "fact": fact_uid,
            "state": "missing",
            "authoritative": false,
        }));
    };
    let intent = for_fact(&store.pool, fact_uid).await?;
    let keys = match fact.actor_uid.as_deref() {
        Some(actor) => published_keys_for_actor(&store.pool, actor).await?,
        None => Vec::new(),
    };
    let direct_valid = fact
        .signature
        .as_deref()
        .is_some_and(|signature| verify(&keys, fact.hash.as_bytes(), signature, None));
    let intent_valid = intent.as_ref().is_some_and(|intent| {
        fact.actor_uid.as_deref() == Some(intent.actor_person_uid.as_str())
            && intent.status == "committed"
            && verify(
                &keys,
                &signing_bytes(
                    &intent.session_id,
                    &intent.session_challenge,
                    intent.sequence,
                    &intent.message_id,
                    &intent.action_base64,
                ),
                &intent.signature,
                Some(&intent.key_id),
            )
    });
    let mechanism = if direct_valid {
        "direct_fact_signature"
    } else if intent_valid {
        "verified_action_intent"
    } else if fact.signature.is_some() || intent.is_some() {
        "invalid"
    } else if fact.actor_uid.is_none() {
        "unsigned_system"
    } else {
        "unsigned_local"
    };
    Ok(json!({
        "fact": fact.uid,
        "record": fact.record_uid,
        "actor": fact.actor_uid,
        "at": fact.at.to_rfc3339(),
        "hash": fact.hash,
        "previous_hash": fact.prev_hash,
        "mechanism": mechanism,
        "state": mechanism,
        "authoritative": direct_valid || intent_valid,
        "fact_signature": fact.signature.as_ref().map(|signature| json!({
            "present": true,
            "signature": signature,
        })),
        "action_intent": intent.as_ref().map(|intent| json!({
            "uid": intent.uid,
            "actor": intent.actor_person_uid,
            "key_id": intent.key_id,
            "message_id": intent.message_id,
            "sequence": intent.sequence,
            "status": intent.status,
            "signature": intent.signature,
        })),
    }))
}

fn verify(keys: &[(String, String)], bytes: &[u8], signature: &str, key_id: Option<&str>) -> bool {
    let Some(signature) = B64
        .decode(signature)
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
    else {
        return false;
    };
    keys.iter()
        .filter(|(id, _)| key_id.is_none_or(|key_id| id == key_id))
        .any(|(_, public_key)| {
            let Ok(key) = B64.decode(public_key) else {
                return false;
            };
            let Ok(key) = <[u8; 32]>::try_from(key) else {
                return false;
            };
            VerifyingKey::from_bytes(&key).is_ok_and(|key| key.verify(bytes, &signature).is_ok())
        })
}
