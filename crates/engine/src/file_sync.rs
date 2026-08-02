//! File Sync (blueprint: Sync/CRDT — the disk-organ projection): the records
//! belonging to one organ mirror to/from a directory of markdown files, head
//! as the filename, body as the file's content. Restores the pre-refactor
//! `file_sync.rs` convention, ported onto the Action write path so a disk
//! edit pushes to every live-subscribed sand exactly like an app edit does
//! (`Action::EditRecordText` fires the same annotation fact the fact_bus
//! carries to Protein subscriptions — no separate push mechanism needed).
//!
//! Selection is deliberately NOT Protein-configurable yet (tracked as future
//! work in the living doc) — v1 hardcodes "every record whose `organ_uid` is
//! this organ" (`Predicate::OrganEq`), per the "simplest thing for now"
//! instruction.
//!
//! **Conflict rule: disk wins.** A tick applies disk-side changes to the
//! Ledger FIRST, then mirrors the (now-reconciled) Ledger state back to disk
//! — the same order the pre-refactor loop used (apply, then mirror).
//!
//! **Identity: tracked by uid, not by re-parsing the filename.** The
//! pre-refactor version matched a changed file back to a record by searching
//! for a record whose `head` equalled the file stem — broken by head
//! collisions and by sanitization changing the stem. `FileSyncState` instead
//! remembers path -> uid in memory, seeded by this Cell's own writes. A
//! renamed file is NOT identity-preserving (matches history): it reads as one
//! record disappearing and a new one appearing.
//!
//! **Deletion is debounced.** A path must be missing for
//! `MISSING_TICKS_BEFORE_DELETE` consecutive ticks before its record is HARD
//! deleted (`Action::DeleteRecord`) — a single missing tick could just be an
//! editor's atomic save (temp-write, rename over the original), and an
//! unattended poll loop doing an immediate irreversible tombstone on that is
//! too sharp a blast radius.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::Engine;
use crate::actions::Action;
use crate::error::EngineError;

const MARKDOWN_EXTENSION: &str = "md";
const MISSING_TICKS_BEFORE_DELETE: u32 = 2;

#[derive(Debug, Clone)]
struct KnownFile {
    uid: String,
    body: String,
    missing_ticks: u32,
}

/// Per-(dir, organ) sync memory a caller keeps across ticks — not persisted;
/// a fresh process re-seeds it (and its selected records' files) on the first
/// tick against an empty state.
#[derive(Debug, Default)]
pub struct FileSyncState {
    known: HashMap<PathBuf, KnownFile>,
}

impl FileSyncState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// What one `file_sync_tick` did — for callers/tests that want to observe it.
#[derive(Debug, Default, Clone)]
pub struct FileSyncReport {
    pub created: Vec<String>,
    pub updated_from_disk: Vec<String>,
    pub deleted: Vec<String>,
    pub written_to_disk: Vec<String>,
}

