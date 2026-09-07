use std::fmt;
use std::sync::Arc;

use argon2::password_hash::{Output, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use tokio::runtime::Handle;
use tokio::sync::Semaphore;
use zeroize::Zeroizing;

pub const MAX_PASSWORD_BYTES: usize = 1024;
pub const MAX_PHC_BYTES: usize = 256;
pub const MAX_WORKERS: usize = 8;

const MEMORY_KIB: u32 = 19_456;
const ITERATIONS: u32 = 2;
const LANES: u32 = 1;
const SALT_BYTES: usize = 16;
const OUTPUT_BYTES: usize = 32;
const PARAMETER_TEXT: &str = "m=19456,t=2,p=1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordError {
    InvalidCapacity,
    InvalidPassword,
    InvalidHash,
    Busy,
    RuntimeUnavailable,
    RandomUnavailable,
    WorkerFailed,
}

impl fmt::Display for PasswordError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidCapacity => "password worker capacity is invalid",
            Self::InvalidPassword => "password byte length is invalid",
            Self::InvalidHash => "password hash does not match the private profile",
            Self::Busy => "password workers are busy",
            Self::RuntimeUnavailable => "password worker runtime is unavailable",
            Self::RandomUnavailable => "password salt randomness is unavailable",
            Self::WorkerFailed => "password worker failed",
        })
    }
}

impl std::error::Error for PasswordError {}

pub struct PasswordInput {
    bytes: Zeroizing<Vec<u8>>,
}

impl PasswordInput {
    pub fn new(bytes: Vec<u8>) -> Result<Self, PasswordError> {
        let bytes = Zeroizing::new(bytes);
        if bytes.is_empty() || bytes.len() > MAX_PASSWORD_BYTES {
            return Err(PasswordError::InvalidPassword);
        }
        Ok(Self { bytes })
    }
}

impl fmt::Debug for PasswordInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordInput([REDACTED])")
    }
}

pub struct PasswordHash {
    encoded: Zeroizing<String>,
}

impl PasswordHash {
    pub fn from_phc(encoded: String) -> Result<Self, PasswordError> {
        let encoded = Zeroizing::new(encoded);
        validate_phc(&encoded)?;
        Ok(Self { encoded })
    }

    pub fn as_phc(&self) -> &str {
        &self.encoded
    }
}

impl fmt::Debug for PasswordHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasswordHash([REDACTED])")
    }
}

fn validate_phc(encoded: &str) -> Result<(), PasswordError> {
    let invalid = || PasswordError::InvalidHash;
    if encoded.len() > MAX_PHC_BYTES {
        return Err(invalid());
    }
    let mut fields = encoded.split('$');
    if fields.next() != Some("")
        || fields.next() != Some("argon2id")
        || fields.next() != Some("v=19")
        || fields.next() != Some(PARAMETER_TEXT)
    {
        return Err(invalid());
    }
    let salt = fields.next().ok_or_else(invalid)?;
    let output = fields.next().ok_or_else(invalid)?;
    if fields.next().is_some() || salt.len() != 22 || output.len() != 43 {
        return Err(invalid());
    }
    let salt_value = SaltString::from_b64(salt).map_err(|_| invalid())?;
    let mut salt_bytes = [0; SALT_BYTES];
    let salt_bytes = salt_value
        .decode_b64(&mut salt_bytes)
        .map_err(|_| invalid())?;
    if salt_bytes.len() != SALT_BYTES
        || SaltString::encode_b64(salt_bytes)
            .map_err(|_| invalid())?
            .as_str()
            != salt
    {
        return Err(invalid());
    }
    let output_value = Output::b64_decode(output).map_err(|_| invalid())?;
    let mut output_text = [0; 43];
    if output_value.len() != OUTPUT_BYTES
        || output_value
            .b64_encode(&mut output_text)
            .map_err(|_| invalid())?
            != output
    {
        return Err(invalid());
    }
    Ok(())
}

fn hasher() -> Result<Argon2<'static>, PasswordError> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, LANES, Some(OUTPUT_BYTES))
        .map_err(|_| PasswordError::WorkerFailed)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn hash_password(password: PasswordInput) -> Result<PasswordHash, PasswordError> {
    let mut salt_bytes = [0; SALT_BYTES];
    getrandom::fill(&mut salt_bytes).map_err(|_| PasswordError::RandomUnavailable)?;
    let salt = SaltString::encode_b64(&salt_bytes).map_err(|_| PasswordError::WorkerFailed)?;
    let encoded = hasher()?
        .hash_password(&password.bytes, &salt)
        .map_err(|_| PasswordError::WorkerFailed)?
        .to_string();
    PasswordHash::from_phc(encoded)
}

fn verify_password(password: PasswordInput, expected: PasswordHash) -> Result<bool, PasswordError> {
    let parsed =
        argon2::PasswordHash::new(expected.as_phc()).map_err(|_| PasswordError::InvalidHash)?;
    match hasher()?.verify_password(&password.bytes, &parsed) {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(_) => Err(PasswordError::WorkerFailed),
    }
}

