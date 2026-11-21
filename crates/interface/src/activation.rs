//! Data-directory ownership and activation of a running Lince instance.

mod transport;

use crate::host::WakeSignal;
use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions, TryLockError},
    hash::{DefaultHasher, Hash, Hasher},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError, mpsc},
    time::Duration,
};
use tokio::{task::JoinHandle, time::sleep};
use transport::{listen, request_show};

type WakeSlot = Arc<Mutex<Option<WakeSignal>>>;
const ACTIVATION_ATTEMPTS: usize = 40;
const ACTIVATION_RETRY_DELAY: Duration = Duration::from_millis(25);

/// Keeps the data directory locked and its activation listener alive.
pub struct InstanceGuard {
    _lock: File,
    pub(crate) endpoint: PathBuf,
    task: JoinHandle<()>,
    events: InstanceEvents,
}

/// A cloneable reader of the current owner's activation events.
#[derive(Clone)]
pub struct InstanceEvents {
    receiver: Arc<Mutex<mpsc::Receiver<()>>>,
    wake: WakeSlot,
}

impl InstanceGuard {
    /// Shares access to the pending activation events.
    pub fn events(&self) -> InstanceEvents {
        self.events.clone()
    }
}

impl InstanceEvents {
    /// Installs the host wake callback and rings it for any already pending events.
    pub fn attach(&self, wake: WakeSignal) {
        let previous = self
            .wake
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(wake.clone());
        drop(previous);
        wake.ring();
    }

    /// Drains pending show events, reporting whether any were received.
    pub fn take_show(&self) -> bool {
        self.receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_iter()
            .last()
            .is_some()
    }
}

fn ring(wake: &WakeSlot) {
    // A host callback can itself change the installed wake signal.
    let signal = wake.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(signal) = signal {
        signal.ring();
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        self.task.abort();
        let _ = fs::remove_file(&self.endpoint);
    }
}

/// Claims an unowned data directory or requests that the running owner show its window.
///
/// # Errors
///
/// Returns an error if ownership or the activation request fails.
pub async fn claim(data_dir: &Path) -> io::Result<Option<InstanceGuard>> {
    claim_with_policy(data_dir, ClaimPolicy::ShowOwner).await
}

/// Claims a dedicated session without activating another instance.
///
/// # Errors
///
/// Returns an error if another instance owns the directory or the listener cannot start.
pub async fn claim_session(data_dir: &Path) -> io::Result<InstanceGuard> {
    claim_with_policy(data_dir, ClaimPolicy::RequireOwnership)
        .await?
        .ok_or_else(|| io::Error::other("Park must own its data directory"))
}

enum ClaimPolicy {
    ShowOwner,
    RequireOwnership,
}

async fn claim_with_policy(
    data_dir: &Path,
    policy: ClaimPolicy,
) -> io::Result<Option<InstanceGuard>> {
    let (lock, endpoint) = instance_lock(data_dir)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => match policy {
            ClaimPolicy::RequireOwnership => return Err(ownership_error(data_dir)),
            ClaimPolicy::ShowOwner => {
                request_existing_owner(&endpoint).await?;
                return Ok(None);
            }
        },
        Err(TryLockError::Error(error)) => return Err(error),
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let wake = Arc::new(Mutex::new(None::<WakeSignal>));
    let task = listen(&endpoint, sender, wake.clone())
        .await
        .map_err(|error| path_error(error, "listen for application activation", &endpoint))?;
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

async fn request_existing_owner(endpoint: &Path) -> io::Result<()> {
    for _ in 1..ACTIVATION_ATTEMPTS {
        if request_show(endpoint).await.is_ok() {
            return Ok(());
        }
        sleep(ACTIVATION_RETRY_DELAY).await;
    }
    request_show(endpoint).await
}

fn ownership_error(data_dir: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::WouldBlock,
        format!(
            "Another Lince instance owns {}. Save and stop it before starting Park, or select a separate directory with --directory.",
            data_dir.display()
        ),
    )
}

fn instance_lock(data_dir: &Path) -> io::Result<(File, PathBuf)> {
    fs::create_dir_all(data_dir)
        .map_err(|error| path_error(error, "create application data directory", data_dir))?;
    let canonical = data_dir
        .canonicalize()
        .map_err(|error| path_error(error, "resolve application data directory", data_dir))?;
    let mut hash = DefaultHasher::new();
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
    let lock = options
        .open(&lock_path)
        .map_err(|error| path_error(error, "open instance lock", &lock_path))?;
    Ok((lock, endpoint))
}

/// Claims the directory for a process without a window or activation listener.
///
/// # Errors
///
/// Returns an error if the lock cannot be opened or acquired.
pub fn claim_headless(data_dir: &Path) -> io::Result<File> {
    let (file, _) = instance_lock(data_dir)?;
    file.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => ownership_error(data_dir),
        TryLockError::Error(error) => error,
    })?;
    Ok(file)
}

