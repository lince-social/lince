use crate::checkpoint::{CheckpointFile, HeldCheckpoint, MAX_DESCRIPTORS, invalid};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    net::{
        self, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags,
        SendAncillaryBuffer, SendAncillaryMessage, SendFlags,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, IoSlice, IoSliceMut},
    mem::MaybeUninit,
    os::fd::{AsFd, BorrowedFd, OwnedFd},
    time::Instant,
};

const PACKET_BYTES: usize = 16384;
const DESCRIPTORS_PER_PACKET: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeerIdentity {
    pub uid: u32,
    pub pid: u32,
}

pub fn peer(socket: impl AsFd) -> io::Result<PeerIdentity> {
    let credentials = net::sockopt::socket_peercred(socket)?;
    Ok(PeerIdentity {
        uid: credentials.uid.as_raw(),
        pid: credentials.pid.as_raw_nonzero().get() as u32,
    })
}

pub fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
    net::socketpair(
        net::AddressFamily::UNIX,
        net::SocketType::SEQPACKET,
        net::SocketFlags::CLOEXEC | net::SocketFlags::NONBLOCK,
        None,
    )
    .map_err(Into::into)
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Frame {
    Begin {
        transaction: String,
        descriptors: usize,
    },
    Descriptors {
        transaction: String,
        start: usize,
        count: usize,
    },
    End {
        transaction: String,
    },
}

fn validate_socket(socket: impl AsFd, expected: PeerIdentity) -> io::Result<()> {
    if net::sockopt::socket_type(socket.as_fd())? != net::SocketType::SEQPACKET
        || peer(socket)? != expected
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Refresh transport endpoint or peer differs from its authority",
        ));
    }
    Ok(())
}

fn ready(socket: impl AsFd, events: PollFlags, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Refresh descriptor transfer timed out",
                )
            })?;
        let timeout = Timespec {
            tv_sec: remaining
                .as_secs()
                .try_into()
                .map_err(|_| invalid("Transfer deadline exceeds clock range"))?,
            tv_nsec: remaining.subsec_nanos().into(),
        };
        let mut descriptors = [PollFd::new(&socket, events)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Refresh descriptor transfer timed out",
                ));
            }
            Ok(_) => return Ok(()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn send_packet(
    socket: impl AsFd,
    frame: &Frame,
    descriptors: &[BorrowedFd<'_>],
    deadline: Instant,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(frame)?;
    if bytes.len() > PACKET_BYTES || descriptors.len() > DESCRIPTORS_PER_PACKET {
        return Err(invalid("Refresh transport packet exceeds limits"));
    }
    loop {
        ready(socket.as_fd(), PollFlags::OUT, deadline)?;
        let mut space =
            [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(DESCRIPTORS_PER_PACKET))];
        let mut ancillary = SendAncillaryBuffer::new(&mut space);
        if !descriptors.is_empty() && !ancillary.push(SendAncillaryMessage::ScmRights(descriptors))
        {
            return Err(invalid("Could not prepare refresh descriptors"));
        }
        match net::sendmsg(
            socket.as_fd(),
            &[IoSlice::new(&bytes)],
            &mut ancillary,
            SendFlags::DONTWAIT | SendFlags::NOSIGNAL,
        ) {
            Ok(written) if written == bytes.len() => return Ok(()),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "Incomplete refresh packet",
                ));
            }
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn receive_packet(socket: impl AsFd, deadline: Instant) -> io::Result<(Frame, Vec<OwnedFd>)> {
    loop {
        ready(socket.as_fd(), PollFlags::IN, deadline)?;
        let mut bytes = [0_u8; PACKET_BYTES];
        let mut space =
            [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(DESCRIPTORS_PER_PACKET))];
        let mut ancillary = RecvAncillaryBuffer::new(&mut space);
        let result = match net::recvmsg(
            socket.as_fd(),
            &mut [IoSliceMut::new(&mut bytes)],
            &mut ancillary,
            RecvFlags::DONTWAIT | RecvFlags::CMSG_CLOEXEC,
        ) {
            Ok(result) => result,
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => continue,
            Err(error) => return Err(error.into()),
        };
        let mut descriptors = Vec::new();
        for message in ancillary.drain() {
            match message {
                RecvAncillaryMessage::ScmRights(rights) => descriptors.extend(rights),
                _ => return Err(invalid("Unexpected ancillary refresh message")),
            }
        }
        if result
            .flags
            .intersects(ReturnFlags::TRUNC | ReturnFlags::CTRUNC)
            || result.bytes > PACKET_BYTES
        {
            return Err(invalid("Truncated refresh transport packet"));
        }
        if result.bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Refresh transport closed",
            ));
        }
        let frame = serde_json::from_slice(&bytes[..result.bytes])?;
        return Ok((frame, descriptors));
    }
}

