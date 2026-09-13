use crate::{ClientMessage, ServerMessage};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use tokio::sync::mpsc;

struct SessionConnection(Connection);

impl Drop for SessionConnection {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"Interface view closed");
    }
}

async fn read(recv: &mut RecvStream) -> Result<ServerMessage, String> {
    let mut size = [0; 4];
    recv.read_exact(&mut size)
        .await
        .map_err(|_| "Live connection closed")?;
    let size = u32::from_be_bytes(size) as usize;
    if size > 8 * 1024 * 1024 {
        return Err("Live response exceeds the size limit".into());
    }
    let mut bytes = vec![0; size];
    recv.read_exact(&mut bytes)
        .await
        .map_err(|_| "Incomplete live response")?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

async fn write(send: &mut SendStream, message: &ClientMessage) -> Result<(), String> {
    let bytes = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Live request exceeds the size limit".into());
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        send.write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|error| error.to_string())?;
        send.write_all(&bytes)
            .await
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Live request timed out".to_string())?
}

pub async fn drive(
    connection: Connection,
    mut outgoing: mpsc::Receiver<ClientMessage>,
    incoming: mpsc::Sender<ServerMessage>,
    wake: impl Fn() + Send + Sync,
) -> Result<(), String> {
    let connection = SessionConnection(connection);
    drive_session(&connection.0, &mut outgoing, &incoming, &wake).await
}

async fn drive_session(
    connection: &Connection,
    outgoing: &mut mpsc::Receiver<ClientMessage>,
    incoming: &mpsc::Sender<ServerMessage>,
    wake: &(impl Fn() + Send + Sync),
) -> Result<(), String> {
    let (mut send, mut recv) =
        tokio::time::timeout(std::time::Duration::from_secs(30), connection.accept_bi())
            .await
            .map_err(|_| "Live connection timed out")?
            .map_err(|error| error.to_string())?;
    let handshake = async {
        loop {
            match read(&mut recv).await? {
                ServerMessage::LiveHello {
                    login_required: true,
                } => {
                    incoming
                        .send(ServerMessage::LiveHello {
                            login_required: true,
                        })
                        .await
                        .map_err(|_| "View closed")?;
                    wake();
                    let Some(message @ ClientMessage::LiveLogin { .. }) = outgoing.recv().await
                    else {
                        return Err("Login required".to_string());
                    };
                    write(&mut send, &message).await?;
                }
                ServerMessage::LiveLoginError { message } => return Err(message),
                ServerMessage::SessionChallenge {
                    session_id,
                    challenge,
                    person: Some(person),
                    ..
                } => {
                    let signer = engine::trust::Signer::generate(
                        &person,
                        &nucleus::new_uid("interface-session"),
                    );
                    let mut proof = nucleus::action_intent::ActionIntentSessionProof {
                        session_id: session_id.clone(),
                        session_challenge: challenge.clone(),
                        person_uid: person,
                        key_id: signer.key_id.clone(),
                        public_key_base64: signer.public_key_b64(),
                        signature: String::new(),
                    };
                    proof.signature = signer.sign_bytes(&proof.signing_bytes());
                    write(
                        &mut send,
                        &ClientMessage::SessionAuthenticate {
                            id: "interface-auth".into(),
                            session_id: proof.session_id,
                            session_challenge: proof.session_challenge,
                            person_uid: proof.person_uid,
                            key_id: proof.key_id,
                            public_key_base64: proof.public_key_base64,
                            signature: proof.signature,
                        },
                    )
                    .await?;
                    match read(&mut recv).await? {
                        ServerMessage::SessionAuthenticated { .. } => {
                            return Ok((signer, session_id, challenge));
                        }
                        ServerMessage::Error { message, .. } => return Err(message),
                        _ => return Err("Live signing authentication failed".into()),
                    }
                }
                _ => {}
            }
        }
    };
    let (signer, session_id, challenge) =
        tokio::time::timeout(std::time::Duration::from_secs(30), handshake)
            .await
            .map_err(|_| "Live login timed out")??;
    incoming
        .send(ServerMessage::SessionAuthenticated {
            id: "interface-auth".into(),
            session_id: session_id.clone(),
            person: signer.actor_uid.clone(),
            key_id: signer.key_id.clone(),
        })
        .await
        .map_err(|_| "View closed")?;
    wake();
    let mut sequence = 0;
    let mut response = Box::pin(read(&mut recv));
    loop {
        tokio::select! {
            message = outgoing.recv() => {
                let Some(message) = message else { return Ok(()) };
                let message = match message {
                    ClientMessage::Act { id, action } => {
                        sequence += 1;
                        let action_base64 = STANDARD.encode(serde_json::to_vec(&action).map_err(|error| error.to_string())?);
                        let signature = signer.sign_bytes(&nucleus::action_intent::signing_bytes(&session_id, &challenge, sequence, &id, &action_base64));
                        ClientMessage::SignedAct { id, session_id: session_id.clone(), session_challenge: challenge.clone(), sequence, action_base64, signature }
                    }
                    message => message,
                };
                write(&mut send, &message).await?;
            }
            message = &mut response => {
                let message = message?;
                drop(response);
                response = Box::pin(read(&mut recv));
                incoming.send(message).await.map_err(|_| "View closed")?;
                wake();
            }
        }
    }
}
