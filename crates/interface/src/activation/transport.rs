//! Local activation request transport for Unix sockets and TCP fallback.

use super::{WakeSlot, ring};
use std::{fs, future::Future, io, path::Path, sync::mpsc::SyncSender, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    task::JoinHandle,
    time::timeout,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const CLIENT_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(unix)]
const SHOW: &[u8; 5] = b"show\n";

async fn send_request<S>(
    connect: impl Future<Output = io::Result<S>>,
    request: &[u8],
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    timeout(REQUEST_TIMEOUT, async {
        let mut stream = connect.await?;
        stream.write_all(request).await?;
        let mut ack = [0; 2];
        stream.read_exact(&mut ack).await?;
        if &ack == b"ok" {
            Ok(())
        } else {
            Err(io::Error::other("Invalid Lince control reply"))
        }
    })
    .await
    .map_err(io::Error::other)?
}

async fn receive_request<const N: usize>(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    expected: &[u8; N],
    sender: &SyncSender<()>,
    wake: &WakeSlot,
) {
    let _ = timeout(CLIENT_TIMEOUT, async {
        let mut request = [0; N];
        stream.read_exact(&mut request).await?;
        if &request == expected {
            let _ = sender.try_send(());
            ring(wake);
            stream.write_all(b"ok").await?;
        }
        Ok::<_, io::Error>(())
    })
    .await;
}

#[cfg(unix)]
pub(super) async fn request_show(path: &Path) -> io::Result<()> {
    send_request(tokio::net::UnixStream::connect(path), SHOW).await
}

#[cfg(unix)]
pub(super) async fn listen(
    path: &Path,
    sender: SyncSender<()>,
    wake: WakeSlot,
) -> io::Result<JoinHandle<()>> {
    use std::os::unix::fs::FileTypeExt;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(path)?,
        Ok(_) => return Err(io::Error::other("Lince control endpoint is not a socket")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    Ok(tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            receive_request(stream, SHOW, &sender, &wake).await;
        }
    }))
}

#[cfg(not(unix))]
pub(super) async fn request_show(path: &Path) -> io::Result<()> {
    let address = fs::read_to_string(path)?;
    let (address, token) = address
        .split_once('\n')
        .ok_or_else(|| io::Error::other("Invalid control endpoint"))?;
    let address: std::net::SocketAddr = address.parse().map_err(io::Error::other)?;
    if !address.ip().is_loopback() {
        return Err(io::Error::other("Control endpoint must be local"));
    }
    send_request(tokio::net::TcpStream::connect(address), token.as_bytes()).await
}

#[cfg(not(unix))]
pub(super) async fn listen(
    path: &Path,
    sender: SyncSender<()>,
    wake: WakeSlot,
) -> io::Result<JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let expected: [u8; 64] = token.as_bytes().try_into().map_err(io::Error::other)?;
    fs::write(path, format!("{}\n{token}", listener.local_addr()?))?;
    Ok(tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            receive_request(stream, &expected, &sender, &wake).await;
        }
    }))
}
