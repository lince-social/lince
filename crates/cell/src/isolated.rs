use std::io::Error;
use std::sync::Arc;

use engine::trust::Signer;
use nucleus::execution::Execution;
use store::Store;

use crate::{Cell, CellRuntime, LaneHub};

impl CellRuntime {
    pub async fn start_loopback_peer(
        &self,
        key_path: &std::path::Path,
    ) -> Result<tokio::task::JoinHandle<()>, Error> {
        let secret = engine::wire::node_secret(key_path).map_err(Error::other)?;
        let wire = Arc::new(
            engine::wire::Wire::bind_loopback(self.engine.clone(), secret)
                .await
                .map_err(Error::other)?,
        );
        wire.set_live_handler(transport::live::LiveHost::new(
            self.engine.clone(),
            self.lanes.clone(),
        ));
        self.engine.attach_social_network(wire.clone());
        self.engine.attach_location_network(wire.clone());
        wire.serve_enrolment();
        *self.wire.write().await = Some(wire.clone());
        Ok(tokio::spawn(async move { wire.serve().await }))
    }
}

impl Cell {
    pub async fn isolated(
        store: Store,
        execution: &Execution,
        secret: [u8; 32],
    ) -> Result<Self, Error> {
        execution
            .scope(async {
                let permissions: Vec<_> = utils::auth::ALL_PERMISSIONS
                    .iter()
                    .map(|permission| (permission.subject, permission.action))
                    .collect();
                store::seed::seed(&store.pool, &permissions)
                    .await
                    .map_err(Error::other)?;
                let organ = store::organs::local(&store.pool)
                    .await
                    .map_err(Error::other)?
                    .ok_or_else(|| Error::other("isolated Cell seed has no local Organ"))?;
                let cell = store::cells::local(&store.pool)
                    .await
                    .map_err(Error::other)?
                    .ok_or_else(|| Error::other("isolated Cell seed has no local Cell"))?;
                let engine = Arc::new(
                    engine::Engine::new(store.clone())
                        .await
                        .map_err(Error::other)?,
                );
                let public_key =
                    Signer::from_bytes(&organ.uid, "simulation", secret).public_key_b64();
                let ordinary_key = engine::roster::cell_key_id(&cell.uid);
                let registered = engine::trust::key_of(&store, &organ.uid, &ordinary_key)
                    .await
                    .map_err(Error::other)?;
                let roster = engine.roster_of(&organ.uid).await.map_err(Error::other)?;
                let key_id = if registered.is_none_or(|key| key == public_key)
                    && roster.is_none_or(|roster| {
                        roster.roster.cells.iter().any(|member| {
                            member.cell_uid == cell.uid && member.operational_key == public_key
                        })
                    }) {
                    ordinary_key
                } else {
                    format!(
                        "simulation:{}:{}",
                        cell.uid,
                        nucleus::fact::sha256_hex(&secret)
                    )
                };
                let signer = Signer::from_bytes(&organ.uid, &key_id, secret);
                engine
                    .set_organ_signer(signer.clone())
                    .await
                    .map_err(Error::other)?;
                engine.set_signer(signer).await.map_err(Error::other)?;
                engine
                    .install_karma_runtime_config(
                        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host(format!(
                            "{uid}:isolated",
                            uid = cell.uid
                        ))
                        .map_err(Error::other)?,
                    )
                    .map_err(Error::other)?;
                Ok(Self {
                    runtime: CellRuntime {
                        speech: None,
                        commands: Default::default(),
                        engine,
                        store,
                        lanes: Arc::new(LaneHub::new()),
                        wire: Arc::new(tokio::sync::RwLock::new(None)),
                        information: None,
                        fiote: None,
                    },
                    supervisors: Vec::new(),
                    tasks: Vec::new(),
                })
            })
            .await
    }
}