#[derive(Debug)]
struct PathError {
    error: io::Error,
    operation: &'static str,
    path: PathBuf,
}

impl fmt::Display for PathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            error,
            operation,
            path,
        } = self;
        write!(
            formatter,
            "Cannot {operation} at {}: {error}",
            path.display()
        )
    }
}

impl Error for PathError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

fn path_error(error: io::Error, operation: &'static str, path: &Path) -> io::Error {
    io::Error::new(
        error.kind(),
        PathError {
            error,
            operation,
            path: path.to_path_buf(),
        },
    )
}

#[cfg(unix)]
fn control_directory(_: &Path) -> io::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let uid = rustix::process::geteuid().as_raw();
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!("lince-ui-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(path_error(
                error,
                "create private application control directory",
                &path,
            ));
        }
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        path_error(
            error,
            "inspect private application control directory",
            &path,
        )
    })?;
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

#[cfg(any(test, feature = "native-presentation"))]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[cfg(unix)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn headless_claim_excludes_another_owner_without_opening_a_socket() {
        let directory = tempfile::tempdir().unwrap();
        let first = claim_headless(directory.path()).unwrap();
        assert!(claim_headless(directory.path()).is_err());
        drop(first);
        assert!(claim_headless(directory.path()).is_ok());
    }

    #[test]
    fn failed_claim_preserves_the_filesystem_cause() {
        let directory = tempfile::tempdir().expect("temporary data directory");
        let path = directory.path().join("regular-file");
        fs::write(&path, []).expect("occupy the data directory path with a regular file");

        let error = claim_headless(&path).expect_err("claim should reject a regular file");
        let cause = error
            .source()
            .and_then(|source| source.downcast_ref::<io::Error>())
            .expect("original filesystem error");
        assert_eq!(error.kind(), cause.kind());
        assert!(cause.raw_os_error().is_some());
        assert_eq!(
            error.to_string(),
            format!(
                "Cannot create application data directory at {}: {cause}",
                path.display()
            )
        );
    }

    #[cfg_attr(test, tokio::test)]
    async fn session_claim_never_activates_an_existing_owner() {
        let directory = tempfile::tempdir().unwrap();
        let sandbox = claim(directory.path()).await.unwrap().unwrap();
        let failure = claim_session(directory.path()).await.err().unwrap();
        assert_eq!(failure.kind(), io::ErrorKind::WouldBlock);
        assert!(failure.to_string().contains("--directory"));
        assert!(!sandbox.events().take_show());
        drop(sandbox);
        let headless = claim_headless(directory.path()).unwrap();
        assert!(claim_session(directory.path()).await.is_err());
        drop(headless);
        let park = claim_session(directory.path()).await.unwrap();
        assert!(claim_session(directory.path()).await.is_err());
        assert!(!park.events().take_show());
        assert!(claim(directory.path()).await.unwrap().is_none());
        assert!(park.events().take_show());
        drop(park);
        assert!(claim_session(directory.path()).await.is_ok());
    }

    #[cfg_attr(test, tokio::test)]
    async fn second_launch_wakes_the_owner_and_lock_lasts_until_shutdown() {
        let directory = tempfile::tempdir().unwrap();
        let first = claim(directory.path()).await.unwrap().unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let events = first.events();
        let wake = Arc::downgrade(&events.wake);
        events.attach(WakeSignal::new(move || {
            assert!(wake.upgrade().unwrap().try_lock().is_ok());
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

    #[cfg_attr(test, tokio::test)]
    async fn separate_directories_keep_their_owners_and_wake_requests_independent() {
        let normal_directory = tempfile::tempdir().unwrap();
        let test_directory = tempfile::tempdir().unwrap();
        let normal = claim(normal_directory.path()).await.unwrap().unwrap();
        let development = claim(test_directory.path()).await.unwrap().unwrap();
        assert!(claim(test_directory.path()).await.unwrap().is_none());
        assert!(development.events().take_show());
        assert!(!normal.events().take_show());
        drop(development);
        assert!(claim(test_directory.path()).await.unwrap().is_some());
        assert!(claim(normal_directory.path()).await.unwrap().is_none());
        assert!(normal.events().take_show());
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

    #[cfg(feature = "native-presentation")]
    crate::native::laboratory_cases! {
        async session_claim_never_activates_an_existing_owner,
        async second_launch_wakes_the_owner_and_lock_lasts_until_shutdown,
        async separate_directories_keep_their_owners_and_wake_requests_independent,
        #[cfg(unix)]
        async malformed_control_messages_cannot_trigger_show,
    }
}
