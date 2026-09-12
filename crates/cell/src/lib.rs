#![recursion_limit = "256"]

pub mod admin_bootstrap;
pub mod discovery;
pub mod information;
pub mod sync_runner;
pub mod transfer;
pub mod wire_supervisor;

use std::io::Error as IoError;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use store::Store;

pub use admin_bootstrap::{AdminBootstrap, ensure_admin};
pub use store::config::InterfaceStorage;
pub use transport::{ClientMessage, LaneEvent, LaneHub, ServerMessage, Session};
pub use utils::diagnostics::{
    Diagnostics, Journal as DiagnosticJournal, Notice, Subscription as DiagnosticSubscription,
};
pub use wire_supervisor::WireSlot;

const HEARTBEAT_PERIOD_SECS: u64 = 60;

#[derive(Clone)]
pub struct CellRuntime {
    pub engine: Arc<engine::Engine>,
    pub store: Store,
    pub lanes: Arc<LaneHub>,
    pub wire: WireSlot,
    pub information: Option<information::InformationChannel>,
}

impl CellRuntime {
    pub async fn interface_storage(&self) -> Result<InterfaceStorage, IoError> {
        store::config::interface_storage(&self.store.pool)
            .await
            .map_err(IoError::other)
    }

    pub async fn interface_close_suspends(&self) -> Result<bool, IoError> {
        store::config::interface_close_suspends(&self.store.pool)
            .await
            .map_err(IoError::other)
    }

