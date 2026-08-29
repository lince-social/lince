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
//! remembers path -> uid in memory, seeded by this Cell's own writes and, on a
//! fresh process, from the files its Records already have on disk. A `.lingua`
//! file carries the uid, so RENAMING one renames the Record rather than reading
//! as one Record disappearing and another appearing — with the Cell running or
//! while it was closed. A rename is only a rename while the Record's own file
//! is gone; a second file claiming a live uid is still refused.
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

/// Which file shape a folder is mirrored in.
///
/// **Several may be configured at once, in PRECEDENCE order**, so one Record
/// can have both a plain `.md` to read and a `.lingua` carrying everything
/// Markdown cannot express. Each format owns what it can say: a Markdown edit
/// can only mean a body edit, because a Markdown file holds nothing else.
/// When two files for one Record changed between ticks and disagree about the
/// body, the earlier format in the list wins — which is why the setting is an
/// ordered list rather than a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileFormat {
    /// `head.md`, body only. What every existing folder is.
    #[default]
    Markdown,
    /// `head.lingua` — the Record's Lingua state above its body.
    Lingua,
}

impl FileFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => MARKDOWN_EXTENSION,
            Self::Lingua => crate::lingua_file::LINGUA_EXTENSION,
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "markdown" | "md" => Some(Self::Markdown),
            "lingua" => Some(Self::Lingua),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct KnownFile {
    uid: String,
    /// The whole file as this Cell last saw it, so any disk change is visible.
    body: String,
    /// What was between the delimiters when we last wrote it. Tracked apart
    /// from the body because the two halves mean different things, and
    /// because it is what a later tick DIFFS a hand edit against: the lines
    /// a person added become asserts and the ones they deleted become
    /// retracts, so a change made elsewhere between two ticks is never
    /// reverted by a file that said nothing about it.
    prelude: Option<String>,
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
    /// Files this tick deliberately did NOT act on, and why.
    ///
    /// A conflict never destroys what a person typed: the file is left exactly
    /// as they saved it and the Record is left exactly as it was. Silence here
    /// would mean regenerating over their edit, which is the one outcome a
    /// sync loop must never produce unattended.
    pub conflicts: Vec<FileConflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileConflict {
    pub path: String,
    pub reason: String,
}

impl Engine {
    /// One disk-wins reconciliation pass for `organ_uid`'s records against
    /// `dir`. Safe to call repeatedly (e.g. on an interval) — `state` carries
    /// the path<->uid memory between calls.
    ///
    /// **Several formats may be configured at once, in PRECEDENCE order.** One
    /// Record then has one file per format, and each format owns what it can
    /// express: Markdown carries the body and nothing else, so a Markdown edit
    /// can only ever mean a body edit; `.lingua` carries the body plus the
    /// Record's Lingua state. When two files for the same Record changed in
    /// the same tick and disagree about the body, the earlier format in the
    /// list wins — which is the whole reason the setting is a list and not a
    /// set.
    pub async fn file_sync_tick(
        &self,
        dir: &Path,
        organ_uid: &str,
        state: &mut FileSyncState,
    ) -> Result<FileSyncReport, EngineError> {
        std::fs::create_dir_all(dir).map_err(EngineError::Io)?;
        let mut report = FileSyncReport::default();
        let config =
            store::records::get_extension(&self.store.pool, organ_uid, "lince.file_sync").await?;
        let formats = configured_formats(config.as_ref());
        let mut disk = HashMap::new();
        for format in &formats {
            disk.extend(scan_disk(dir, format.extension())?);
        }
        // Every file this Organ's Records already have on disk, ignoring the
        // filter. Two jobs, and both are about not destroying text: it is what
        // lets a file written by an earlier run be recognised instead of read
        // as new (`reattach_written_files`), and it is how the sweep below
        // tells a Record that is GONE from one that is merely not selected.
        let mut written_before: HashMap<PathBuf, DesiredFile> = HashMap::new();
        {
            let all = self.selected_records(organ_uid, None).await?;
            for format in &formats {
                written_before.extend(self.desired_files(dir, &all, *format).await?);
            }
        }
        self.reattach_written_files(&written_before, &disk, state);
        // Paths whose metadata block a person edited in a way this Cell cannot
        // apply. Skipped by the mirror half below so the edit survives.
        let mut conflicted: HashSet<PathBuf> = HashSet::new();

        // --- 1) disk -> Ledger (disk wins) -------------------------------
        //
        // Grouped by RECORD rather than by file, because with several formats
        // a Record has several files and the questions "was it edited" and
        // "was it deleted" are about the Record, not about any one of them.
        let mut by_uid: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for (path, known) in state.known.iter() {
            by_uid
                .entry(known.uid.clone())
                .or_default()
                .push(path.clone());
        }
        for paths in by_uid.values_mut() {
            paths.sort_by_key(|path| format_rank(&formats, path));
        }

        let mut to_delete: Vec<(String, Vec<PathBuf>)> = Vec::new();
        let mut pending_delete: HashSet<PathBuf> = HashSet::new();
        let mut to_edit: Vec<(String, Vec<PathBuf>)> = Vec::new();
        for (uid, paths) in &by_uid {
            let missing: Vec<&PathBuf> = paths.iter().filter(|p| !disk.contains_key(*p)).collect();
            let changed: Vec<PathBuf> = paths
                .iter()
                .filter(|p| {
                    disk.get(*p)
                        .zip(state.known.get(*p))
                        .is_some_and(|(text, known)| *text != known.body)
                })
                .cloned()
                .collect();
            if !missing.is_empty() {
                for path in &missing {
                    if let Some(known) = state.known.get_mut(*path) {
                        known.missing_ticks += 1;
                    }
                }
                // **A Record dies only when ALL of its files are gone.**
                // Deleting the `.md` while the `.lingua` stays is not a
                // deletion — it is one projection removed, and the mirror half
                // puts it back. Treating any single missing file as a deletion
                // would make tidying a folder destroy Records.
                let all_gone = paths.iter().all(|path| {
                    state
                        .known
                        .get(path)
                        .is_some_and(|k| k.missing_ticks >= MISSING_TICKS_BEFORE_DELETE)
                });
                if all_gone {
                    to_delete.push((uid.clone(), paths.clone()));
                    continue;
                }
                // Debounce only while the count is still climbing. Once a file
                // has been gone that long and the Record did NOT die — because
                // its other projections are still there — it is simply a
                // missing projection, and the mirror half below puts it back.
                for path in missing {
                    let still_debouncing = state
                        .known
                        .get(path)
                        .is_some_and(|k| k.missing_ticks < MISSING_TICKS_BEFORE_DELETE);
                    if still_debouncing {
                        pending_delete.insert(path.clone());
                    }
                }
            } else {
                for path in paths {
                    if let Some(known) = state.known.get_mut(path) {
                        known.missing_ticks = 0;
                    }
                }
            }
            if !changed.is_empty() {
                to_edit.push((uid.clone(), changed));
            }
        }

        for (uid, paths) in to_edit {
            self.apply_file_edits(
                &uid,
                &paths,
                &disk,
                &formats,
                state,
                &mut report,
                &mut conflicted,
            )
            .await?;
        }
        for (uid, paths) in to_delete {
            self.act(
                Action::DeleteRecord {
                    target: uid.clone(),
                },
                None,
            )
            .await?;
            report.deleted.push(uid);
            for path in paths {
                state.known.remove(&path);
            }
        }

        // New files (never-seen paths) become new records. Grouped by STEM, so
        // a `.lingua` and a `.md` dropped in together are ONE Record with two
        // projections rather than two Records that happen to share a name.
        let mut fresh: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for path in disk.keys() {
            if state.known.contains_key(path) {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            fresh.entry(stem).or_default().push(path.clone());
        }
        // Uids the files in this batch CLAIM for themselves, gathered before
        // any of them is adopted. A folder written by hand cross-links by uid,
        // so at the moment one file is validated the Record its link names
        // does not exist yet — but the file that will become it is right
        // there. Without this, a pair of files that reference each other
        // refuses in both directions forever.
        let mut declared: HashSet<String> = HashSet::new();
        for path in fresh.values().flatten() {
            if format_of(&formats, path) != Some(FileFormat::Lingua) {
                continue;
            }
            if let Ok((Some(projection), _)) = crate::lingua_file::parse_file(&disk[path]) {
                if !projection.uid.trim().is_empty() {
                    declared.insert(projection.uid.trim().to_string());
                }
            }
        }
        // Applied only after EVERY Record in the batch exists, for the same
        // reason: asserting a link needs its object to be there.
        let mut pending: Vec<(String, crate::lingua_file::Projection, PathBuf)> = Vec::new();

        let mut stems: Vec<&String> = fresh.keys().collect();
        stems.sort();
        for stem in stems {
            let mut paths = fresh[stem].clone();
            paths.sort_by_key(|path| format_rank(&formats, path));
            let Some(source) = paths.first().cloned() else {
                continue;
            };
            let text = disk[&source].clone();
            let format = format_of(&formats, &source).unwrap_or(FileFormat::Markdown);
            let (projection, body) = match format {
                FileFormat::Markdown => (None, text.clone()),
                FileFormat::Lingua => match crate::lingua_file::parse_file(&text) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        report.conflicts.push(FileConflict {
                            path: source.display().to_string(),
                            reason: err.to_string(),
                        });
                        conflicted.insert(source.clone());
                        continue;
                    }
                },
            };
            // Everything the metadata names must resolve BEFORE the Record
            // exists. Validating afterwards would leave a bare Record behind
            // whenever a concept turned out to be unknown — a half-applied
            // file is worse than a refused one, because nobody goes looking
            // for it.
            if let Some(projection) = &projection {
                if let Err(reason) = self.check_projection(projection, &declared).await? {
                    report.conflicts.push(FileConflict {
                        path: source.display().to_string(),
                        reason,
                    });
                    conflicted.insert(source.clone());
                    continue;
                }
            }
            // A file may carry the uid of the Record it is going to BECOME.
            // That is what lets a folder of files cross-link before any of
            // them has been adopted: a link is `[[Title|uid]]` and a link
            // without a uid is refused, so the uids have to be agreed in
            // advance or the folder is only valid after a second pass.
            let given_uid = projection
                .as_ref()
                .map(|p| p.uid.trim())
                .filter(|uid| !uid.is_empty());
            // **A file that carries a uid we already have is that Record under
            // a new filename**, never a second Record asking for a taken
            // identifier. The uid is the identity; the filename is a
            // projection of the head. Without this, renaming a file — or
            // renaming it while the Cell was closed — read as one Record
            // appearing and another going missing: a refused adoption, then a
            // debounced delete of the Record the file was still describing.
            let existing = match given_uid {
                Some(given) => store::records::get(&self.store.pool, given).await?,
                None => None,
            };
            // Only when the Record's OWN file is gone. If it is still sitting
            // there, a second file claiming its uid is a collision, not a
            // rename — two files cannot both be one Record, and adopting
            // either way round is the one mistake that cannot be undone.
            let still_on_disk = |uid: &str| {
                written_before
                    .iter()
                    .any(|(path, desired)| desired.uid == uid && disk.contains_key(path))
            };
            if let Some(row) = existing.filter(|row| {
                row.organ_uid.as_deref() == Some(organ_uid) && !still_on_disk(&row.uid)
            }) {
                if row.head != *stem {
                    self.act(
                        Action::EditRecordText {
                            target: row.uid.clone(),
                            head: Some(stem.clone()),
                            body: None,
                        },
                        None,
                    )
                    .await?;
                    report.updated_from_disk.push(row.uid.clone());
                }
                // The path it used to live at is not a deletion — it is the
                // same file, moved. Dropping it here stops the debounce that
                // would otherwise delete the Record two ticks from now.
                let remembered = written_before
                    .values()
                    .find(|desired| desired.uid == row.uid)
                    .cloned();
                state.known.retain(|_, known| known.uid != row.uid);
                for path in &paths {
                    state.known.insert(
                        path.clone(),
                        KnownFile {
                            uid: row.uid.clone(),
                            // What the RECORD says, not what the file says: any
                            // edit made in the same breath as the rename is
                            // then an ordinary disk change, applied by the next
                            // tick under the same disk-wins rule.
                            body: remembered
                                .as_ref()
                                .map(|d| d.text.clone())
                                .unwrap_or_default(),
                            prelude: remembered.as_ref().and_then(|d| d.prelude.clone()),
                            missing_ticks: 0,
                        },
                    );
                }
                continue;
            }
            let uid = if let Some(given) = given_uid {
                match store::records::create_with_uid(
                    &self.store.pool,
                    store::records::NewRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: &stem,
                        body: &body,
                        quantity: store::exact::zero(),
                    },
                    given,
                )
                .await
                {
                    Ok(row) => row.uid,
                    // Malformed, or already taken by something else. Never
                    // adopt and never overwrite: two different things under
                    // one identifier is the failure that cannot be undone.
                    Err(err) => {
                        report.conflicts.push(FileConflict {
                            path: source.display().to_string(),
                            reason: format!("{err}"),
                        });
                        conflicted.insert(source.clone());
                        continue;
                    }
                }
            } else {
                let outcome = self
                    .act(
                        Action::CreateRecord {
                            slug: None,
                            kind: nucleus::RecordKind::Plain,
                            head: stem.clone(),
                            body: body.clone(),
                            quantity: 0.0,
                        },
                        None,
                    )
                    .await?;
                let Some(uid) = outcome.created else {
                    continue;
                };
                uid
            };
            if let Some(projection) = &projection {
                pending.push((uid.clone(), projection.clone(), source.clone()));
            }
            // `store::records::create` stamps the LOCAL organ; re-stamp if
            // this sync targets a different (e.g. remote-contact) organ.
            if let Some(row) = store::records::get(&self.store.pool, &uid).await? {
                if row.organ_uid.as_deref() != Some(organ_uid) {
                    store::records::set_organ_origin(&self.store.pool, &uid, Some(organ_uid))
                        .await?;
                }
            }
            report.created.push(uid.clone());
            for path in paths {
                let prelude = if path == source {
                    projection.as_ref().map(render_prelude_only)
                } else {
                    None
                };
                let body = disk.get(&path).cloned().unwrap_or_default();
                state.known.insert(
                    path.clone(),
                    KnownFile {
                        uid: uid.clone(),
                        body,
                        prelude,
                        missing_ticks: 0,
                    },
                );
            }
        }

        for (uid, projection, source) in pending {
            self.apply_projection(&uid, &projection).await?;
            self.apply_level(&uid, &projection, &source, &mut report)
                .await?;
        }

        // --- 2) Ledger -> disk (mirror) -----------------------------------
        let matched = self.selected_records(organ_uid, config.as_ref()).await?;
        // Re-read AFTER the disk half: a file adopted a moment ago is a Record
        // this Organ owns, and the sweep below has to see it as one. Read
        // before, it would sweep the very file it had just adopted.
        let owned: HashSet<PathBuf> = {
            let all = self.selected_records(organ_uid, None).await?;
            let mut paths = HashSet::new();
            for format in &formats {
                paths.extend(desired_paths(dir, &all, format.extension()).into_keys());
            }
            paths
        };
        let mut desired = HashMap::new();
        for format in &formats {
            desired.extend(self.desired_files(dir, &matched, *format).await?);
        }

        // Stray files no longer selected (deactivated elsewhere, re-homed to
        // another organ, deleted, ...) — never a path we're debouncing, and
        // never one we refused.
        //
        // Only CONFIGURED extensions are swept. Files left by a format that is
        // no longer listed are reported rather than deleted: they are a
        // person's text, and a setting change is not consent to remove it.
        for entry in std::fs::read_dir(dir).map_err(EngineError::Io)? {
            let entry = entry.map_err(EngineError::Io)?;
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if conflicted.contains(&path) {
                // A file we refused is not a stray. It corresponds to no
                // Record precisely BECAUSE we refused it, and sweeping it here
                // would delete the text somebody wrote as the direct result of
                // reporting a problem with it.
                continue;
            }
            let configured = formats.iter().any(|f| ext == Some(f.extension()));
            if configured && !desired.contains_key(&path) && owned.contains(&path) {
                // The Record is still here — the FILTER is what excludes it,
                // and a filter is a choice about what to mirror, never consent
                // to delete what somebody wrote. This is how a new note used
                // to disappear: dropping a plain file in adopted it as a
                // Record with no concepts, the filter did not select it, and
                // the same tick swept the file it had just read.
                report.conflicts.push(FileConflict {
                    path: path.display().to_string(),
                    reason: "this Record is not selected by this folder's filter — kept rather \
                             than deleted"
                        .to_string(),
                });
            } else if configured && !desired.contains_key(&path) {
                std::fs::remove_file(&path).map_err(EngineError::Io)?;
            } else if !configured
                && path.is_file()
                && matches!(
                    ext,
                    Some(MARKDOWN_EXTENSION) | Some(crate::lingua_file::LINGUA_EXTENSION)
                )
            {
                report.conflicts.push(FileConflict {
                    path: path.display().to_string(),
                    reason: "written in a format this folder no longer syncs — kept rather than \
                             deleted"
                        .to_string(),
                });
            }
        }
        // Drop stale tracked paths that no longer correspond to a selected
        // record (head-rename moved it to a new path, or it fell out of
        // selection) — unless we're mid-debounce on it.
        // A file kept because the filter excludes its Record stays REMEMBERED,
        // or the next tick would read it as new and adopt the same text as a
        // second Record — one duplicate per tick, forever.
        state.known.retain(|path, _| {
            desired.contains_key(path) || pending_delete.contains(path) || owned.contains(path)
        });

        for (path, desired) in &desired {
            if pending_delete.contains(path) {
                continue; // leave it absent — don't fight the debounce
            }
            if conflicted.contains(path) {
                continue; // leave what they typed alone until it is resolved
            }
            // Present AND matching. Remembering the content is not enough:
            // the file may have been deleted since, and a Record whose other
            // projections survive still wants this one on disk.
            let up_to_date = disk.contains_key(path)
                && state
                    .known
                    .get(path)
                    .is_some_and(|known| known.body == desired.text);
            if up_to_date {
                continue;
            }
            std::fs::write(path, &desired.text).map_err(EngineError::Io)?;
            report.written_to_disk.push(desired.uid.clone());
            state.known.insert(
                path.clone(),
                KnownFile {
                    uid: desired.uid.clone(),
                    body: desired.text.clone(),
                    prelude: desired.prelude.clone(),
                    missing_ticks: 0,
                },
            );
        }

        // The panel reads this. A tick that resolved everything must CLEAR it,
        // not skip the write — a conflict list that only ever grows would keep
        // reporting a file the person already fixed.
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .insert(organ_uid.to_string(), report.conflicts.clone());
        Ok(report)
    }

    /// Apply this tick's edits for ONE Record, across however many of its
    /// files changed.
    ///
    /// `paths` arrives in precedence order. The body is taken from the first
    /// one, so when a `.lingua` and a `.md` were both edited between ticks the
    /// configured order decides rather than the filesystem's iteration order.
    #[allow(clippy::too_many_arguments)]
    async fn apply_file_edits(
        &self,
        uid: &str,
        paths: &[PathBuf],
        disk: &HashMap<PathBuf, String>,
        formats: &[FileFormat],
        state: &mut FileSyncState,
        report: &mut FileSyncReport,
        conflicted: &mut HashSet<PathBuf>,
    ) -> Result<(), EngineError> {
        let mut body: Option<String> = None;
        for path in paths {
            let Some(text) = disk.get(path) else { continue };
            let format = format_of(formats, path).unwrap_or(FileFormat::Markdown);
            let (projection, file_body) = match format {
                FileFormat::Markdown => (None, text.clone()),
                FileFormat::Lingua => match crate::lingua_file::parse_file(text) {
                    Ok(parsed) => parsed,
                    Err(err) => {
                        report.conflicts.push(FileConflict {
                            path: path.display().to_string(),
                            reason: err.to_string(),
                        });
                        conflicted.insert(path.clone());
                        continue;
                    }
                },
            };
            let Some(projection) = projection else {
                // Markdown: a body edit is all it can be.
                if body.is_none() {
                    body = Some(file_body);
                }
                if let Some(entry) = state.known.get_mut(path) {
                    entry.body = text.clone();
                    entry.missing_ticks = 0;
                }
                continue;
            };
            let known_prelude = state.known.get(path).and_then(|k| k.prelude.clone());
            let disk_prelude = render_prelude_only(&projection);
            if Some(&disk_prelude) != known_prelude.as_ref() {
                // The LEVEL is writable from disk; the rest of the metadata is
                // not yet. So compare the block with the level lines removed:
                // if that matches, the only thing they touched is a number
                // this Cell knows how to turn into a Ledger Fact.
                let only_level = known_prelude
                    .as_deref()
                    .is_some_and(|known| strip_level(known) == strip_level(&disk_prelude));
                if !only_level {
                    if let Err(reason) = self
                        .apply_prelude_edit(uid, &projection, known_prelude.as_deref())
                        .await?
                    {
                        // Refused — and therefore this file contributes
                        // NOTHING, its body included. Taking the body from a
                        // file we are refusing would apply half of one edit,
                        // which is the shape of change nobody can reason
                        // about afterwards.
                        report.conflicts.push(FileConflict {
                            path: path.display().to_string(),
                            reason,
                        });
                        conflicted.insert(path.clone());
                        continue;
                    }
                }
                self.apply_level(uid, &projection, path, report).await?;
            }
            if body.is_none() {
                body = Some(file_body);
            }
            if let Some(entry) = state.known.get_mut(path) {
                entry.body = text.clone();
                entry.prelude = Some(disk_prelude);
                entry.missing_ticks = 0;
            }
        }
        if let Some(body) = body {
            self.act(
                Action::EditRecordText {
                    target: uid.to_string(),
                    head: None,
                    body: Some(body),
                },
                None,
            )
            .await?;
            report.updated_from_disk.push(uid.to_string());
        }
        Ok(())
    }

    /// Turn a hand edit of a `.lingua` metadata block into real assertions.
    ///
    /// The block is a PROJECTION of Lingua state, so editing it is retracting
    /// and asserting on somebody's behalf from a text file. Three rules make
    /// that safe enough to run unattended:
    ///
    /// - **Nothing is invented.** Every Concept, unit and link is resolved
    ///   BEFORE anything is written, by the same check the creation path uses,
    ///   and a block naming one unknown thing applies in full or not at all.
    ///   A half-applied block is exactly the state nobody can reason about
    ///   afterwards, and the reason says WHICH line failed.
    /// - **The comparison is against what LINCE LAST WROTE**, not against the
    ///   Record as it stands now. That is what makes this a diff of INTENT:
    ///   lines a person added become asserts, lines they deleted become
    ///   retracts, and lines they left alone are not touched at all — so a
    ///   change made elsewhere between two ticks is not silently reverted by
    ///   a file that never mentioned it.
    /// - **A changed quantity is a retract AND an assert.**
    ///   `assertions::assert` is idempotent on `(subject, predicate, object)`
    ///   and returns the existing row untouched, so asserting `@chapter 3`
    ///   over `@chapter 1` would otherwise keep the 1 and report success.
    ///
    /// `Ok(Err(reason))` is a refusal to report, not a failure.
    async fn apply_prelude_edit(
        &self,
        uid: &str,
        disk: &crate::lingua_file::Projection,
        known_prelude: Option<&str>,
    ) -> Result<Result<(), String>, EngineError> {
        // An EDIT has no batch to point into: everything a Record already
        // managed links to has to exist by now.
        if let Err(reason) = self.check_projection(disk, &HashSet::new()).await? {
            return Ok(Err(reason));
        }
        let known = match known_prelude {
            Some(text) => match crate::lingua_file::parse_prelude(text) {
                Ok(projection) => projection,
                // We wrote this block and cannot read it back. Refusing is the
                // only honest answer: without it there is no way to tell an
                // added line from an unchanged one, and every assertion would
                // look new.
                Err(err) => {
                    return Ok(Err(format!(
                        "Lince could not read back the block it wrote for this file ({err}), so \
                         it cannot tell which lines you changed"
                    )));
                }
            },
            None => crate::lingua_file::Projection::default(),
        };

        // What identifies an assertion is its tuple; the quantity and unit are
        // what it SAYS, and a change to those is a different statement about
        // the same tuple.
        fn tuple(line: &crate::lingua_file::Line) -> (String, Option<String>) {
            (
                line.predicate.clone(),
                line.object.as_ref().map(|link| link.uid.clone()),
            )
        }
        fn said(line: &crate::lingua_file::Line) -> (Option<String>, Option<String>) {
            (line.quantity.clone(), line.unit.clone())
        }

        let mut retract: Vec<&crate::lingua_file::Line> = Vec::new();
        let mut assert: Vec<&crate::lingua_file::Line> = Vec::new();
        for line in &disk.assertions {
            match known.assertions.iter().find(|k| tuple(k) == tuple(line)) {
                Some(existing) if said(existing) == said(line) => {}
                Some(existing) => {
                    retract.push(existing);
                    assert.push(line);
                }
                None => assert.push(line),
            }
        }
        for line in &known.assertions {
            if !disk.assertions.iter().any(|d| tuple(d) == tuple(line)) {
                retract.push(line);
            }
        }

        let identity_now = disk
            .assertions
            .iter()
            .find(|l| l.identity)
            .map(|l| l.predicate.clone());
        let identity_was = known
            .assertions
            .iter()
            .find(|l| l.identity)
            .map(|l| l.predicate.clone());

        // Clear the identity first when it is moving. An identity assertion
        // must be unary and unquantified, and retracting the assertion that
        // currently IS the identity while it still holds that role leaves the
        // Record pointing at a retracted row.
        if identity_now != identity_was {
            self.act(
                Action::SetIdentity {
                    subject: uid.to_string(),
                    predicate: None,
                },
                None,
            )
            .await?;
        }
        for line in retract {
            self.act(
                Action::RetractRecord {
                    subject: uid.to_string(),
                    predicate: line.predicate.clone(),
                    object: line.object.as_ref().map(|link| link.uid.clone()),
                },
                None,
            )
            .await?;
        }
        for line in assert {
            self.act(
                Action::AssertRecord {
                    subject: uid.to_string(),
                    predicate: line.predicate.clone(),
                    object: line.object.as_ref().map(|link| link.uid.clone()),
                    quantity: line.quantity.clone(),
                    unit: line.unit.clone(),
                },
                None,
            )
            .await?;
        }
        if identity_now != identity_was {
            if let Some(predicate) = identity_now {
                self.act(
                    Action::SetIdentity {
                        subject: uid.to_string(),
                        predicate: Some(predicate),
                    },
                    None,
                )
                .await?;
            }
        }
        Ok(Ok(()))
    }

    /// Make the file's `quantity:` line true in the database.
    ///
    /// A Record's level is a FOLD of Ledger Facts, not a column, so this
    /// cannot assign — it appends the exact difference between what the file
    /// says and what the fold currently holds, as an ordinary user-caused
    /// Fact. The Ledger keeps its property (every level is the sum of its
    /// history, and that history is auditable) while the file behaves the way
    /// a person expects a file to behave: you write 12, it is 12.
    ///
    /// Exact all the way through — the typed text is parsed straight to a
    /// decimal, so `3.50` never becomes a float on its way to a signed Fact.
    async fn apply_level(
        &self,
        uid: &str,
        projection: &crate::lingua_file::Projection,
        path: &Path,
        report: &mut FileSyncReport,
    ) -> Result<(), EngineError> {
        let Some((amount, unit)) = &projection.quantity else {
            return Ok(());
        };
        let Ok(target) = nucleus::DecimalValue::parse_inferred(amount.trim()) else {
            report.conflicts.push(FileConflict {
                path: path.display().to_string(),
                reason: format!(
                    "`{amount}` is not an exact decimal amount, so the level was left \
                                 as it was"
                ),
            });
            return Ok(());
        };
        if let Some(unit) = unit {
            match store::concepts::resolve(&self.store.pool, unit).await? {
                Some(_) => {
                    self.act(
                        Action::SetUnit {
                            target: uid.to_string(),
                            unit: Some(unit.clone()),
                        },
                        None,
                    )
                    .await?;
                }
                None => {
                    report.conflicts.push(FileConflict {
                        path: path.display().to_string(),
                        reason: format!(
                            "the unit @{unit} is not a Concept here, so the level kept \
                                         the unit it had"
                        ),
                    });
                }
            }
        }
        self.act(
            Action::SetQuantityExact {
                target: uid.to_string(),
                amount: target.to_string(),
            },
            None,
        )
        .await?;
        Ok(())
    }

    /// What the last tick for `organ_uid` refused to act on. Empty when the
    /// folder is clean — and equally empty when nothing has ticked yet, which
    /// the surface has to say rather than showing an all-clear.
    pub fn file_sync_conflicts(&self, organ_uid: &str) -> Vec<FileConflict> {
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .get(organ_uid)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether this Organ's folder has been reconciled at least once in this
    /// process. Distinguishes "nothing wrong" from "nothing known".
    pub fn file_sync_has_ticked(&self, organ_uid: &str) -> bool {
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .contains_key(organ_uid)
    }
}

