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

/// Relay one browser socket to a contact Cell's live session.
pub(crate) async fn relay(
    wire: std::sync::Arc<engine::wire::Wire>,
    organ: String,
    socket: WebSocket,
) -> Result<(), String> {
    let connection = wire
        .open_live(&organ)
        .await
        .map_err(|error| error.to_string())?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|error| format!("no session stream: {error}"))?;
    let (mut sink, mut stream) = socket.split();

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
                    .map_err(|error| format!("relay write: {error}"))?;
                send.write_all(body)
                    .await
                    .map_err(|error| format!("relay write: {error}"))?;
            }
            // Host Cell -> browser.
            frame = read_frame(&mut recv) => {
                let Some(bytes) = frame? else { break };
                let text = String::from_utf8(bytes)
                    .map_err(|_| "host sent a non-UTF8 frame".to_string())?;
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream) -> Result<Option<Vec<u8>>, String> {
    let mut len = [0u8; 4];
    if recv.read_exact(&mut len).await.is_err() {
        return Ok(None); // the host hung up; that is how a session ends
    }
    let len = u32::from_be_bytes(len);
    if len > MAX_FRAME_BYTES {
        return Err(format!("host frame of {len} bytes is over the cap"));
    }
    let mut buf = vec![0u8; len as usize];
    recv.read_exact(&mut buf)
        .await
        .map_err(|error| format!("short frame: {error}"))?;
    Ok(Some(buf))
}
