use crate::{Checkpoint, MAX_FILE_BYTES, Result};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use ropey::Rope;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const HEADER: &[u8] = b"LINCE-DRAFT-1\n";
const LIMIT: usize = 3 * MAX_FILE_BYTES + 8192;

#[derive(Clone)]
pub struct Draft {
    pub path: PathBuf,
    pub checkpoint: Checkpoint,
    pub bom: bool,
    pub detached: bool,
}

pub struct Store {
    dir: Dir,
    _lock: std::fs::File,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path).map_err(|e| e.to_string())?;
        if !std::fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("The recovery directory must not be a link".into());
        }
        let dir = Dir::open_ambient_dir(path, ambient_authority()).map_err(|e| e.to_string())?;
        let mut options = private_options();
        options.create(true);
        let lock = dir
            .open_with("session.lock", &options)
            .map_err(|e| e.to_string())?
            .into_std();
        lock.try_lock()
            .map_err(|e| format!("Recovery is already in use: {e}"))?;
        Ok(Self { dir, _lock: lock })
    }

    pub fn load(&self) -> Result<(Vec<Draft>, Vec<String>)> {
        let mut drafts = Vec::new();
        let mut errors = Vec::new();
        let mut bytes = 0;
        for entry in self.dir.entries().map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with(".draft") {
                continue;
            }
            if drafts.len() >= 64 {
                errors
                    .push("Recovery is limited to 64 drafts; remaining files were retained".into());
                break;
            }
            let loaded: Result<Draft> = (|| {
                let mut options = OpenOptions::new();
                options.read(true);
                #[cfg(unix)]
                {
                    use cap_std::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
                }
                let file = self
                    .dir
                    .open_with(&name, &options)
                    .map_err(|e| e.to_string())?;
                let metadata = file.metadata().map_err(|e| e.to_string())?;
                if !metadata.is_file() || metadata.len() > LIMIT as u64 {
                    return Err("Invalid recovery file size or type".into());
                }
                if bytes + metadata.len() > 512 * 1024 * 1024 {
                    return Err("Recovery exceeds the 512 MiB loading limit".into());
                }
                let mut value = Vec::new();
                file.take((LIMIT + 1) as u64)
                    .read_to_end(&mut value)
                    .map_err(|e| e.to_string())?;
                bytes += value.len() as u64;
                let draft = decode(&value)?;
                if filename(&draft.path)? != name.to_string_lossy() {
                    return Err("Recovery path does not match its file name".into());
                }
                Ok(draft)
            })();
            match loaded {
                Ok(draft) => drafts.push(draft),
                Err(e) => errors.push(format!(
                    "{}: {e}; recovery file retained",
                    name.to_string_lossy()
                )),
            }
        }
        drafts.sort_by(|a, b| a.path.cmp(&b.path));
        Ok((drafts, errors))
    }

    pub fn write(&self, draft: &Draft) -> Result<()> {
        let bytes = encode(draft)?;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = format!(
            "checkpoint-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut options = private_options();
        options.create_new(true);
        let result = (|| {
            let mut file = self
                .dir
                .open_with(&temporary, &options)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            self.dir
                .rename(&temporary, &self.dir, filename(&draft.path)?)
                .map_err(|e| e.to_string())?;
            self.sync()
        })();
        if result.is_err() {
            let _ = self.dir.remove_file(&temporary);
        }
        result
    }

    pub fn remove(&self, path: &Path) -> Result<()> {
        match self.dir.remove_file(filename(path)?) {
            Ok(()) => self.sync(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn sync(&self) -> Result<()> {
        #[cfg(unix)]
        self.dir
            .open(".")
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options
}

fn filename(path: &Path) -> Result<String> {
    let path = path.to_str().ok_or("Recovery requires a UTF-8 path")?;
    let digest = Sha256::digest(path.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{digest}.draft"))
}

fn encode(draft: &Draft) -> Result<Vec<u8>> {
    let path = draft
        .path
        .to_str()
        .ok_or("Recovery requires a UTF-8 path")?;
    if !draft.path.is_absolute() || path.len() > 4096 {
        return Err("Invalid recovery path".into());
    }
    let working = draft.checkpoint.working.to_string();
    let mut bytes = HEADER.to_vec();
    bytes.extend([u8::from(draft.bom), u8::from(draft.detached)]);
    for (value, limit) in [
        (path, 4096),
        (&draft.checkpoint.baseline, MAX_FILE_BYTES),
        (&draft.checkpoint.observed, MAX_FILE_BYTES),
        (&working, MAX_FILE_BYTES),
    ] {
        if value.len() > limit {
            return Err("Recovery text exceeds its limit".into());
        }
        bytes.extend((value.len() as u64).to_le_bytes());
        bytes.extend(value.as_bytes());
    }
    let digest = Sha256::digest(&bytes);
    bytes.extend(digest);
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<Draft> {
    if bytes.len() > LIMIT || bytes.len() < HEADER.len() + 34 || !bytes.starts_with(HEADER) {
        return Err("Invalid recovery header".into());
    }
    let (body, checksum) = bytes.split_at(bytes.len() - 32);
    if Sha256::digest(body).as_slice() != checksum {
        return Err("Recovery checksum failed".into());
    }
    let mut rest = &body[HEADER.len()..];
    if rest[0] > 1 || rest[1] > 1 {
        return Err("Invalid recovery flags".into());
    }
    let (bom, detached) = (rest[0] != 0, rest[1] != 0);
    rest = &rest[2..];
    let mut field = |limit| -> Result<String> {
        if rest.len() < 8 {
            return Err("Truncated recovery length".into());
        }
        let size = u64::from_le_bytes(rest[..8].try_into().unwrap());
        rest = &rest[8..];
        if size > limit as u64 || size > rest.len() as u64 {
            return Err("Invalid recovery length".into());
        }
        let value = std::str::from_utf8(&rest[..size as usize])
            .map_err(|e| e.to_string())?
            .to_owned();
        rest = &rest[size as usize..];
        Ok(value)
    };
    let path = PathBuf::from(field(4096)?);
    let baseline = field(MAX_FILE_BYTES)?.into();
    let observed = field(MAX_FILE_BYTES)?.into();
    let working = Rope::from_str(&field(MAX_FILE_BYTES)?);
    if !rest.is_empty() || !path.is_absolute() {
        return Err("Invalid recovery content".into());
    }
    Ok(Draft {
        path,
        checkpoint: Checkpoint {
            baseline,
            observed,
            working,
        },
        bom,
        detached,
    })
}

#[cfg(test)]
mod tests;
