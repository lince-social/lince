use engine::trust::Signer;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};
use store::sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
pub use utils::owner_backup::{BackupRequest, Passphrase};
use utils::owner_backup::{Entry, FileKind, Manifest};
use zeroize::Zeroizing;

#[cfg(test)]
mod tests;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn plain_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, Component::ParentDir) {
            return Err(invalid(
                "Choose a backup path without parent-directory traversal",
            ));
        }
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(invalid(
                "Owner backup paths cannot pass through symbolic links",
            ));
        }
    }
    fs::canonicalize(absolute)
}

fn regular_file(path: &Path, limit: u64) -> io::Result<File> {
    let before = fs::symlink_metadata(path)?;
    if !before.is_file() || before.len() > limit {
        return Err(invalid(
            "Owner backup material must be a bounded regular file",
        ));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    let after = fs::symlink_metadata(path)?;
    if !after.is_file() || opened.len() != before.len() || after.len() != before.len() {
        return Err(invalid("Owner backup material changed while opening"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != opened.dev()
            || before.ino() != opened.ino()
            || after.dev() != opened.dev()
            || after.ino() != opened.ino()
        {
            return Err(invalid("Owner backup material changed while opening"));
        }
    }
    Ok(file)
}

fn key_bytes(path: &Path, bytes: u64) -> io::Result<Zeroizing<Vec<u8>>> {
    let mut content = Zeroizing::new(Vec::new());
    regular_file(path, bytes)?
        .take(bytes + 1)
        .read_to_end(&mut content)?;
    if content.len() as u64 != bytes {
        return Err(invalid(
            "The backup owner key must contain exactly 32 bytes",
        ));
    }
    Ok(content)
}

fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn entry(directory: &Path, kind: FileKind) -> io::Result<Entry> {
    let mut file = regular_file(&directory.join(filename(kind)), limit(kind))?;
    let bytes = file.metadata()?.len();
    let mut buffer = Zeroizing::new(vec![0; utils::owner_backup::CHUNK_BYTES]);
    let mut hash = Sha256::new();
    let mut read = 0;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > limit(kind) {
            return Err(invalid("Owner backup input exceeds its limit"));
        }
        hash.update(&buffer[..count]);
    }
    if read != bytes {
        return Err(invalid("Owner backup input changed while hashing"));
    }
    Ok(Entry {
        kind,
        bytes,
        sha256: hash.finalize().into(),
    })
}

fn filename(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Database => "lince.db",
        FileKind::OwnerRoot => "root-ed25519-v1.key",
        FileKind::AuthorityWallet => "social-authority-wallet-v1.key",
        FileKind::RecordKeyring => "cell-x25519-keyring-v1.json",
    }
}

fn limit(kind: FileKind) -> u64 {
    match kind {
        FileKind::Database => utils::owner_backup::MAX_DATABASE_BYTES,
        FileKind::OwnerRoot | FileKind::AuthorityWallet => 32,
        FileKind::RecordKeyring => utils::owner_backup::MAX_KEYRING_BYTES,
    }
}

async fn open_database(path: &Path, readonly: bool) -> io::Result<store::Store> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(readonly)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(io::Error::other)?;
    Ok(store::Store { pool })
}

