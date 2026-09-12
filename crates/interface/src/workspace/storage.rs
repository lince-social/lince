use super::{Document, write_atomic};
use crate::wake::WakeSignal;
use cell::InterfaceStorage;
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

type SaveResult = Result<(), String>;

struct Request {
    document: Document,
    finished: Option<mpsc::SyncSender<SaveResult>>,
}

pub(super) struct Writer {
    sender: Option<mpsc::SyncSender<Request>>,
    status: Arc<Mutex<Option<SaveResult>>>,
    due: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}

impl Writer {
    pub(super) fn start(
        path: PathBuf,
        last: Option<Vec<u8>>,
        settings: InterfaceStorage,
        wake: Option<WakeSignal>,
    ) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let status = Arc::new(Mutex::new(None));
        let result = status.clone();
        let due = Arc::new(AtomicBool::new(true));
        let pending = due.clone();
        let task = thread::Builder::new()
            .name("workspace-storage".into())
            .spawn(move || {
                let mut storage = Storage::new(path, last, settings);
                let interval = Duration::from_secs(settings.snapshot_seconds);
                let mut next = Instant::now() + interval;
                loop {
                    match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
                        Ok(request) => {
                            let saved =
                                timestamp().and_then(|now| storage.save(&request.document, now));
                            *result.lock().unwrap() = Some(saved.clone());
                            if let Some(finished) = request.finished {
                                let _ = finished.send(saved);
                            }
                            if let Some(wake) = &wake {
                                wake.ring();
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            next = Instant::now() + interval;
                            pending.store(true, Ordering::Release);
                            if let Some(wake) = &wake {
                                wake.ring();
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            status,
            due,
            task: Some(task),
        })
    }

    pub(super) fn due(&self) -> bool {
        self.due.swap(false, Ordering::AcqRel)
    }

    pub(super) fn result(&self) -> Option<SaveResult> {
        self.status.lock().unwrap().take()
    }

    pub(super) fn save(&self, document: Document, flush: bool) -> SaveResult {
        let sender = self.sender.as_ref().unwrap();
        if flush {
            let (finished, receiver) = mpsc::sync_channel(1);
            sender
                .send(Request {
                    document,
                    finished: Some(finished),
                })
                .map_err(|_| "Workspace saving stopped".to_owned())?;
            receiver
                .recv()
                .map_err(|_| "Workspace saving stopped".to_owned())?
        } else {
            match sender.try_send(Request {
                document,
                finished: None,
            }) {
                Ok(()) => Ok(()),
                Err(mpsc::TrySendError::Full(_)) => Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => Err("Workspace saving stopped".into()),
            }
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

struct Storage {
    path: PathBuf,
    last: Option<Vec<u8>>,
    prune_pending: bool,
    settings: InterfaceStorage,
}

impl Storage {
    fn new(path: PathBuf, last: Option<Vec<u8>>, settings: InterfaceStorage) -> Self {
        Self {
            path,
            last,
            prune_pending: false,
            settings,
        }
    }

    fn save(&mut self, document: &Document, now: u64) -> SaveResult {
        let bytes = serde_json::to_vec_pretty(document)
            .map_err(|error| format!("Could not save workspaces: {error}"))?;
        if self.last.as_ref() != Some(&bytes) {
            self.write_snapshot(&bytes, now)
                .map_err(|error| format!("Could not save workspaces: {error}"))?;
            self.last = Some(bytes);
            self.prune_pending = true;
        }
        if self.prune_pending {
            self.prune().map_err(|error| {
                format!("Snapshot saved, but old snapshots could not be removed: {error}")
            })?;
            self.prune_pending = false;
        }
        Ok(())
    }

    fn write_snapshot(&self, bytes: &[u8], now: u64) -> io::Result<()> {
        let directory = self.path.with_extension("snapshots");
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => super::sync_parent(&directory)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !std::fs::symlink_metadata(&directory)?.file_type().is_dir() {
                    return Err(io::Error::other("snapshot path is not a directory"));
                }
            }
            Err(error) => return Err(error),
        }
        let files = snapshots(&directory)?;
        let newest = files.last().map(|path| sequence(path)).unwrap_or(0);
        let name = now.max(
            newest
                .checked_add(1)
                .ok_or_else(|| io::Error::other("snapshot sequence is full"))?,
        );
        write_atomic(&directory.join(format!("{name:020}.json")), bytes)
    }

    fn prune(&self) -> io::Result<()> {
        let directory = self.path.with_extension("snapshots");
        let files = snapshots(&directory)?;
        let mut buckets = std::collections::BTreeMap::new();
        for path in &files {
            if super::read_snapshot(path).is_ok_and(|document| document.is_some()) {
                buckets.insert(
                    sequence(path) / (self.settings.history_seconds * 1_000_000),
                    path,
                );
            }
        }
        let retained: std::collections::HashSet<_> = buckets
            .into_values()
            .rev()
            .take(self.settings.history_count + 1)
            .collect();
        let mut removed = false;
        for path in &files {
            if !retained.contains(path)
                && super::read_snapshot(path).is_ok_and(|document| document.is_some())
            {
                std::fs::remove_file(path)?;
                removed = true;
            }
        }
        if removed {
            super::sync_directory(&directory)?;
        }
        Ok(())
    }
}

fn timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_micros() as u64)
        .map_err(|error| format!("Could not date workspace snapshot: {error}"))
}

fn sequence(path: &Path) -> u64 {
    path.file_stem().unwrap().to_str().unwrap().parse().unwrap()
}

pub(super) fn load(path: &Path) -> io::Result<Option<Document>> {
    let directory = path.with_extension("snapshots");
    match std::fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return super::read_snapshot(path),
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::other("snapshot path is not a directory"));
        }
        Ok(_) => {}
    }
    let files = snapshots(&directory)?;
    for file in files.iter().rev() {
        if let Ok(Some(document)) = super::read_snapshot(file) {
            return Ok(Some(document));
        }
    }
    if files.is_empty() {
        super::read_snapshot(path)
    } else {
        Err(io::Error::other("no valid workspace snapshot"))
    }
}

