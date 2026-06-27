use injection::cross_cutting::{FileSyncConfig, InjectedServices};
use persistence::write_coordinator::{SqlParameter, WriteOutcome};
use sqlx::FromRow;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Error,
    path::{Path, PathBuf},
    time::Duration,
};

const MARKDOWN_EXTENSION: &str = "md";
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
struct SyncRecord {
    id: i64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordFile {
    id: i64,
    head: String,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiskFile {
    head: String,
    body: String,
}

pub async fn configure_from_active_configuration(services: InjectedServices) -> Result<(), Error> {
    configure_from_organs(services).await
}

pub async fn configure_from_organs(services: InjectedServices) -> Result<(), Error> {
    tracing::info!("file sync: loading organ file sync configuration");
    let rows = sqlx::query_as::<_, (i64, i64, Option<String>)>(
        "SELECT id, file_sync_enabled, file_sync_path FROM organ ORDER BY id",
    )
    .fetch_all(&*services.db)
    .await
    .map_err(Error::other)?;

    let mut configs = Vec::with_capacity(rows.len());
    for (organ_id, enabled, path) in rows {
        let config = FileSyncConfig {
            organ_id,
            enabled: enabled != 0,
            path: resolve_sync_path(organ_id, path.as_deref())?,
        };
        tracing::info!(
            organ_id = config.organ_id,
            enabled = config.enabled,
            path = %config.path.display(),
            "file sync: loaded organ config"
        );
        configs.push(config);
    }

    let enabled_count = configs.iter().filter(|config| config.enabled).count();
    tracing::info!(
        total = configs.len(),
        enabled = enabled_count,
        "file sync: organ configuration ready"
    );

    *services
        .file_sync_config
        .write()
        .map_err(|_| Error::other("File sync config lock poisoned"))? = configs;

    Ok(())
}

pub async fn start_if_enabled(services: InjectedServices) -> Result<(), Error> {
    let configs = active_configs(&services)?;
    tracing::info!(
        total = configs.len(),
        enabled = configs.iter().filter(|config| config.enabled).count(),
        "file sync: startup check"
    );
    for config in configs.into_iter().filter(|config| config.enabled) {
        tracing::info!(
            organ_id = config.organ_id,
            path = %config.path.display(),
            "file sync: starting enabled organ sync"
        );
        fs::create_dir_all(&config.path)?;
        mirror_records_to_files(services.clone(), &config).await?;
        tokio::spawn(watch_file_sync_dir(services.clone(), config));
    }

    Ok(())
}

pub async fn sync_after_record_change(services: InjectedServices) -> Result<(), Error> {
    let configs = active_configs(&services)?;
    tracing::info!(
        total = configs.len(),
        enabled = configs.iter().filter(|config| config.enabled).count(),
        "file sync: sync-after-change requested"
    );
    for config in configs
        .into_iter()
        .filter(|config| config.enabled)
    {
        tracing::info!(
            organ_id = config.organ_id,
            path = %config.path.display(),
            "file sync: mirroring records after change"
        );
        fs::create_dir_all(&config.path)?;
        mirror_records_to_files(services.clone(), &config).await?;
    }
    Ok(())
}

fn active_configs(services: &InjectedServices) -> Result<Vec<FileSyncConfig>, Error> {
    Ok(services
        .file_sync_config
        .read()
        .map_err(|_| Error::other("File sync config lock poisoned"))?
        .clone())
}

fn resolve_sync_path(organ_id: i64, configured_path: Option<&str>) -> Result<PathBuf, Error> {
    if let Some(path) = configured_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path);
    }

    let config_dir = utils::config::lince_data_dir()
        .ok_or_else(|| Error::other("Unable to resolve user config directory"))?;
    Ok(config_dir.join("files").join(format!("organ-{organ_id}")))
}

