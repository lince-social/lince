use super::{DEFAULT_INTERVAL, Engine, EngineError, FileConflict, FileFormat, FileSyncState};
use nucleus::sync::{PERSONAL_FILE_ORIGIN, valid_file_origin};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use store::sqlx::{self, Sqlite, Transaction};
use tokio::{sync::oneshot, task::JoinHandle};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectorySync {
    pub uid: String,
    pub name: String,
    pub path: String,
    pub protein: Option<String>,
    pub format: FileFormat,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List,
    Save {
        settings: DirectorySync,
        create: bool,
    },
    SetEnabled {
        uid: String,
        enabled: bool,
    },
    Remove {
        uid: String,
    },
}

impl Request {
    pub fn permission(&self) -> &'static str {
        match self {
            Self::List => "record:read",
            _ => "record:update",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub settings: DirectorySync,
    pub checked: bool,
    pub conflicts: Vec<FileConflict>,
}

fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::Consequence(message.into())
}

pub(crate) async fn require_origin(
    tx: &mut Transaction<'_, Sqlite>,
    origin: &str,
) -> Result<(), EngineError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM directory_sync WHERE uid = ? AND removed = 0)",
    )
    .bind(origin)
    .fetch_one(&mut **tx)
    .await?;
    if !exists {
        return Err(invalid(format!(
            "File sync origin '{origin}' is unavailable"
        )));
    }
    Ok(())
}

async fn read<'e>(
    executor: impl sqlx::Executor<'e, Database = Sqlite>,
) -> Result<Vec<DirectorySync>, EngineError> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT settings FROM directory_sync WHERE removed = 0 ORDER BY uid")
            .fetch_all(executor)
            .await?;
    rows.into_iter()
        .map(|row| serde_json::from_str(&row).map_err(EngineError::Json))
        .collect()
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn normalized_path(path: &Path) -> Result<PathBuf, EngineError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(invalid("Choose an absolute directory without '..'"));
    }
    if path.exists() {
        if !path.is_dir() {
            return Err(invalid("The sync path must be a directory"));
        }
        return path.canonicalize().map_err(EngineError::Io);
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("The sync path is unavailable"))?;
    let name = path
        .file_name()
        .ok_or_else(|| invalid("The sync path is unavailable"))?;
    Ok(normalized_path(parent)?.join(name))
}

impl Engine {
    pub(crate) async fn check_legacy_sync_path(&self, config: &Value) -> Result<(), EngineError> {
        let Some(path) = config["path"].as_str().filter(|path| !path.is_empty()) else {
            return Ok(());
        };
        if config["enabled"] != true {
            return Ok(());
        }
        let directories = read(&self.store.pool).await?;
        if directories.is_empty() {
            return Ok(());
        }
        let path = normalized_path(Path::new(path))?;
        if directories
            .iter()
            .any(|item| paths_overlap(&path, Path::new(&item.path)))
        {
            return Err(invalid("This folder overlaps a named file sync"));
        }
        Ok(())
    }

    async fn save_directory_sync(
        &self,
        mut settings: DirectorySync,
        create: bool,
    ) -> Result<String, EngineError> {
        if !valid_file_origin(&settings.uid) || settings.uid == PERSONAL_FILE_ORIGIN {
            return Err(invalid(
                "Choose an origin such as institute using lowercase letters, numbers and hyphens. Personal is reserved.",
            ));
        }
        settings.name = settings.name.trim().to_owned();
        if settings.name.is_empty() || settings.name.len() > 128 {
            return Err(invalid("Choose a name of up to 128 bytes"));
        }
        if settings.path.len() > 4096 {
            return Err(invalid("The sync path is too long"));
        }
        let path = normalized_path(Path::new(settings.path.trim()))?;
        settings.path = path
            .to_str()
            .ok_or_else(|| invalid("The sync path must be UTF-8"))?
            .to_owned();
        if let Some(protein) = &mut settings.protein {
            *protein = self.resolve(protein).await?;
            self.file_sync_protein(protein).await?;
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        let previous = read(&mut *tx).await?;
        let existing = previous.iter().find(|item| item.uid == settings.uid);
        if create == existing.is_some() {
            return Err(invalid(if create {
                "That file origin already has a sync. Edit the existing sync or choose another origin."
            } else {
                "This File Sync is unavailable"
            }));
        }
        if previous
            .iter()
            .any(|item| item.uid != settings.uid && paths_overlap(&path, Path::new(&item.path)))
        {
            return Err(invalid("This directory overlaps another File Sync"));
        }
        let legacy: Vec<String> = sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE namespace = 'lince.file_sync'",
        )
        .fetch_all(&mut *tx)
        .await?;
        for cfg in legacy {
            let cfg: Value = serde_json::from_str(&cfg)?;
            if let Some(other) = cfg["path"].as_str().filter(|path| !path.is_empty()) {
                if normalized_path(Path::new(other)).is_ok_and(|other| paths_overlap(&path, &other))
                {
                    return Err(invalid("This folder overlaps an Organ's File Sync"));
                }
            }
        }
        let uid = settings.uid.clone();
        let saved = sqlx::query(
            "INSERT INTO directory_sync(uid, settings) VALUES (?, ?)
             ON CONFLICT(uid) DO UPDATE SET settings = excluded.settings WHERE removed = 0",
        )
        .bind(&uid)
        .bind(serde_json::to_string(&settings)?)
        .execute(&mut *tx)
        .await?;
        if saved.rows_affected() == 0 {
            return Err(invalid(
                "This origin belongs to a removed File Sync. Choose another origin.",
            ));
        }
        tx.commit().await?;
        Ok(uid)
    }

