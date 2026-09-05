use serde::{Deserialize, Serialize};

const ACTION_DOMAIN: &str = "lince.action-intent.v1";
const SESSION_DOMAIN: &str = "lince.action-intent-session.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionIntentSessionProof {
    pub session_id: String,
    pub session_challenge: String,
    pub person_uid: String,
    pub key_id: String,
    pub public_key_base64: String,
    pub signature: String,
}

impl ActionIntentSessionProof {
    pub fn signing_bytes(&self) -> Vec<u8> {
        session_authentication_bytes(
            &self.session_id,
            &self.session_challenge,
            &self.person_uid,
            &self.key_id,
            &self.public_key_base64,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedActionIntent {
    pub session_id: String,
    pub session_challenge: String,
    pub sequence: u64,
    pub message_id: String,
    pub action_base64: String,
    pub signature: String,
}

impl SignedActionIntent {
    pub fn signing_bytes(&self) -> Vec<u8> {
        signing_bytes(
            &self.session_id,
            &self.session_challenge,
            self.sequence,
            &self.message_id,
            &self.action_base64,
        )
    }
}

pub fn signing_bytes(
    session_id: &str,
    session_challenge: &str,
    sequence: u64,
    message_id: &str,
    action_base64: &str,
) -> Vec<u8> {
    format!(
        "{ACTION_DOMAIN}\n{session_id}\n{session_challenge}\n{sequence}\n{message_id}\n{action_base64}"
    )
    .into_bytes()
}

pub fn session_authentication_bytes(
    session_id: &str,
    session_challenge: &str,
    person_uid: &str,
    key_id: &str,
    public_key_base64: &str,
) -> Vec<u8> {
    format!(
        "{SESSION_DOMAIN}\n{session_id}\n{session_challenge}\n{person_uid}\n{key_id}\n{public_key_base64}"
    )
    .into_bytes()
}
