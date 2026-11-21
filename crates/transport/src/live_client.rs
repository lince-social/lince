//! Authentication and framed I/O for a remote Interface view.

use crate::{ClientMessage, ServerMessage};
use iroh::endpoint::{Connection, RecvStream, SendStream};
use serde_json::{from_slice, to_vec};
use std::{pin::pin, time::Duration};
use tokio::{sync::mpsc, time::timeout};

mod actions;

const FRAME_LIMIT: usize = 8 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

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
    let size = usize::try_from(u32::from_be_bytes(size))
        .ok()
        .filter(|&size| size <= FRAME_LIMIT)
        .ok_or("Live response exceeds the size limit")?;
    let mut bytes = vec![0; size];
    recv.read_exact(&mut bytes)
        .await
        .map_err(|_| "Incomplete live response")?;
    from_slice(&bytes).map_err(|error| error.to_string())
}

async fn read_next(mut recv: RecvStream) -> Result<(RecvStream, ServerMessage), String> {
    let message = read(&mut recv).await?;
    Ok((recv, message))
}

async fn write(send: &mut SendStream, message: &ClientMessage) -> Result<(), String> {
    let bytes = to_vec(message).map_err(|error| error.to_string())?;
    let size = u32::try_from(bytes.len())
        .ok()
        .filter(|_| bytes.len() <= FRAME_LIMIT)
        .ok_or("Live request exceeds the size limit")?;
    timeout(WRITE_TIMEOUT, async {
        send.write_all(&size.to_be_bytes())
            .await
            .map_err(|error| error.to_string())?;
        send.write_all(&bytes)
            .await
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "Live request timed out".to_string())?
}

/// Drives one remote view connection from login through frame delivery.
///
/// # Errors
/// Returns a diagnostic for a failed login, handshake, frame, or response delivery.
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
    let (mut send, mut recv) = timeout(HANDSHAKE_TIMEOUT, connection.accept_bi())
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
    let (signer, session_id, challenge) = timeout(HANDSHAKE_TIMEOUT, handshake)
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
    let mut actions = actions::Actions::new(signer, session_id, challenge);
    // Keep partial reads alive while sending requests and reuse the future's storage.
    let mut response = pin!(read_next(recv));
    loop {
        tokio::select! {
            message = outgoing.recv() => {
                let Some(message) = message else { return Ok(()) };
                let message = match message {
                    ClientMessage::Act { id, action } => match actions.prepare(id, action) {
                        Ok(message) => message,
                        Err(error) => {
                            incoming.send(error).await.map_err(|_| "View closed")?;
                            wake();
                            continue;
                        }
                    },
                    message => message,
                };
                write(&mut send, &message).await?;
            }
            message = &mut response => {
                let (recv, mut message) = message?;
                response.set(read_next(recv));
                actions.restore(&mut message);
                incoming.send(message).await.map_err(|_| "View closed")?;
                wake();
            }
        }
    }
}