impl Engine {
    /// One disk-wins reconciliation pass for `organ_uid`'s records against
    /// `dir`. Safe to call repeatedly (e.g. on an interval) — `state` carries
    /// the path<->uid memory between calls.
    pub async fn file_sync_tick(
        &self,
        dir: &Path,
        organ_uid: &str,
        state: &mut FileSyncState,
    ) -> Result<FileSyncReport, EngineError> {
        std::fs::create_dir_all(dir).map_err(EngineError::Io)?;
        let mut report = FileSyncReport::default();
        let disk = scan_disk(dir)?;

        // --- 1) disk -> Ledger (disk wins) -------------------------------
        let mut to_edit = Vec::new();
        let mut to_delete = Vec::new();
        let mut pending_delete: HashSet<PathBuf> = HashSet::new();
        for (path, known) in state.known.iter_mut() {
            match disk.get(path) {
                None => {
                    known.missing_ticks += 1;
                    if known.missing_ticks >= MISSING_TICKS_BEFORE_DELETE {
                        to_delete.push((path.clone(), known.uid.clone()));
                    } else {
                        pending_delete.insert(path.clone());
                    }
                }
                Some(body) if *body != known.body => {
                    to_edit.push((path.clone(), known.uid.clone(), body.clone()));
                }
                Some(_) => known.missing_ticks = 0,
            }
        }
        for (path, uid, body) in to_edit {
            self.act(
                Action::EditRecordText {
                    target: uid.clone(),
                    head: None,
                    body: Some(body.clone()),
                },
                None,
            )
            .await?;
            report.updated_from_disk.push(uid.clone());
            if let Some(entry) = state.known.get_mut(&path) {
                entry.body = body;
                entry.missing_ticks = 0;
            }
        }
        for (path, uid) in to_delete {
            self.act(
                Action::DeleteRecord {
                    target: uid.clone(),
                },
                None,
            )
            .await?;
            report.deleted.push(uid);
            state.known.remove(&path);
        }

        // New files (never-seen paths) become new records.
        for (path, body) in &disk {
            if state.known.contains_key(path) {
                continue;
            }
            let head = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let outcome = self
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head,
                        body: body.clone(),
                        quantity: 0.0,
                    },
                    None,
                )
                .await?;
            let Some(uid) = outcome.created else {
                continue;
            };
            // `store::records::create` stamps the LOCAL organ; re-stamp if
            // this sync targets a different (e.g. remote-contact) organ.
            if let Some(row) = store::records::get(&self.store.pool, &uid).await? {
                if row.organ_uid.as_deref() != Some(organ_uid) {
                    store::records::set_organ_origin(&self.store.pool, &uid, Some(organ_uid))
                        .await?;
                }
            }
            report.created.push(uid.clone());
            state.known.insert(
                path.clone(),
                KnownFile {
                    uid,
                    body: body.clone(),
                    missing_ticks: 0,
                },
            );
        }

        // --- 2) Ledger -> disk (mirror) -----------------------------------
        let protein = protein::Protein {
            source: protein::Source::Record,
            filter: vec![protein::Predicate::OrganEq(organ_uid.to_string())],
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let matched = protein::matching_records(&self.store, &protein, None).await?;
        let desired = desired_paths(dir, &matched);

        // Stray .md files no longer selected (deactivated elsewhere, re-homed
        // to another organ, deleted, ...) — never a path we're debouncing.
        for entry in std::fs::read_dir(dir).map_err(EngineError::Io)? {
            let entry = entry.map_err(EngineError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some(MARKDOWN_EXTENSION)
                && !desired.contains_key(&path)
            {
                std::fs::remove_file(&path).map_err(EngineError::Io)?;
            }
        }
        // Drop stale tracked paths that no longer correspond to a selected
        // record (head-rename moved it to a new path, or it fell out of
        // selection) — unless we're mid-debounce on it.
        state
            .known
            .retain(|path, _| desired.contains_key(path) || pending_delete.contains(path));

        for (path, (uid, body)) in &desired {
            if pending_delete.contains(path) {
                continue; // leave it absent — don't fight the debounce
            }
            let up_to_date = state
                .known
                .get(path)
                .is_some_and(|known| &known.body == body);
            if up_to_date {
                continue;
            }
            std::fs::write(path, body).map_err(EngineError::Io)?;
            report.written_to_disk.push(uid.clone());
            state.known.insert(
                path.clone(),
                KnownFile {
                    uid: uid.clone(),
                    body: body.clone(),
                    missing_ticks: 0,
                },
            );
        }

        Ok(report)
    }
}

