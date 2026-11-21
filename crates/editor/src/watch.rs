// non-lean-yet
// Scope: mod polling, struct Changes, mod tests, struct Watch.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::missing_panics_doc] State the panic conditions for exported functions. Sites
//   118, 171.
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 118.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 74, 146, 162,
//   175, 176.
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites 69,
//   80, 155, 156.
// - [Clippy: clippy::expect_used] Check the stated invariant and handle reachable failures. Sites
//   97, 161, 172.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 76, 118.
// - [Clippy: clippy::needless_collect] avoid using `collect()` when not needed. Sites 123, 134.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 12, 56.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 12, 13,
//   14, 15, 56, 66, 68, 72, 76, 118, 171.

use crate::Result;
mod polling;

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Default)]
pub struct Changes {
    pub paths: BTreeSet<PathBuf>,
    pub rescan: bool,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn polling_fallback_reports_changes_and_releases_unused_paths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("file.txt");
        std::fs::write(&path, "before").unwrap();
        let mut watch = Watch::new(|| {}).unwrap();
        watch.watcher = None;
        watch
            .set(BTreeSet::from([directory.path().to_path_buf()]))
            .unwrap();
        assert_eq!(watch.fallback.len(), 1);
        let until = std::time::Instant::now() + Duration::from_secs(3);
        while !watch.drain().rescan {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::write(&path, "different size after").unwrap();
        watch.polling.as_ref().unwrap().set(watch.fallback.clone());
        let until = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let changes = watch.drain();
            if changes.rescan || changes.paths.contains(&path) {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        watch.set(BTreeSet::new()).unwrap();
        assert!(watch.watched.is_empty());
        assert!(watch.fallback.is_empty());
    }
}

pub struct Watch {
    watcher: Option<RecommendedWatcher>,
    polling: Option<polling::Polling>,
    watched: BTreeSet<PathBuf>,
    fallback: BTreeSet<PathBuf>,
    changes: Arc<Mutex<Changes>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Watch {
    pub fn event_driven(&self) -> bool {
        self.watcher.is_some()
    }

    pub fn pending(&self) -> Arc<Mutex<Changes>> {
        self.changes.clone()
    }

    pub fn use_polling(&mut self) {
        self.watcher = None;
        self.watched = self.fallback.clone();
    }
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let watcher =
            notify::recommended_watcher(Self::handler(changes.clone(), wake.clone())).ok();
        let result = Self {
            watcher,
            polling: None,
            watched: BTreeSet::new(),
            fallback: BTreeSet::new(),
            changes,
            wake,
        };
        Ok(result)
    }

    fn handler(
        pending: Arc<Mutex<Changes>>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> impl FnMut(notify::Result<Event>) + Send + 'static {
        move |event: notify::Result<Event>| {
            let mut pending = pending.lock().expect("watch events");
            match event {
                Ok(event) if matches!(event.kind, EventKind::Access(_)) => return,
                Ok(event) => {
                    pending.rescan |= event.need_rescan();
                    if pending.paths.len() + event.paths.len() > 512 {
                        pending.rescan = true;
                    } else {
                        pending.paths.extend(event.paths);
                    }
                }
                Err(error) => {
                    pending.error = Some(error.to_string());
                    pending.rescan = true;
                }
            }
            drop(pending);
            wake();
        }
    }

    pub fn set(&mut self, wanted: BTreeSet<PathBuf>) -> Result<()> {
        let mut limited = false;
        for path in self
            .watched
            .difference(&wanted)
            .cloned()
            .collect::<Vec<_>>()
        {
            if self.fallback.remove(&path) {
            } else if let Some(watcher) = &mut self.watcher {
                let _ = watcher.unwatch(&path);
            }
            self.watched.remove(&path);
        }
        for path in wanted
            .difference(&self.watched)
            .cloned()
            .collect::<Vec<_>>()
        {
            if self
                .watcher
                .as_mut()
                .is_none_or(|watcher| watcher.watch(&path, RecursiveMode::NonRecursive).is_err())
            {
                if self.fallback.len() >= 128 {
                    limited = true;
                    continue;
                }
                self.fallback.insert(path.clone());
            }
            self.watched.insert(path);
        }
        if self.fallback.is_empty() {
            self.polling = None;
        } else {
            if self.polling.is_none() {
                self.polling = Some(polling::Polling::new(
                    self.changes.clone(),
                    self.wake.clone(),
                )?);
            }
            self.polling
                .as_ref()
                .expect("fallback paths have a polling watcher")
                .set(self.fallback.clone());
        }
        if limited {
            Err("Polling is limited to 128 directories; collapse unused directories".into())
        } else {
            Ok(())
        }
    }

    pub fn drain(&mut self) -> Changes {
        let mut changes = std::mem::take(&mut *self.changes.lock().expect("watch events"));
        if changes.error.is_some() && self.watcher.is_some() {
            self.watcher = None;
            let wanted = self.watched.clone();
            self.watched = self.fallback.clone();
            changes.error = self.set(wanted).err();
        }
        changes
    }
}
