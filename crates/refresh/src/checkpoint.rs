use rustix::fs::{MemfdFlags, SealFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Write},
    os::{
        fd::{AsFd, BorrowedFd, OwnedFd},
        unix::fs::FileExt,
    },
};

pub const MAX_STATE_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_HEADER_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DESCRIPTORS: usize = 65536;
const MAGIC: &[u8; 8] = b"LNCRFR01";
const PREFIX_BYTES: usize = 12;

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hexadecimal(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableIdentity {
    pub revision: Option<String>,
    pub sha256: String,
    pub device: u64,
    pub inode: u64,
    pub target: String,
}

impl ExecutableIdentity {
    pub fn validate(&self) -> io::Result<()> {
        if self
            .revision
            .as_ref()
            .is_some_and(|revision| !hexadecimal(revision, 40))
            || !hexadecimal(&self.sha256, 64)
            || self.inode == 0
            || self.target.is_empty()
            || self.target.len() > 256
        {
            return Err(invalid("Invalid executable identity"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointScope {
    ProtocolExperiment,
    FullSession,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptorPurpose {
    WaylandClient,
    WaylandListener,
    QueuedProtocol,
    ShmPool,
    DmaBuf,
    Fence,
    Drm,
    Input,
    Seat,
    Pty,
    Control,
    Timer,
    Backend,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorIdentity {
    pub device: u64,
    pub inode: u64,
    pub mode: u32,
    pub access_mode: u32,
}

impl DescriptorIdentity {
    pub fn observe(descriptor: impl AsFd) -> io::Result<Self> {
        let stat = rustix::fs::fstat(descriptor.as_fd())?;
        Ok(Self {
            device: stat.st_dev as u64,
            inode: stat.st_ino as u64,
            mode: stat.st_mode & 0o170000,
            access_mode: (rustix::fs::fcntl_getfl(descriptor.as_fd())?
                & rustix::fs::OFlags::ACCMODE)
                .bits(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DescriptorSpec {
    pub purpose: DescriptorPurpose,
    pub owner: String,
    pub identity: DescriptorIdentity,
}

impl DescriptorSpec {
    pub fn capture(
        purpose: DescriptorPurpose,
        owner: String,
        descriptor: impl AsFd,
    ) -> io::Result<Self> {
        if owner.is_empty() || owner.len() > 128 || owner.chars().any(char::is_control) {
            return Err(invalid(
                "Descriptor owner must be a bounded logical identifier",
            ));
        }
        Ok(Self {
            purpose,
            owner,
            identity: DescriptorIdentity::observe(descriptor)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub schema: u32,
    pub scope: CheckpointScope,
    pub transaction: String,
    pub source: ExecutableIdentity,
    pub candidate: ExecutableIdentity,
    pub source_generation: u64,
    pub candidate_generation: u64,
    pub descriptors: Vec<DescriptorSpec>,
    pub payload_length: usize,
    pub payload_sha256: String,
}

impl Envelope {
    pub fn validate(&self) -> io::Result<()> {
        self.source.validate()?;
        self.candidate.validate()?;
        if self.schema != 1
            || (self.scope == CheckpointScope::FullSession
                && (self.source.revision.is_none() || self.candidate.revision.is_none()))
            || !hexadecimal(&self.transaction, 64)
            || self.source.target != self.candidate.target
            || self.source_generation.checked_add(1) != Some(self.candidate_generation)
            || self.descriptors.len() > MAX_DESCRIPTORS
            || self.payload_length > MAX_STATE_BYTES
            || !hexadecimal(&self.payload_sha256, 64)
            || self.descriptors.iter().any(|descriptor| {
                descriptor.owner.is_empty()
                    || descriptor.owner.len() > 128
                    || descriptor.owner.chars().any(char::is_control)
            })
        {
            return Err(invalid("Invalid refresh checkpoint envelope"));
        }
        Ok(())
    }

    pub fn validate_descriptors(&self, descriptors: &[OwnedFd]) -> io::Result<()> {
        self.validate()?;
        if descriptors.len() != self.descriptors.len() {
            return Err(invalid(
                "Descriptor table length differs from the checkpoint",
            ));
        }
        for (expected, descriptor) in self.descriptors.iter().zip(descriptors) {
            if DescriptorIdentity::observe(descriptor)? != expected.identity {
                return Err(invalid(
                    "Descriptor identity differs from the checkpoint table",
                ));
            }
        }
        Ok(())
    }
}

pub struct CheckpointFile {
    file: File,
    envelope: Envelope,
    payload_offset: u64,
}

impl std::fmt::Debug for CheckpointFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CheckpointFile")
            .field("transaction", &self.envelope.transaction)
            .field("descriptors", &self.envelope.descriptors.len())
            .field("payload_length", &self.envelope.payload_length)
            .finish_non_exhaustive()
    }
}

fn required_seals() -> SealFlags {
    SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL
}

impl CheckpointFile {
    pub fn create(mut envelope: Envelope, payload: &[u8]) -> io::Result<Self> {
        if payload.len() > MAX_STATE_BYTES {
            return Err(invalid("Refresh state exceeds checkpoint byte limit"));
        }
        envelope.payload_length = payload.len();
        envelope.payload_sha256 = sha256(payload);
        envelope.validate()?;
        let header = serde_json::to_vec(&envelope)?;
        if header.len() > MAX_HEADER_BYTES {
            return Err(invalid("Refresh descriptor metadata exceeds byte limit"));
        }
        let descriptor = rustix::fs::memfd_create(
            "lince-refresh-checkpoint",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )?;
        let mut file = File::from(descriptor);
        file.write_all(MAGIC)?;
        file.write_all(&(header.len() as u32).to_be_bytes())?;
        file.write_all(&header)?;
        file.write_all(payload)?;
        rustix::fs::fcntl_add_seals(&file, required_seals())?;
        Ok(Self {
            file,
            envelope,
            payload_offset: (PREFIX_BYTES + header.len()) as u64,
        })
    }

    pub fn open(descriptor: OwnedFd) -> io::Result<Self> {
        rustix::io::fcntl_setfd(&descriptor, rustix::io::FdFlags::CLOEXEC)?;
        if !rustix::fs::fcntl_get_seals(&descriptor)?.contains(required_seals()) {
            return Err(invalid(
                "Checkpoint descriptor is not sealed against mutation",
            ));
        }
        let file = File::from(descriptor);
        let mut prefix = [0_u8; PREFIX_BYTES];
        file.read_exact_at(&mut prefix, 0)?;
        if &prefix[..8] != MAGIC {
            return Err(invalid("Invalid refresh checkpoint file magic"));
        }
        let header_length = u32::from_be_bytes(prefix[8..12].try_into().unwrap()) as usize;
        if header_length == 0 || header_length > MAX_HEADER_BYTES {
            return Err(invalid("Invalid refresh checkpoint header length"));
        }
        let mut header = vec![0_u8; header_length];
        file.read_exact_at(&mut header, PREFIX_BYTES as u64)?;
        let envelope: Envelope = serde_json::from_slice(&header)
            .map_err(|_| invalid("Checkpoint header does not match its typed schema"))?;
        envelope.validate()?;
        let payload_offset = (PREFIX_BYTES + header_length) as u64;
        if file.metadata()?.len() != payload_offset + envelope.payload_length as u64 {
            return Err(invalid("Truncated checkpoint or unexpected trailing data"));
        }
        let mut digest = Sha256::new();
        let mut remaining = envelope.payload_length;
        let mut offset = payload_offset;
        let mut chunk = [0_u8; 65536];
        while remaining != 0 {
            let length = remaining.min(chunk.len());
            file.read_exact_at(&mut chunk[..length], offset)?;
            digest.update(&chunk[..length]);
            offset += length as u64;
            remaining -= length;
        }
        let observed = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if observed != envelope.payload_sha256 {
            return Err(invalid("Checkpoint payload checksum mismatch"));
        }
        Ok(Self {
            file,
            envelope,
            payload_offset,
        })
    }

    pub fn descriptor(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }

    pub fn payload(&self) -> io::Result<Vec<u8>> {
        let mut payload = vec![0_u8; self.envelope.payload_length];
        self.file.read_exact_at(&mut payload, self.payload_offset)?;
        Ok(payload)
    }
}

pub struct HeldCheckpoint {
    pub checkpoint: CheckpointFile,
    pub descriptors: Vec<OwnedFd>,
}

impl HeldCheckpoint {
    pub fn new(checkpoint: CheckpointFile, descriptors: Vec<OwnedFd>) -> io::Result<Self> {
        checkpoint.envelope().validate_descriptors(&descriptors)?;
        Ok(Self {
            checkpoint,
            descriptors,
        })
    }
}
