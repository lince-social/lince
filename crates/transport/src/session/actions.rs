//! Local Action dispatch and signed Action intent sessions.

use engine::{
    action_intent::ActionIntentSession,
    actions::{Action, ActionOutcome},
};
use nucleus::{
    MessageState,
    action_intent::{ActionIntentSessionProof, SignedActionIntent},
    new_uid,
};

use super::{Session, action_error, engine_error, session_denied};
use crate::{ClientMessage, ServerMessage};

const INTENT_UNAVAILABLE: &str = "action_intent_session_unavailable";

#[derive(Default)]
pub(super) enum ActionIntents {
    #[default]
    Pending,
    Unavailable,
    Failed {
        message: String,
        code: String,
    },
    Ready(ActionIntentSession),
}

impl ActionIntents {
    pub(super) fn ready(&self) -> Option<&ActionIntentSession> {
        match self {
            Self::Ready(session) => Some(session),
            _ => None,
        }
    }

    fn ready_mut(&mut self) -> Option<&mut ActionIntentSession> {
        match self {
            Self::Ready(session) => Some(session),
            _ => None,
        }
    }

    fn unavailable(&self, id: String) -> ServerMessage {
        let (message, code) = match self {
            Self::Failed { message, code } => (message.as_str(), code.as_str()),
            _ => (
                "signed Action intent session was not initialized",
                INTENT_UNAVAILABLE,
            ),
        };
        ServerMessage::Error {
            id,
            message: message.into(),
            code: Some(code.into()),
        }
    }
}

impl Session {
    /// Starts the connection's one Action signing session and returns its challenge.
    ///
    /// Trusted local connections receive a challenge with signing disabled.
    pub async fn initialize_action_intent(&mut self) -> ServerMessage {
        if matches!(self.action_intent, ActionIntents::Pending) {
            // Record the attempt before awaiting so even a cancelled attempt gets one challenge path.
            self.action_intent = ActionIntents::Unavailable;
            if let Some(subject) = self.subject.as_deref() {
                self.action_intent = match self.engine.begin_action_intent_session(subject).await {
                    Ok(session) => ActionIntents::Ready(session),
                    Err(error) => {
                        let (message, code) = engine_error(&error);
                        ActionIntents::Failed {
                            message,
                            code: code.unwrap_or_else(|| INTENT_UNAVAILABLE.into()),
                        }
                    }
                };
            }
        }

        let (session_id, challenge, person) = self.action_intent.ready().map_or_else(
            || {
                (
                    new_uid("unavailable-action-intent"),
                    new_uid("unavailable-action-challenge"),
                    None,
                )
            },
            |session| {
                (
                    session.session_id().into(),
                    session.challenge().into(),
                    Some(session.person_uid().into()),
                )
            },
        );
        ServerMessage::SessionChallenge {
            session_id,
            challenge,
            algorithm: "ed25519".into(),
            person,
            signing_required: self.subject.is_some(),
        }
    }

    pub(super) async fn handle_action(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::Act { id, action } => {
                if self.subject.is_some() {
                    vec![ServerMessage::Error {
                        id,
                        message: "authenticated WebSocket Actions require a signed intent".into(),
                        code: Some("action_intent_required".into()),
                    }]
                } else {
                    vec![self.act_local(id, action).await]
                }
            }
            ClientMessage::SignedAct {
                id,
                session_id,
                session_challenge,
                sequence,
                action_base64,
                signature,
            } => {
                vec![
                    self.signed_act(SignedActionIntent {
                        session_id,
                        session_challenge,
                        sequence,
                        message_id: id,
                        action_base64,
                        signature,
                    })
                    .await,
                ]
            }
            _ => vec![session_denied("Action required")],
        }
    }

    async fn act_local(&self, id: String, action: Action) -> ServerMessage {
        if self.local_sync
            && let Some(service) = &self.fiote
            && let Action::CreateMessage {
                content,
                thread,
                body,
                author: None,
                state: MessageState::Finished,
                parent: None,
                references,
            } = &action
            && references.is_empty()
        {
            match service.send_content(thread, body, content).await {
                Ok(Some(outcome)) => return action_ok(id, outcome),
                Ok(None) => {}
                Err(message) => {
                    return ServerMessage::Error {
                        id,
                        message,
                        code: Some("fiote".into()),
                    };
                }
            }
        }
        match self.engine.act(action, None).await {
            Ok(outcome) => action_ok(id, outcome),
            Err(error) => action_error(id, error),
        }
    }

    pub(super) async fn authenticate_action_intent(
        &mut self,
        id: String,
        proof: ActionIntentSessionProof,
    ) -> ServerMessage {
        if self.subject.is_none() {
            return ServerMessage::Error {
                id,
                message: "trusted local sessions do not register remote Action keys".into(),
                code: Some("action_intent_not_required".into()),
            };
        }
        let Some(session) = self.action_intent.ready_mut() else {
            return self.action_intent.unavailable(id);
        };
        let key_id = proof.key_id.clone();
        match self
            .engine
            .authenticate_action_intent_session(session, proof)
            .await
        {
            Ok(()) => ServerMessage::SessionAuthenticated {
                id,
                session_id: session.session_id().to_string(),
                person: session.person_uid().to_string(),
                key_id,
            },
            Err(error) => action_error(id, error),
        }
    }

    async fn signed_act(&mut self, intent: SignedActionIntent) -> ServerMessage {
        let id = intent.message_id.clone();
        if self.subject.is_none() {
            return ServerMessage::Error {
                id,
                message: "signed Action intents require an authenticated app user".into(),
                code: Some("action_intent_subject_required".into()),
            };
        }
        let Some(session) = self.action_intent.ready_mut() else {
            return self.action_intent.unavailable(id);
        };
        let verified = match self
            .engine
            .access_scope(true, self.engine.verify_action_intent(session, intent))
            .await
        {
            Ok(verified) => verified,
            Err(error) => return action_error(id, error),
        };
        match self.engine.act_verified_intent(verified).await {
            Ok(outcome) => action_ok(id, outcome),
            Err(error) => action_error(id, error),
        }
    }
}

fn action_ok(id: String, outcome: ActionOutcome) -> ServerMessage {
    ServerMessage::ActionOk {
        id,
        created: outcome.created,
        facts: outcome.facts.len(),
        warnings: outcome.warnings,
        data: outcome.data,
    }
}
