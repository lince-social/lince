use std::collections::{BTreeSet, VecDeque};
use std::io::ErrorKind;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Instant, SystemTime};

use str0m::{
    Candidate,
    net::{Protocol, Transmit},
};
use turn_client_proto::{
    api::{TurnConfig, TurnEvent, TurnPollRet, TurnRecvRet},
    prelude::*,
    stun::{
        agent::StunAgent,
        types::{
            TransportType,
            attribute::XorMappedAddress,
            message::{BINDING, Message, MessageWriteVec},
        },
    },
    types::TurnCredentials,
};

use super::{error, peer::Network, relay};
use crate::{MediaError, Result};

type StunTransmit<T> = turn_client_proto::api::Transmit<T>;

pub struct Datagram {
    pub source: SocketAddr,
    pub destination: SocketAddr,
    pub data: Vec<u8>,
}

struct Socket {
    udp: UdpSocket,
    address: SocketAddr,
    discovery: Vec<(SocketAddr, StunAgent)>,
}

struct Relay {
    socket: relay::Socket,
    client: relay::Client,
    address: Option<SocketAddr>,
    expires: SystemTime,
    permissions: BTreeSet<IpAddr>,
    closed: bool,
}

enum Gathered {
    Stun(SocketAddr),
    Relay(Relay),
}

pub struct Ice {
    sockets: Vec<Socket>,
    relays: Vec<Relay>,
    pub candidates: VecDeque<Candidate>,
    pub received: VecDeque<Datagram>,
    relay_only: bool,
    gathering: Option<mpsc::Receiver<Gathered>>,
    active: Arc<AtomicBool>,
}

struct Endpoint {
    address: SocketAddr,
    tls_host: Option<String>,
    tcp: bool,
}

fn endpoint(url: &str) -> Result<Endpoint> {
    let (scheme, rest) = url
        .split_once(':')
        .ok_or_else(|| MediaError("Invalid ICE server address".into()))?;
    if !["stun", "turn", "turns"].contains(&scheme) || rest.contains('@') || rest.contains('/') {
        return Err(MediaError("Invalid ICE server address".into()));
    }
    let (authority, query) = rest.split_once('?').unwrap_or((rest, ""));
    if !["", "transport=udp", "transport=tcp"].contains(&query)
        || (scheme == "turns" && query == "transport=udp")
        || (scheme == "stun" && query == "transport=tcp")
    {
        return Err(MediaError("Unsupported ICE server transport".into()));
    }
    let default_port = if scheme == "turns" { 5349 } else { 3478 };
    let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
        let (host, suffix) = ipv6
            .split_once(']')
            .ok_or_else(|| MediaError("Invalid IPv6 ICE server".into()))?;
        let port = if suffix.is_empty() {
            default_port
        } else {
            suffix
                .strip_prefix(':')
                .and_then(|p| p.parse::<u16>().ok())
                .ok_or_else(|| MediaError("Invalid ICE port".into()))?
        };
        (host.to_owned(), port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (
            host.to_owned(),
            port.parse::<u16>()
                .map_err(|_| MediaError("Invalid ICE port".into()))?,
        )
    } else {
        (authority.to_owned(), default_port)
    };
    if host.is_empty() || port == 0 {
        return Err(MediaError("Invalid ICE server address".into()));
    }
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|_| MediaError("ICE server name could not be resolved".into()))?
        .next()
        .ok_or_else(|| MediaError("ICE server has no network address".into()))?;
    Ok(Endpoint {
        address,
        tls_host: (scheme == "turns").then_some(host),
        tcp: scheme == "turns" || query == "transport=tcp",
    })
}

impl Socket {
    fn bind(address: SocketAddr) -> Result<Self> {
        let udp = UdpSocket::bind(address).map_err(error)?;
        udp.set_nonblocking(true).map_err(error)?;
        Ok(Self {
            address: udp.local_addr().map_err(error)?,
            udp,
            discovery: Vec::new(),
        })
    }

    fn send(&self, data: &[u8], destination: SocketAddr) -> Result<()> {
        match self.udp.send_to(data, destination) {
            Ok(_) => Ok(()),
            Err(err)
                if matches!(
                    err.kind(),
                    ErrorKind::WouldBlock
                        | ErrorKind::NetworkUnreachable
                        | ErrorKind::HostUnreachable
                        | ErrorKind::PermissionDenied
                ) =>
            {
                Ok(())
            }
            Err(err) => Err(error(err)),
        }
    }
}

