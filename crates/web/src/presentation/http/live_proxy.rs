//! The guest half of live mode (Ontology §11).
//!
//! A browser cannot speak QUIC-to-a-NodeId, so it does not try. It opens an
//! ordinary websocket to its OWN Cell on localhost — no certificate, no
//! hostname, nothing to configure — and this relays those frames to the host
//! Cell over `lince/live/1`.
//!
//! That split is what makes the session survive changing networks. The only
//! leg that crosses a network is the iroh one, and it is authenticated by key
//! rather than address: no hostname to go stale, no certificate bound to one,
//! and QUIC migrates the path under a connection that stays open. The browser
//! leg never leaves the machine.
//!
//! The route itself lives beside its siblings in `lib.rs`, where the local
//! authentication it needs is defined; this holds the relay.

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};

/// Ceiling on one relayed frame, matching the host side.
const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

/// Why a live session could not be opened, in a shape the board can act on.
///
/// The browser used to be told nothing: the socket was accepted, the session
/// failed, and the socket was dropped without a word. A board cannot tell that
/// apart from a network blip, so it reconnected a second later, forever — a
/// connection per second and a status light flickering green/red with no way to
/// find out why. A reason travels now, and `fatal` says whether trying again
/// unprompted could ever help.
pub(crate) struct LiveFailure {
    pub code: &'static str,
    pub message: String,
}

impl LiveFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Retrying on a timer cannot fix these; only the user can.
    fn fatal(&self) -> bool {
        matches!(self.code, "live_login_required" | "live_login_refused")
    }
}

impl std::fmt::Display for LiveFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

/// A username and password held for a remote Organ, in memory only.
///
/// NEVER written to disk. It is kept for the life of the process so that a
/// reconnect — a dropped link, a laptop lid, the host Cell restarting — does
/// not throw the user back to a login box, which is the whole difference
/// between "I am logged into that Lince" and "I have to log in again". Ending
/// the process ends it; so does `DELETE /organ/{uid}/session`.
#[derive(Clone)]
pub(crate) struct RemoteLogin {
    pub username: String,
    pub password: String,
}