pub(super) fn snapshots(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() == 25
            && name.ends_with(".json")
            && name.as_bytes()[..20].iter().all(u8::is_ascii_digit)
            && name[..20].parse::<u64>().is_ok()
            && entry.file_type()?.is_file()
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) mod tests {
    use super::*;

    fn document(name: &str) -> Document {
        let mut workspaces = super::super::Workspaces::default().entries;
        workspaces[0].name = name.into();
        Document {
            areas: Vec::new(),
            theme: Default::default(),
            active: 1,
            workspaces,
            sands: Vec::new(),
            records: Vec::new(),
        }
    }

    #[cfg_attr(test, test)]
    fn one_snapshot_stream_keeps_spaced_history_and_does_not_rewrite_unchanged_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let snapshot_dir = path.with_extension("snapshots");
        let settings = InterfaceStorage {
            history_count: 2,
            ..Default::default()
        };
        let mut storage = Storage::new(path.clone(), None, settings);
        for (name, seconds) in [
            ("First", 10),
            ("Second", 40),
            ("Third", 310),
            ("Fourth", 340),
            ("Fifth", 610),
        ] {
            storage.save(&document(name), seconds * 1_000_000).unwrap();
        }
        assert!(!path.exists());
        assert!(!path.with_extension("backups").exists());
        let files = snapshots(&snapshot_dir).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|path| {
                super::super::read_snapshot(path)
                    .unwrap()
                    .unwrap()
                    .workspaces[0]
                    .name
                    .clone()
            })
            .collect();
        assert_eq!(names, ["Second", "Fourth", "Fifth"]);
        storage.save(&document("Fifth"), 800_000_000).unwrap();
        assert_eq!(snapshots(&snapshot_dir).unwrap(), files);
        storage.save(&document("Sixth"), 910_000_000).unwrap();
        assert_eq!(snapshots(&snapshot_dir).unwrap().len(), 3);
        assert!(!files[0].exists());
        assert_eq!(load(&path).unwrap().unwrap().workspaces[0].name, "Sixth");
        let last = serde_json::to_vec_pretty(&load(&path).unwrap().unwrap()).unwrap();
        let files = snapshots(&snapshot_dir).unwrap();
        let mut restarted = Storage::new(path, Some(last), settings);
        restarted.save(&document("Sixth"), 920_000_000).unwrap();
        assert_eq!(snapshots(&snapshot_dir).unwrap(), files);
    }

    #[cfg_attr(test, test)]
    fn loading_recovers_from_a_broken_latest_snapshot_and_never_accepts_invalid_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let snapshot_dir = path.with_extension("snapshots");
        let mut storage = Storage::new(path.clone(), None, Default::default());
        storage.save(&document("Recover this"), 1_000_000).unwrap();
        std::fs::write(snapshot_dir.join("00000000000310000000.json"), b"broken").unwrap();
        std::fs::write(
            snapshot_dir.join("99999999999999999999.json"),
            b"invalid name",
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().workspaces[0].name,
            "Recover this"
        );
        let mut invalid = document("Invalid");
        invalid.workspaces.clear();
        std::fs::write(
            snapshot_dir.join("00000000000320000000.json"),
            serde_json::to_vec(&invalid).unwrap(),
        )
        .unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().workspaces[0].name,
            "Recover this"
        );
        std::fs::remove_file(snapshot_dir.join("00000000000001000000.json")).unwrap();
        assert!(load(&path).is_err());
    }

    #[cfg_attr(test, test)]
    fn timer_wakes_an_idle_interface_and_quit_flushes_a_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let (sender, receiver) = mpsc::channel();
        let writer = Writer::start(
            path.clone(),
            None,
            InterfaceStorage {
                snapshot_seconds: 1,
                ..Default::default()
            },
            Some(WakeSignal::new(move || {
                let _ = sender.send(());
            })),
        )
        .unwrap();
        assert!(writer.due());
        assert!(!writer.due());
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(writer.due());
        writer.save(document("Saved on quit"), true).unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().workspaces[0].name,
            "Saved on quit"
        );
        drop(writer);
        while receiver.try_recv().is_ok() {}
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(20)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
    }

    #[cfg(unix)]
    #[cfg_attr(test, test)]
    fn snapshot_storage_is_private_and_rejects_directory_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let snapshot_dir = path.with_extension("snapshots");
        let other = directory.path().join("other");
        std::fs::create_dir(&other).unwrap();
        symlink(&other, &snapshot_dir).unwrap();
        let mut storage = Storage::new(path.clone(), None, Default::default());
        assert!(storage.save(&document("Retained"), 1).is_err());
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read_dir(&other).unwrap().count(), 0);
        std::fs::remove_file(&snapshot_dir).unwrap();
        storage.save(&document("Retained"), 1).unwrap();
        let files = snapshots(&snapshot_dir).unwrap();
        assert_eq!(
            std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&snapshot_dir)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }

    crate::laboratory_cases! {
        one_snapshot_stream_keeps_spaced_history_and_does_not_rewrite_unchanged_state,
        loading_recovers_from_a_broken_latest_snapshot_and_never_accepts_invalid_state,
        timer_wakes_an_idle_interface_and_quit_flushes_a_snapshot,
        #[cfg(unix)]
        snapshot_storage_is_private_and_rejects_directory_symlinks,
    }
}
