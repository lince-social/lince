// non-lean-yet
// Scope: const MARKER, const AAD_TAG, const SALT_LEN, const NONCE_LEN.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 182.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 22.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   44.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   70, 74, 150, 168, 175, 181, 182.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 138, 159.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 11, 22,
//   23, 24, 90, 138, 159.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, KeyInit, Payload},
};
use zeroize::Zeroize;

pub const MARKER: &str = "lince-vault.v1";

const AAD_TAG: &str = "lince-vault.v1 record ";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const MEMORY_KIB: u32 = 19456;
const ITERATIONS: u32 = 2;
const PARALLELISM: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultError {
    NotAVault,
    Unopenable,
}

impl std::fmt::Display for VaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAVault => write!(f, "this description is not a vault"),
            Self::Unopenable => write!(f, "this vault would not open"),
        }
    }
}

impl std::error::Error for VaultError {}

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    getrandom::fill(&mut out).expect("system entropy unavailable");
    out
}

struct DerivedKey([u8; KEY_LEN]);

impl Drop for DerivedKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

fn derive(
    password: &str,
    salt: &[u8],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<DerivedKey, VaultError> {
    if !(8..=65_536).contains(&memory_kib)
        || !(1..=4).contains(&iterations)
        || !(1..=4).contains(&parallelism)
    {
        return Err(VaultError::Unopenable);
    }
    let params = Params::new(memory_kib, iterations, parallelism, Some(KEY_LEN))
        .map_err(|_| VaultError::Unopenable)?;
    let mut key = DerivedKey([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key.0)
        .map_err(|_| VaultError::Unopenable)?;
    Ok(key)
}

fn aad(record_uid: &str) -> Vec<u8> {
    format!("{AAD_TAG}{record_uid}").into_bytes()
}

pub(crate) fn backup_cipher(
    password: &str,
    salt: &[u8; SALT_LEN],
) -> Result<XChaCha20Poly1305, VaultError> {
    let key = derive(password, salt, MEMORY_KIB, ITERATIONS, PARALLELISM)?;
    Ok(XChaCha20Poly1305::new((&key.0).into()))
}

pub fn is_locked(description: &str) -> bool {
    parse(description).is_some()
}

struct Envelope {
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: Vec<u8>,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

fn parse(description: &str) -> Option<Envelope> {
    let line = description.trim();
    let mut fields = line.split(' ');
    if fields.next()? != MARKER {
        return None;
    }
    let mut memory_kib = None;
    let mut iterations = None;
    let mut parallelism = None;
    for pair in fields.next()?.split(',') {
        let (name, value) = pair.split_once('=')?;
        let value = value.parse::<u32>().ok()?;
        match name {
            "m" => memory_kib = Some(value),
            "t" => iterations = Some(value),
            "p" => parallelism = Some(value),
            _ => return None,
        }
    }
    let salt = b64().decode(fields.next()?).ok()?;
    let nonce = b64().decode(fields.next()?).ok()?;
    let ciphertext = b64().decode(fields.next()?).ok()?;
    if fields.next().is_some() || salt.len() != SALT_LEN || nonce.len() != NONCE_LEN {
        return None;
    }
    Some(Envelope {
        memory_kib: memory_kib?,
        iterations: iterations?,
        parallelism: parallelism?,
        salt,
        nonce,
        ciphertext,
    })
}

pub fn lock(record_uid: &str, password: &str, description: &str) -> Result<String, VaultError> {
    let salt = random_bytes::<SALT_LEN>();
    let nonce = random_bytes::<NONCE_LEN>();
    let key = derive(password, &salt, MEMORY_KIB, ITERATIONS, PARALLELISM)?;
    let ciphertext = XChaCha20Poly1305::new((&key.0).into())
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: description.as_bytes(),
                aad: &aad(record_uid),
            },
        )
        .map_err(|_| VaultError::Unopenable)?;
    Ok(format!(
        "{MARKER} m={MEMORY_KIB},t={ITERATIONS},p={PARALLELISM} {} {} {}",
        b64().encode(salt),
        b64().encode(nonce),
        b64().encode(ciphertext)
    ))
}

pub fn unlock(record_uid: &str, password: &str, description: &str) -> Result<String, VaultError> {
    let envelope = parse(description).ok_or(VaultError::NotAVault)?;
    let key = derive(
        password,
        &envelope.salt,
        envelope.memory_kib,
        envelope.iterations,
        envelope.parallelism,
    )
    .map_err(|_| VaultError::Unopenable)?;
    let mut plaintext = XChaCha20Poly1305::new((&key.0).into())
        .decrypt(
            envelope
                .nonce
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::Unopenable)?,
            Payload {
                msg: &envelope.ciphertext,
                aad: &aad(record_uid),
            },
        )
        .map_err(|_| VaultError::Unopenable)?;
    let opened = String::from_utf8(plaintext.clone()).map_err(|_| VaultError::Unopenable);
    plaintext.zeroize();
    opened
}