async fn watch_file_sync_dir(services: InjectedServices, config: FileSyncConfig) {
    let mut snapshot = match scan_disk_files(&config.path) {
        Ok(files) => files,
        Err(error) => {
            tracing::warn!(
                organ_id = config.organ_id,
                path = %config.path.display(),
                error = %error,
                "file sync: initial disk scan failed"
            );
            BTreeMap::new()
        }
    };
    tracing::info!(
        organ_id = config.organ_id,
        path = %config.path.display(),
        files = snapshot.len(),
        "file sync: watcher started"
    );
    let mut interval = tokio::time::interval(WATCH_INTERVAL);

    loop {
        interval.tick().await;

        let desired = match desired_files_from_records(&services, &config).await {
            Ok(files) => files,
            Err(error) => {
                tracing::warn!(
                    organ_id = config.organ_id,
                    path = %config.path.display(),
                    error = %error,
                    "file sync: watcher could not load desired records"
                );
                continue;
            }
        };

        let current = match scan_disk_files(&config.path) {
            Ok(files) => files,
            Err(error) => {
                tracing::warn!(
                    organ_id = config.organ_id,
                    path = %config.path.display(),
                    error = %error,
                    "file sync: watcher disk scan failed"
                );
                continue;
            }
        };

        if disk_matches_desired(&current, &desired) {
            snapshot = desired_as_disk_files(&desired);
            continue;
        }

        if apply_disk_changes_to_records(services.clone(), &config, &snapshot, &current)
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    organ_id = config.organ_id,
                    path = %config.path.display(),
                    error = %error,
                    "file sync: applying disk changes failed"
                );
            })
            .is_err()
        {
            continue;
        }

        if mirror_records_to_files(services.clone(), &config)
            .await
            .inspect_err(|error| {
                tracing::warn!(
                    organ_id = config.organ_id,
                    path = %config.path.display(),
                    error = %error,
                    "file sync: watcher mirror failed"
                );
            })
            .is_err()
        {
            continue;
        }

        snapshot = match scan_disk_files(&config.path) {
            Ok(files) => files,
            Err(error) => {
                tracing::warn!(
                    organ_id = config.organ_id,
                    path = %config.path.display(),
                    error = %error,
                    "file sync: post-mirror disk scan failed"
                );
                BTreeMap::new()
            }
        };
    }
}

async fn apply_disk_changes_to_records(
    services: InjectedServices,
    config: &FileSyncConfig,
    previous: &BTreeMap<PathBuf, DiskFile>,
    current: &BTreeMap<PathBuf, DiskFile>,
) -> Result<(), Error> {
    for (path, file) in current {
        match previous.get(path) {
            Some(previous_file) if previous_file == file => {}
            Some(previous_file) => {
                if let Some(id) =
                    record_id_for_head(&services, config.organ_id, &previous_file.head).await?
                {
                    tracing::info!(
                        organ_id = config.organ_id,
                        record_id = id,
                        path = %path.display(),
                        "file sync: updating record from changed file"
                    );
                    crate::write::update_record_head_body_from_file_sync(
                        services.clone(),
                        id,
                        previous_file.head.clone(),
                        file.body.clone(),
                    )
                    .await?;
                }
            }
            None => {
                tracing::info!(
                    organ_id = config.organ_id,
                    path = %path.display(),
                    "file sync: inserting record from new file"
                );
                crate::write::insert_record_from_file_sync(
                    services.clone(),
                    config.organ_id,
                    file.head.clone(),
                    file.body.clone(),
                )
                .await?;
            }
        }
    }

    for (path, file) in previous {
        if !current.contains_key(path)
            && let Some(id) = record_id_for_head(&services, config.organ_id, &file.head).await?
        {
            tracing::info!(
                organ_id = config.organ_id,
                record_id = id,
                path = %path.display(),
                "file sync: deleting record for removed file"
            );
            crate::write::delete_record_from_file_sync(services.clone(), id).await?;
        }
    }

    Ok(())
}

async fn record_id_for_head(
    services: &InjectedServices,
    organ_id: i64,
    head: &str,
) -> Result<Option<i64>, Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id
         FROM record
         WHERE COALESCE(owner_organ_id, 1) = ?
           AND COALESCE(head, '') = ?
         ORDER BY id
         LIMIT 1",
    )
    .bind(organ_id)
    .bind(head)
    .fetch_optional(&*services.db)
    .await
    .map_err(Error::other)
}