/// Relay one browser socket to a contact Cell's live session.
///
/// `login` is presented only if the host asks for one. A host that granted this
/// Organ a device binding says so in its `live_hello` and no credential leaves
/// this machine.
pub(crate) async fn relay(
    wire: std::sync::Arc<engine::wire::Wire>,
    organ: String,
    login: Option<RemoteLogin>,
    socket: WebSocket,
) -> Result<(), LiveFailure> {
    let (mut sink, mut stream) = socket.split();
    let session = match open_session(&wire, &organ, login.as_ref()).await {
        Ok(session) => session,
        Err(failure) => {
            report_unavailable(&mut sink, &failure).await;
            return Err(failure);
        }
    };
    let LiveSession {
        connection,
        mut send,
        mut recv,
        organ: _,
    } = session;
    // The session is live only now. Until this frame lands the board keeps its
    // subscriptions queued and its status light off — a websocket upgrade to
    // our OWN Cell says nothing about whether the far Cell let us in.
    let hello = serde_json::json!({ "type": "live_ready", "organ": organ }).to_string();
    if sink.send(Message::Text(hello.into())).await.is_err() {
        return Ok(()); // the board went away while we were dialling
    }
    let _connection = connection; // held open for the life of the relay

    loop {
        tokio::select! {
            // Browser -> host Cell.
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
                    // Control and binary frames are the browser's business,
                    // not the session's.
                    _ => continue,
                };
                let body = text.as_bytes();
                send.write_all(&(body.len() as u32).to_be_bytes())
                    .await
                    .map_err(|error| LiveFailure::new("live_dropped", format!("relay write: {error}")))?;
                send.write_all(body)
                    .await
                    .map_err(|error| LiveFailure::new("live_dropped", format!("relay write: {error}")))?;
            }
            // Host Cell -> browser.
            frame = read_frame(&mut recv) => {
                let Some(bytes) = frame? else { break };
                let text = String::from_utf8(bytes)
                    .map_err(|_| LiveFailure::new("live_protocol", "host sent a non-UTF8 frame"))?;
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// An open live session: the connection, its stream, and the uid the host
/// named for itself during the login (absent when no login was asked for).
struct LiveSession {
    connection: iroh::endpoint::Connection,
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
    organ: Option<String>,
}

/// Dial a contact's Cell and get past its login, or say why not.
///
/// Shared by the relay and by `verify_login` so that "does this credential
/// work" is answered by the same code that later has to USE it — a check that
/// passes against a different code path is a check that lies.
async fn open_session(
    wire: &std::sync::Arc<engine::wire::Wire>,
    organ: &str,
    login: Option<&RemoteLogin>,
) -> Result<LiveSession, LiveFailure> {
    let connection = wire
        .open_live(organ)
        .await
        .map_err(|error| LiveFailure::new("live_unreachable", error.to_string()))?;
    // The HOST opens the stream and speaks first, so this end accepts.
    let (mut send, mut recv) = connection.accept_bi().await.map_err(|error| {
        LiveFailure::new("live_unreachable", format!("no session stream: {error}"))
    })?;
    let organ = handshake(&mut send, &mut recv, login).await?;
    Ok(LiveSession {
        connection,
        send,
        recv,
        organ,
    })
}

/// Tell the board why its session did not open, and only then hang up.
///
/// The close is explicit and awaited: returning from the handler drops the
/// socket, and a frame written into a socket that is dropped in the same breath
/// is a frame nobody reads — the exact mistake already paid for once on the
/// iroh side of this file.
async fn report_unavailable(
    sink: &mut futures::stream::SplitSink<WebSocket, Message>,
    failure: &LiveFailure,
) {
    let notice = serde_json::json!({
        "type": "live_unavailable",
        "code": failure.code,
        "fatal": failure.fatal(),
        "message": failure.message,
    })
    .to_string();
    let _ = sink.send(Message::Text(notice.into())).await;
    let _ = sink.close().await;
}

/// Prove a credential against a contact's Cell without starting a session.
///
/// Opens a connection, runs the same handshake the relay runs, and hangs up.
/// Verifying by USING the credential is the point: it answers "will this
/// actually get me in" rather than "is this well formed", so a wrong password
/// is reported while the user is still looking at the box they typed it into.
pub(crate) async fn verify_login(
    wire: std::sync::Arc<engine::wire::Wire>,
    organ: &str,
    login: &RemoteLogin,
) -> Result<Option<String>, String> {
    let session = open_session(&wire, organ, Some(login))
        .await
        .map_err(|failure| failure.message)?;
    session.connection.close(0u32.into(), b"login check");
    Ok(session.organ)
}

/// Log into an Organ this Cell has never met, from a pasted public value.
///
/// THE case the whole credential path exists for: a Lince installed a minute
/// ago, holding no keys anyone has seen, pointed at an Organ from anywhere in
/// the world. Pairing cannot serve it — that needs the far side to open its
/// discovery door and then decide to trust this device — and a device-bound
/// grant cannot serve it either, because there is no device yet.
///
/// Returns the Organ uid the host claimed, which is what gives the new host a
/// stable name to bind sands to. The uid is not TRUSTED here: it names a row in
/// our own contact list and nothing more. What was proved is the password.
pub(crate) async fn login_with_invite(
    wire: std::sync::Arc<engine::wire::Wire>,
    invite: &engine::pairing::PairingInvite,
    login: &RemoteLogin,
) -> Result<String, String> {
    let connection = wire
        .open_live_at(invite)
        .await
        .map_err(|error| error.to_string())?;
    let (mut send, mut recv) = connection
        .accept_bi()
        .await
        .map_err(|error| format!("no session stream: {error}"))?;
    let organ = handshake(&mut send, &mut recv, Some(login)).await;
    connection.close(0u32.into(), b"login check");
    match organ.map_err(|failure| failure.message)? {
        Some(organ) => Ok(organ),
        // A host that let us in without asking is one that already granted this
        // device a binding — so it is already a contact and belongs on the
        // other route.
        None => Err("That Lince already knows this device; it is in your list.".to_string()),
    }
}

/// The login exchange, before a single session frame is relayed.
///
/// Kept OUT of the relay loop above on purpose: these frames are between the
/// two Cells and must never reach the browser, which would put a password
/// exchange on a socket that page scripts can observe.
async fn handshake(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    login: Option<&RemoteLogin>,
) -> Result<Option<String>, LiveFailure> {
    let Some(bytes) = read_frame(recv).await? else {
        return Err(LiveFailure::new(
            "live_unreachable",
            "the host closed before saying hello",
        ));
    };
    let hello: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| LiveFailure::new("live_protocol", format!("unreadable hello: {error}")))?;
    if hello.get("type").and_then(serde_json::Value::as_str) != Some("live_hello") {
        return Err(LiveFailure::new(
            "live_protocol",
            "the host did not open with a hello",
        ));
    }
    if !hello
        .get("login_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(None);
    }

    // No credential is held for this host, so there is nothing to try. Saying
    // so — instead of dropping the socket — is what stops the board dialling
    // once a second for the rest of the session over a password it never had.
    let Some(login) = login else {
        return Err(LiveFailure::new(
            "live_login_required",
            "That Lince wants a login. Connect to it and try again.",
        ));
    };
    let request = serde_json::json!({
        "type": "live_login",
        "username": login.username,
        "password": login.password,
    });
    let body = serde_json::to_vec(&request)
        .map_err(|error| LiveFailure::new("live_protocol", error.to_string()))?;
    send.write_all(&(body.len() as u32).to_be_bytes())
        .await
        .map_err(|error| LiveFailure::new("live_dropped", format!("login write: {error}")))?;
    send.write_all(&body)
        .await
        .map_err(|error| LiveFailure::new("live_dropped", format!("login write: {error}")))?;

    let Some(bytes) = read_frame(recv).await? else {
        return Err(LiveFailure::new(
            "live_dropped",
            "the host closed during the login",
        ));
    };
    let reply: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        LiveFailure::new("live_protocol", format!("unreadable login reply: {error}"))
    })?;
    match reply.get("type").and_then(serde_json::Value::as_str) {
        // The host names ITSELF here. A guest logging in from a fresh install
        // has no contact row and so no uid to bind a sand to; this is where it
        // gets one.
        Some("live_login_ok") => Ok(reply
            .get("organ")
            .and_then(serde_json::Value::as_str)
            .filter(|organ| !organ.is_empty())
            .map(str::to_string)),
        _ => Err(LiveFailure::new(
            "live_login_refused",
            reply
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("That Lince refused the login."),
        )),
    }
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Option<Vec<u8>>, LiveFailure> {
    let mut len = [0u8; 4];
    if recv.read_exact(&mut len).await.is_err() {
        return Ok(None); // the host hung up; that is how a session ends
    }
    let len = u32::from_be_bytes(len);
    if len > MAX_FRAME_BYTES {
        return Err(LiveFailure::new(
            "live_protocol",
            format!("host frame of {len} bytes is over the cap"),
        ));
    }
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf)
        .await
        .map_err(|error| LiveFailure::new("live_dropped", format!("short frame: {error}")))?;
    Ok(Some(buf))
}
