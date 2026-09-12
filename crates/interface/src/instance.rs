use crate::wake::WakeSignal;
use std::{
    fs::{File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinHandle,
};

pub struct InstanceGuard {
    _lock: File,
    endpoint: PathBuf,
    task: JoinHandle<()>,
    events: InstanceEvents,
}

#[derive(Clone)]
pub struct InstanceEvents {
    receiver: Arc<Mutex<mpsc::Receiver<()>>>,
    wake: Arc<Mutex<Option<WakeSignal>>>,
}

impl InstanceGuard {
    pub fn events(&self) -> InstanceEvents {
        self.events.clone()
    }
}

impl InstanceEvents {
    pub fn attach(&self, wake: WakeSignal) {
        *self.wake.lock().unwrap() = Some(wake.clone());
        wake.ring();
    }
    pub fn take_show(&self) -> bool {
        let mut show = false;
        while self.receiver.lock().unwrap().try_recv().is_ok() {
            show = true;
        }
        show
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.endpoint);
    }
}

pub async fn claim(data_dir: &Path) -> io::Result<Option<InstanceGuard>> {
    std::fs::create_dir_all(data_dir)?;
    let canonical = data_dir.canonicalize()?;
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    canonical.hash(&mut hash);
    let directory = control_directory(&canonical)?;
    let stem = format!("{:016x}", hash.finish());
    let lock_path = directory.join(format!("{stem}.lock"));
    let endpoint = directory.join(format!("{stem}.sock"));
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let lock = options.open(lock_path)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            for attempt in 0..40 {
                match request_show(&endpoint).await {
                    Ok(()) => return Ok(None),
                    Err(error) if attempt == 39 => return Err(error),
                    Err(_) => tokio::time::sleep(std::time::Duration::from_millis(25)).await,
                }
            }
            unreachable!();
        }
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let wake = Arc::new(Mutex::new(None::<WakeSignal>));
    let task = listen(&endpoint, sender, wake.clone()).await?;
    Ok(Some(InstanceGuard {
        _lock: lock,
        endpoint,
        task,
        events: InstanceEvents {
            receiver: Arc::new(Mutex::new(receiver)),
            wake,
        },
    }))
}

#[cfg(unix)]
fn control_directory(_: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = rustix::process::geteuid().as_raw();
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!("lince-ui-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Lince control directory must be private and owned by this user",
        ));
    }
    Ok(path)
}

#[cfg(not(unix))]
fn control_directory(data_dir: &Path) -> io::Result<PathBuf> {
    Ok(data_dir.to_path_buf())
}

#[cfg(unix)]
async fn request_show(path: &Path) -> io::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut stream = tokio::net::UnixStream::connect(path).await?;
        stream.write_all(b"show\n").await?;
        let mut ack = [0; 2];
        stream.read_exact(&mut ack).await?;
        if &ack != b"ok" {
            return Err(io::Error::other("Invalid Lince control reply"));
        }
        Ok(())
    })
    .await
    .map_err(io::Error::other)?
}

#[cfg(unix)]
async fn listen(
    path: &Path,
    sender: mpsc::SyncSender<()>,
    wake: Arc<Mutex<Option<WakeSignal>>>,
) -> io::Result<JoinHandle<()>> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => return Err(io::Error::other("Lince control endpoint is not a socket")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    Ok(tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                let mut message = [0; 5];
                stream.read_exact(&mut message).await?;
                if &message == b"show\n" {
                    let _ = sender.try_send(());
                    if let Some(wake) = wake.lock().unwrap().as_ref() {
                        wake.ring();
                    }
                    stream.write_all(b"ok").await?;
                }
                Ok::<_, io::Error>(())
            })
            .await;
        }
    }))
}

#[cfg(not(unix))]
async fn request_show(path: &Path) -> io::Result<()> {
    let address = std::fs::read_to_string(path)?;
    let (address, token) = address
        .split_once('\n')
        .ok_or_else(|| io::Error::other("Invalid control endpoint"))?;
    let address: std::net::SocketAddr = address.parse().map_err(io::Error::other)?;
    if !address.ip().is_loopback() {
        return Err(io::Error::other("Control endpoint must be local"));
    }
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut stream = tokio::net::TcpStream::connect(address).await?;
        stream.write_all(token.as_bytes()).await?;
        let mut ack = [0; 2];
        stream.read_exact(&mut ack).await?;
        if &ack != b"ok" {
            return Err(io::Error::other("Invalid Lince control reply"));
        }
        Ok(())
    })
    .await
    .map_err(io::Error::other)?
}

#[cfg(not(unix))]
async fn listen(
    path: &Path,
    sender: mpsc::SyncSender<()>,
    wake: Arc<Mutex<Option<WakeSignal>>>,
) -> io::Result<JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let token = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::fs::write(path, format!("{}\n{token}", listener.local_addr()?))?;
    Ok(tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                let mut message = [0; 64];
                stream.read_exact(&mut message).await?;
                if message == token.as_bytes() {
                    let _ = sender.try_send(());
                    if let Some(wake) = wake.lock().unwrap().as_ref() {
                        wake.ring();
                    }
                    stream.write_all(b"ok").await?;
                }
                Ok::<_, io::Error>(())
            })
            .await;
        }
    }))
}

pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg_attr(test, tokio::test)]
    async fn second_launch_wakes_the_owner_and_lock_lasts_until_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let first = claim(directory.path()).await.unwrap().unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let events = first.events();
        events.attach(WakeSignal::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(claim(directory.path()).await.unwrap().is_none());
        assert!(events.take_show());
        assert!(!events.take_show());
        assert!(wakes.load(Ordering::SeqCst) >= 2);
        drop(events);
        assert!(claim(directory.path()).await.unwrap().is_none());
        drop(first);
        assert!(claim(directory.path()).await.unwrap().is_some());
    }

    #[cfg(unix)]
    #[cfg_attr(test, tokio::test)]
    async fn malformed_control_messages_cannot_trigger_show() {
        let directory = tempfile::tempdir().unwrap();
        let first = claim(directory.path()).await.unwrap().unwrap();
        let mut stream = tokio::net::UnixStream::connect(&first.endpoint)
            .await
            .unwrap();
        stream.write_all(b"quit\n").await.unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
        assert!(!first.events().take_show());
        assert!(claim(directory.path()).await.unwrap().is_none());
    }

    crate::laboratory_cases! {
        async second_launch_wakes_the_owner_and_lock_lasts_until_shutdown,
        #[cfg(unix)]
        async malformed_control_messages_cannot_trigger_show,
    }
}
