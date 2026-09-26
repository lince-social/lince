use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use turn_client_proto::{api::TurnConfig, tcp::TurnClientTcp, udp::TurnClientUdp};
use turn_client_rustls::{TurnClientRustls, rustls};

use super::error;
use crate::{MediaError, Result as MediaResult};

turn_client_proto::impl_client!(pub Client, (Udp, TurnClientUdp), (Tcp, TurnClientTcp), (Tls, TurnClientRustls));

enum Stream {
    Udp(UdpSocket),
    Tcp(TcpStream),
}

pub struct Socket {
    pub address: SocketAddr,
    remote: SocketAddr,
    stream: Stream,
    pending: VecDeque<u8>,
}

impl Socket {
    pub fn open(remote: SocketAddr, tcp: bool) -> MediaResult<Self> {
        let (stream, address) = if tcp {
            let socket =
                TcpStream::connect_timeout(&remote, Duration::from_secs(2)).map_err(error)?;
            socket.set_nonblocking(true).map_err(error)?;
            socket.set_nodelay(true).map_err(error)?;
            let address = socket.local_addr().map_err(error)?;
            (Stream::Tcp(socket), address)
        } else {
            let socket = UdpSocket::bind(if remote.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            })
            .map_err(error)?;
            socket.connect(remote).map_err(error)?;
            socket.set_nonblocking(true).map_err(error)?;
            let address = socket.local_addr().map_err(error)?;
            (Stream::Udp(socket), address)
        };
        Ok(Self {
            address,
            remote,
            stream,
            pending: VecDeque::new(),
        })
    }

    pub fn allocate(&self, config: TurnConfig, tls_host: Option<String>) -> MediaResult<Client> {
        Ok(match (&self.stream, tls_host) {
            (Stream::Udp(_), None) => {
                Client::Udp(TurnClientUdp::allocate(self.address, self.remote, config))
            }
            (Stream::Tcp(_), None) => {
                Client::Tcp(TurnClientTcp::allocate(self.address, self.remote, config))
            }
            (Stream::Tcp(_), Some(host)) => {
                let provider = rustls_rustcrypto::provider();
                let roots = rustls::RootCertStore {
                    roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
                };
                let tls = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
                    .with_safe_default_protocol_versions()
                    .map_err(error)?
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                let host = rustls::pki_types::ServerName::try_from(host)
                    .map_err(|_| MediaError("Invalid TURN TLS host name".into()))?;
                Client::Tls(TurnClientRustls::allocate(
                    self.address,
                    self.remote,
                    config,
                    host,
                    Arc::new(tls),
                ))
            }
            _ => return Err(MediaError("TURN TLS requires TCP".into())),
        })
    }

    pub fn recv(&mut self, buffer: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        let count = match &mut self.stream {
            Stream::Udp(socket) => socket.recv(buffer)?,
            Stream::Tcp(socket) => {
                let count = socket.read(buffer)?;
                if count == 0 {
                    return Err(std::io::Error::from(ErrorKind::UnexpectedEof));
                }
                count
            }
        };
        Ok((count, self.remote))
    }

    pub fn send(&mut self, data: &[u8], _: SocketAddr) -> MediaResult<()> {
        match &self.stream {
            Stream::Udp(socket) => match socket.send(data) {
                Ok(_) => Ok(()),
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::WouldBlock | ErrorKind::ConnectionRefused
                    ) =>
                {
                    Ok(())
                }
                Err(err) => Err(error(err)),
            },
            Stream::Tcp(_) => {
                if self.pending.len() + data.len() > 262_144 {
                    return Err(MediaError(
                        "TURN connection cannot keep up; rejoin the call".into(),
                    ));
                }
                self.pending.extend(data);
                self.flush()
            }
        }
    }

    pub fn flush(&mut self) -> MediaResult<()> {
        let Stream::Tcp(socket) = &mut self.stream else {
            return Ok(());
        };
        while !self.pending.is_empty() {
            let count = match socket.write(self.pending.as_slices().0) {
                Ok(0) => return Err(MediaError("TURN connection closed".into())),
                Ok(count) => count,
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(error(err)),
            };
            self.pending.drain(..count);
        }
        Ok(())
    }
}
