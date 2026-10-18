use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Artifact {
    pub name: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Manifest {
    pub id: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub license: &'static str,
    pub artifact: Artifact,
}

#[derive(Clone)]
pub struct Cache {
    root: PathBuf,
    pub manifest: Manifest,
}

pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Cache {
    pub fn new(root: PathBuf, manifest: Manifest) -> Result<Self, String> {
        for value in [manifest.id, manifest.revision] {
            if value.is_empty()
                || value.len() > 128
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
                || value == "."
                || value == ".."
            {
                return Err("Invalid local model manifest path.".into());
            }
        }
        if manifest.artifact.name.len() > 512
            || manifest.artifact.name.split('/').count() > 4
            || manifest.artifact.name.split('/').any(|part| {
                part.is_empty()
                    || part.len() > 128
                    || part == "."
                    || part == ".."
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            })
        {
            return Err("Invalid local model artifact path.".into());
        }
        if manifest.artifact.bytes == 0
            || manifest.artifact.bytes > 4_294_967_296
            || manifest.artifact.sha256.len() != 64
            || !manifest
                .artifact
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("Invalid local model manifest bounds or checksum.".into());
        }
        Ok(Self { root, manifest })
    }

    pub fn directory(&self) -> PathBuf {
        self.root
            .join(format!("{}-{}", self.manifest.id, self.manifest.revision))
    }

    pub fn path(&self) -> PathBuf {
        self.directory().join(self.manifest.artifact.name)
    }

    fn temporary(&self) -> Result<tempfile::NamedTempFile, String> {
        let path = self.path();
        let parent = path
            .parent()
            .ok_or("Model artifact needs a parent directory.")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())
    }

    pub fn present(&self) -> bool {
        std::fs::symlink_metadata(self.path()).is_ok_and(|metadata| {
            metadata.is_file() && metadata.len() == self.manifest.artifact.bytes
        })
    }

    pub fn verified_file(&self) -> Result<std::fs::File, String> {
        let mut file = self.open_regular(&self.path())?;
        verify(&mut file, self.manifest.artifact, None, None)?;
        std::io::Seek::rewind(&mut file).map_err(|error| error.to_string())?;
        Ok(file)
    }

    fn open_regular(&self, path: &Path) -> Result<std::fs::File, String> {
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|error| format!("Local model file is unavailable: {error}"))?;
        if !metadata.is_file() || metadata.len() != self.manifest.artifact.bytes {
            return Err("Local model file has the wrong type or size. Import or download the registered checkpoint again.".into());
        }
        std::fs::File::open(path).map_err(|error| error.to_string())
    }

    pub async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        check_cancel(&cancelled)?;
        if let Some(source) = source {
            let cache = self.clone();
            return tokio::task::spawn_blocking(move || {
                cache.import(&source, &cancelled, &progress)
            })
            .await
            .map_err(|error| error.to_string())?;
        }
        let url = format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.manifest.repository, self.manifest.revision, self.manifest.artifact.name
        );
        self.download(&url, cancelled, progress).await
    }

    pub(crate) async fn download(
        &self,
        url: &str,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        check_cancel(&cancelled)?;
        let mut temporary = self.temporary()?;
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(1800))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|error| error.to_string())?;
        let mut response = tokio::select! {
            biased;
            _ = until_cancelled(&cancelled) => return Err("Local model operation cancelled.".into()),
            response = client.get(url).send() => response.map_err(|error| format!("Model download failed: {error}"))?,
        }
            .error_for_status()
            .map_err(|error| format!("Model download rejected: {error}"))?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        loop {
            let chunk = tokio::select! {
                biased;
                _ = until_cancelled(&cancelled) => return Err("Local model operation cancelled.".into()),
                chunk = response.chunk() => chunk.map_err(|error| format!("Model download interrupted: {error}"))?,
            };
            let Some(chunk) = chunk else { break };
            check_cancel(&cancelled)?;
            bytes = bytes
                .checked_add(chunk.len() as u64)
                .ok_or("Model size overflow.")?;
            if bytes > self.manifest.artifact.bytes {
                return Err("Model download exceeds the registered size.".into());
            }
            hash.update(&chunk);
            temporary
                .write_all(&chunk)
                .map_err(|error| error.to_string())?;
            progress.store(bytes, Ordering::Release);
        }
        if bytes != self.manifest.artifact.bytes
            || hex(&hash.finalize()) != self.manifest.artifact.sha256
        {
            return Err("Model download did not match the registered size and SHA-256. No model was installed.".into());
        }
        self.promote(temporary, &cancelled)
    }

    fn import(
        &self,
        source: &Path,
        cancelled: &AtomicBool,
        progress: &AtomicU64,
    ) -> Result<(), String> {
        let mut source = self.open_regular(source)?;
        let mut temporary = self.temporary()?;
        verify(
            &mut source,
            self.manifest.artifact,
            Some((cancelled, progress)),
            Some(temporary.as_file_mut()),
        )?;
        self.promote(temporary, cancelled)
    }

    fn promote(
        &self,
        temporary: tempfile::NamedTempFile,
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        check_cancel(cancelled)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        temporary
            .persist(self.path())
            .map_err(|error| error.to_string())?;
        let mut manifest =
            tempfile::NamedTempFile::new_in(self.directory()).map_err(|error| error.to_string())?;
        manifest
            .write_all(
                &serde_json::to_vec_pretty(&self.manifest).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        manifest
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        let manifest_path = if self.manifest.artifact.name.contains('/') {
            let path = self.path();
            path.with_file_name(format!(
                "{}.lince-manifest.json",
                path.file_name()
                    .ok_or("Invalid model artifact name.")?
                    .to_string_lossy()
            ))
        } else {
            self.directory().join("manifest.json")
        };
        manifest
            .persist(manifest_path)
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

async fn until_cancelled(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

pub fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Acquire) {
        Err("Local model operation cancelled.".into())
    } else {
        Ok(())
    }
}

fn verify(
    source: &mut impl Read,
    artifact: Artifact,
    control: Option<(&AtomicBool, &AtomicU64)>,
    mut target: Option<&mut std::fs::File>,
) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 262_144];
    let mut bytes = 0u64;
    loop {
        if let Some((cancelled, _)) = control {
            check_cancel(cancelled)?;
        }
        let count = source
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .ok_or("Model size overflow.")?;
        if bytes > artifact.bytes {
            return Err("Model file exceeds the registered size.".into());
        }
        hash.update(&buffer[..count]);
        if let Some(target) = target.as_mut() {
            target
                .write_all(&buffer[..count])
                .map_err(|error| error.to_string())?;
        }
        if let Some((_, progress)) = control {
            progress.store(bytes, Ordering::Release);
        }
    }
    if bytes != artifact.bytes || hex(&hash.finalize()) != artifact.sha256 {
        return Err(
            "Local model checksum mismatch. Import or download the registered checkpoint again."
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: Manifest = Manifest {
        id: "fixture",
        repository: "fixture/model",
        revision: "pinned",
        license: "fixture",
        artifact: Artifact {
            name: "model.bin",
            bytes: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        },
    };

    async fn serve(
        response: &'static [u8],
        hold: bool,
    ) -> (
        String,
        Arc<tokio::sync::Notify>,
        tokio::task::JoinHandle<()>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/model.bin", listener.local_addr().unwrap());
        let received = Arc::new(tokio::sync::Notify::new());
        let ready = received.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            stream.write_all(response).await.unwrap();
            ready.notify_one();
            if hold {
                let mut byte = [0];
                assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
            }
        });
        (url, received, server)
    }

    #[tokio::test]
    async fn stalled_download_cancellation_closes_the_request_and_preserves_verified_assets() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"abc").unwrap();
        let cache = Cache::new(root.path().join("models"), MANIFEST).unwrap();
        cache
            .acquire(
                Some(source),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            )
            .await
            .unwrap();
        for response in [
            b"".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\na".as_slice(),
        ] {
            let (url, received, server) = serve(response, true).await;
            let cancelled = Arc::new(AtomicBool::new(false));
            let progress = Arc::new(AtomicU64::new(0));
            let task = {
                let cache = cache.clone();
                let cancelled = cancelled.clone();
                let progress = progress.clone();
                tokio::spawn(async move { cache.download(&url, cancelled, progress).await })
            };
            tokio::time::timeout(std::time::Duration::from_secs(2), received.notified())
                .await
                .unwrap();
            if !response.is_empty() {
                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    while progress.load(Ordering::Acquire) == 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                    }
                })
                .await
                .unwrap();
            }
            cancelled.store(true, Ordering::Release);
            let result = tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap();
            assert!(result.unwrap_err().contains("cancelled"));
            tokio::time::timeout(std::time::Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(std::fs::read(cache.path()).unwrap(), b"abc");
            assert_eq!(std::fs::read_dir(cache.directory()).unwrap().count(), 2);
        }
    }

    #[tokio::test]
    async fn incomplete_corrupt_oversized_and_rejected_downloads_cannot_replace_a_verified_model() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"abc").unwrap();
        let cache = Cache::new(root.path().join("models"), MANIFEST).unwrap();
        cache
            .acquire(
                Some(source),
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            )
            .await
            .unwrap();
        for response in [
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nab".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nbad".as_slice(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nabcd".as_slice(),
            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".as_slice(),
        ] {
            let (url, _, server) = serve(response, false).await;
            assert!(
                cache
                    .download(
                        &url,
                        Arc::new(AtomicBool::new(false)),
                        Arc::new(AtomicU64::new(0))
                    )
                    .await
                    .is_err()
            );
            server.await.unwrap();
            assert_eq!(std::fs::read(cache.path()).unwrap(), b"abc");
            assert_eq!(std::fs::read_dir(cache.directory()).unwrap().count(), 2);
            assert!(cache.verified_file().is_ok());
        }
        let (url, _, server) =
            serve(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc", false).await;
        let progress = Arc::new(AtomicU64::new(0));
        cache
            .download(&url, Arc::new(AtomicBool::new(false)), progress.clone())
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(progress.load(Ordering::Acquire), 3);
        assert!(cache.verified_file().is_ok());
    }

    #[tokio::test]
    async fn verified_import_is_atomic_corruption_and_cancellation_preserve_installed_model() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.bin");
        std::fs::write(&source, b"abc").unwrap();
        let cache = Cache::new(root.path().join("models"), MANIFEST).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        cache
            .acquire(Some(source.clone()), cancelled.clone(), progress.clone())
            .await
            .unwrap();
        assert!(cache.present());
        assert_eq!(progress.load(Ordering::Acquire), 3);
        let mut verified = cache.verified_file().unwrap();
        let mut bytes = Vec::new();
        verified.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"abc");
        assert!(cache.directory().join("manifest.json").exists());
        std::fs::write(&source, b"bad").unwrap();
        assert!(
            cache
                .acquire(Some(source.clone()), cancelled.clone(), progress.clone())
                .await
                .unwrap_err()
                .contains("checksum")
        );
        assert_eq!(std::fs::read(cache.path()).unwrap(), b"abc");
        std::fs::write(&source, b"abc").unwrap();
        cancelled.store(true, Ordering::Release);
        assert!(
            cache
                .acquire(Some(source), cancelled, progress)
                .await
                .unwrap_err()
                .contains("cancelled")
        );
        assert_eq!(std::fs::read_dir(cache.directory()).unwrap().count(), 2);
        std::fs::write(cache.path(), b"bad").unwrap();
        assert!(cache.verified_file().unwrap_err().contains("checksum"));
    }

    #[test]
    fn manifests_cannot_escape_storage_or_accept_unbounded_artifacts() {
        for id in ["../other", "/absolute", ".", "..", ""] {
            assert!(Cache::new(PathBuf::from("models"), Manifest { id, ..MANIFEST }).is_err());
        }
        for name in [
            "../file",
            "/file",
            "encoder/../../file",
            "encoder//file",
            "encoder/../file",
            "encoder\\file",
        ] {
            assert!(
                Cache::new(
                    PathBuf::from("models"),
                    Manifest {
                        artifact: Artifact {
                            name,
                            ..MANIFEST.artifact
                        },
                        ..MANIFEST
                    }
                )
                .is_err()
            );
        }
        assert!(
            Cache::new(
                PathBuf::from("models"),
                Manifest {
                    artifact: Artifact {
                        bytes: u64::MAX,
                        ..MANIFEST.artifact
                    },
                    ..MANIFEST
                }
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn a_multi_file_model_keeps_nested_assets_and_their_manifests_separate() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        std::fs::write(&source, b"abc").unwrap();
        let first = Cache::new(
            root.path().join("models"),
            Manifest {
                artifact: Artifact {
                    name: "encoder/config.json",
                    ..MANIFEST.artifact
                },
                ..MANIFEST
            },
        )
        .unwrap();
        let second = Cache::new(
            root.path().join("models"),
            Manifest {
                artifact: Artifact {
                    name: "tokenizer/config.json",
                    ..MANIFEST.artifact
                },
                ..MANIFEST
            },
        )
        .unwrap();
        for cache in [&first, &second] {
            cache
                .acquire(
                    Some(source.clone()),
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(AtomicU64::new(0)),
                )
                .await
                .unwrap();
            assert!(cache.present());
            assert!(
                cache
                    .path()
                    .with_file_name("config.json.lince-manifest.json")
                    .is_file()
            );
            cache.verified_file().unwrap();
        }
        assert_eq!(first.directory(), second.directory());
        assert_ne!(first.path(), second.path());
    }
}
