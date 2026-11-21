//! Signs outgoing Actions and restores the caller's reply IDs.

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{
    actions::Action, transfer_receipts::sign_person_consent, trust::Signer,
    trust_analysis::prepare_contact_action,
};
use nucleus::{action_intent::signing_bytes, new_uid};
use serde_json::to_vec;

use crate::{ClientMessage, ServerMessage};

const MAX_PENDING: usize = 128;
const MAX_REPLY_ID_BYTES: usize = 512;

pub(super) struct Actions {
    signer: Signer,
    session_id: String,
    challenge: String,
    sequence: u64,
    replies: BTreeMap<String, String>,
}

impl Actions {
    pub(super) fn new(signer: Signer, session_id: String, challenge: String) -> Self {
        Self {
            signer,
            session_id,
            challenge,
            sequence: 0,
            replies: BTreeMap::new(),
        }
    }

    pub(super) fn prepare(
        &mut self,
        id: String,
        mut action: Action,
    ) -> Result<ClientMessage, ServerMessage> {
        if id.trim().is_empty() || id.len() > MAX_REPLY_ID_BYTES || id.contains(['\r', '\n']) {
            return Err(refusal(
                id,
                "Action reply IDs must contain 1–512 bytes without line breaks.",
                "live_action_id_invalid",
            ));
        }
        if self.replies.len() >= MAX_PENDING {
            return Err(refusal(
                id,
                "The live connection has too many pending Actions; wait for a reply and retry.",
                "live_action_busy",
            ));
        }
        if let Err(error) = sign_person_consent(&mut action, &self.signer)
            .and_then(|()| prepare_contact_action(&mut action, &self.signer))
        {
            return Err(ServerMessage::Error {
                id,
                message: error.to_string(),
                code: error.code().map(str::to_owned),
            });
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            return Err(refusal(
                id,
                "The live Action sequence is exhausted; reconnect before sending more Actions.",
                "live_action_sequence_exhausted",
            ));
        };
        let bytes = match to_vec(&action) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Err(refusal(
                    id,
                    format!("The Action could not be encoded: {error}"),
                    "live_action_encoding",
                ));
            }
        };
        let action_base64 = STANDARD.encode(bytes);
        let wire_id = new_uid("interface-action");
        let signature = self.signer.sign_bytes(&signing_bytes(
            &self.session_id,
            &self.challenge,
            sequence,
            &wire_id,
            &action_base64,
        ));
        self.replies.insert(wire_id.clone(), id);
        self.sequence = sequence;
        Ok(ClientMessage::SignedAct {
            id: wire_id,
            session_id: self.session_id.clone(),
            session_challenge: self.challenge.clone(),
            sequence,
            action_base64,
            signature,
        })
    }

    pub(super) fn restore(&mut self, message: &mut ServerMessage) {
        if let ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } = message
            && let Some(original) = self.replies.remove(id)
        {
            *id = original;
        }
    }
}

fn refusal(id: String, message: impl Into<String>, code: &str) -> ServerMessage {
    ServerMessage::Error {
        id,
        message: message.into(),
        code: Some(code.into()),
    }
}

#[cfg(test)]
mod tests;
