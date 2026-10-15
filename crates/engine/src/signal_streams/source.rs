use super::*;
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, protocol::WebSocketConfig};

pub(super) fn validate_url(value: &str) -> Result<(), EngineError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > 2048 || value.chars().any(char::is_control) {
        return Err(invalid("Invalid source URL"));
    }
    let url = value
        .parse::<tokio_tungstenite::tungstenite::http::Uri>()
        .map_err(|_| invalid("Use a WebSocket URL"))?;
    if !matches!(url.scheme_str(), Some("ws" | "wss"))
        || url.host().is_none()
        || url
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(invalid("Use ws or wss without credentials in the URL"));
    }
    Ok(())
}

pub(super) fn start(channel: Arc<Channel>) {
    if channel.config.source_url.is_empty() {
        return;
    }
    {
        let mut inner = channel.inner.lock().unwrap();
        if inner.source_started || inner.closed {
            return;
        }
        inner.source_started = true;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        channel.inner.lock().unwrap().source_started = false;
        return;
    };
    handle.spawn(async move {
        let Ok(mut producer) = channel.publisher() else {
            return;
        };
        loop {
            if channel.is_closed() {
                break;
            }
            if producer.snapshot().consumers == 0 {
                if producer.demand.changed().await.is_err() {
                    break;
                }
                continue;
            }
            let epoch = producer.snapshot().epoch;
            let config = WebSocketConfig::default()
                .max_message_size(Some(MAX_PAYLOAD))
                .max_frame_size(Some(MAX_PAYLOAD));
            let connection = tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(10), tokio_tungstenite::connect_async_with_config(channel.config.source_url.as_str(), Some(config), false)) => result,
                _ = producer.demand.changed() => continue,
            };
            if let Ok(Ok((mut socket, _))) = connection {
                let mut sequence = channel.inner.lock().unwrap().sequence;
                loop {
                    let message = tokio::select! {
                        _ = producer.demand.changed() => {
                            if producer.snapshot().epoch != epoch { break; }
                            continue;
                        }
                        message = tokio::time::timeout(Duration::from_secs(30), socket.next()) => message,
                    };
                    let Ok(Some(Ok(message))) = message else {
                        break;
                    };
                    let bytes = match message {
                        Message::Binary(bytes) => bytes.to_vec(),
                        Message::Text(text) if channel.config.value_type == "scalar" => {
                            text.as_bytes().to_vec()
                        }
                        Message::Ping(bytes) => {
                            if socket.send(Message::Pong(bytes)).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        Message::Close(_) => break,
                        _ => continue,
                    };
                    sequence += 1;
                    let metadata = Metadata {
                        epoch: epoch.clone(),
                        sequence,
                        captured_at: 0,
                        content_type: if channel.config.value_type == "scalar" {
                            "text/decimal"
                        } else {
                            "image/jpeg"
                        }
                        .into(),
                    };
                    if producer.publish(metadata, bytes).await.is_err() {
                        break;
                    }
                }
                let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
            }
            if producer.snapshot().epoch == epoch {
                channel.state.send_modify(|state| {
                    state.status = "Source unavailable; retrying".into();
                    state.value = None;
                });
                producer.demand.borrow_and_update();
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                    _ = producer.demand.changed() => {},
                }
            }
        }
    });
}
