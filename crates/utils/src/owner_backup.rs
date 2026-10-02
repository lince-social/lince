use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, Payload},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
};
use zeroize::Zeroizing;

pub const CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_DATABASE_BYTES: u64 = 1024 * 1024 * 1024;
pub const MAX_KEYRING_BYTES: u64 = 64 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_ARCHIVE_BYTES: u64 = MAX_DATABASE_BYTES
    + MAX_KEYRING_BYTES
    + 64
    + MAX_MANIFEST_BYTES as u64
    + 48
    + (MAX_DATABASE_BYTES / CHUNK_BYTES as u64 + 5) * 20;

const MAGIC: &[u8; 16] = b"lince.owner.v1\0\0";

pub struct Passphrase(Zeroizing<String>);

impl Passphrase {
    pub fn new(value: String) -> io::Result<Self> {
        let value = Zeroizing::new(value);
        if value.is_empty() || value.len() > 4096 {
            return Err(invalid(
                "Enter an owner backup passphrase of at most 4096 bytes",
            ));
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Passphrase([REDACTED])")
    }
}

#[derive(Debug)]
pub struct BackupRequest {
    pub destination: std::path::PathBuf,
    pub passphrase: Passphrase,
}

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    Database,
    OwnerRoot,
    AuthorityWallet,
    RecordKeyring,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub kind: FileKind,
    pub bytes: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub organ: String,
    pub cell: String,
    pub created_at: i64,
    pub files: Vec<Entry>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn unopenable() -> io::Error {
    invalid("The owner backup would not open: wrong passphrase or damaged data")
}

impl Manifest {
    pub fn validate(&self) -> io::Result<()> {
        for identity in [&self.organ, &self.cell] {
            if identity.is_empty()
                || identity.len() > 160
                || !identity.is_ascii()
                || identity.chars().any(char::is_control)
            {
                return Err(invalid("Invalid owner backup identity"));
            }
        }
        if self.created_at < 0 || !(2..=4).contains(&self.files.len()) {
            return Err(invalid("Invalid owner backup manifest"));
        }
        let mut kinds = BTreeSet::new();
        for entry in &self.files {
            if !kinds.insert(entry.kind) {
                return Err(invalid("Duplicate owner backup entry"));
            }
            let valid = match entry.kind {
                FileKind::Database => (100..=MAX_DATABASE_BYTES).contains(&entry.bytes),
                FileKind::OwnerRoot | FileKind::AuthorityWallet => entry.bytes == 32,
                FileKind::RecordKeyring => (1..=MAX_KEYRING_BYTES).contains(&entry.bytes),
            };
            if !valid {
                return Err(invalid("Owner backup entry exceeds its size limit"));
            }
        }
        if !kinds.contains(&FileKind::Database) || !kinds.contains(&FileKind::OwnerRoot) {
            return Err(invalid(
                "Owner backup is missing its database or owner root",
            ));
        }
        if serde_json::to_vec(self)?.len() > MAX_MANIFEST_BYTES {
            return Err(invalid("Owner backup manifest exceeds its size limit"));
        }
        Ok(())
    }
}

struct Frames {
    cipher: XChaCha20Poly1305,
    header: [u8; 48],
    sequence: u64,
}

impl Frames {
    fn new(header: [u8; 48], password: &str) -> io::Result<Self> {
        if password.is_empty() || password.len() > 4096 {
            return Err(invalid(
                "Enter an owner backup passphrase of at most 4096 bytes",
            ));
        }
        let salt: &[u8; 16] = header[16..32].try_into().expect("Fixed header salt");
        Ok(Self {
            cipher: crate::vault::backup_cipher(password, salt).map_err(|_| unopenable())?,
            header,
            sequence: 0,
        })
    }

    fn parameters(&mut self, bytes: usize) -> io::Result<([u8; 24], [u8; 60])> {
        let mut nonce = [0; 24];
        nonce[..16].copy_from_slice(&self.header[32..]);
        nonce[16..].copy_from_slice(&self.sequence.to_be_bytes());
        let mut aad = [0; 60];
        aad[..48].copy_from_slice(&self.header);
        aad[48..56].copy_from_slice(&self.sequence.to_be_bytes());
        aad[56..].copy_from_slice(&(bytes as u32).to_le_bytes());
        self.sequence = self.sequence.checked_add(1).ok_or_else(unopenable)?;
        Ok((nonce, aad))
    }

