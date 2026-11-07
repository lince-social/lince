use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    os::{fd::{AsFd, BorrowedFd, OwnedFd}, unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt}},
    path::Path,
};
use rustix::{event::{PollFd, PollFlags, Timespec, poll}, process::{pidfd_open, PidfdFlags}};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use crate::{checkpoint::invalid, transport::PeerIdentity};

#[derive(Clone)]
pub struct Token([u8; 32]);

impl std::fmt::Debug for Token {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { formatter.write_str("Token(<private>)") }
}

impl Token {
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0_u8; 32];
        let mut filled = 0;
        while filled < bytes.len() {
            match rustix::rand::getrandom(&mut bytes[filled..], rustix::rand::GetRandomFlags::empty()) {
                Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Entropy source returned no data")),
                Ok(count) => filled += count,
                Err(rustix::io::Errno::INTR) => {},
                Err(error) => return Err(error.into()),
            }
        }
        Ok(Self(bytes))
    }

    pub fn from_hex(value: &str) -> io::Result<Self> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) { return Err(invalid("Invalid refresh capability encoding")); }
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| invalid("Invalid refresh capability encoding"))?;
        }
        Ok(Self(bytes))
    }

    pub fn encoded(&self) -> String { self.0.iter().map(|byte| format!("{byte:02x}")).collect() }
    pub fn authenticates(&self, other: &Token) -> bool { bool::from(self.0.ct_eq(&other.0)) }

    pub fn read(path: &Path) -> io::Result<Self> {
        let mut file = OpenOptions::new().read(true).custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32).open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0 || metadata.nlink() != 1 || metadata.len() > 65 {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Refresh capability file is not private or has an invalid owner"));
        }
        let mut value = String::new();
        file.read_to_string(&mut value)?;
        Self::from_hex(value.strip_suffix('\n').unwrap_or(&value))
    }

    pub fn create(path: &Path) -> io::Result<Self> {
        let token = Self::generate()?;
        let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        file.write_all(token.encoded().as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(token)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_ticks: u64,
    pub uid: u32,
    pub boot_id: String,
}

impl ProcessIdentity {
    pub fn observe(pid: u32) -> io::Result<Self> {
        if pid == 0 || pid > i32::MAX as u32 { return Err(invalid("Invalid refresh session PID")); }
        let path = format!("/proc/{pid}");
        let metadata = fs::metadata(&path)?;
        let stat = fs::read_to_string(format!("{path}/stat"))?;
        let start_ticks = stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(19))
            .ok_or_else(|| invalid("Missing refresh process start identity"))?.parse().map_err(|_| invalid("Invalid refresh process start identity"))?;
        let boot_id = fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim().to_owned();
        if boot_id.len() != 36 || boot_id.bytes().any(|byte| !byte.is_ascii_hexdigit() && byte != b'-') { return Err(invalid("Invalid kernel boot identity")); }
        Ok(Self { pid, start_ticks, uid: metadata.uid(), boot_id })
    }

    pub fn matches_peer(&self, peer: PeerIdentity) -> bool { self.pid == peer.pid && self.uid == peer.uid }
}

pub struct ProcessLease {
    identity: ProcessIdentity,
    descriptor: OwnedFd,
}

impl std::fmt::Debug for ProcessLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { formatter.debug_struct("ProcessLease").field("identity", &self.identity).finish_non_exhaustive() }
}

impl ProcessLease {
    pub fn open(expected: ProcessIdentity) -> io::Result<Self> {
        if ProcessIdentity::observe(expected.pid)? != expected { return Err(invalid("Refresh process identity changed before acquiring its lease")); }
        let pid = rustix::process::Pid::from_raw(expected.pid as i32).ok_or_else(|| invalid("Invalid process identity"))?;
        let descriptor = pidfd_open(pid, PidfdFlags::empty())?;
        let lease = Self { identity: expected, descriptor };
        if !lease.live()? || ProcessIdentity::observe(lease.identity.pid)? != lease.identity { return Err(invalid("Refresh process changed while acquiring its lease")); }
        Ok(lease)
    }

    pub fn identity(&self) -> &ProcessIdentity { &self.identity }
    pub fn descriptor(&self) -> BorrowedFd<'_> { self.descriptor.as_fd() }

    pub fn live(&self) -> io::Result<bool> {
        loop {
            let mut descriptors = [PollFd::new(&self.descriptor, PollFlags::IN)];
            match poll(&mut descriptors, Some(&Timespec { tv_sec: 0, tv_nsec: 0 })) {
                Ok(count) => return Ok(count == 0),
                Err(rustix::io::Errno::INTR) => {},
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub fn authenticate(&self, peer: PeerIdentity) -> io::Result<()> {
        if !self.identity.matches_peer(peer) || !self.live()? || ProcessIdentity::observe(peer.pid)? != self.identity {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "Refresh authority differs from the live session owner"));
        }
        Ok(())
    }
}