async fn prepare_snapshot(
    source: &Path,
    stage: &Path,
    keys: &Path,
) -> io::Result<(String, String)> {
    let source_store = open_database(source, true).await?;
    let result = source_store.snapshot_into(&stage.join("lince.db")).await;
    source_store.pool.close().await;
    result.map_err(io::Error::other)?;
    let store = open_database(&stage.join("lince.db"), false).await?;
    let result = async {
        let integrity: String = store::sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&store.pool).await.map_err(io::Error::other)?;
        if integrity != "ok" { return Err(invalid("The owner database failed integrity validation")); }
        let foreign_errors = store::sqlx::query("PRAGMA foreign_key_check")
            .fetch_optional(&store.pool).await.map_err(io::Error::other)?;
        if foreign_errors.is_some() { return Err(invalid("The owner database has invalid references")); }
        let organ = store::organs::local(&store.pool).await.map_err(io::Error::other)?
            .ok_or_else(|| invalid("The owner database has no local Organ"))?;
        let cell = store::cells::local(&store.pool).await.map_err(io::Error::other)?
            .ok_or_else(|| invalid("The owner database has no local Cell"))?;
        let root_bytes = key_bytes(&keys.join(filename(FileKind::OwnerRoot)), 32)?;
        let secret = Zeroizing::new(root_bytes.as_slice().try_into().map_err(|_| invalid("Invalid owner root"))?);
        let root = Signer::from_bytes(&organ.uid, engine::roster::ROOT_KEY_ID, *secret);
        private_write(&stage.join(filename(FileKind::OwnerRoot)), &root_bytes)?;
        let wallet_path = keys.join(filename(FileKind::AuthorityWallet));
        let wallet_bytes = match fs::symlink_metadata(&wallet_path) {
            Ok(_) => Some(key_bytes(&wallet_path, 32)?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let wallet_secret: Option<Zeroizing<[u8; 32]>> = wallet_bytes.as_ref().map(|bytes| {
            bytes.as_slice().try_into().map(Zeroizing::new).map_err(|_| invalid("Invalid authority wallet key"))
        }).transpose()?;
        let engine = engine::Engine::new(store.clone()).await.map_err(io::Error::other)?;
        engine.social_validate_backup_wallets(&root, wallet_secret.as_ref().map(|key| &**key))
            .await.map_err(io::Error::other)?;
        if let Some(bytes) = wallet_bytes {
            private_write(&stage.join(filename(FileKind::AuthorityWallet)), &bytes)?;
        }
        let keyring_path = keys.join(filename(FileKind::RecordKeyring));
        match fs::symlink_metadata(&keyring_path) {
            Ok(_) => {
                let mut bytes = Zeroizing::new(Vec::new());
                regular_file(&keyring_path, limit(FileKind::RecordKeyring))?
                    .take(limit(FileKind::RecordKeyring) + 1).read_to_end(&mut bytes)?;
                engine::seal::validate_backup_keyring(&bytes, &cell.uid).map_err(io::Error::other)?;
                private_write(&stage.join(filename(FileKind::RecordKeyring)), &bytes)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let mut tx = store.pool.begin().await.map_err(io::Error::other)?;
        store::sqlx::query("DELETE FROM social_device_state WHERE kind IN ('account','session')")
            .execute(&mut *tx).await.map_err(io::Error::other)?;
        store::sqlx::query("UPDATE social_private_outbox SET state='held',error='Owner backup excludes live sessions; establish fresh authority before resuming' WHERE state IN ('pending','stored')")
            .execute(&mut *tx).await.map_err(io::Error::other)?;
        store::sqlx::query("UPDATE social_private_destination SET state='cancelled',error='Owner backup excludes live sessions' WHERE envelope IN (SELECT id FROM social_private_outbox WHERE state='held')")
            .execute(&mut *tx).await.map_err(io::Error::other)?;
        tx.commit().await.map_err(io::Error::other)?;
        store::sqlx::query("VACUUM").execute(&store.pool).await.map_err(io::Error::other)?;
        store::sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&store.pool).await.map_err(io::Error::other)?;
        Ok((organ.uid, cell.uid))
    }.await;
    store.pool.close().await;
    result
}

pub fn capture_stopped(data_directory: &Path, request: BackupRequest) -> io::Result<Manifest> {
    let directory = plain_path(data_directory)?;
    let keys = plain_path(&directory.join("keys"))?;
    let database = directory.join("lince.db");
    drop(regular_file(&database, limit(FileKind::Database))?);
    for suffix in ["-wal", "-shm"] {
        let path = directory.join(format!("lince.db{suffix}"));
        match fs::symlink_metadata(&path) {
            Ok(_) => drop(regular_file(&path, limit(FileKind::Database))?),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    let name = request
        .destination
        .file_name()
        .ok_or_else(|| invalid("Choose an owner backup filename"))?;
    let parent = plain_path(
        request
            .destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    if parent.starts_with(&directory) {
        return Err(invalid(
            "Choose a backup destination outside the live Lince data directory",
        ));
    }
    let destination = parent.join(name);
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Choose a new backup file; existing files are never replaced",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    let stage =
        tempfile::Builder::new()
            .prefix(".lince-owner-capture-")
            .tempdir_in(directory.parent().ok_or_else(|| {
                invalid("Choose a Lince data directory below a parent directory")
            })?)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (organ, cell) = runtime.block_on(prepare_snapshot(&database, stage.path(), &keys))?;
    drop(runtime);
    let mut files = Vec::new();
    for kind in [
        FileKind::Database,
        FileKind::OwnerRoot,
        FileKind::AuthorityWallet,
        FileKind::RecordKeyring,
    ] {
        if stage.path().join(filename(kind)).try_exists()? {
            files.push(entry(stage.path(), kind)?);
        }
    }
    let manifest = Manifest {
        organ,
        cell,
        created_at: chrono::Utc::now().timestamp(),
        files,
    };
    manifest.validate()?;
    let mut target = tempfile::Builder::new()
        .prefix(".lince-owner-encrypted-")
        .tempfile_in(&parent)?;
    utils::owner_backup::seal_archive(
        target.as_file_mut(),
        request.passphrase.expose(),
        &manifest,
        |entry| regular_file(&stage.path().join(filename(entry.kind)), entry.bytes),
    )?;
    target.as_file().sync_all()?;
    target
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(manifest)
}
