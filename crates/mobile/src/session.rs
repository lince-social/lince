use base64::{Engine as _, engine::general_purpose::STANDARD};
use cell::{ClientMessage, ServerMessage};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Identity {
    #[default]
    Owner,
    Locked,
    Person(String),
}

pub struct Signing {
    signer: engine::trust::Signer,
    id: String,
    challenge: String,
    sequence: u64,
    pub login: engine::login::LoginSession,
}

impl Signing {
    pub async fn open(
        runtime: &cell::CellRuntime,
        username: &str,
        password: engine::private_password::PasswordInput,
    ) -> Result<(cell::Session, Self), String> {
        let login = runtime
            .engine
            .login_password(username, password, None)
            .await
            .map_err(|error| error.to_string())?;
        let mut session = cell::Session::authenticated(
            runtime.engine.clone(),
            runtime.lanes.clone(),
            nucleus::new_uid("mobile-person"),
            login.clone(),
        );
        let ServerMessage::SessionChallenge {
            session_id,
            challenge,
            person: Some(person),
            ..
        } = session.initialize_action_intent().await
        else {
            return Err("This Person cannot open an authenticated session".into());
        };
        let signer = engine::trust::Signer::generate(&person, &nucleus::new_uid("mobile-signing"));
        let mut proof = nucleus::action_intent::ActionIntentSessionProof {
            session_id: session_id.clone(),
            session_challenge: challenge.clone(),
            person_uid: person,
            key_id: signer.key_id.clone(),
            public_key_base64: signer.public_key_b64(),
            signature: String::new(),
        };
        proof.signature = signer.sign_bytes(&proof.signing_bytes());
        let response = session
            .handle(ClientMessage::SessionAuthenticate {
                id: "mobile-auth".into(),
                session_id: proof.session_id,
                session_challenge: proof.session_challenge,
                person_uid: proof.person_uid,
                key_id: proof.key_id,
                public_key_base64: proof.public_key_base64,
                signature: proof.signature,
            })
            .await;
        if !response
            .iter()
            .any(|message| matches!(message, ServerMessage::SessionAuthenticated { .. }))
        {
            return Err("Could not authenticate this Person's changes".into());
        }
        Ok((
            session,
            Self {
                signer,
                id: session_id,
                challenge,
                sequence: 0,
                login,
            },
        ))
    }

    pub fn sign(&mut self, request: ClientMessage) -> Result<ClientMessage, String> {
        let ClientMessage::Act { id, action } = request else {
            return Ok(request);
        };
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Sign in again to continue")?;
        let action_base64 =
            STANDARD.encode(serde_json::to_vec(&action).map_err(|error| error.to_string())?);
        let signature = self
            .signer
            .sign_bytes(&nucleus::action_intent::signing_bytes(
                &self.id,
                &self.challenge,
                self.sequence,
                &id,
                &action_base64,
            ));
        Ok(ClientMessage::SignedAct {
            id,
            session_id: self.id.clone(),
            session_challenge: self.challenge.clone(),
            sequence: self.sequence,
            action_base64,
            signature,
        })
    }
}

impl Drop for Signing {
    fn drop(&mut self) {
        self.login.revoke();
    }
}