    pub fn local_session(&self) -> Session {
        Session::new(
            self.engine.clone(),
            self.lanes.clone(),
            nucleus::new_uid("local-ui"),
            None,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct CellOptions {
    pub data_dir: Option<PathBuf>,
    pub local_base_url: Option<String>,
    pub language: Option<String>,
}

pub struct Cell {
    runtime: CellRuntime,
    supervisors: Vec<tokio::task::JoinHandle<()>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Cell {
    pub async fn open(options: CellOptions) -> Result<Cell, IoError> {
        if let Some(directory) = options.data_dir.clone() {
            utils::config::set_lince_data_dir_override(directory)?;
        }
        let store = Store::open(&default_lince_db_url()?)
            .await
            .map_err(IoError::other)?;
        seed_cell(
            &store,
            options.local_base_url.as_deref(),
            options.language.as_deref(),
        )
        .await?;

        let engine = Arc::new(
            engine::Engine::new(store.clone())
                .await
                .map_err(IoError::other)?,
        );
        let local_organ = store::organs::local(&store.pool)
            .await
            .map_err(IoError::other)?
            .ok_or_else(|| IoError::other("local Organ was not initialized"))?;
        let this_cell = store::cells::local(&store.pool)
            .await
            .map_err(IoError::other)?
            .ok_or_else(|| IoError::other("this Cell has no Cell Record"))?;
        let key_dir = utils::config::lince_data_dir()
            .ok_or_else(|| IoError::other("Cannot find the Lince data directory"))?;

        let mut supervisors = Vec::new();
        match start_karma(&engine, &this_cell.uid) {
            Ok(handle) => supervisors.push(handle),
            Err(error) => tracing::warn!(%error, "Karma schedules will not fire on this Cell"),
        }

        let organ_signer = engine::trust::Signer::load_or_create(
            &key_dir.join("keys").join("organ-ed25519-v1.key"),
            &local_organ.uid,
            &engine::roster::cell_key_id(&this_cell.uid),
        )
        .map_err(IoError::other)?;
        engine
            .set_organ_signer(organ_signer)
            .await
            .map_err(IoError::other)?;

        let mut tasks = Vec::new();
        let lanes = Arc::new(LaneHub::new());
        let wire = bind_wire(&engine, &store, &local_organ, &key_dir, &lanes, &mut tasks).await;
        let runtime = CellRuntime {
            engine: engine.clone(),
            store: store.clone(),
            lanes,
            wire: Arc::new(tokio::sync::RwLock::new(wire.clone())),
            information: None,
        };

        if let Some(wire) = wire.clone()
            && let Err(error) = publish_pairing_invite(&store, &local_organ.uid, &wire).await
        {
            tracing::warn!(%error, "Could not prepare the pairing invitation");
        }
        if let Err(error) =
            publish_local_roster(&engine, &local_organ.uid, &key_dir, wire.as_deref()).await
        {
            tracing::warn!(%error, "cannot publish the Cell roster");
        }
        if let Some(wire) = wire {
            wire.set_transfer_handler(Arc::new(transfer::TransferPeerHandler::new(
                runtime.clone(),
            )));
        }

        supervisors.push(engine::file_sync::spawn_supervisor(engine.clone()));
        supervisors.push(engine.clone().run(HEARTBEAT_PERIOD_SECS));
        tasks.push(transfer::spawn_worker(runtime.clone()));
        tasks.push(sync_runner::spawn_runner(runtime.clone()));
        tasks.push(wire_supervisor::spawn(runtime.clone(), key_dir));

        Ok(Cell {
            runtime,
            supervisors,
            tasks,
        })
    }

    pub async fn shutdown(mut self) {
        if let Some(wire) = self.runtime.wire.write().await.take() {
            wire.shutdown().await;
        }
        for handle in self.supervisors.drain(..) {
            handle.abort();
        }
        for handle in self.tasks.drain(..) {
            handle.abort();
        }
    }

    pub async fn start_information(
        &mut self,
        server: bool,
        address: Option<String>,
    ) -> Result<(), IoError> {
        if self.runtime.information.is_none() {
            let (channel, task) =
                information::start(self.runtime.store.clone(), server, address).await?;
            self.runtime.information = Some(channel);
            self.tasks.push(task);
        }
        Ok(())
    }

    pub fn runtime(&self) -> &CellRuntime {
        &self.runtime
    }

    pub fn engine(&self) -> &Arc<engine::Engine> {
        &self.runtime.engine
    }

    pub fn lanes(&self) -> &Arc<LaneHub> {
        &self.runtime.lanes
    }

    pub fn session(&self, connection_id: impl Into<String>, subject: Option<String>) -> Session {
        Session::new(
            self.runtime.engine.clone(),
            self.runtime.lanes.clone(),
            connection_id,
            subject,
        )
    }
}

impl Drop for Cell {
    fn drop(&mut self) {
        for handle in self.supervisors.drain(..) {
            handle.abort();
        }
        for handle in self.tasks.drain(..) {
            handle.abort();
        }
    }
}

pub fn default_lince_db_url() -> Result<String, IoError> {
    let dir = utils::config::lince_data_dir()
        .ok_or_else(|| IoError::other("Cannot find the Lince data directory"))?;
    std::fs::create_dir_all(&dir).map_err(|error| {
        IoError::other(format!(
            "Cannot open the Lince data directory {}: {error}",
            dir.display()
        ))
    })?;
    Ok(format!("sqlite://{}", dir.join("lince.db").display()))
}

async fn seed_cell(
    store: &Store,
    local_base_url: Option<&str>,
    language: Option<&str>,
) -> Result<(), IoError> {
    let permissions: Vec<(&str, &str)> = utils::auth::ALL_PERMISSIONS
        .iter()
        .map(|permission| (permission.subject, permission.action))
        .collect();
    store::seed::seed(&store.pool, &permissions)
        .await
        .map_err(IoError::other)?;
    store::organs::ensure_local(
        &store.pool,
        local_base_url.unwrap_or("http://127.0.0.1:6174"),
    )
    .await
    .map_err(IoError::other)?;
    if let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) {
        store::config::set_language(&store.pool, language)
            .await
            .map_err(IoError::other)?;
    }
    Ok(())
}

fn start_karma(
    engine: &Arc<engine::Engine>,
    cell_uid: &str,
) -> Result<tokio::task::JoinHandle<()>, engine::EngineError> {
    let config = engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(format!(
        "{cell_uid}:{}",
        std::process::id()
    ))?;
    engine.install_karma_runtime_config(config.clone())?;
    let director = engine.clone().start_karma_deadline_director(config);
    Ok(tokio::spawn(async move {
        match director.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Karma deadline director stopped"),
            Err(error) => tracing::warn!(%error, "Karma deadline director panicked"),
        }
    }))
}

async fn bind_wire(
    engine: &Arc<engine::Engine>,
    store: &Store,
    local_organ: &store::organs::OrganRecord,
    key_dir: &Path,
    lanes: &Arc<LaneHub>,
    tasks: &mut Vec<tokio::task::JoinHandle<()>>,
) -> Option<Arc<engine::wire::Wire>> {
    let secret = match engine::wire::node_secret(&key_dir.join("keys").join("node-ed25519-v1.key"))
    {
        Ok(secret) => secret,
        Err(error) => {
            tracing::warn!(%error, "no node key; peers unreachable");
            return None;
        }
    };
    let discovery = match discovery::for_organ(store, &local_organ.uid).await {
        Ok(discovery) => discovery,
        Err(error) => {
            tracing::warn!(%error, "Cannot read discovery settings. Peer connections remain off");
            return None;
        }
    };
    let wire = match engine::wire::Wire::bind_with_discovery(
        engine.clone(),
        secret,
        discovery.reach,
        Some(local_organ.head.as_str()),
        discovery.local,
    )
    .await
    {
        Ok(wire) => Arc::new(wire),
        Err(error) => {
            tracing::warn!(%error, "iroh endpoint unavailable; peers unreachable");
            return None;
        }
    };
    wire.set_live_handler(transport::live::LiveHost::new(
        engine.clone(),
        lanes.clone(),
    ));
    wire.serve_enrolment();
    let serving = wire.clone();
    tasks.push(tokio::spawn(async move { serving.serve().await }));
    Some(wire)
}

async fn publish_pairing_invite(
    store: &Store,
    organ_uid: &str,
    wire: &engine::wire::Wire,
) -> Result<(), IoError> {
    let invite = wire.pairing_invite().await.map_err(IoError::other)?;
    let encoded = invite.encode();
    let existing = store::records::get_extension(&store.pool, organ_uid, "lince.pairing")
        .await
        .map_err(IoError::other)?;
    let unchanged = existing
        .as_ref()
        .and_then(|fields| fields.get("invite").and_then(serde_json::Value::as_str))
        == Some(encoded.as_str());
    if unchanged {
        return Ok(());
    }
    let svg = invite.qr_svg().map_err(IoError::other)?;
    store::records::set_extension(
        &store.pool,
        organ_uid,
        "lince.pairing",
        &serde_json::json!({ "invite": encoded, "qr_svg": svg }),
    )
    .await
    .map_err(IoError::other)?;
    Ok(())
}
async fn publish_local_roster(
    engine: &engine::Engine,
    organ_uid: &str,
    key_dir: &std::path::Path,
    wire: Option<&engine::wire::Wire>,
) -> Result<(), IoError> {
    let root_path = key_dir.join("keys").join("root-ed25519-v1.key");
    let held = engine.roster_of(organ_uid).await.map_err(IoError::other)?;

    engine.set_root_key_path(root_path.clone());
    engine.set_sealing_keyring_path(key_dir.join("keys").join("cell-x25519-keyring-v1.json"));
    let creating = !root_path.try_exists()?;
    if creating {
        if held.is_some() {
            tracing::info!(
                "root key is not on this Cell; the published roster stays valid \
                 until it expires, and enrolling or revoking a device needs it back"
            );
            return Ok(());
        }
    }
    let root =
        engine::trust::Signer::load_or_create(&root_path, organ_uid, engine::roster::ROOT_KEY_ID)
            .map_err(IoError::other)?;
    let certificate_path = root_path.with_extension("revocation.json");
    if !certificate_path.try_exists()? {
        let (revoked_key, signature) = engine.revocation_certificate(&root);
        let certificate = serde_json::json!({
            "organ_uid": organ_uid,
            "revoked_key": revoked_key,
            "signature": signature,
        });
        if let Err(error) = std::fs::write(
            &certificate_path,
            serde_json::to_vec_pretty(&certificate).map_err(IoError::other)?,
        ) {
            tracing::warn!(%error, "could not write the pre-signed revocation certificate");
        } else {
            tracing::info!(
                path = %certificate_path.display(),
                "wrote the pre-signed revocation certificate; keep it with the root key, offline"
            );
        }
    }
    engine
        .publish_root_key(&root)
        .await
        .map_err(IoError::other)?;

    let Some(wire) = wire else {
        return Ok(());
    };

    let node_id = wire.node_id().to_string();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .map_err(IoError::other)?
        .ok_or_else(|| IoError::other("this Cell has no Cell Record"))?;
    let operational_key = engine
        .local_organ_public_key()
        .await
        .map_err(IoError::other)?
        .filter(|key| !key.is_empty())
        .ok_or_else(|| {
            IoError::other("Cannot publish the Cell roster: the local signing key is missing")
        })?;
    let mut cells: Vec<engine::roster::CellEntry> = held
        .as_ref()
        .map(|held| held.roster.cells.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|held_cell| held_cell.cell_uid != cell.uid)
        .collect();
    let sealing_key = engine
        .published_sealing_key()
        .await
        .map_err(IoError::other)?;
    cells.push(engine::roster::CellEntry {
        cell_uid: cell.uid.clone(),
        node_id,
        label: cell.label.clone(),
        operational_key,
        sealing_key,
        capabilities: engine::roster::full_capabilities(),
        front_door: wire.reach() != engine::wire::Reach::Local,
    });
    if engine::roster::needs_publishing(held.as_ref(), &root.public_key_b64(), &cells) {
        engine
            .publish_roster(&root, cells)
            .await
            .map_err(IoError::other)?;
    }
    if let Err(error) = engine.sign_public_record(&root).await {
        tracing::warn!(%error, "could not sign the public directory record");
        return Ok(());
    }
    republish_public_record(engine, organ_uid).await;
    Ok(())
}

async fn republish_public_record(engine: &engine::Engine, organ_uid: &str) {
    match engine.republish_public_record(organ_uid).await {
        Ok(true) => tracing::info!("published this Organ's front door under its identity key"),
        Ok(false) => {}
        Err(error) => tracing::warn!(%error, "could not publish the directory record"),
    }
}
