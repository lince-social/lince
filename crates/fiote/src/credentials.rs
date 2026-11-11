use crate::config::{Secret, Settings};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct Store {
    directory: PathBuf,
}

impl Store {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn directory(&self) -> Result<(), String> {
        if !self.directory.exists() {
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder
                .create(&self.directory)
                .map_err(|_| "Cannot create local credential storage")?;
        }
        let metadata = fs::symlink_metadata(&self.directory)
            .map_err(|_| "Cannot inspect local credential storage")?;
        if !metadata.is_dir() {
            return Err("Local credential storage must be a directory, not a link".into());
        }
        private(&metadata)?;
        Ok(())
    }

    fn path(&self, slot: &str, suffix: &str) -> PathBuf {
        let name: String = Sha256::digest(slot.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        self.directory.join(format!("{name}.{suffix}"))
    }

    fn read(&self, slot: &str) -> Result<Option<Secret>, String> {
        let file = match open_private(&self.path(slot, "json"), false) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Cannot read the saved provider credential".into()),
        };
        let metadata = file
            .metadata()
            .map_err(|_| "Cannot inspect the saved provider credential")?;
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err("Invalid saved provider credential".into());
        }
        private(&metadata)?;
        let mut bytes = Zeroizing::new(Vec::new());
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read the saved provider credential")?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Saved provider credential exceeds its size limit".into());
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "Saved provider credential is damaged. Reconnect this provider.".into())
    }

    pub async fn key(&self, slot: &str) -> Result<Option<Secret>, String> {
        let store = self.clone();
        let slot = slot.to_owned();
        tokio::task::spawn_blocking(move || {
            if !store.directory.exists() {
                return Ok(None);
            }
            store.directory()?;
            store.read(&slot)
        })
        .await
        .map_err(|_| "Credential reader stopped".to_string())?
    }

    pub async fn put(&self, slot: String, secret: Secret) -> Result<(), String> {
        self.write(slot, secret, None, false).await.map(|_| ())
    }

    pub async fn refreshed(
        &self,
        slot: String,
        secret: Secret,
        previous: Secret,
    ) -> Result<(), String> {
        if secret.0 == previous.0 {
            return Ok(());
        }
        self.write(slot, secret, Some(previous), false)
            .await
            .map(|_| ())
    }

    async fn write(
        &self,
        slot: String,
        secret: Secret,
        previous: Option<Secret>,
        missing_only: bool,
    ) -> Result<bool, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            store.directory()?;
            let lock = store.lock(&slot, "write.lock")?;
            lock.lock()
                .map_err(|_| "Cannot lock local credential storage")?;
            if let Some(previous) = previous {
                if store.read(&slot)?.is_none_or(|saved| saved.0 != previous.0) {
                    return Ok(false);
                }
            }
            if missing_only && store.read(&slot)?.is_some() {
                return Ok(false);
            }
            let bytes = Zeroizing::new(
                serde_json::to_vec(&secret).map_err(|_| "Cannot encode provider credential")?,
            );
            if bytes.len() as u64 > MAX_BYTES {
                return Err("Provider credential exceeds its size limit".into());
            }
            let mut temporary = tempfile::NamedTempFile::new_in(&store.directory)
                .map_err(|_| "Cannot save provider credential")?;
            temporary
                .write_all(&bytes)
                .and_then(|()| temporary.as_file().sync_all())
                .map_err(|_| "Cannot save provider credential")?;
            temporary
                .persist(store.path(&slot, "json"))
                .map_err(|_| "Cannot replace provider credential")?;
            #[cfg(unix)]
            File::open(&store.directory)
                .and_then(|file| file.sync_all())
                .map_err(|_| "Cannot sync provider credential")?;
            Ok(true)
        })
        .await
        .map_err(|_| "Credential writer stopped".to_string())?
    }

    pub async fn import_if_missing(&self, slot: String, secret: Secret) -> Result<bool, String> {
        self.write(slot, secret, None, true).await
    }

    pub async fn remove(&self, slot: &str) -> Result<(), String> {
        let store = self.clone();
        let slot = slot.to_owned();
        tokio::task::spawn_blocking(move || {
            if !store.directory.exists() {
                return Ok(());
            }
            store.directory()?;
            let lock = store.lock(&slot, "write.lock")?;
            lock.lock()
                .map_err(|_| "Cannot lock local credential storage")?;
            match fs::remove_file(store.path(&slot, "json")) {
                Ok(()) => {
                    #[cfg(unix)]
                    File::open(&store.directory)
                        .and_then(|file| file.sync_all())
                        .map_err(|_| "Cannot sync credential removal")?;
                    Ok(())
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err("Cannot remove saved provider credential".into()),
            }
        })
        .await
        .map_err(|_| "Credential removal worker stopped".to_string())?
    }

    fn lock(&self, slot: &str, suffix: &str) -> Result<File, String> {
        let file = open_private(&self.path(slot, suffix), true)
            .map_err(|_| "Cannot open account credential lock")?;
        let metadata = file
            .metadata()
            .map_err(|_| "Cannot inspect account credential lock")?;
        if !metadata.is_file() {
            return Err("Account credential lock must be a regular file".into());
        }
        private(&metadata)?;
        Ok(file)
    }

    pub async fn refresh_guard(&self, slot: &str) -> Result<File, String> {
        let store = self.clone();
        let slot = slot.to_owned();
        tokio::task::spawn_blocking(move || {
            store.directory()?;
            let file = store.lock(&slot, "refresh.lock")?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                match file.try_lock() {
                    Ok(()) => return Ok(file),
                    Err(std::fs::TryLockError::WouldBlock)
                        if std::time::Instant::now() < deadline =>
                    {
                        std::thread::sleep(std::time::Duration::from_millis(20))
                    }
                    Err(_) => {
                        return Err(
                            "Another process is refreshing this account. Retry shortly.".into()
                        );
                    }
                }
            }
        })
        .await
        .map_err(|_| "Account credential lock worker stopped".to_string())?
    }
}

fn open_private(path: &Path, create: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(create).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    options.open(path)
}

fn private(metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 || metadata.uid() != rustix::process::geteuid().as_raw() {
            return Err(
                "Credential storage must be owned by your user and inaccessible to other users"
                    .into(),
            );
        }
    }
    #[cfg(not(unix))]
    let _ = metadata;
    Ok(())
}

pub fn slot(settings: &Settings) -> String {
    format!(
        "{}:{}:{}:{}",
        settings.provider.0, settings.auth_method, settings.endpoint, settings.account
    )
}

#[cfg(test)]
mod tests;