    fn write(&mut self, target: &mut impl Write, plaintext: &[u8]) -> io::Result<()> {
        let size = plaintext.len() + 16;
        let (nonce, aad) = self.parameters(size)?;
        let encrypted = self
            .cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| unopenable())?;
        target.write_all(&(size as u32).to_le_bytes())?;
        target.write_all(&encrypted)
    }

    fn read(
        &mut self,
        source: &mut impl Read,
        limit: usize,
        exact: Option<usize>,
    ) -> io::Result<Zeroizing<Vec<u8>>> {
        let mut size = [0; 4];
        source.read_exact(&mut size)?;
        let size = u32::from_le_bytes(size) as usize;
        if size <= 16 || size > limit + 16 || exact.is_some_and(|exact| size != exact + 16) {
            return Err(unopenable());
        }
        let mut encrypted = vec![0; size];
        source.read_exact(&mut encrypted)?;
        let (nonce, aad) = self.parameters(size)?;
        self.cipher
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &encrypted,
                    aad: &aad,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| unopenable())
    }
}

pub fn seal_archive<R: Read, W: Write>(
    target: &mut W,
    password: &str,
    manifest: &Manifest,
    mut source: impl FnMut(&Entry) -> io::Result<R>,
) -> io::Result<()> {
    manifest.validate()?;
    let mut header = [0; 48];
    header[..16].copy_from_slice(MAGIC);
    getrandom::fill(&mut header[16..]).map_err(io::Error::other)?;
    let mut frames = Frames::new(header, password)?;
    target.write_all(&header)?;
    let plaintext = Zeroizing::new(serde_json::to_vec(manifest)?);
    frames.write(target, &plaintext)?;
    let mut buffer = Zeroizing::new(vec![0; CHUNK_BYTES]);
    for entry in &manifest.files {
        let mut reader = source(entry)?;
        let mut left = entry.bytes;
        let mut digest = Sha256::new();
        while left > 0 {
            let size = left.min(CHUNK_BYTES as u64) as usize;
            reader.read_exact(&mut buffer[..size])?;
            digest.update(&buffer[..size]);
            frames.write(target, &buffer[..size])?;
            left -= size as u64;
        }
        let hash: [u8; 32] = digest.finalize().into();
        if reader.read(&mut [0])? != 0 || hash != entry.sha256 {
            return Err(invalid("Owner backup source changed during encryption"));
        }
    }
    target.flush()
}

pub fn open_archive_into_staging<R: Read, W: Write>(
    source: &mut R,
    password: &str,
    mut stage: impl FnMut(&Entry) -> io::Result<W>,
) -> io::Result<Manifest> {
    let mut header = [0; 48];
    source.read_exact(&mut header)?;
    if &header[..16] != MAGIC {
        return Err(invalid("This file is not a Lince owner backup"));
    }
    let mut frames = Frames::new(header, password)?;
    let plaintext = frames.read(source, MAX_MANIFEST_BYTES, None)?;
    let manifest: Manifest = serde_json::from_slice(&plaintext).map_err(|_| unopenable())?;
    manifest.validate()?;
    for entry in &manifest.files {
        let mut target = stage(entry)?;
        let mut left = entry.bytes;
        let mut digest = Sha256::new();
        while left > 0 {
            let size = left.min(CHUNK_BYTES as u64) as usize;
            let plaintext = frames.read(source, CHUNK_BYTES, Some(size))?;
            digest.update(&plaintext);
            target.write_all(&plaintext)?;
            left -= size as u64;
        }
        let hash: [u8; 32] = digest.finalize().into();
        if hash != entry.sha256 {
            return Err(unopenable());
        }
        target.flush()?;
    }
    if source.read(&mut [0])? != 0 {
        return Err(invalid("Owner backup has unexpected trailing data"));
    }
    Ok(manifest)
}