    pub(crate) async fn directory_sync_command(
        &self,
        request: Request,
    ) -> Result<Value, EngineError> {
        let changed =
            match request {
                Request::List => None,
                Request::Save { settings, create } => {
                    Some(self.save_directory_sync(settings, create).await?)
                }
                Request::SetEnabled { uid, enabled } => {
                    let updated = sqlx::query(
                    "UPDATE directory_sync SET settings = json_set(settings, '$.enabled', json(?))
                     WHERE uid = ? AND removed = 0",
                )
                .bind(if enabled { "true" } else { "false" })
                .bind(&uid).execute(&self.store.pool).await?;
                    if updated.rows_affected() == 0 {
                        return Err(invalid("This File Sync is unavailable"));
                    }
                    Some(uid)
                }
                Request::Remove { uid } => {
                    let updated = sqlx::query(
                        "UPDATE directory_sync SET removed = 1 WHERE uid = ? AND removed = 0",
                    )
                    .bind(&uid)
                    .execute(&self.store.pool)
                    .await?;
                    if updated.rows_affected() == 0 {
                        return Err(invalid("This File Sync is unavailable"));
                    }
                    Some(uid)
                }
            };
        if let Some(uid) = changed {
            self.file_sync_conflicts
                .lock()
                .expect("file sync conflicts")
                .remove(&uid);
        }
        Ok(json!({"directories": self.directory_sync_statuses().await?}))
    }

    pub async fn directory_sync_statuses(&self) -> Result<Vec<Status>, EngineError> {
        let settings = read(&self.store.pool).await?;
        let conflicts = self
            .file_sync_conflicts
            .lock()
            .expect("file sync conflicts");
        let directories: Vec<_> = settings
            .into_iter()
            .map(|settings| {
                let details = settings
                    .enabled
                    .then(|| conflicts.get(&settings.uid))
                    .flatten();
                Status {
                    checked: details.is_some(),
                    conflicts: details.cloned().unwrap_or_default(),
                    settings,
                }
            })
            .collect();
        Ok(directories)
    }
}

impl DirectorySync {
    pub(super) fn baseline_key(&self) -> String {
        format!("{}:{}:{}", self.uid, self.format.extension(), self.path)
    }

    pub(super) fn export_config(&self) -> Value {
        json!({"format":self.format,"protein":self.protein})
    }
}

pub(super) struct Worker {
    settings: DirectorySync,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

pub(super) async fn reconcile(
    engine: &Arc<Engine>,
    workers: &mut HashMap<String, Worker>,
) -> Result<(), EngineError> {
    let organ = store::organs::local(&engine.store.pool)
        .await?
        .ok_or_else(|| invalid("The local Organ is unavailable"))?;
    let mut wanted: HashMap<_, _> = read(&engine.store.pool)
        .await?
        .into_iter()
        .filter(|item| item.enabled)
        .map(|item| (item.uid.clone(), item))
        .collect();
    let mut stopped = Vec::new();
    for (uid, worker) in std::mem::take(workers) {
        if wanted.get(&uid) == Some(&worker.settings) && !worker.task.is_finished() {
            wanted.remove(&uid);
            workers.insert(uid, worker);
        } else {
            let _ = worker.stop.send(());
            stopped.push(worker.task);
        }
    }
    for task in stopped {
        let _ = task.await;
    }
    for (uid, settings) in wanted {
        let engine = engine.clone();
        let config = settings.clone();
        let organ = organ.uid.clone();
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut state = FileSyncState::new();
            let mut ticker = tokio::time::interval(DEFAULT_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    biased;
                    _ = &mut stopped => break,
                    _ = ticker.tick() => {
                        let _ = engine.file_sync_tick_with(
                            Path::new(&config.path), &organ, &mut state, Some(&config),
                        ).await;
                    }
                }
            }
        });
        workers.insert(
            uid,
            Worker {
                settings,
                stop,
                task,
            },
        );
    }
    Ok(())
}