pub fn send(
    socket: impl AsFd,
    expected_peer: PeerIdentity,
    held: &HeldCheckpoint,
    deadline: Instant,
) -> io::Result<()> {
    validate_socket(socket.as_fd(), expected_peer)?;
    let envelope = held.checkpoint.envelope();
    envelope.validate_descriptors(&held.descriptors)?;
    send_packet(
        socket.as_fd(),
        &Frame::Begin {
            transaction: envelope.transaction.clone(),
            descriptors: held.descriptors.len(),
        },
        &[held.checkpoint.descriptor()],
        deadline,
    )?;
    for (index, chunk) in held.descriptors.chunks(DESCRIPTORS_PER_PACKET).enumerate() {
        let descriptors = chunk.iter().map(AsFd::as_fd).collect::<Vec<_>>();
        send_packet(
            socket.as_fd(),
            &Frame::Descriptors {
                transaction: envelope.transaction.clone(),
                start: index * DESCRIPTORS_PER_PACKET,
                count: chunk.len(),
            },
            &descriptors,
            deadline,
        )?;
    }
    send_packet(
        socket,
        &Frame::End {
            transaction: envelope.transaction.clone(),
        },
        &[],
        deadline,
    )
}

pub fn receive(
    socket: impl AsFd,
    expected_peer: PeerIdentity,
    expected_transaction: &str,
    deadline: Instant,
) -> io::Result<HeldCheckpoint> {
    validate_socket(socket.as_fd(), expected_peer)?;
    let (frame, mut files) = receive_packet(socket.as_fd(), deadline)?;
    let count = match frame {
        Frame::Begin {
            transaction,
            descriptors,
        } if transaction == expected_transaction
            && descriptors <= MAX_DESCRIPTORS
            && files.len() == 1 =>
        {
            descriptors
        }
        _ => return Err(invalid("Unexpected refresh transfer start")),
    };
    let checkpoint = CheckpointFile::open(files.pop().unwrap())?;
    if checkpoint.envelope().transaction != expected_transaction
        || checkpoint.envelope().descriptors.len() != count
    {
        return Err(invalid("Checkpoint differs from refresh transfer"));
    }
    let mut descriptors = Vec::with_capacity(count);
    while descriptors.len() < count {
        let (frame, chunk) = receive_packet(socket.as_fd(), deadline)?;
        match frame {
            Frame::Descriptors {
                transaction,
                start,
                count: packet_count,
            } if transaction == expected_transaction
                && start == descriptors.len()
                && packet_count == chunk.len()
                && packet_count > 0
                && packet_count <= DESCRIPTORS_PER_PACKET
                && packet_count <= count - descriptors.len() =>
            {
                descriptors.extend(chunk);
            }
            _ => {
                return Err(invalid(
                    "Missing, reordered or excessive refresh descriptors",
                ));
            }
        }
    }
    let (frame, extra) = receive_packet(socket, deadline)?;
    match frame {
        Frame::End { transaction } if transaction == expected_transaction && extra.is_empty() => {
            HeldCheckpoint::new(checkpoint, descriptors)
        }
        _ => Err(invalid("Unexpected refresh transfer completion")),
    }
}
