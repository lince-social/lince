use std::{path::PathBuf, time::Duration};

use tokio::sync::{mpsc, watch};
use utils::{build_info, self_update};

pub use self_update::{AssetKind, Availability, UpdateCommand, UpdateStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    Idle,
    Checking,
    Downloading,
    ReadyToRestart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Information {
    pub version: String,
    pub revision: String,
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub address: Option<String>,
    pub last_updated: Option<String>,
    pub last_checked: Option<String>,
    pub automatic: bool,
    pub server: bool,
    pub checks_enabled: bool,
    pub phase: UpdatePhase,
    pub update: Option<UpdateStatus>,
    pub note: String,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct InformationChannel {
    pub state: watch::Receiver<Information>,
    requests: mpsc::Sender<UpdateCommand>,
}

impl InformationChannel {
    pub fn request(&self, command: UpdateCommand) -> Result<(), String> {
        self.requests
            .try_send(command)
            .map_err(|error| error.to_string())
    }

    pub fn ready_to_restart(&self) -> bool {
        self.state.borrow().phase == UpdatePhase::ReadyToRestart
    }

    pub async fn wait_for_restart(&mut self) {
        loop {
            if self.ready_to_restart() {
                return;
            }
            if self.state.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

#[async_trait::async_trait]
trait Updater: Send + Sync + 'static {
    async fn check(&self, kind: AssetKind) -> Result<UpdateStatus, String>;
    async fn apply(&self, status: &UpdateStatus) -> Result<(), String>;
}

struct PublishedUpdater;

#[async_trait::async_trait]
impl Updater for PublishedUpdater {
    async fn check(&self, kind: AssetKind) -> Result<UpdateStatus, String> {
        self_update::net::check(kind).await
    }

    async fn apply(&self, status: &UpdateStatus) -> Result<(), String> {
        self_update::net::download_and_apply(status).await
    }
}

pub(crate) async fn start(
    store: store::Store,
    server: bool,
    address: Option<String>,
) -> Result<(InformationChannel, tokio::task::JoinHandle<()>), std::io::Error> {
    use store::sqlx::Row;
    let settings = store::sqlx::query(
        "SELECT automatic, installed_revision, installed_at FROM self_update WHERE id = 1",
    )
    .fetch_one(&store.pool)
    .await
    .map_err(std::io::Error::other)?;
    let executable = build_info::appimage_path()
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_default();
    let installed_revision: Option<String> = settings.get("installed_revision");
    let last_updated = if installed_revision.as_deref() == Some(build_info::revision()) {
        settings.get("installed_at")
    } else {
        std::fs::metadata(&executable)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .map(|time| chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339())
    };
    let state = Information {
        version: build_info::VERSION.into(),
        revision: build_info::revision().into(),
        directory: utils::config::lince_data_dir().unwrap_or_default(),
        executable,
        address,
        last_updated,
        last_checked: None,
        automatic: settings.get::<bool, _>("automatic"),
        server,
        checks_enabled: !self_update::checks_disabled(),
        phase: UpdatePhase::Idle,
        update: None,
        note: build_info::self_apply_block_for(server).message().into(),
        error: None,
    };
    Ok(spawn(store, state, PublishedUpdater))
}

fn spawn(
    store: store::Store,
    state: Information,
    updater: impl Updater,
) -> (InformationChannel, tokio::task::JoinHandle<()>) {
    let (updates, receiver) = watch::channel(state.clone());
    let (requests, commands) = mpsc::channel(8);
    let task = tokio::spawn(self_update::net::run_watch_loop(
        Watcher {
            store,
            state,
            updater,
            updates,
            next: tokio::time::Instant::now(),
        },
        commands,
    ));
    (
        InformationChannel {
            state: receiver,
            requests,
        },
        task,
    )
}

struct Watcher<U> {
    store: store::Store,
    state: Information,
    updater: U,
    updates: watch::Sender<Information>,
    next: tokio::time::Instant,
}

impl<U: Updater> self_update::net::UpdateWatcher for Watcher<U> {
    fn deadline(&self) -> Option<tokio::time::Instant> {
        (self.state.phase != UpdatePhase::ReadyToRestart).then_some(self.next)
    }

    async fn handle(&mut self, command: UpdateCommand) {
        let Self {
            store,
            state,
            updater,
            updates,
            next,
        } = self;
        if state.phase == UpdatePhase::ReadyToRestart {
            return;
        }
        let kind = if state.server {
            AssetKind::Server
        } else {
            AssetKind::Desktop
        };
        state.error = None;
        match command {
            UpdateCommand::Automatic(enabled) => {
                let saved = store::sqlx::query("UPDATE self_update SET automatic = ? WHERE id = 1")
                    .bind(enabled)
                    .execute(&store.pool)
                    .await;
                match saved {
                    Ok(_) => state.automatic = enabled,
                    Err(error) => {
                        state.error = Some(format!("Could not save update settings: {error}"))
                    }
                }
            }
            UpdateCommand::Check => {
                if !state.checks_enabled {
                    state.error = Some("Update checks are disabled by LINCE_UPDATE_CHECK.".into());
                } else {
                    state.phase = UpdatePhase::Checking;
                    updates.send_replace(state.clone());
                    match updater.check(kind).await {
                        Ok(status) => {
                            state.update = Some(status);
                            state.last_checked = Some(chrono::Utc::now().to_rfc3339());
                        }
                        Err(error) => {
                            state.update = None;
                            state.error = Some(error);
                        }
                    }
                    state.phase = UpdatePhase::Idle;
                }
                *next = tokio::time::Instant::now()
                    + Duration::from_secs(if state.error.is_some() { 3600 } else { 86400 });
            }
            UpdateCommand::DownloadAndRestart => {}
        }
        let apply = matches!(command, UpdateCommand::DownloadAndRestart)
            || (state.checks_enabled && (state.server || state.automatic) && state.error.is_none());
        if apply {
            if let Some(status) = state.update.clone().filter(|status| {
                status.can_self_apply && status.availability == Availability::Available
            }) {
                state.phase = UpdatePhase::Downloading;
                updates.send_replace(state.clone());
                match updater.apply(&status).await {
                    Ok(()) => {
                        let installed = store::sqlx::query("UPDATE self_update SET installed_revision = ?, installed_at = ? WHERE id = 1")
                            .bind(&status.revision).bind(chrono::Utc::now().to_rfc3339())
                            .execute(&store.pool).await;
                        if let Err(error) = installed {
                            state.error = Some(format!(
                                "Update installed, but its date could not be saved: {error}"
                            ));
                        }
                        state.phase = UpdatePhase::ReadyToRestart;
                    }
                    Err(error) => {
                        state.phase = UpdatePhase::Idle;
                        state.error = Some(error);
                        *next = tokio::time::Instant::now() + Duration::from_secs(3600);
                    }
                }
            } else if matches!(command, UpdateCommand::DownloadAndRestart) {
                state.error = Some(
                    state
                        .update
                        .as_ref()
                        .map(|status| status.self_apply_note.clone())
                        .unwrap_or_else(|| "Check for a verified update first.".into()),
                );
            }
        }
        if state.server
            && let Some(error) = &state.error
        {
            tracing::warn!(%error, "Lince update");
        }
        if state.server
            && let Some(update) = state.update.as_ref()
            && update.availability == Availability::Available
            && !update.can_self_apply
        {
            tracing::warn!(message = %update.self_apply_note, "Lince update requires manual installation");
        }
        updates.send_replace(state.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct FakeUpdater {
        result: Arc<Mutex<Result<UpdateStatus, String>>>,
        failure: Option<String>,
        checks: Arc<AtomicUsize>,
        applies: Arc<AtomicUsize>,
        kinds: Arc<Mutex<Vec<AssetKind>>>,
    }

    #[async_trait::async_trait]
    impl Updater for FakeUpdater {
        async fn check(&self, kind: AssetKind) -> Result<UpdateStatus, String> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            self.kinds.lock().unwrap().push(kind);
            self.result.lock().unwrap().clone()
        }

        async fn apply(&self, _: &UpdateStatus) -> Result<(), String> {
            self.applies.fetch_add(1, Ordering::SeqCst);
            self.failure.clone().map_or(Ok(()), Err)
        }
    }

    fn fake() -> FakeUpdater {
        FakeUpdater {
            result: Arc::new(Mutex::new(Ok(UpdateStatus {
                availability: Availability::Available,
                version: "0.7.1".into(),
                revision: "new".into(),
                channel: "rolling".into(),
                asset_name: Some("lince.AppImage".into()),
                asset_url: Some("https://example.com/lince.AppImage".into()),
                asset_sha256: Some("ab".repeat(32)),
                can_self_apply: true,
                self_apply_note: "This build can replace itself.".into(),
            }))),
            failure: None,
            checks: Arc::new(AtomicUsize::new(0)),
            applies: Arc::new(AtomicUsize::new(0)),
            kinds: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn state() -> Information {
        Information {
            version: "0.7.0".into(),
            revision: "old".into(),
            directory: PathBuf::from("/data"),
            executable: PathBuf::from("/app/lince"),
            address: Some("127.0.0.1:6174".into()),
            last_updated: None,
            last_checked: None,
            automatic: false,
            server: false,
            checks_enabled: true,
            phase: UpdatePhase::Idle,
            update: None,
            note: String::new(),
            error: None,
        }
    }

    async fn until(
        channel: &mut InformationChannel,
        predicate: impl Fn(&Information) -> bool,
    ) -> Information {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state = channel.state.borrow_and_update().clone();
                if predicate(&state) {
                    return state;
                }
                channel.state.changed().await.unwrap();
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn manual_update_is_announced_once_then_installed_and_recorded() {
        let store = store::Store::open_memory().await.unwrap();
        let fake = fake();
        let (mut channel, task) = spawn(store.clone(), state(), fake.clone());
        until(&mut channel, |state| state.update.is_some()).await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
        assert_eq!(*fake.kinds.lock().unwrap(), vec![AssetKind::Desktop]);
        assert!(
            tokio::time::timeout(Duration::from_millis(30), channel.state.changed())
                .await
                .is_err()
        );
        assert_eq!(fake.checks.load(Ordering::SeqCst), 1);
        channel.request(UpdateCommand::DownloadAndRestart).unwrap();
        channel.wait_for_restart().await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 1);
        let saved: (String, String) = store::sqlx::query_as(
            "SELECT installed_revision, installed_at FROM self_update WHERE id = 1",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(saved.0, "new");
        assert!(!saved.1.is_empty());
        channel.request(UpdateCommand::DownloadAndRestart).unwrap();
        tokio::task::yield_now().await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 1);
        task.abort();
    }

    #[tokio::test]
    async fn automatic_preference_is_saved_and_server_uses_its_own_asset() {
        let store = store::Store::open_memory().await.unwrap();
        let fake = fake();
        let (mut channel, task) = spawn(store.clone(), state(), fake.clone());
        until(&mut channel, |state| state.update.is_some()).await;
        channel.request(UpdateCommand::Automatic(true)).unwrap();
        channel.wait_for_restart().await;
        assert!(channel.state.borrow().automatic);
        let automatic: bool =
            store::sqlx::query_scalar("SELECT automatic FROM self_update WHERE id = 1")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert!(automatic);
        task.abort();
        let mut server = state();
        server.server = true;
        let (mut channel, task) = spawn(store, server, fake.clone());
        channel.wait_for_restart().await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 2);
        assert_eq!(fake.kinds.lock().unwrap().last(), Some(&AssetKind::Server));
        task.abort();
    }

    #[tokio::test]
    async fn failed_verification_clears_the_previous_offer_and_never_installs() {
        let store = store::Store::open_memory().await.unwrap();
        let fake = fake();
        let (mut channel, task) = spawn(store, state(), fake.clone());
        until(&mut channel, |state| state.update.is_some()).await;
        *fake.result.lock().unwrap() = Err("the update manifest signature does not verify".into());
        channel.request(UpdateCommand::Check).unwrap();
        let rejected = until(&mut channel, |state| state.error.is_some()).await;
        assert!(rejected.update.is_none());
        channel.request(UpdateCommand::DownloadAndRestart).unwrap();
        until(&mut channel, |state| {
            state.error.as_deref() == Some("Check for a verified update first.")
        })
        .await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
        task.abort();
    }

    #[tokio::test]
    async fn download_failure_keeps_the_running_version_and_does_not_restart() {
        let store = store::Store::open_memory().await.unwrap();
        let mut fake = fake();
        fake.failure = Some("the downloaded file failed its checksum".into());
        let mut automatic = state();
        automatic.automatic = true;
        let (mut channel, task) = spawn(store.clone(), automatic, fake);
        let failed = until(&mut channel, |state| state.error.is_some()).await;
        assert_eq!(failed.phase, UpdatePhase::Idle);
        assert_eq!(failed.revision, "old");
        assert!(!channel.ready_to_restart());
        let installed: Option<String> =
            store::sqlx::query_scalar("SELECT installed_revision FROM self_update WHERE id = 1")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert!(installed.is_none());
        task.abort();
    }

    #[tokio::test]
    async fn disabled_checks_and_manual_installations_do_not_download() {
        let store = store::Store::open_memory().await.unwrap();
        let fake = fake();
        let mut disabled = state();
        disabled.checks_enabled = false;
        disabled.server = true;
        let (mut channel, task) = spawn(store.clone(), disabled, fake.clone());
        until(&mut channel, |state| state.error.is_some()).await;
        assert_eq!(fake.checks.load(Ordering::SeqCst), 0);
        assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
        task.abort();
        if let Ok(status) = fake.result.lock().unwrap().as_mut() {
            status.can_self_apply = false;
            status.self_apply_note = "Update with nixos-rebuild switch".into();
        }
        let mut automatic = state();
        automatic.automatic = true;
        let (mut channel, task) = spawn(store, automatic, fake.clone());
        until(&mut channel, |state| state.update.is_some()).await;
        assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
        channel.request(UpdateCommand::DownloadAndRestart).unwrap();
        assert!(
            until(&mut channel, |state| state.error.is_some())
                .await
                .error
                .unwrap()
                .contains("nixos-rebuild")
        );
        assert_eq!(fake.applies.load(Ordering::SeqCst), 0);
        task.abort();
    }
}
