use std::{net::SocketAddr, path::PathBuf, time::Duration};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use web::{HttpServeMode, serve_cell_api_only};

async fn boot_cell() -> SocketAddr {
    let data_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("live-relay-reason-cell");
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).expect("create the test data dir");
    utils::config::set_lince_data_dir_override(data_dir).expect("set the data dir override");
    unsafe { std::env::set_var("LINCE_DISCOVERY_INTERNET", "0") };

    let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = serve_cell_api_only(
            Some("127.0.0.1:0".to_string()),
            "test-jwt-secret-that-is-long-enough-to-be-accepted-by-the-bootstrap".to_string(),
            false,
            None,
            Some(addr_tx),
            HttpServeMode::FullUi,
        )
        .await;
        if let Err(error) = result {
            eprintln!("Cell stopped: {error}");
        }
    });

    tokio::time::timeout(Duration::from_secs(120), addr_rx)
        .await
        .expect("the Cell should bind within 120s")
        .expect("the Cell should report its bound address")
}

async fn read_text_frame(stream: &mut tokio::net::TcpStream) -> String {
    let mut header = [0u8; 2];
    stream
        .read_exact(&mut header)
        .await
        .expect("the relay closed without sending anything at all");
    assert_eq!(header[0] & 0x0f, 0x1, "expected a text frame");
    let short = (header[1] & 0x7f) as usize;
    let len = if short == 126 {
        let mut extended = [0u8; 2];
        stream
            .read_exact(&mut extended)
            .await
            .expect("frame length");
        u16::from_be_bytes(extended) as usize
    } else {
        short
    };
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .await
        .expect("frame payload");
    String::from_utf8(payload).expect("a text frame is utf8")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_that_cannot_be_opened_says_why_before_hanging_up() {
    let addr = boot_cell().await;

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect to the Cell");
    let request = format!(
        "GET /live/organ-nobody-has-ever-met/connect HTTP/1.1\r\n\
         Host: {addr}\r\n\
         Connection: Upgrade\r\n\
         Upgrade: websocket\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send the upgrade request");

    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        let read = stream.read(&mut byte).await.expect("read response head");
        assert_ne!(read, 0, "the Cell closed before completing the upgrade");
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).to_string();
    assert!(
        head.starts_with("HTTP/1.1 101"),
        "the relay socket should upgrade: {head}",
    );

    let frame = tokio::time::timeout(Duration::from_secs(30), read_text_frame(&mut stream))
        .await
        .expect("the relay must not hang up in silence");
    let message: serde_json::Value = serde_json::from_str(&frame).expect("a JSON frame");

    assert_eq!(
        message["type"], "live_unavailable",
        "the board is told the session did not open: {frame}",
    );
    assert_eq!(
        message["fatal"], false,
        "unreachable is not refused, and must not stop the board retrying: {frame}",
    );
    assert!(
        message["message"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "and the reason is not blank: {frame}",
    );
}
