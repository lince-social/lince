use super::*;
use std::{
    collections::BTreeMap,
    sync::Condvar,
    time::{Duration, Instant, SystemTime},
};

#[derive(Default)]
struct State {
    paths: BTreeSet<PathBuf>,
    changed: bool,
    stopped: bool,
}

pub(super) struct Polling {
    state: Arc<(Mutex<State>, Condvar)>,
}

#[derive(PartialEq, Eq)]
struct Fingerprint {
    length: u64,
    modified: Option<SystemTime>,
    identity: u64,
}

fn fingerprint(path: &std::path::Path) -> Option<Fingerprint> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        metadata.ino()
    };
    #[cfg(not(unix))]
    let identity = 0;
    Some(Fingerprint {
        length: metadata.len(),
        modified: metadata.modified().ok(),
        identity,
    })
}

impl Polling {
    pub(super) fn new(
        changes: Arc<Mutex<Changes>>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self> {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let control = state.clone();
        std::thread::Builder::new()
            .name("editor-poll".into())
            .spawn(move || {
                let mut previous: BTreeMap<PathBuf, Fingerprint> = BTreeMap::new();
                let mut roots = BTreeSet::new();
                loop {
                    let (lock, signal) = &*control;
                    let guard = lock.lock().expect("poll state");
                    let (mut guard, _) = signal
                        .wait_timeout_while(guard, Duration::from_secs(5), |state| {
                            !state.changed && !state.stopped
                        })
                        .expect("poll wake");
                    if guard.stopped {
                        break;
                    }
                    guard.changed = false;
                    let paths = guard.paths.clone();
                    drop(guard);
                    let mut current = BTreeMap::new();
                    let mut dirty = BTreeSet::new();
                    let began = Instant::now();
                    for root in &paths {
                        if let Some(value) = fingerprint(root) {
                            current.insert(root.clone(), value);
                        }
                        if let Ok(entries) = std::fs::read_dir(root) {
                            for entry in entries {
                                if current.len() >= 20_000
                                    || began.elapsed() >= Duration::from_millis(250)
                                {
                                    dirty.insert(root.clone());
                                    break;
                                }
                                if let Ok(entry) = entry {
                                    let path = entry.path();
                                    if let Some(value) = fingerprint(&path) {
                                        current.insert(path, value);
                                    }
                                }
                            }
                        }
                    }
                    for (path, value) in &current {
                        if previous.get(path) != Some(value) {
                            dirty.insert(path.clone());
                        }
                    }
                    for path in previous.keys() {
                        if !current.contains_key(path)
                            && paths.iter().any(|root| path.starts_with(root))
                        {
                            dirty.insert(path.clone());
                        }
                    }
                    let rescan = roots != paths || dirty.len() > 512;
                    roots = paths;
                    previous = current;
                    if rescan || !dirty.is_empty() {
                        let mut pending = changes.lock().expect("poll events");
                        pending.rescan |= rescan || pending.paths.len() + dirty.len() > 512;
                        if !pending.rescan {
                            pending.paths.extend(dirty);
                        }
                        drop(pending);
                        wake();
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { state })
    }

    pub(super) fn set(&self, paths: BTreeSet<PathBuf>) {
        let mut state = self.state.0.lock().expect("poll state");
        state.paths = paths;
        state.changed = true;
        self.state.1.notify_one();
    }
}

impl Drop for Polling {
    fn drop(&mut self) {
        self.state.0.lock().expect("poll state").stopped = true;
        self.state.1.notify_one();
    }
}