impl Ice {
    pub fn new(network: &Network) -> Result<Self> {
        network.configuration()?;
        let mut result = Self {
            sockets: Vec::new(),
            relays: Vec::new(),
            candidates: VecDeque::new(),
            received: VecDeque::new(),
            relay_only: network.relay_only,
            gathering: None,
            active: Arc::new(AtomicBool::new(true)),
        };
        if !network.relay_only {
            let mut ips = BTreeSet::new();
            for interface in if_addrs::get_if_addrs().map_err(error)? {
                let ip = interface.ip();
                if ip.is_unspecified()
                    || ip.is_multicast()
                    || matches!(ip, IpAddr::V6(v) if v.is_unicast_link_local())
                    || !ips.insert(ip)
                {
                    continue;
                }
                if result.sockets.len() >= 16 {
                    break;
                }
                let Ok(socket) = Socket::bind(SocketAddr::new(ip, 0)) else {
                    continue;
                };
                result
                    .candidates
                    .push_back(Candidate::host(socket.address, Protocol::Udp).map_err(error)?);
                result.sockets.push(socket);
            }
        }
        if !network.turn.is_empty() || !network.stun.is_empty() {
            let (sender, receiver) = mpsc::sync_channel(20);
            let network = network.clone();
            let active = result.active.clone();
            std::thread::Builder::new()
                .name("lince-ice-gather".into())
                .spawn(move || {
                    if !network.relay_only {
                        for url in &network.stun {
                            if !active.load(Ordering::Acquire) {
                                return;
                            }
                            if let Ok(endpoint) = endpoint(url) {
                                if sender.send(Gathered::Stun(endpoint.address)).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    for server in &network.turn {
                        for url in &server.urls {
                            if !active.load(Ordering::Acquire) {
                                return;
                            }
                            let Ok(relay) = create_relay(url, server) else {
                                continue;
                            };
                            if sender.send(Gathered::Relay(relay)).is_err() {
                                return;
                            }
                        }
                    }
                })
                .map_err(error)?;
            result.gathering = Some(receiver);
        }
        if result.sockets.is_empty() && result.gathering.is_none() {
            return Err(MediaError("No usable network interface".into()));
        }
        Ok(result)
    }

    pub fn poll(&mut self, now: Instant) -> Result<()> {
        self.gather(now)?;
        let mut buffer = [0u8; 65536];
        for socket in &mut self.sockets {
            for _ in 0..128 {
                let (len, remote) = match socket.udp.recv_from(&mut buffer) {
                    Ok(packet) => packet,
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(err) => return Err(error(err)),
                };
                if let Some((_, agent)) = socket
                    .discovery
                    .iter_mut()
                    .find(|(server, _)| *server == remote)
                {
                    if let Ok(message) = Message::from_bytes(&buffer[..len]) {
                        if agent.handle_stun_message_with_time(&message, remote, protocol_time(now))
                        {
                            if let Ok(mapped) = message.attribute::<XorMappedAddress>() {
                                let address = mapped.addr(message.transaction_id());
                                self.candidates.push_back(
                                    Candidate::server_reflexive(
                                        address,
                                        socket.address,
                                        Protocol::Udp,
                                    )
                                    .map_err(error)?,
                                );
                            }
                        }
                    }
                    continue;
                }
                if self.received.len() < 256 {
                    self.received.push_back(Datagram {
                        source: remote,
                        destination: socket.address,
                        data: buffer[..len].to_vec(),
                    });
                }
            }
            for (_, agent) in &mut socket.discovery {
                let _ = agent.poll(protocol_time(now));
                while let Some(transmit) = agent.poll_transmit(protocol_time(now)) {
                    let _ = socket.udp.send_to(transmit.data, transmit.to);
                }
            }
        }
        for relay in &mut self.relays {
            if relay.expires <= SystemTime::now() {
                relay.closed = true;
            }
            if relay.closed {
                continue;
            }
            if relay.socket.flush().is_err() {
                relay.closed = true;
                continue;
            }
            for _ in 0..128 {
                let (len, source) = match relay.socket.recv(&mut buffer) {
                    Ok(packet) => packet,
                    Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        relay.closed = true;
                        break;
                    }
                };
                let transmit = StunTransmit::new(
                    &buffer[..len],
                    relay.client.transport(),
                    source,
                    relay.socket.address,
                );
                if let TurnRecvRet::PeerData(data) = relay.client.recv(transmit, protocol_time(now))
                {
                    if let Some(destination) = relay.address {
                        if self.received.len() < 256 {
                            self.received.push_back(Datagram {
                                source: data.peer,
                                destination,
                                data: data.data().to_vec(),
                            });
                        }
                    }
                }
            }
            if relay.closed {
                continue;
            }
            if matches!(relay.client.poll(protocol_time(now)), TurnPollRet::Closed) {
                relay.closed = true;
            }
            for _ in 0..128 {
                let Some(data) = relay.client.poll_recv(protocol_time(now)) else {
                    break;
                };
                if let Some(destination) = relay.address {
                    if self.received.len() < 256 {
                        self.received.push_back(Datagram {
                            source: data.peer,
                            destination,
                            data: data.data().to_vec(),
                        });
                    }
                }
            }
            while let Some(event) = relay.client.poll_event() {
                if let TurnEvent::PermissionCreateFailed(_, address) = &event {
                    relay.permissions.remove(address);
                }
                if let TurnEvent::AllocationCreated(TransportType::Udp, address) = event {
                    relay.address = Some(address);
                    self.candidates.push_back(
                        Candidate::relayed(address, relay.socket.address, Protocol::Udp)
                            .map_err(error)?,
                    );
                }
            }
            while let Some(transmit) = relay.client.poll_transmit(protocol_time(now)) {
                if relay
                    .socket
                    .send(transmit.data.as_ref(), transmit.to)
                    .is_err()
                {
                    relay.closed = true;
                    break;
                }
            }
        }
        if self.relay_only
            && self.gathering.is_none()
            && self.relays.iter().all(|relay| relay.closed)
        {
            return Err(MediaError(
                "TURN allocation failed; check the relay address and credentials".into(),
            ));
        }
        Ok(())
    }

    fn gather(&mut self, now: Instant) -> Result<()> {
        for _ in 0..20 {
            let Some(receiver) = &self.gathering else {
                break;
            };
            match receiver.try_recv() {
                Ok(Gathered::Relay(relay)) => self.relays.push(relay),
                Ok(Gathered::Stun(remote)) => {
                    for socket in &mut self.sockets {
                        if socket.address.ip().is_loopback()
                            || socket.address.is_ipv4() != remote.is_ipv4()
                        {
                            continue;
                        }
                        let mut agent = StunAgent::builder(TransportType::Udp, socket.address)
                            .remote_addr(remote)
                            .build();
                        let message = Message::builder_request(BINDING, MessageWriteVec::new());
                        let transmit = agent
                            .send_request(message.as_slice(), remote, protocol_time(now))
                            .map_err(error)?;
                        socket.send(transmit.data.as_ref(), remote)?;
                        socket.discovery.push((remote, agent));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.gathering = None;
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn send(&mut self, transmit: Transmit, now: Instant) -> Result<()> {
        if let Some(relay) = self.relays.iter_mut().find(|relay| {
            relay.address == Some(transmit.source) || relay.socket.address == transmit.source
        }) {
            if relay.closed {
                return Ok(());
            }
            let address = transmit.destination;
            if !relay
                .client
                .have_permission(TransportType::Udp, address.ip())
            {
                if relay.permissions.insert(address.ip()) {
                    relay
                        .client
                        .create_permission(TransportType::Udp, address.ip(), protocol_time(now))
                        .map_err(error)?;
                }
                return Ok(());
            }
            relay.permissions.remove(&address.ip());
            if let Some(packet) = relay
                .client
                .send_to(
                    TransportType::Udp,
                    address,
                    transmit.contents.to_vec(),
                    protocol_time(now),
                )
                .map_err(error)?
            {
                let packet = packet.build();
                relay.socket.send(&packet.data, packet.to)?;
            }
            return Ok(());
        }
        if let Some(socket) = self
            .sockets
            .iter()
            .find(|socket| socket.address == transmit.source)
        {
            socket.send(&transmit.contents, transmit.destination)?;
        }
        Ok(())
    }
}

impl Drop for Ice {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        for relay in &mut self.relays {
            let now = protocol_time(Instant::now());
            let _ = relay.client.delete(now);
            while let Some(transmit) = relay.client.poll_transmit(now) {
                let _ = relay.socket.send(transmit.data.as_ref(), transmit.to);
            }
        }
    }
}

fn protocol_time(now: Instant) -> turn_client_proto::stun::Instant {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    turn_client_proto::stun::Instant::from_nanos(
        now.saturating_duration_since(*EPOCH.get_or_init(|| now))
            .as_nanos()
            .min(i64::MAX as u128) as i64,
    )
}

fn create_relay(url: &str, server: &super::peer::TurnServer) -> Result<Relay> {
    let endpoint = endpoint(url)?;
    let remote = endpoint.address;
    let socket = relay::Socket::open(remote, endpoint.tcp)?;
    let credentials = TurnCredentials::new(&server.username, &server.password);
    let mut config = TurnConfig::new(credentials);
    if remote.is_ipv6() {
        config.set_address_family(turn_client_proto::types::AddressFamily::IPV6);
    }
    let client = socket.allocate(config, endpoint.tls_host)?;
    Ok(Relay {
        socket,
        client,
        address: None,
        expires: server.expires_at,
        permissions: BTreeSet::new(),
        closed: false,
    })
}
