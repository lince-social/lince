use super::{
    ALPN_SYNC, Connection, DIAL_TIMEOUT, EndpointAddr, EndpointId, Wire, WireRequest, WireResponse,
};
use crate::{Engine, EngineError, roster::CellEntry, sync::OpBatch};

impl Engine {
    pub async fn cell_delivery_status(&self) -> Result<serde_json::Value, EngineError> {
        let Some(organ) = store::organs::local(&self.store.pool).await? else {
            return Ok(serde_json::json!({"cells": []}));
        };
        let Some(signed) = self.roster_of(&organ.uid).await? else {
            return Ok(serde_json::json!({"cells": []}));
        };
        let ours = store::cells::local(&self.store.pool).await?;
        let recovery = if !crate::roster::roster_signature_is_valid(&signed) {
            Some(
                "Device membership has expired. Open the device holding this Organ's root key to renew it, then reconnect. Your local data is kept.",
            )
        } else if ours.as_ref().is_none_or(|ours| {
            !signed
                .roster
                .cells
                .iter()
                .any(|cell| cell.cell_uid == ours.uid)
        }) {
            Some(
                "This device was removed from the Organ. It keeps previously received data, but cannot exchange new changes. Ask the Organ owner to enrol a fresh profile.",
            )
        } else {
            None
        };
        let states = store::peer_delivery::list(&self.store.pool, &organ.uid).await?;
        let cells: Vec<_> = signed.roster.cells.iter()
            .filter(|cell| ours.as_ref().is_none_or(|ours| cell.cell_uid != ours.uid))
            .map(|cell| serde_json::json!({
                "cell_uid": cell.cell_uid,
                "label": cell.label,
                "delivery": states.iter().find(|state| state.cell_uid == cell.cell_uid && state.node_id == cell.node_id),
            })).collect();
        Ok(serde_json::json!({"cells": cells, "recovery": recovery}))
    }
}

impl Wire {
    pub(super) async fn restore_sibling_routes(&self) -> Result<(), EngineError> {
        let Some(organ) = store::organs::local(&self.engine.store.pool).await? else {
            return Ok(());
        };
        let Some(signed) = self.engine.roster_of(&organ.uid).await? else {
            return Ok(());
        };
        if !crate::roster::roster_signature_is_valid(&signed) {
            return Ok(());
        }
        for state in store::peer_delivery::list(&self.engine.store.pool, &organ.uid).await? {
            if !signed
                .roster
                .cells
                .iter()
                .any(|cell| cell.cell_uid == state.cell_uid && cell.node_id == state.node_id)
            {
                continue;
            }
            let Ok(id) = state.node_id.parse::<EndpointId>() else {
                continue;
            };
            let mut addr = EndpointAddr::new(id);
            for socket in state
                .addresses
                .iter()
                .take(16)
                .filter_map(|text| text.parse::<std::net::SocketAddr>().ok())
            {
                addr = addr.with_ip_addr(socket);
            }
            self.remember_addr(addr);
        }
        Ok(())
    }

    pub(super) async fn pull_siblings(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let Some(organ) = store::organs::local(pool).await? else {
            return Ok(0);
        };
        let Some(signed) = self.engine.roster_of(&organ.uid).await? else {
            return Ok(0);
        };
        let Some(ours) = store::cells::local(pool).await? else {
            return Ok(0);
        };
        if !crate::roster::roster_signature_is_valid(&signed)
            || !signed
                .roster
                .cells
                .iter()
                .any(|cell| cell.cell_uid == ours.uid && cell.node_id == self.node_id().to_string())
        {
            return Ok(0);
        }
        let mut moved = 0;
        for member in signed
            .roster
            .cells
            .iter()
            .filter(|cell| cell.cell_uid != ours.uid)
        {
            let result = self.exchange_sibling(&organ.uid, member).await;
            let stored = match result {
                Ok((count, covered, addresses)) => {
                    moved += count;
                    Ok((covered, addresses))
                }
                Err(error) => Err(error.to_string()),
            };
            store::peer_delivery::note(pool, &organ.uid, &member.cell_uid, &member.node_id, stored)
                .await?;
        }
        Ok(moved)
    }

