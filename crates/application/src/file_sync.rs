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
    let configuration = services.repository.configuration.get_active().await?;
    let path = resolve_sync_path(configuration.file_sync_path.as_deref())?;
    let config = FileSyncConfig {
        enabled: configuration.file_sync_enabled != 0,
        path,
    };

    *services
        .file_sync_config
        .write()
        .map_err(|_| Error::other("File sync config lock poisoned"))? = Some(config);

    Ok(())
}

pub async fn start_if_enabled(services: InjectedServices) -> Result<(), Error> {
    let Some(config) = active_config(&services)? else {
        return Ok(());
    };
    if !config.enabled {
        return Ok(());
    }

    fs::create_dir_all(&config.path)?;
    mirror_records_to_files(services.clone(), &config.path).await?;
    tokio::spawn(watch_file_sync_dir(services, config.path));

    Ok(())
}

pub async fn sync_after_record_change(services: InjectedServices) -> Result<(), Error> {
    let Some(config) = active_config(&services)? else {
        return Ok(());
    };
    if !config.enabled {
        return Ok(());
    }

    fs::create_dir_all(&config.path)?;
    mirror_records_to_files(services, &config.path).await
}

fn active_config(services: &InjectedServices) -> Result<Option<FileSyncConfig>, Error> {
    Ok(services
        .file_sync_config
        .read()
        .map_err(|_| Error::other("File sync config lock poisoned"))?
        .clone())
}

fn resolve_sync_path(configured_path: Option<&str>) -> Result<PathBuf, Error> {
    if let Some(path) = configured_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        && path.is_dir()
    {
        return Ok(path);
    }

    let config_dir = dirs::config_dir()
        .ok_or_else(|| Error::other("Unable to resolve user config directory"))?;
    Ok(config_dir.join("lince").join("files"))
}

async fn watch_file_sync_dir(services: InjectedServices, path: PathBuf) {
    let mut snapshot = match scan_disk_files(&path) {
        Ok(files) => files,
        Err(_) => BTreeMap::new(),
    };
    let mut interval = tokio::time::interval(WATCH_INTERVAL);

    loop {
        interval.tick().await;

        let desired = match desired_files_from_records(&services).await {
            Ok(files) => files,
            Err(_) => continue,
        };

        let current = match scan_disk_files(&path) {
            Ok(files) => files,
            Err(_) => continue,
        };

        if disk_matches_desired(&current, &desired) {
            snapshot = desired_as_disk_files(&desired);
            continue;
        }

        if apply_disk_changes_to_records(services.clone(), &snapshot, &current)
            .await
            .is_err()
        {
            continue;
        }

        if mirror_records_to_files(services.clone(), &path)
            .await
            .is_err()
        {
            continue;
        }

        snapshot = match scan_disk_files(&path) {
            Ok(files) => files,
            Err(_) => BTreeMap::new(),
        };
    }
}

async fn apply_disk_changes_to_records(
    services: InjectedServices,
    previous: &BTreeMap<PathBuf, DiskFile>,
    current: &BTreeMap<PathBuf, DiskFile>,
) -> Result<(), Error> {
    for (path, file) in current {
        match previous.get(path) {
            Some(previous_file) if previous_file == file => {}
            Some(previous_file) => {
                if let Some(id) = record_id_for_head(&services, &previous_file.head).await? {
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
                crate::write::insert_record_from_file_sync(
                    services.clone(),
                    file.head.clone(),
                    file.body.clone(),
                )
                .await?;
            }
        }
    }

    for (path, file) in previous {
        if !current.contains_key(path)
            && let Some(id) = record_id_for_head(&services, &file.head).await?
        {
            crate::write::delete_record_from_file_sync(services.clone(), id).await?;
        }
    }

    Ok(())
}

async fn record_id_for_head(services: &InjectedServices, head: &str) -> Result<Option<i64>, Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM record WHERE COALESCE(head, '') = ? ORDER BY id LIMIT 1",
    )
    .bind(head)
    .fetch_optional(&*services.db)
    .await
    .map_err(Error::other)
}

async fn mirror_records_to_files(services: InjectedServices, dir: &Path) -> Result<(), Error> {
    let desired = desired_files_from_records(&services).await?;
    let desired_paths = desired.keys().cloned().collect::<BTreeSet<_>>();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some(MARKDOWN_EXTENSION)
            && !desired_paths.contains(&path)
        {
            fs::remove_file(path)?;
        }
    }

    for (path, file) in desired {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, file.body)?;
    }

    Ok(())
}

async fn desired_files_from_records(
    services: &InjectedServices,
) -> Result<BTreeMap<PathBuf, RecordFile>, Error> {
    let Some(config) = active_config(services)? else {
        return Ok(BTreeMap::new());
    };
    let rows = sqlx::query_as::<_, SyncRecord>("SELECT id, head, body FROM record ORDER BY id")
        .fetch_all(&*services.db)
        .await
        .map_err(Error::other)?;

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
