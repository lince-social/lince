use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::Engine;
use crate::actions::Action;
use crate::error::EngineError;

const MARKDOWN_EXTENSION: &str = "md";
const MISSING_TICKS_BEFORE_DELETE: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileFormat {
    #[default]
    Markdown,
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
    body: String,
    prelude: Option<String>,
    missing_ticks: u32,
}

#[derive(Debug, Default)]
pub struct FileSyncState {
    known: HashMap<PathBuf, KnownFile>,
}

impl FileSyncState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default, Clone)]
pub struct FileSyncReport {
    pub created: Vec<String>,
    pub updated_from_disk: Vec<String>,
    pub deleted: Vec<String>,
    pub written_to_disk: Vec<String>,
    pub conflicts: Vec<FileConflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileConflict {
    pub path: String,
    pub reason: String,
}

impl Engine {
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
        let mut written_before: HashMap<PathBuf, DesiredFile> = HashMap::new();
        {
            let all = self.selected_records(organ_uid, None).await?;
            for format in &formats {
                written_before.extend(self.desired_files(dir, &all, *format).await?);
            }
        }
        self.reattach_written_files(&written_before, &disk, state);
        let mut conflicted: HashSet<PathBuf> = HashSet::new();

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
            if let Err(reason) = self.check_body_links(&body, &declared).await? {
                report.conflicts.push(FileConflict {
                    path: source.display().to_string(),
                    reason,
                });
                conflicted.insert(source.clone());
                continue;
            }
            let given_uid = projection
                .as_ref()
                .map(|p| p.uid.trim())
                .filter(|uid| !uid.is_empty());
            let existing = match given_uid {
                Some(given) => store::records::get(&self.store.pool, given).await?,
                None => None,
            };
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

        let matched = self.selected_records(organ_uid, config.as_ref()).await?;
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

        for entry in std::fs::read_dir(dir).map_err(EngineError::Io)? {
            let entry = entry.map_err(EngineError::Io)?;
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if conflicted.contains(&path) {
                continue;
            }
            let configured = formats.iter().any(|f| ext == Some(f.extension()));
            if configured && !desired.contains_key(&path) && owned.contains(&path) {
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
        state.known.retain(|path, _| {
            desired.contains_key(path) || pending_delete.contains(path) || owned.contains(path)
        });

        for (path, desired) in &desired {
            if pending_delete.contains(path) {
                continue;
            }
            if conflicted.contains(path) {
                continue;
            }
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

        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .insert(organ_uid.to_string(), report.conflicts.clone());
        Ok(report)
    }

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
                let only_level = known_prelude
                    .as_deref()
                    .is_some_and(|known| strip_level(known) == strip_level(&disk_prelude));
                if !only_level {
                    if let Err(reason) = self
                        .apply_prelude_edit(uid, &projection, known_prelude.as_deref())
                        .await?
                    {
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

    async fn apply_prelude_edit(
        &self,
        uid: &str,
        disk: &crate::lingua_file::Projection,
        known_prelude: Option<&str>,
    ) -> Result<Result<(), String>, EngineError> {
        if let Err(reason) = self.check_projection(disk, &HashSet::new()).await? {
            return Ok(Err(reason));
        }
        let known = match known_prelude {
            Some(text) => match crate::lingua_file::parse_prelude(text) {
                Ok(projection) => projection,
                Err(err) => {
                    return Ok(Err(format!(
                        "Lince could not read back the block it wrote for this file ({err}), so \
                         it cannot tell which lines you changed"
                    )));
                }
            },
            None => crate::lingua_file::Projection::default(),
        };

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

    pub fn file_sync_conflicts(&self, organ_uid: &str) -> Vec<FileConflict> {
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .get(organ_uid)
            .cloned()
            .unwrap_or_default()
    }

    pub fn file_sync_has_ticked(&self, organ_uid: &str) -> bool {
        self.file_sync_conflicts
            .lock()
            .expect("file sync conflicts")
            .contains_key(organ_uid)
    }
}

fn format_rank(formats: &[FileFormat], path: &Path) -> usize {
    format_of(formats, path)
        .and_then(|f| formats.iter().position(|c| *c == f))
        .unwrap_or(usize::MAX)
}

fn format_of(formats: &[FileFormat], path: &Path) -> Option<FileFormat> {
    let ext = path.extension().and_then(|e| e.to_str())?;
    formats.iter().copied().find(|f| f.extension() == ext)
}

fn strip_level(prelude: &str) -> String {
    prelude
        .lines()
        .filter(|line| !line.trim_start().starts_with("quantity:"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone)]
struct DesiredFile {
    uid: String,
    text: String,
    prelude: Option<String>,
}

impl Engine {
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
        matched.retain(|row| {
            !matches!(
                row.slug.as_deref(),
                Some(store::organs::LOCAL_ORGAN_SLUG) | Some(store::cells::LOCAL_CELL_SLUG)
            )
        });
        Ok(matched)
    }

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
        let mut object_uids: Vec<String> = assertions
            .iter()
            .filter_map(|a| a.object_uid.clone())
            .collect();
        object_uids.sort();
        object_uids.dedup();
        let heads = store::records::heads_for(&self.store.pool, &object_uids).await?;
        let linkable: Vec<(String, String)> = matched
            .iter()
            .filter(|row| !row.head.trim().is_empty())
            .map(|row| (row.head.clone(), row.uid.clone()))
            .collect();

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
            let body = crate::body_links::link_first_mentions(&body, &linkable, &uid);
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

    async fn check_body_links(
        &self,
        body: &str,
        pending: &HashSet<String>,
    ) -> Result<Result<(), String>, EngineError> {
        for mention in crate::body_links::mentions(body) {
            let Some(uid) = mention.uid else {
                continue;
            };
            if !pending.contains(&uid)
                && store::records::get(&self.store.pool, &uid).await?.is_none()
            {
                return Ok(Err(format!(
                    "[[{}|{uid}]] in the body points at no Record here — a link is its uid, \
                     never its title, so nothing was retargeted by name",
                    mention.title
                )));
            }
        }
        Ok(Ok(()))
    }

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

fn render_prelude_only(projection: &crate::lingua_file::Projection) -> String {
    let whole = crate::lingua_file::render_file(projection, "");
    let (prelude, _) = crate::lingua_file::split(&whole);
    prelude.unwrap_or_default().to_string()
}

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

pub const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

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

fn desired_watch(config: Option<serde_json::Value>) -> Option<PathBuf> {
    let config = config?;
    let enabled = config
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let path = config.get("path").and_then(|v| v.as_str()).unwrap_or("");
    (enabled && !path.trim().is_empty()).then(|| PathBuf::from(path))
}

fn configured_filter(config: Option<&serde_json::Value>) -> Option<protein::Predicate> {
    let raw = config?.get("filter")?;
    if raw.is_null() || raw.as_str().map(str::trim).is_some_and(str::is_empty) {
        return None;
    }
    let value = match raw.as_str() {
        Some(text) => serde_json::from_str::<serde_json::Value>(text).ok()?,
        None => raw.clone(),
    };
    serde_json::from_value::<protein::Predicate>(value).ok()
}

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

fn fact_may_touch_file_sync(fact: &nucleus::Fact) -> bool {
    fact.payload
        .as_deref()
        .is_some_and(|p| p.contains("lince.file_sync"))
}

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