/// Where a path's format sits in the configured precedence order. Unlisted
/// formats sort last so they can never win a body.
fn format_rank(formats: &[FileFormat], path: &Path) -> usize {
    format_of(formats, path)
        .and_then(|f| formats.iter().position(|c| *c == f))
        .unwrap_or(usize::MAX)
}

fn format_of(formats: &[FileFormat], path: &Path) -> Option<FileFormat> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    formats.iter().copied().find(|f| f.extension() == ext)
}

/// A prelude with its level lines removed, for asking "did they change
/// anything BUT the level".
fn strip_level(prelude: &str) -> String {
    prelude
        .lines()
        .filter(|line| !line.trim_start().starts_with("quantity:"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What one Record wants on disk.
#[derive(Clone)]
struct DesiredFile {
    uid: String,
    /// The whole file, prelude included.
    text: String,
    /// The prelude alone, so a later tick can tell a metadata edit from a body
    /// edit without re-deriving it.
    prelude: Option<String>,
}

impl Engine {
    /// The Records this folder mirrors: `organ_uid` AND whatever else the
    /// owner configured.
    ///
    /// Origin is not negotiable and is not part of the configurable half:
    /// mirroring a Record whose origin is somebody else's Organ would put
    /// their writing in this owner's folder, where editing the file edits
    /// THEIR Record. So the extra filter can only narrow.
    async fn selected_records(
        &self,
        organ_uid: &str,
        config: Option<&serde_json::Value>,
    ) -> Result<Vec<store::records::RecordRow>, EngineError> {
        let mut filter = vec![protein::Predicate::OrganEq(organ_uid.to_string())];
        if let Some(extra) = configured_filter(config) {
            filter.push(extra);
        }
        let protein = protein::Protein {
            source: protein::Source::Record,
            filter,
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let mut matched = protein::matching_records(&self.store, &protein, None).await?;
        // Identity is not content. The Organ and Cell Records became visible
        // here the moment origins were stamped on every row (the Organ's
        // origin is itself, the Cell's is its Organ), and mirroring them to
        // disk would put "Local Lince.md" and "this cell.md" in the owner's
        // notes folder — where renaming or deleting the file would edit the
        // identity. Excluded by slug, which is fixed for both.
        matched.retain(|row| {
            !matches!(
                row.slug.as_deref(),
                Some(store::organs::LOCAL_ORGAN_SLUG) | Some(store::cells::LOCAL_CELL_SLUG)
            )
        });
        Ok(matched)
    }

    /// Recognise files this Cell wrote in an EARLIER RUN.
    ///
    /// The path<->uid memory lives in `FileSyncState`, which is per-process,
    /// while the folder is on disk and outlives it. Without this, every file
    /// is unknown after a restart and the disk half reads the whole folder as
    /// new: each file asks to create a Record under a uid that already exists,
    /// is refused, and — because a refused path is left alone by the mirror
    /// too — the folder sits in a loop of one conflict per file per tick with
    /// nothing syncing in either direction. Editing a note while the Cell was
    /// closed did nothing at all.
    ///
    /// Seeded with what the RECORD currently says, so the ordinary changed-
    /// check that follows sees exactly the edits made while nothing was
    /// watching, and applies them under the same disk-wins rule as any edit
    /// made with the Cell running. A file that matches no selected Record's
    /// path is untouched here and still becomes a new Record below.
    fn reattach_written_files(
        &self,
        owned: &HashMap<PathBuf, DesiredFile>,
        disk: &HashMap<PathBuf, String>,
        state: &mut FileSyncState,
    ) {
        for (path, desired) in owned {
            if !disk.contains_key(path) || state.known.contains_key(path) {
                continue;
            }
            state.known.insert(
                path.clone(),
                KnownFile {
                    uid: desired.uid.clone(),
                    body: desired.text.clone(),
                    prelude: desired.prelude.clone(),
                    missing_ticks: 0,
                },
            );
        }
    }

    /// The file every selected Record wants, keyed by path.
    ///
    /// Batched on purpose: this runs on an interval over every selected
    /// Record, so a per-Record assertion query would turn a projection into an
    /// N+1 walk of the whole store.
    async fn desired_files(
        &self,
        dir: &Path,
        matched: &[store::records::RecordRow],
        format: FileFormat,
    ) -> Result<HashMap<PathBuf, DesiredFile>, EngineError> {
        let paths = desired_paths(dir, matched, format.extension());
        if format == FileFormat::Markdown {
            return Ok(paths
                .into_iter()
                .map(|(path, (uid, body))| {
                    (
                        path,
                        DesiredFile {
                            uid,
                            text: body,
                            prelude: None,
                        },
                    )
                })
                .collect());
        }

        let uids: Vec<String> = matched.iter().map(|r| r.uid.clone()).collect();
        let assertions = store::assertions::for_subjects(&self.store.pool, &uids).await?;
        let concept_names: HashMap<String, String> = store::concepts::list_all(&self.store.pool)
            .await?
            .into_iter()
            .map(|c| (c.uid, c.canonical_name))
            .collect();
        // Titles for whatever the links point at — including Records outside
        // this folder's selection, which are exactly the interesting ones.
        let mut object_uids: Vec<String> = assertions
            .iter()
            .filter_map(|a| a.object_uid.clone())
            .collect();
        object_uids.sort();
        object_uids.dedup();
        let heads = store::records::heads_for(&self.store.pool, &object_uids).await?;

        let mut by_subject: HashMap<&str, Vec<&store::assertions::ProjectedAssertion>> =
            HashMap::new();
        for row in &assertions {
            by_subject
                .entry(row.subject_uid.as_str())
                .or_default()
                .push(row);
        }

        let mut out = HashMap::new();
        for (path, (uid, body)) in paths {
            let Some(record) = matched.iter().find(|r| r.uid == uid) else {
                continue;
            };
            let lines = by_subject
                .get(uid.as_str())
                .map(Vec::as_slice)
                .unwrap_or(&[])
                .iter()
                .map(|row| crate::lingua_file::Line {
                    predicate: concept_names
                        .get(&row.predicate_uid)
                        .cloned()
                        .unwrap_or_else(|| row.predicate.clone()),
                    identity: record.identity_predicate_uid.as_deref()
                        == Some(row.predicate_uid.as_str()),
                    object: row
                        .object_uid
                        .as_ref()
                        .map(|object| crate::lingua_file::Link {
                            title: heads.get(object).cloned().unwrap_or_default(),
                            uid: object.clone(),
                        }),
                    quantity: row.quantity.map(crate::lingua_file::decimal_text),
                    unit: row
                        .unit_uid
                        .as_ref()
                        .and_then(|unit| concept_names.get(unit).cloned()),
                })
                .collect();
            let quantity = {
                let text = crate::lingua_file::decimal_text(record.quantity);
                // A zero level is the default for every Record that never
                // carried one. Printing it on all of them would put a number
                // nobody set into every file.
                if text == "0" {
                    None
                } else {
                    Some((
                        text,
                        record
                            .unit_uid
                            .as_ref()
                            .and_then(|unit| concept_names.get(unit).cloned()),
                    ))
                }
            };
            let projection = crate::lingua_file::Projection {
                uid: uid.clone(),
                assertions: lines,
                quantity,
            };
            out.insert(
                path,
                DesiredFile {
                    uid,
                    text: crate::lingua_file::render_file(&projection, &body),
                    prelude: Some(render_prelude_only(&projection)),
                },
            );
        }
        Ok(out)
    }

    /// Everything a hand-written prelude names, resolved — or the reason it
    /// cannot be. `Ok(Err(reason))` is a refusal to report, not a failure.
    /// `pending` names Records that do not exist YET but will before this tick
    /// applies anything — the uids other files in the same batch claimed for
    /// themselves. A link may point into that set; it still may not point at
    /// nothing.
    async fn check_projection(
        &self,
        projection: &crate::lingua_file::Projection,
        pending: &HashSet<String>,
    ) -> Result<Result<(), String>, EngineError> {
        for line in &projection.assertions {
            if store::concepts::resolve(&self.store.pool, &line.predicate)
                .await?
                .is_none()
            {
                return Ok(Err(format!(
                    "@{} is not a Concept here — nothing was created, because a file must never \
                     invent meanings",
                    line.predicate
                )));
            }
            if let Some(unit) = &line.unit {
                if store::concepts::resolve(&self.store.pool, unit)
                    .await?
                    .is_none()
                {
                    return Ok(Err(format!("the unit @{unit} is not a Concept here")));
                }
            }
            if let Some(link) = &line.object {
                if !pending.contains(&link.uid)
                    && store::records::get(&self.store.pool, &link.uid)
                        .await?
                        .is_none()
                {
                    return Ok(Err(format!(
                        "[[{}|{}]] points at no Record here — a link is its uid, never its title, \
                         so nothing was retargeted by name",
                        link.title, link.uid
                    )));
                }
            }
        }
        Ok(Ok(()))
    }

    /// Apply an already-validated prelude to a freshly created Record.
    async fn apply_projection(
        &self,
        uid: &str,
        projection: &crate::lingua_file::Projection,
    ) -> Result<(), EngineError> {
        for line in &projection.assertions {
            self.act(
                Action::AssertRecord {
                    subject: uid.to_string(),
                    predicate: line.predicate.clone(),
                    object: line.object.as_ref().map(|link| link.uid.clone()),
                    quantity: line.quantity.clone(),
                    unit: line.unit.clone(),
                },
                None,
            )
            .await?;
            if line.identity {
                self.act(
                    Action::SetIdentity {
                        subject: uid.to_string(),
                        predicate: Some(line.predicate.clone()),
                    },
                    None,
                )
                .await?;
            }
        }
        Ok(())
    }
}

/// The prelude alone, for comparing what is on disk against what we wrote.
fn render_prelude_only(projection: &crate::lingua_file::Projection) -> String {
    let whole = crate::lingua_file::render_file(projection, "");
    let (prelude, _) = crate::lingua_file::split(&whole);
    prelude.unwrap_or_default().to_string()
}

/// Which shapes this Organ's folder is mirrored in, in PRECEDENCE order.
///
/// Accepts `"format": "lingua"` (one) or `"formats": ["lingua", "markdown"]`
/// (several — the first wins a contested body). An unreadable or absent value
/// means Markdown alone: the shape every existing folder already has, so a typo
/// can never silently rewrite a whole notes directory into another format.
/// Duplicates are dropped rather than refused, because listing a format twice
/// says nothing a single mention does not.
fn configured_formats(config: Option<&serde_json::Value>) -> Vec<FileFormat> {
    let mut out: Vec<FileFormat> = Vec::new();
    if let Some(list) = config
        .and_then(|value| value.get("formats"))
        .and_then(|value| value.as_array())
    {
        for value in list {
            if let Some(format) = value.as_str().and_then(FileFormat::parse) {
                if !out.contains(&format) {
                    out.push(format);
                }
            }
        }
    }
    if let Some(format) = config
        .and_then(|value| value.get("format"))
        .and_then(|value| value.as_str())
        .and_then(FileFormat::parse)
    {
        if !out.contains(&format) {
            out.push(format);
        }
    }
    if out.is_empty() {
        out.push(FileFormat::Markdown);
    }
    out
}

fn scan_disk(dir: &Path, extension: &str) -> Result<HashMap<PathBuf, String>, EngineError> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir(dir).map_err(EngineError::Io)? {
        let entry = entry.map_err(EngineError::Io)?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(extension) {
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
    extension: &str,
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
        let path = dir.join(format!("{stem}.{extension}"));
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
        fields: None,
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

/// The owner's extra selection filter, or `None` for "everything from this
/// Organ" (Ontology §12, C5 — the File Sync half of one selector language).
///
/// It is a `Predicate` in the SAME vocabulary every other filter uses, stored
/// as JSON under `lince.file_sync.filter`. Not a bespoke mini-language: the
/// whole point of the cluster is that there is one selector, so a Concept tag,
/// a Record kind, a text match or an `All`/`Any`/`Not` of them all work here
/// because they work everywhere.
///
/// A filter that will not parse is IGNORED — everything from the Organ syncs —
/// rather than failing closed and silently mirroring nothing. Same choice as
/// the unreadable contact scope, and made the same way: the failure that
/// leaves someone with no files and no error is worse than the one that leaves
/// them with too many, and the surface says which happened.
fn configured_filter(config: Option<&serde_json::Value>) -> Option<protein::Predicate> {
    let raw = config?.get("filter")?;
    // An absent filter and an explicitly cleared one are the same request.
    if raw.is_null() || raw.as_str().map(str::trim).is_some_and(str::is_empty) {
        return None;
    }
    // Stored as a JSON string by the surface (a text field), or as an object
    // by anything writing the extension directly. Both are accepted, because
    // refusing one of them would make the same setting mean different things
    // depending on which wrote it.
    let value = match raw.as_str() {
        Some(text) => serde_json::from_str::<serde_json::Value>(text).ok()?,
        None => raw.clone(),
    };
    serde_json::from_value::<protein::Predicate>(value).ok()
}

/// Whether a stored filter is present but unreadable — what the surface needs
/// to say "this is being ignored" rather than showing a filter that is not
/// running. Mirrors `organs::scope_unreadable` deliberately: two settings that
/// fail the same way should report the same way.
pub fn unreadable_filter(config: Option<&serde_json::Value>) -> Option<String> {
    let raw = config?.get("filter")?;
    if raw.is_null() || raw.as_str().map(str::trim).is_some_and(str::is_empty) {
        return None;
    }
    if configured_filter(config).is_some() {
        return None;
    }
    Some(match raw.as_str() {
        Some(text) => text.to_string(),
        None => raw.to_string(),
    })
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
        fields: None,
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
