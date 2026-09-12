use super::{CAPACITY, Diagnostics, Notice, Subscription};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

enum Request {
    Changed,
    Stop,
}

pub struct Journal {
    subscription: Option<Subscription>,
    sender: SyncSender<Request>,
    task: Option<JoinHandle<()>>,
}

impl Journal {
    pub fn open(path: PathBuf, log: Diagnostics) -> io::Result<Self> {
        let notices = read(&path)?;
        if !notices.is_empty() {
            log.change(|state| {
                state.next_id = notices
                    .iter()
                    .map(|notice| notice.id)
                    .max()
                    .unwrap_or(0)
                    .max(state.next_id);
                let fresh = std::mem::take(&mut state.notices);
                state.notices = notices;
                for mut notice in fresh {
                    state.next_id = state.next_id.saturating_add(1);
                    notice.id = state.next_id;
                    state.notices.push(notice);
                }
                let excess = state.notices.len().saturating_sub(CAPACITY);
                state.notices.drain(..excess);
                true
            });
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let changed = sender.clone();
        let subscription = log.subscribe(move || match changed.try_send(Request::Changed) {
            Ok(())
            | Err(mpsc::TrySendError::Full(_))
            | Err(mpsc::TrySendError::Disconnected(_)) => {}
        });
        let task = std::thread::Builder::new().name("notification-storage".into()).spawn(move || {
            let mut revision = None;
            let mut failure = None;
            while let Ok(request) = receiver.recv() {
                let mut stop = matches!(request, Request::Stop);
                let deadline = Instant::now() + Duration::from_millis(250);
                while !stop {
                    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                        Ok(Request::Changed) => {}
                        Ok(Request::Stop) => stop = true,
                        Err(_) => break,
                    }
                }
                let (current, notices) = log.snapshot();
                if revision != Some(current) {
                    match write(&path, &notices) {
                        Ok(()) => { revision = Some(current); failure = None; }
                        Err(error) => {
                            let message = format!("Could not save notifications. They remain available in this session: {error}");
                            if failure.as_ref() != Some(&message) {
                                log.report("utils::diagnostics", &message);
                                eprintln!("{message}");
                                failure = Some(message);
                            }
                        }
                    }
                }
                if stop { break }
            }
        })?;
        match sender.try_send(Request::Changed) {
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(error) => return Err(io::Error::other(error)),
        }
        Ok(Self {
            subscription: Some(subscription),
            sender,
            task: Some(task),
        })
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        self.subscription.take();
        if self.sender.send(Request::Stop).is_err() {
            eprintln!("Notification storage stopped before it could flush.");
        }
        if let Some(task) = self.task.take()
            && task.join().is_err()
        {
            eprintln!("Notification storage failed while closing.");
        }
    }
}

fn read(path: &Path) -> io::Result<Vec<Notice>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_file() => {
            return Err(io::Error::other(
                "Notification history is not a regular file",
            ));
        }
        _ => {}
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 256 * 1024 {
        return Err(io::Error::other("Notification history is too large"));
    }
    let notices: Vec<Notice> = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    let mut ids = std::collections::HashSet::new();
    if notices.len() > CAPACITY
        || notices.iter().any(|notice| {
            notice.id == 0
                || notice.id == u64::MAX
                || !ids.insert(notice.id)
                || notice.source.len() > 128
                || notice.message.len() > 2048
                || notice.occurrences == 0
                || chrono::DateTime::parse_from_rfc3339(&notice.last_seen).is_err()
        })
    {
        return Err(io::Error::other("Notification history has invalid entries"));
    }
    Ok(notices)
}

fn write(path: &Path, notices: &[Notice]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut directory = fs::DirBuilder::new();
    directory.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(parent)?;
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| {
        file.write_all(&serde_json::to_vec_pretty(notices).map_err(io::Error::other)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err()
        && let Err(error) = fs::remove_file(&temporary)
        && error.kind() != io::ErrorKind::NotFound
    {
        eprintln!("Could not remove temporary notification history: {error}");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restart_restores_notices_and_dismissals_are_flushed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notifications.json");
        let log = Diagnostics::default();
        let journal = Journal::open(path.clone(), log.clone()).unwrap();
        log.report("cell", "Could not save");
        log.report("cell", "Could not save");
        drop(journal);
        let restored = Diagnostics::default();
        let journal = Journal::open(path.clone(), restored.clone()).unwrap();
        let notices = restored.snapshot().1;
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].occurrences, 2);
        restored.dismiss(notices[0].id);
        drop(journal);
        assert!(read(&path).unwrap().is_empty());
    }

    #[test]
    fn malformed_history_is_preserved_and_oversized_history_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notifications.json");
        fs::write(&path, b"broken history").unwrap();
        assert!(Journal::open(path.clone(), Diagnostics::default()).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken history");
        fs::write(&path, vec![b' '; 256 * 1024 + 1]).unwrap();
        assert!(read(&path).is_err());
    }

    fn wait_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready() {
            assert!(Instant::now() < deadline, "notification storage timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn failed_writes_keep_live_notices_and_recover_without_idle_writes() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("data");
        let path = parent.join("notifications.json");
        let log = Diagnostics::default();
        let journal = Journal::open(path.clone(), log.clone()).unwrap();
        wait_until(|| path.exists());
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(350));
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&parent).unwrap();
        fs::write(&parent, b"blocked").unwrap();
        log.report("cell", "Keep this warning");
        wait_until(|| {
            log.snapshot()
                .1
                .iter()
                .any(|notice| notice.message.contains("Could not save notifications"))
        });
        fs::remove_file(&parent).unwrap();
        log.report("cell", "Storage restored");
        drop(journal);
        assert!(
            read(&path)
                .unwrap()
                .iter()
                .any(|notice| notice.message == "Keep this warning")
        );
    }

    #[cfg(unix)]
    #[test]
    fn history_is_private_and_existing_symlinks_are_rejected() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("data/notifications.json");
        let log = Diagnostics::default();
        let journal = Journal::open(path.clone(), log.clone()).unwrap();
        log.report("cell", "Private diagnostic");
        drop(journal);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let link = directory.path().join("linked.json");
        symlink(&path, &link).unwrap();
        assert!(Journal::open(link, Diagnostics::default()).is_err());
        assert_eq!(read(&path).unwrap().len(), 1);
    }
}