async fn mirror_records_to_files(
    services: InjectedServices,
    config: &FileSyncConfig,
) -> Result<(), Error> {
    let dir = &config.path;
    let desired = desired_files_from_records(&services, config).await?;
    let desired_paths = desired.keys().cloned().collect::<BTreeSet<_>>();
    tracing::info!(
        organ_id = config.organ_id,
        path = %dir.display(),
        records = desired.len(),
        "file sync: mirroring desired records to directory"
    );

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some(MARKDOWN_EXTENSION)
            && !desired_paths.contains(&path)
        {
            tracing::info!(
                organ_id = config.organ_id,
                path = %path.display(),
                "file sync: removing stale markdown file"
            );
            fs::remove_file(path)?;
        }
    }

    for (path, file) in desired {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        tracing::debug!(
            organ_id = config.organ_id,
            record_id = file.id,
            path = %path.display(),
            bytes = file.body.len(),
            "file sync: writing markdown file"
        );
        fs::write(path, file.body)?;
    }

    Ok(())
}

async fn desired_files_from_records(
    services: &InjectedServices,
    config: &FileSyncConfig,
) -> Result<BTreeMap<PathBuf, RecordFile>, Error> {
    let rows = sqlx::query_as::<_, SyncRecord>(
        "SELECT id, head, body
         FROM record
         WHERE COALESCE(owner_organ_id, 1) = ?
         ORDER BY id",
    )
    .bind(config.organ_id)
    .fetch_all(&*services.db)
    .await
    .map_err(Error::other)?;

    tracing::info!(
        organ_id = config.organ_id,
        path = %config.path.display(),
        records = rows.len(),
        "file sync: loaded records for organ"
    );

    Ok(records_to_files(&config.path, rows))
}

fn records_to_files(dir: &Path, rows: Vec<SyncRecord>) -> BTreeMap<PathBuf, RecordFile> {
    let mut stem_counts = BTreeMap::<String, usize>::new();
    for row in &rows {
        *stem_counts
            .entry(file_stem(row.id, row.head.as_deref()))
            .or_default() += 1;
    }

    let mut files = BTreeMap::new();
    for row in rows {
        let mut stem = file_stem(row.id, row.head.as_deref());
        if stem_counts.get(&stem).copied().unwrap_or_default() > 1 {
            stem = format!("{stem} -- {}", row.id);
        }
        let path = dir.join(format!("{stem}.{MARKDOWN_EXTENSION}"));
        files.insert(
            path,
            RecordFile {
                id: row.id,
                head: row.head.unwrap_or_default(),
                body: row.body.unwrap_or_default(),
            },
        );
    }
    files
}

fn scan_disk_files(dir: &Path) -> Result<BTreeMap<PathBuf, DiskFile>, Error> {
    fs::create_dir_all(dir)?;
    let mut files = BTreeMap::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some(MARKDOWN_EXTENSION) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let head = stem.to_string();
        files.insert(
            path,
            DiskFile {
                head,
                body: fs::read_to_string(entry.path())?,
            },
        );
    }
    Ok(files)
}

fn disk_matches_desired(
    disk: &BTreeMap<PathBuf, DiskFile>,
    desired: &BTreeMap<PathBuf, RecordFile>,
) -> bool {
    if disk.len() != desired.len() {
        return false;
    }

    disk.iter().all(|(path, disk_file)| {
        desired
            .get(path)
            .is_some_and(|record_file| disk_file.body == record_file.body)
    })
}

fn desired_as_disk_files(desired: &BTreeMap<PathBuf, RecordFile>) -> BTreeMap<PathBuf, DiskFile> {
    desired
        .iter()
        .map(|(path, file)| {
            (
                path.clone(),
                DiskFile {
                    head: file.head.clone(),
                    body: file.body.clone(),
                },
            )
        })
        .collect()
}

fn file_stem(id: i64, head: Option<&str>) -> String {
    let sanitized = sanitize_file_stem(head.unwrap_or_default());
    if sanitized.is_empty() {
        format!("record-{id}")
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

pub(crate) fn text_param(value: String) -> SqlParameter {
    if value.is_empty() {
        SqlParameter::Null
    } else {
        SqlParameter::Text(value)
    }
}

pub(crate) fn empty_outcome() -> WriteOutcome {
    WriteOutcome {
        rows_affected: 0,
        changed_tables: BTreeSet::new(),
        last_insert_rowid: None,
    }
}