fn scan_disk(dir: &Path) -> Result<HashMap<PathBuf, String>, EngineError> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir).map_err(EngineError::Io)? {
        let entry = entry.map_err(EngineError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(MARKDOWN_EXTENSION) {
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let body = std::fs::read_to_string(&path).map_err(EngineError::Io)?;
        out.insert(path, body);
    }
    Ok(out)
}

/// The path + current body every currently-selected record wants, keyed by
/// its desired filename — collisions (same sanitized head) get a `-- {uid}`
/// disambiguating suffix, matching the pre-refactor convention (which used
/// ` -- {id}`).
fn desired_paths(
    dir: &Path,
    matched: &[store::records::RecordRow],
) -> HashMap<PathBuf, (String, String)> {
    let mut stem_counts: HashMap<String, usize> = HashMap::new();
    for r in matched {
        *stem_counts.entry(file_stem(&r.head, &r.uid)).or_default() += 1;
    }
    let mut out = HashMap::new();
    for r in matched {
        let mut stem = file_stem(&r.head, &r.uid);
        if stem_counts.get(&stem).copied().unwrap_or(0) > 1 {
            stem = format!("{stem} -- {}", r.uid);
        }
        let path = dir.join(format!("{stem}.{MARKDOWN_EXTENSION}"));
        out.insert(path, (r.uid.clone(), r.body.clone()));
    }
    out
}

fn file_stem(head: &str, uid: &str) -> String {
    let sanitized = sanitize_file_stem(head);
    if sanitized.is_empty() {
        format!("record-{uid}")
    } else {
        sanitized
    }
}

fn sanitize_file_stem(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_space = false;
    for ch in value.chars() {
        let next = match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            ch if ch.is_control() => ' ',
            ch => ch,
        };
        if next.is_whitespace() {
            if !last_was_space {
                output.push(' ');
            }
            last_was_space = true;
        } else {
            output.push(next);
            last_was_space = false;
        }
    }
    output.trim_matches([' ', '.']).to_string()
}

/// Spawn a continuous watch loop for `organ_uid`'s file sync at `dir`,
/// ticking every `interval`. Errors on a tick (transient IO/DB trouble) are
/// swallowed and retried next tick — a background sync loop should not die
/// from one bad pass.
pub fn spawn_watch(
    engine: std::sync::Arc<Engine>,
    dir: PathBuf,
    organ_uid: String,
    interval: std::time::Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut state = FileSyncState::new();
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            let _ = engine.file_sync_tick(&dir, &organ_uid, &mut state).await;
        }
    })
}

/// How often a boot-spawned watch loop reconciles disk against the Ledger.
pub const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// Boot-time wiring (the "simplest thing for now" v1): read every `kind=organ`
/// record's `lince.file_sync` extension once at startup and, for each one
/// with `{ enabled: true, path: "..." }`, spawn its watch loop. A tick
/// immediately reconciles disk against whatever's already selected for that
/// organ, so the very first tick after boot writes out the current records —
/// there's no separate "initial dump" step. Kept for tests / callers that
/// only want a one-shot seed; `spawn_supervisor` below is the live-reactive
/// version wired at boot.
pub async fn spawn_configured_watchers(
    engine: std::sync::Arc<Engine>,
) -> Result<Vec<tokio::task::JoinHandle<()>>, EngineError> {
    let protein = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::KindEq("organ".to_string())],
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let organs = protein::matching_records(&engine.store, &protein, None).await?;
    let mut handles = Vec::new();
    for organ in organs {
        let Some(config) =
            store::records::get_extension(&engine.store.pool, &organ.uid, "lince.file_sync")
                .await?
        else {
            continue;
        };
        let enabled = config
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if !enabled || path.trim().is_empty() {
            continue;
        }
        handles.push(spawn_watch(
            engine.clone(),
            PathBuf::from(path),
            organ.uid.clone(),
            DEFAULT_INTERVAL,
        ));
    }
    Ok(handles)
}

/// What `lince.file_sync` wants for one organ right now — `None` means
/// disabled/unset/no path.
fn desired_watch(config: Option<serde_json::Value>) -> Option<PathBuf> {
    let config = config?;
    let enabled = config
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("");
    (enabled && !path.trim().is_empty()).then(|| PathBuf::from(path))
}

