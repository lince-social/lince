// non-lean-yet
// Scope: type SaveResult, struct Request, struct QueueState, struct Queue.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 47, 115, 161,
//   163, 165, 167, 203, 557, 578, 600, 631, 650, 688.
// - [review: §5 Errors] Check discarded error causes and preserve useful context. Sites 131.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (195, 202,
//   238, 251).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (550, 565, 569, 574, 575, 582, 583, 584, 585, 587, 588, 589, 591, 592, 597, 601,
//   602, 607, 609, 616, 618, 620, 623, 629, 631, 632, 636, 637, 639, 646, 660, 663, 665, 667, 682,
//   686, 687, 691, 692, 693, 694, 696, 701).
// - [Clippy probe: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably
//   fires. Sites 464.

use super::{Document, write_atomic};
use crate::{
    native::wake::WakeSignal,
    workspace_save_queue::{ReservationMode, SaveQueue, Slot},
};
use cell::InterfaceStorage;
use std::{
    ffi::OsStr,
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

struct WorkerQueue(Arc<SaveQueue<Request>>);
impl Drop for WorkerQueue {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub(super) struct Capture {
    slot: Slot<Request>,
    flush: bool,
}

impl Capture {
    pub(super) fn save(self, document: Document) -> SaveResult {
        let (finished, completion) = if self.flush {
            let (sender, receiver) = mpsc::sync_channel(1);
            (Some(sender), Some(receiver))
        } else {
            (None, None)
        };
        self.slot
            .send(Request { document, finished })
            .map_err(|error| error.to_string())?;
        if let Some(completion) = completion {
            let _wait = nucleus::performance::Span::new("workspace-save-flush-await");
            completion
                .recv()
                .map_err(|_| "Workspace saving stopped".to_owned())?
        } else {
            Ok(())
        }
    }
}

pub(super) struct Writer {
    queue: Arc<SaveQueue<Request>>,
    status: Arc<Mutex<Option<SaveResult>>>,
    due: Arc<AtomicBool>,
    saved_revision: Arc<Mutex<Option<(String, u64)>>>,
    task: Option<thread::JoinHandle<()>>,
}

impl Writer {
    pub(super) fn start(
        path: PathBuf,
        last: Option<Vec<u8>>,
        settings: InterfaceStorage,
        wake: Option<WakeSignal>,
    ) -> io::Result<Self> {
        let (queue, receiver) = SaveQueue::<Request>::channel();
        let worker_queue = queue.clone();
        let status = Arc::new(Mutex::new(None));
        let result = status.clone();
        let due = Arc::new(AtomicBool::new(true));
        let pending = due.clone();
        let saved_revision = Arc::new(Mutex::new(None));
        let revision = saved_revision.clone();
        let task = thread::Builder::new()
            .name("workspace-storage".into())
            .spawn(move || {
                let _lifetime = WorkerQueue(worker_queue);
                let mut storage = Storage::new(path, last, settings);
                let interval = Duration::from_secs(settings.snapshot_seconds);
                let mut next = Instant::now() + interval;
                loop {
                    match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
                        Ok(queued) => {
                            let Request {
                                mut document,
                                finished,
                            } = queued.start();
                            let saved_canvas_revision = document
                                .canvas_state
                                .as_ref()
                                .map_or(0, |state| state.snapshot.revision);
                            if let Some(state) = &mut document.canvas_state {
                                state.mark_saved_through(saved_canvas_revision);
                            }
                            let saved = timestamp().and_then(|now| storage.save(&document, now));
                            if saved.is_ok() {
                                *revision
                                    .lock()
                                    .expect("the saved revision lock was poisoned") = document
                                    .canvas_id
                                    .take()
                                    .map(|id| (id, saved_canvas_revision));
                            }
                            *result
                                .lock()
                                .expect("the workspace save result lock was poisoned") =
                                Some(saved.clone());
                            if let Some(finished) = finished {
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
            queue,
            status,
            due,
            saved_revision,
            task: Some(task),
        })
    }

    pub(super) fn request_save(&self) {
        self.due.store(true, Ordering::Release);
    }

    pub(super) fn saved_revision(&self, canvas: &str) -> u64 {
        self.saved_revision
            .lock()
            .expect("the saved revision lock was poisoned")
            .as_ref()
            .filter(|(id, _)| id == canvas)
            .map_or(0, |(_, revision)| *revision)
    }

    pub(super) fn due(&self) -> bool {
        self.due.swap(false, Ordering::AcqRel)
    }

    pub(super) fn result(&self) -> Option<SaveResult> {
        self.status
            .lock()
            .expect("the workspace save result lock was poisoned")
            .take()
    }

    pub(super) fn prepare(&self, flush: bool) -> Result<Option<Capture>, String> {
        let _admission = nucleus::performance::Span::new("workspace-save-admission-await");
        let mode = if flush {
            ReservationMode::Wait
        } else {
            ReservationMode::Defer
        };
        match self
            .queue
            .reserve(mode)
            .map_err(|error| error.to_string())?
        {
            Some(slot) => {
                nucleus::performance::add("workspace-captures-admitted", 1);
                Ok(Some(Capture { slot, flush }))
            }
            None => {
                nucleus::performance::add("workspace-captures-deferred", 1);
                self.request_save();
                Ok(None)
            }
        }
    }

    pub(super) fn save(&self, document: Document, flush: bool) -> SaveResult {
        match self.prepare(flush)? {
            Some(capture) => capture.save(document),
            None => Ok(()),
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.queue.close();
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
        let _save = nucleus::performance::Span::new("workspace-storage-save");
        let invalid = {
            let _validate = nucleus::performance::Span::new("workspace-storage-validation");
            document.validation_error()
        };
        if let Some(error) = invalid {
            return Err(format!("Could not save workspaces: {error}"));
        }
        let bytes = {
            let _encode = nucleus::performance::Span::new("workspace-storage-encoding");
            serde_json::to_vec_pretty(document)
                .map_err(|error| format!("Could not save workspaces: {error}"))?
        };
        if self.last.as_ref() != Some(&bytes) {
            let _commit = nucleus::performance::Span::new("workspace-storage-commit");
            self.write_snapshot(&bytes, now)
                .map_err(|error| format!("Could not save workspaces: {error}"))?;
            self.last = Some(bytes);
            self.prune_pending = true;
        }
        if self.prune_pending {
            let _prune = nucleus::performance::Span::new("workspace-storage-prune");
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
        let newest = files
            .last()
            .and_then(|path| sequence(path.file_name()?))
            .unwrap_or(0);
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
        let retained = {
            let _selection = nucleus::performance::Span::new("workspace-history-selection");
            let mut retained = std::collections::HashSet::new();
            let mut retained_bucket = None;
            for path in files.iter().rev() {
                let Some(sequence) = path.file_name().and_then(sequence) else {
                    continue;
                };
                let bucket = sequence / (self.settings.history_seconds * 1_000_000);
                if retained_bucket == Some(bucket) {
                    continue;
                }
                nucleus::performance::add("workspace-history-retention-reads", 1);
                if super::read_snapshot(path).is_ok_and(|document| document.is_some()) {
                    retained.insert(path);
                    retained_bucket = Some(bucket);
                    if retained.len() == self.settings.history_count + 1 {
                        break;
                    }
                }
            }
            retained
        };
        let mut removed = false;
        for path in &files {
            if retained.contains(path) {
                continue;
            }
            nucleus::performance::add("workspace-history-retirement-reads", 1);
            if super::read_snapshot(path).is_ok_and(|document| document.is_some()) {
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

fn sequence(name: &OsStr) -> Option<u64> {
    let digits = name.to_str()?.strip_suffix(".json")?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub(super) fn load(path: &Path) -> io::Result<Option<Document>> {
    let directory = path.with_extension("snapshots");
    match std::fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return super::recovery::read(path);
        }
        Err(error) => return Err(error),
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::other("snapshot path is not a directory"));
        }
        Ok(_) => {}
    }
    let files = snapshots(&directory)?;
    for (skipped, file) in files.iter().rev().enumerate() {
        if let Ok(Some(mut document)) = super::recovery::read(file) {
            if skipped > 0 {
                document.recovery.skipped_snapshots(skipped);
            }
            return Ok(Some(document));
        }
    }
    if files.is_empty() {
        super::recovery::read(path)
    } else {
        Err(io::Error::other("no valid workspace snapshot"))
    }
}

pub(super) fn snapshots(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if sequence(&entry.file_name()).is_some() && entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

#[allow(
    clippy::unwrap_used,
    reason = "Runnable Laboratory fixtures must fail if their temporary snapshots cannot be written or read"
)]
pub(crate) mod tests {
    use super::*;

    fn document(name: &str) -> Document {
        let mut workspaces = super::super::Workspaces::default().entries;
        workspaces[0].name = name.into();
        Document {
            workspace_focus: Vec::new(),
            physical_workspaces: Vec::new(),
            shader_editors: Vec::new(),
            shader_settings: Default::default(),
            terrain_effects: Vec::new(),
            deleted_workspaces: Default::default(),
            trash_retention_days: super::super::default_trash_retention_days(),
            canvas_id: None,
            hidden_records: Default::default(),
            canvas_state: None,
            recovery: Default::default(),
            snapshot_error: None,
            assertions: Vec::new(),
            shaders: Vec::new(),
            layouts: Vec::new(),
            imports: Vec::new(),
            areas: Vec::new(),
            proteins: Vec::new(),
            karma_castles: Vec::new(),
            simulations: Vec::new(),
            frequency_castles: Vec::new(),
            transfer_castles: Vec::new(),
            recorders: Vec::new(),
            documents: Vec::new(),
            source_views: Vec::new(),
            demeters: Vec::new(),
            media: Vec::new(),
            drawings: Vec::new(),
            extension_sands: Vec::new(),
            editors: Vec::new(),
            explorers: Vec::new(),
            calendars: Vec::new(),
            maps: Vec::new(),
            kanbans: Vec::new(),
            instincts: Vec::new(),
            theme: Default::default(),
            controls: Default::default(),
            active: 1,
            workspaces,
            sands: Vec::new(),
            records: Vec::new(),
        }
    }

    #[test]
    fn physical_surface_configuration_survives_storage_and_recovers_invalid_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let mut saved = document("Physical workstation");
        let configuration = crate::native::physical_workspace::replay_configuration(1);
        saved.physical_workspaces.push(configuration.clone());
        let mut storage = Storage::new(path.clone(), None, Default::default());
        storage.save(&saved, 1_000_000).unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().physical_workspaces,
            vec![configuration.clone()]
        );
        let mut damaged = serde_json::to_value(&saved).unwrap();
        let mut invalid = serde_json::to_value(&configuration).unwrap();
        invalid["configuration"]["surfaces"][0]["size_meters"][0] = 0.into();
        damaged["physical_workspaces"]
            .as_array_mut()
            .unwrap()
            .push(invalid);
        let snapshot = path
            .with_extension("snapshots")
            .join("00000000000002000000.json");
        std::fs::write(snapshot, serde_json::to_vec(&damaged).unwrap()).unwrap();
        let recovered = load(&path).unwrap().unwrap();
        assert_eq!(recovered.physical_workspaces, vec![configuration]);
        assert_eq!(recovered.workspaces[0].name, "Physical workstation");
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
    fn storage_worker_rejects_invalid_state_and_preserves_the_last_durable_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("interface.json");
        let writer = Writer::start(path.clone(), None, Default::default(), None).unwrap();
        writer.save(document("Durable"), true).unwrap();
        let mut invalid = document("Invalid");
        invalid.workspaces.clear();
        assert!(writer.save(invalid, true).is_err());
        assert_eq!(load(&path).unwrap().unwrap().workspaces[0].name, "Durable");
        writer.save(document("Next valid state"), true).unwrap();
        assert_eq!(
            load(&path).unwrap().unwrap().workspaces[0].name,
            "Next valid state"
        );
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

    crate::native::laboratory_cases! {
        storage_worker_rejects_invalid_state_and_preserves_the_last_durable_snapshot,
        one_snapshot_stream_keeps_spaced_history_and_does_not_rewrite_unchanged_state,
        loading_recovers_from_a_broken_latest_snapshot_and_never_accepts_invalid_state,
        timer_wakes_an_idle_interface_and_quit_flushes_a_snapshot,
        #[cfg(unix)]
        snapshot_storage_is_private_and_rejects_directory_symlinks,
    }
}