#[derive(Clone)]
pub struct PasswordWork {
    semaphore: Arc<Semaphore>,
    capacity: usize,
}

impl PasswordWork {
    pub fn new(capacity: usize) -> Result<Self, PasswordError> {
        if !(1..=MAX_WORKERS).contains(&capacity) {
            return Err(PasswordError::InvalidCapacity);
        }
        Ok(Self {
            semaphore: Arc::new(Semaphore::new(capacity)),
            capacity,
        })
    }

    pub async fn hash(&self, password: PasswordInput) -> Result<PasswordHash, PasswordError> {
        self.execute(move || hash_password(password)).await
    }

    pub async fn verify(
        &self,
        password: PasswordInput,
        expected: PasswordHash,
    ) -> Result<bool, PasswordError> {
        self.execute(move || verify_password(password, expected))
            .await
    }

    async fn execute<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> Result<T, PasswordError> + Send + 'static,
    ) -> Result<T, PasswordError> {
        let permit = self
            .semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| PasswordError::Busy)?;
        let runtime = Handle::try_current().map_err(|_| PasswordError::RuntimeUnavailable)?;
        runtime
            .spawn_blocking(move || {
                let result = work();
                drop(permit);
                result
            })
            .await
            .map_err(|_| PasswordError::WorkerFailed)?
    }
}

impl fmt::Debug for PasswordWork {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasswordWork")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;

    use tokio::sync::oneshot;

    use super::*;

    const TIMEOUT: Duration = Duration::from_secs(10);

    struct Release(mpsc::SyncSender<()>);

    impl Drop for Release {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }

    struct Dropped(Arc<AtomicBool>);

    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn gate() -> (Release, mpsc::Receiver<()>) {
        let (send, receive) = mpsc::sync_channel(1);
        (Release(send), receive)
    }

    fn input() -> PasswordInput {
        PasswordInput::new(b"isolated test password".to_vec()).unwrap()
    }

    fn expected() -> PasswordHash {
        PasswordHash::from_phc(format!(
            "$argon2id$v=19$m=19456,t=2,p=1${}${}",
            "A".repeat(22),
            "A".repeat(43)
        ))
        .unwrap()
    }

    async fn idle(work: &PasswordWork) {
        let permits = tokio::time::timeout(
            TIMEOUT,
            work.semaphore
                .clone()
                .acquire_many_owned(work.capacity as u32),
        )
        .await
        .unwrap()
        .unwrap();
        drop(permits);
    }

