use super::*;
use chacha20poly1305::{
    ChaCha20Poly1305,
    aead::{Aead, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use vodozemac::olm::{Account, AccountPickle, Session, SessionPickle};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountState {
    pub account: String,
    pub signing_secret: String,
    pub pickup_secret: String,
    pub route: ReplyRoute,
    pub expires_at: i64,
    pub pickup_counter: i64,
}

impl AccountState {
    pub fn account(&self, key: &[u8; 32]) -> Result<Account, EngineError> {
        let pickle = AccountPickle::from_encrypted(&self.account, key)
            .map_err(|_| invalid("This device's private reply keys cannot be recovered. Retained conversation history remains separate"))?;
        Ok(Account::from_pickle(pickle))
    }

    pub fn signing_key(&self) -> Result<Signer, EngineError> {
        secret_signer(&self.signing_secret)
    }

    pub fn pickup_key(&self) -> Result<Signer, EngineError> {
        secret_signer(&self.pickup_secret)
    }
}

pub fn secret_signer(secret: &str) -> Result<Signer, EngineError> {
    let bytes: [u8; 32] = B64
        .decode(secret)
        .map_err(|_| invalid("Invalid private reply authority"))?
        .try_into()
        .map_err(|_| invalid("Invalid private reply authority"))?;
    Ok(Signer::from_bytes("", "social-reply", bytes))
}

pub fn new_account(
    key: &[u8; 32],
    services: Vec<String>,
    expires_at: i64,
) -> Result<AccountState, EngineError> {
    let mut account = Account::new();
    account.generate_fallback_key();
    let prekey = account
        .fallback_key()
        .into_values()
        .next()
        .ok_or_else(|| invalid("No offline reply key was generated"))?;
    account.mark_keys_as_published();
    let signer = new_social_signer("")?;
    let pickup = new_social_signer("")?;
    let mut route = ReplyRoute {
        mailbox: nucleus::new_uid("mail"),
        pickup_key: pickup.public_key_b64(),
        signing_key: signer.public_key_b64(),
        identity_key: account.curve25519_key().to_base64(),
        prekey: prekey.to_base64(),
        services,
    };
    route.mailbox = super::request_auth::mailbox_id(&route)?;
    Ok(AccountState {
        account: account.pickle().encrypt(key),
        signing_secret: B64.encode(signer.secret_bytes()),
        pickup_secret: B64.encode(pickup.secret_bytes()),
        route,
        expires_at,
        pickup_counter: 0,
    })
}

pub fn open_session(pickle: &str, key: &[u8; 32]) -> Result<Session, EngineError> {
    Ok(Session::from_pickle(SessionPickle::from_encrypted(pickle, key)
        .map_err(|_| invalid("The device's live conversation keys are unavailable. Establish a fresh session; do not restore an earlier ratchet"))?))
}

pub fn seal_local(
    scope: &str,
    value: &impl Serialize,
    key: &[u8; 32],
) -> Result<String, EngineError> {
    let plaintext = zeroize::Zeroizing::new(serde_json::to_vec(value)?);
    if plaintext.len() > 512 * 1024 {
        return Err(invalid("The private session state exceeds its bound"));
    }
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|_| invalid("Secure randomness is unavailable"))?;
    let aad = format!("lince/social/local-session/1\n{scope}");
    let ciphertext = ChaCha20Poly1305::new(key.into())
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: &plaintext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| invalid("Cannot save private session state"))?;
    let mut bytes = nonce.to_vec();
    bytes.extend(ciphertext);
    Ok(B64.encode(bytes))
}

pub fn open_local<T: serde::de::DeserializeOwned>(
    scope: &str,
    body: &str,
    key: &[u8; 32],
) -> Result<T, EngineError> {
    if body.len() > 700 * 1024 {
        return Err(invalid("The private session state is oversized"));
    }
    let bytes = B64
        .decode(body)
        .map_err(|_| invalid("Invalid encrypted session state"))?;
    if bytes.len() < 28 {
        return Err(invalid("Incomplete encrypted session state"));
    }
    let nonce: [u8; 12] = bytes[..12].try_into().unwrap();
    let aad = format!("lince/social/local-session/1\n{scope}");
    let plaintext = zeroize::Zeroizing::new(
        ChaCha20Poly1305::new(key.into())
            .decrypt(
                (&nonce).into(),
                Payload {
                    msg: &bytes[12..],
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| {
                invalid("Private session state changed or belongs to another device/route")
            })?,
    );
    Ok(serde_json::from_slice(&plaintext)?)
}

pub(super) fn storage_key(path: &std::path::Path) -> Result<[u8; 32], EngineError> {
    let read = || -> Result<[u8; 32], EngineError> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|_| invalid("Cannot inspect the device session key"))?;
        if !metadata.is_file() {
            return Err(invalid(
                "The device session key must be a regular private file",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(invalid(
                    "The device session key must be accessible only to its owner",
                ));
            }
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| invalid("Cannot read the device session key"))?
            .take(33)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid("Cannot read the device session key"))?;
        bytes.try_into().map_err(|_| {
            invalid(
                "Invalid device session key. Keep retained history and provision fresh sessions",
            )
        })
    };
    if path.exists() {
        return read();
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Configure the device's private data directory"))?;
    std::fs::create_dir_all(parent)
        .map_err(|_| invalid("Cannot create the device data directory"))?;
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|_| invalid("Secure randomness is unavailable"))?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key)
                .and_then(|()| file.sync_all())
                .map_err(|_| invalid("Cannot durably save the device session key"))?;
            std::fs::File::open(parent)
                .and_then(|file| file.sync_all())
                .map_err(|_| invalid("Cannot durably save the device session key directory"))?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read(),
        Err(_) => Err(invalid("Cannot create the private device session key")),
    }
}

impl Engine {
    pub async fn social_reset_private_sessions(&self) -> Result<Value, EngineError> {
        self.social_require_local_write().await?;
        self.social_storage_key().await?;
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_device_state WHERE kind IN ('account','session')")
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("UPDATE social_private_outbox SET state='held',error='Fresh device keys and sessions are required after session reset; retained plaintext Messages remain available' WHERE state IN ('pending','stored')")
            .execute(&mut *tx).await?;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='Live session keys were reset' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE state='held')")
            .execute(&mut *tx).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"status":"Live private session keys were reset. Retained history and the separate owner authority wallet remain. Fresh keys wait for owner authorization; pending plaintext messages need fresh sessions before retry","sessions":"reset"}),
        )
    }

    pub async fn social_authority_storage_key(&self) -> Result<[u8; 32], EngineError> {
        let path = self.sealing_keyring_path.lock().expect("device key path")
            .as_ref().map(|path| path.with_file_name("social-authority-wallet-v1.key"))
            .ok_or_else(|| invalid("Configure the owner's private key directory before authorizing private replies"))?;
        tokio::task::spawn_blocking(move || storage_key(&path))
            .await
            .map_err(|_| invalid("Owner authority wallet loading stopped"))?
    }

    pub async fn social_storage_key(&self) -> Result<[u8; 32], EngineError> {
        let path = self.sealing_keyring_path.lock().expect("device key path")
            .as_ref().map(|path| path.with_file_name("social-session-storage-v1.key"))
            .ok_or_else(|| invalid("Configure this device's private key directory before enabling private social replies"))?;
        tokio::task::spawn_blocking(move || storage_key(&path))
            .await
            .map_err(|_| invalid("Device session key loading stopped"))?
    }
}