    async fn exchange_sibling(
        &self,
        organ: &str,
        member: &CellEntry,
    ) -> Result<(usize, i64, Vec<String>), EngineError> {
        let id = member
            .node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("Invalid device address".into()))?;
        let connection = match tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(id), ALPN_SYNC),
        )
        .await
        {
            Ok(Ok(connection)) => connection,
            result => {
                self.note_if_stale(member, id).await;
                return Err(EngineError::Consequence(match result {
                    Ok(Err(error)) => {
                        format!("Could not connect: {error}. Check Wi-Fi and device membership.")
                    }
                    _ => "Device did not answer. Check that it is awake and on the same network."
                        .into(),
                }));
            }
        };
        self.refresh_roster(&connection, organ).await;
        if !self.roster_names_node(&member.node_id).await
            || !self.roster_names_node(&self.node_id().to_string()).await
        {
            connection.close(0u32.into(), b"roster membership changed");
            return Err(EngineError::Forbidden("Device membership changed".into()));
        }
        if let Err(error) = self.collect_door_requests(&connection).await {
            tracing::debug!(%error, cell = %member.label, "door not collected this pass");
        }
        let writable = self.engine.roster_of(organ).await?.is_some_and(|signed| {
            crate::roster::roster_signature_is_valid(&signed)
                && signed.roster.cells.iter().any(|current| {
                    current.cell_uid == member.cell_uid
                        && current.node_id == member.node_id
                        && current.may(crate::roster::CAP_WRITE)
                })
        });
        if !writable {
            return Err(EngineError::Forbidden(
                "This device has no permission to exchange Organ records".into(),
            ));
        }
        let pool = &self.engine.store.pool;
        let mut moved = 0;
        for _ in 0..4 {
            let vector = store::sync_ops::version_vector_for_organ(pool, organ).await?;
            let response = self
                .exchange(
                    &connection,
                    &WireRequest::FetchOpsSince { vector, limit: 500 },
                )
                .await?;
            let WireResponse::Ops {
                ops, from_organ, ..
            } = response
            else {
                return Err(unexpected(response));
            };
            if from_organ != organ {
                return Err(EngineError::Forbidden(
                    "The device sent a different Organ's data".into(),
                ));
            }
            if ops.is_empty() {
                break;
            }
            self.engine
                .import_op_batch(&OpBatch { from_organ, ops })
                .await?;
            moved += 1;
        }
        let mut vector = self.sibling_vector(&connection, organ).await?;
        for _ in 0..4 {
            let page = self.engine.export_sync_page(organ, &vector, 500).await?;
            if page.batch.ops.is_empty() {
                break;
            }
            let response = self
                .exchange(&connection, &WireRequest::PushOps { batch: page.batch })
                .await?;
            if !matches!(response, WireResponse::BatchSaved { complete: true, .. }) {
                return Err(unexpected(response));
            }
            moved += 1;
            vector = self.sibling_vector(&connection, organ).await?;
        }
        let covered = store::sync_ops::seq_covered_by_vector(pool, organ, &vector).await?;
        let addresses = connection
            .paths()
            .iter()
            .filter_map(|path| match path.remote_addr() {
                iroh::TransportAddr::Ip(addr) => Some(addr.to_string()),
                _ => None,
            })
            .take(16)
            .collect();
        Ok((moved, covered, addresses))
    }

    async fn sibling_vector(
        &self,
        connection: &Connection,
        organ: &str,
    ) -> Result<Vec<store::sync_ops::VectorEntry>, EngineError> {
        match self
            .exchange(
                connection,
                &WireRequest::FetchVector {
                    organ_uid: organ.into(),
                },
            )
            .await?
        {
            WireResponse::Vector { vector } => Ok(vector),
            response => Err(unexpected(response)),
        }
    }
}

fn unexpected(response: WireResponse) -> EngineError {
    match response {
        WireResponse::Refused { code, message } => {
            EngineError::Forbidden(format!("{code}: {message}"))
        }
        WireResponse::Error { message } => EngineError::Consequence(message),
        WireResponse::BatchSaved {
            complete: false, ..
        } => EngineError::Consequence(
            "The other device could not save every change; delivery is unconfirmed".into(),
        ),
        _ => EngineError::Consequence("Unexpected sync reply; delivery is unconfirmed".into()),
    }
}