/// Start/stop `organ_uid`'s watch loop to match its current `lince.file_sync`
/// extension, diffing against what the supervisor already has running.
async fn reconcile_one(
    engine: &std::sync::Arc<Engine>,
    organ_uid: &str,
    running: &mut HashMap<String, (PathBuf, tokio::task::JoinHandle<()>)>,
) -> Result<(), EngineError> {
    let config =
        store::records::get_extension(&engine.store.pool, organ_uid, "lince.file_sync").await?;
    let desired = desired_watch(config);
    match (desired, running.get(organ_uid)) {
        (Some(path), Some((current, _))) if *current == path => {}
        (Some(path), existing) => {
            if let Some((_, handle)) = existing {
                handle.abort();
            }
            let handle = spawn_watch(
                engine.clone(),
                path.clone(),
                organ_uid.to_string(),
                DEFAULT_INTERVAL,
            );
            running.insert(organ_uid.to_string(), (path, handle));
        }
        (None, Some(_)) => {
            if let Some((_, handle)) = running.remove(organ_uid) {
                handle.abort();
            }
        }
        (None, None) => {}
    }
    Ok(())
}

/// Reconcile every `kind=organ` record's watch loop against its current
/// `lince.file_sync` extension — the seed pass, and the resync used after a
/// missed fact_bus event (broadcast lag).
async fn reconcile_all(
    engine: &std::sync::Arc<Engine>,
    running: &mut HashMap<String, (PathBuf, tokio::task::JoinHandle<()>)>,
) -> Result<(), EngineError> {
    let protein = protein::Protein {
        source: protein::Source::Record,
        filter: vec![protein::Predicate::KindEq("organ".to_string())],
        include: Default::default(),
        aggregate: None,
        order: vec![],
        limit: None,
    };
    let organs = protein::matching_records(&engine.store, &protein, None).await?;
    let seen: HashSet<String> = organs.iter().map(|o| o.uid.clone()).collect();
    for organ in &organs {
        reconcile_one(engine, &organ.uid, running).await?;
    }
    let vanished: Vec<String> = running
        .keys()
        .filter(|uid| !seen.contains(*uid))
        .cloned()
        .collect();
    for uid in vanished {
        if let Some((_, handle)) = running.remove(&uid) {
            handle.abort();
        }
    }
    Ok(())
}

/// A `SetExtension` on `lince.file_sync` fires an annotation fact whose
/// payload is `{"extension": "lince.file_sync"}` (`Action::SetExtension` in
/// `actions.rs`) — cheap enough to string-match without parsing JSON.
fn fact_may_touch_file_sync(fact: &nucleus::Fact) -> bool {
    fact.payload
        .as_deref()
        .is_some_and(|p| p.contains("lince.file_sync"))
}

/// Live-reactive replacement for a one-shot boot call: seeds every enabled
/// organ's watch loop, then listens on the fact bus and starts/stops loops
/// as `lince.file_sync` extensions change — toggling File Sync from the
/// Organ sand takes effect on the spot, no reboot required. A broadcast lag
/// (slow consumer, full channel) triggers a full resync rather than trusting
/// partial state.
pub fn spawn_supervisor(engine: std::sync::Arc<Engine>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut running: HashMap<String, (PathBuf, tokio::task::JoinHandle<()>)> = HashMap::new();
        let _ = reconcile_all(&engine, &mut running).await;
        let mut bus = engine.subscribe();
        loop {
            match bus.recv().await {
                Ok(fact) => {
                    let should_reconcile =
                        fact_may_touch_file_sync(&fact) || running.contains_key(&fact.record_uid);
                    if should_reconcile {
                        let _ = reconcile_one(&engine, &fact.record_uid, &mut running).await;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let _ = reconcile_all(&engine, &mut running).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
