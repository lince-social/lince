use crate::{ClientMessage, ServerMessage};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use engine::{actions::Action, trust::Signer};
use std::collections::BTreeMap;

const MAX_PENDING: usize = 128;

pub(super) struct Actions {
    signer: Signer,
    session_id: String,
    challenge: String,
    sequence: u64,
    replies: BTreeMap<String, String>,
}

impl Actions {
    pub fn new(signer: Signer, session_id: String, challenge: String) -> Self {
        Self {
            signer,
            session_id,
            challenge,
            sequence: 0,
            replies: BTreeMap::new(),
        }
    }

    pub fn prepare(
        &mut self,
        id: String,
        mut action: Action,
    ) -> Result<ClientMessage, ServerMessage> {
        let fail = |message: &str, code: &str| ServerMessage::Error {
            id: id.clone(),
            message: message.into(),
            code: Some(code.into()),
        };
        if id.trim().is_empty() || id.len() > 512 || id.contains(['\r', '\n']) {
            return Err(fail(
                "Action reply IDs must contain 1–512 bytes without line breaks.",
                "live_action_id_invalid",
            ));
        }
        if self.replies.len() >= MAX_PENDING {
            return Err(fail(
                "The live connection has too many pending Actions; wait for a reply and retry.",
                "live_action_busy",
            ));
        }
        engine::transfer_receipts::sign_person_consent(&mut action, &self.signer)
            .and_then(|()| {
                engine::trust_analysis::prepare_contact_action(&mut action, &self.signer)
            })
            .map_err(|error| ServerMessage::Error {
                id: id.clone(),
                message: error.to_string(),
                code: error.code().map(str::to_owned),
            })?;
        let sequence = self.sequence.checked_add(1).ok_or_else(|| {
            fail(
                "The live Action sequence is exhausted; reconnect before sending more Actions.",
                "live_action_sequence_exhausted",
            )
        })?;
        let action_base64 = STANDARD.encode(serde_json::to_vec(&action).map_err(|error| {
            fail(
                &format!("The Action could not be encoded: {error}"),
                "live_action_encoding",
            )
        })?);
        let wire_id = nucleus::new_uid("interface-action");
        let signature = self
            .signer
            .sign_bytes(&nucleus::action_intent::signing_bytes(
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

    pub fn restore(&mut self, message: &mut ServerMessage) {
        match message {
            ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => {
                if let Some(original) = self.replies.remove(id) {
                    *id = original;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