    #[tokio::test]
    async fn private_password_hash_and_verify_share_clone_capacity_without_waiting() {
        let work = PasswordWork::new(1).unwrap();
        let other = work.clone();
        let permit = work.semaphore.clone().try_acquire_owned().unwrap();
        let mut hash = Box::pin(other.hash(input()));
        assert!(matches!(
            hash.as_mut().poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(PasswordError::Busy))
        ));
        let mut verify = Box::pin(other.verify(input(), expected()));
        assert_eq!(
            verify
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(PasswordError::Busy))
        );
        drop(permit);
        assert_eq!(other.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn private_password_busy_does_not_enqueue_or_invoke_work() {
        let work = PasswordWork::new(1).unwrap();
        let permit = work.semaphore.clone().try_acquire_owned().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        for _ in 0..128 {
            let calls = calls.clone();
            assert_eq!(
                work.execute(move || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
                .await,
                Err(PasswordError::Busy)
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        drop(permit);
        idle(&work).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn private_password_dropped_caller_keeps_secret_and_permit_inside_worker() {
        let work = PasswordWork::new(1).unwrap();
        let (release, wait) = gate();
        let (entered, started) = oneshot::channel();
        let dropped = Arc::new(AtomicBool::new(false));
        let observed = dropped.clone();
        let password = input();
        let mut caller = Box::pin(work.execute(move || {
            let _dropped = Dropped(observed);
            entered.send(()).unwrap();
            wait.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(password.bytes.as_slice(), b"isolated test password");
            Ok(())
        }));
        assert!(
            caller
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        tokio::time::timeout(TIMEOUT, started)
            .await
            .unwrap()
            .unwrap();
        drop(caller);
        assert!(!dropped.load(Ordering::SeqCst));
        assert_eq!(work.semaphore.available_permits(), 0);
        assert!(matches!(work.hash(input()).await, Err(PasswordError::Busy)));
        assert_eq!(
            work.verify(input(), expected()).await,
            Err(PasswordError::Busy)
        );
        drop(release);
        idle(&work).await;
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(work.execute(|| Ok(7)).await, Ok(7));
    }

    #[tokio::test]
    async fn private_password_aborted_async_task_cannot_release_active_worker_capacity() {
        let work = PasswordWork::new(1).unwrap();
        let worker = work.clone();
        let (release, wait) = gate();
        let (entered, started) = oneshot::channel();
        let caller = tokio::spawn(async move {
            worker
                .execute(move || {
                    entered.send(()).unwrap();
                    wait.recv_timeout(TIMEOUT).unwrap();
                    Ok(())
                })
                .await
        });
        tokio::time::timeout(TIMEOUT, started)
            .await
            .unwrap()
            .unwrap();
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        assert_eq!(work.execute(|| Ok(())).await, Err(PasswordError::Busy));
        drop(release);
        idle(&work).await;
        assert_eq!(work.execute(|| Ok(())).await, Ok(()));
    }

    #[test]
    fn private_password_cancelled_queued_worker_keeps_its_reserved_capacity() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .max_blocking_threads(1)
            .build()
            .unwrap();
        runtime.block_on(async {
            let work = PasswordWork::new(1).unwrap();
            let (release_blocker, wait_blocker) = gate();
            let (blocker_entered, blocker_started) = oneshot::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                blocker_entered.send(()).unwrap();
                wait_blocker.recv_timeout(TIMEOUT).unwrap();
            });
            tokio::time::timeout(TIMEOUT, blocker_started)
                .await
                .unwrap()
                .unwrap();
            let (release_worker, wait_worker) = gate();
            let (worker_entered, mut worker_started) = oneshot::channel();
            let mut caller = Box::pin(work.execute(move || {
                worker_entered.send(()).unwrap();
                wait_worker.recv_timeout(TIMEOUT).unwrap();
                Ok(())
            }));
            assert!(
                caller
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            drop(caller);
            assert_eq!(work.semaphore.available_permits(), 0);
            assert_eq!(
                worker_started.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            );
            assert_eq!(work.execute(|| Ok(())).await, Err(PasswordError::Busy));
            drop(release_blocker);
            tokio::time::timeout(TIMEOUT, blocker)
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(TIMEOUT, worker_started)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(work.semaphore.available_permits(), 0);
            drop(release_worker);
            idle(&work).await;
        });
    }

    #[tokio::test]
    async fn private_password_failed_work_releases_capacity_and_returns_only_typed_error() {
        let work = PasswordWork::new(1).unwrap();
        assert_eq!(
            work.execute::<()>(|| Err(PasswordError::RandomUnavailable))
                .await,
            Err(PasswordError::RandomUnavailable)
        );
        assert_eq!(work.semaphore.available_permits(), 1);
        assert_eq!(work.execute(|| Ok(9)).await, Ok(9));
    }

    #[test]
    fn private_password_unpolled_cancellation_drops_inputs_without_reserving_capacity() {
        let work = PasswordWork::new(1).unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let witness = Dropped(dropped.clone());
        let operation = work.execute(move || {
            drop(witness);
            Ok(())
        });
        assert_eq!(work.semaphore.available_permits(), 1);
        assert!(!dropped.load(Ordering::SeqCst));
        drop(operation);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(work.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn private_password_panicking_work_releases_capacity_and_redacts_join_failure() {
        let work = PasswordWork::new(1).unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let observed = dropped.clone();
        assert_eq!(
            work.execute::<()>(move || {
                let _dropped = Dropped(observed);
                panic!("isolated injected worker failure");
            })
            .await,
            Err(PasswordError::WorkerFailed)
        );
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(work.semaphore.available_permits(), 1);
        assert_eq!(work.execute(|| Ok(11)).await, Ok(11));
    }

    #[test]
    fn private_password_missing_runtime_refuses_without_losing_capacity_or_running_work() {
        let work = PasswordWork::new(1).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let mut operation = Box::pin(work.execute(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));
        assert_eq!(
            operation
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(PasswordError::RuntimeUnavailable))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(work.semaphore.available_permits(), 1);
    }

    #[tokio::test]
    async fn private_password_capacity_bounds_simultaneous_worker_bodies() {
        let work = PasswordWork::new(2).unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let mut releases = Vec::new();
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let worker = work.clone();
            let active = active.clone();
            let (release, wait) = gate();
            let (entered, started) = oneshot::channel();
            releases.push(release);
            tasks.push(tokio::spawn(async move {
                worker
                    .execute(move || {
                        assert!(active.fetch_add(1, Ordering::SeqCst) < 2);
                        entered.send(()).unwrap();
                        wait.recv_timeout(TIMEOUT).unwrap();
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .await
            }));
            tokio::time::timeout(TIMEOUT, started)
                .await
                .unwrap()
                .unwrap();
        }
        assert_eq!(active.load(Ordering::SeqCst), 2);
        assert_eq!(work.execute(|| Ok(())).await, Err(PasswordError::Busy));
        drop(releases);
        for task in tasks {
            assert_eq!(
                tokio::time::timeout(TIMEOUT, task).await.unwrap().unwrap(),
                Ok(())
            );
        }
        idle(&work).await;
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
}
