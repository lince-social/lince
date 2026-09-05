use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};

const MAX_FRAME_BYTES: u32 = 8 * 1024 * 1024;

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

    fn fatal(&self) -> bool {
        matches!(self.code, "live_login_required" | "live_login_refused")
    }
}

impl std::fmt::Display for LiveFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

#[derive(Clone)]
pub(crate) struct RemoteLogin {
    pub username: String,
    pub password: String,
}

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
    wire.remember_live_connection(&organ, &connection);
    let hello = serde_json::json!({ "type": "live_ready", "organ": organ }).to_string();
    if sink.send(Message::Text(hello.into())).await.is_err() {
        return Ok(());
    }
    let _connection = connection;

    loop {
        tokio::select! {
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                let text = match message {
                    Message::Text(text) => text,
                    Message::Close(_) => break,
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

struct LiveSession {
    connection: iroh::endpoint::Connection,
    send: iroh::endpoint::SendStream,
    recv: iroh::endpoint::RecvStream,
    organ: Option<String>,
}

async fn open_session(
    wire: &std::sync::Arc<engine::wire::Wire>,
    organ: &str,
    login: Option<&RemoteLogin>,
) -> Result<LiveSession, LiveFailure> {
    let connection = wire
        .open_live(organ)
        .await
        .map_err(|error| LiveFailure::new("live_unreachable", error.to_string()))?;
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
        None => Err("That Lince already knows this device; it is in your list.".to_string()),
    }
}

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
        return Ok(None);
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
