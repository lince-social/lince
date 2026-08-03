//! iroh peer transport (Ontology §11 "Transport: iroh").
//!
//! What changes versus the signed-HTTP path in `peers.rs`: how two Organs find
//! and reach each other. What does NOT change: the op log, checkpoints, Loro
//! merge, trust, visibility, and `sync_out`/`sync_in`. The JSON bodies carried
//! here are the same ones `/organ/inbox` and `/organ/ops` carry today.
//!
//! The security shape, stated once because everything below depends on it: an
//! iroh connection is MUTUALLY AUTHENTICATED at the QUIC/TLS handshake against
//! raw public keys, so `Connection::remote_id()` returns a peer identity that
//! has already proven possession of the matching private key. That single call
//! replaces `verify_signed_request` — the timestamp freshness window, the
//! replay bound, and the per-request signature all become unnecessary, because
//! there is no unauthenticated moment on the wire to defend.
//!
//! Payload signing is NOT retired by any of this. Transport auth answers "who
//! is on this socket"; op-batch signatures answer "who wrote this op", which
//! must still hold a year later, from a backup, with no connection in sight.

use std::path::Path;
use std::sync::Arc;

use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::sync::{Introduction, OpBatch, WireOp};

/// Sync between Organs that already know each other. Versioned so a protocol
/// change bumps to `/2` and both can be served during a transition — an old
/// peer gets old behaviour instead of a broken half-upgrade (Ontology §11
/// "Compatibility and revocation floor").
pub const ALPN_SYNC: &[u8] = b"lince/sync/1";

/// First contact from an Organ we hold no contact row for. Deliberately a
/// SEPARATE ALPN rather than a check inside the sync handler: a stranger then
/// cannot negotiate the sync protocol at all, so the gate holds at the TLS
/// layer and the sync handler never runs for an unknown peer even if a later
/// bug weakens its own checks.
pub const ALPN_THREAD: &[u8] = b"lince/thread/1";

/// Ceiling on one request or response frame. A peer is authenticated but not
/// therefore trusted with unbounded memory — an authenticated contact having
/// a bug is exactly as fatal as a hostile one if nothing bounds the read.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// The per-Cell node key, created at first boot at mode 0600.
///
/// This is NOT the Organ identity key, and the separation is deliberate from
/// day one even on a single-Cell Organ (Ontology §11). The node key is on the
/// network constantly and lives on every device including the least trusted
/// one; if it doubled as the identity key, compromising any running Cell would
/// forge that Organ's history forever. Split, a stolen node key costs one
/// connection identity and the attacker still cannot sign a single op.
/// Separating later would invalidate every key anyone had already saved.
pub fn node_secret(path: &Path) -> Result<SecretKey, EngineError> {
    Ok(SecretKey::from_bytes(&crate::trust::load_or_create_secret(
        path,
    )?))
}

/// How this endpoint should be reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// LAN plus internet: relays, DNS and pkarr address publishing. The
    /// DEFAULT, because a Cell that is not resolvable across the internet
    /// cannot serve the case that motivates the whole design — the always-on
    /// Cell telling the phone about a change the laptop made.
    Internet,
    /// No relays and no address publishing: reachable only where a direct
    /// path already exists. Used by tests, and by anyone who wants a Cell to
    /// leak neither approximate location nor online-hours to key holders.
    Local,
}

/// One request/response exchange, carrying the same JSON the HTTP peer routes
/// carry. Internally tagged so an unrecognised `op` from a NEWER peer fails to
/// deserialize and is answered with an error, rather than being silently
/// misread as something else (Ontology §11: fail closed on the unknown).
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum WireRequest {
    /// Key and name exchange. Under iroh this establishes nothing the
    /// connection did not already establish — it is no longer a challenge.
    Introduction,
    PushOps { batch: OpBatch },
    FetchOps { after: i64, limit: i64 },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "ok", rename_all = "snake_case")]
pub enum WireResponse {
    Introduction {
        intro: Introduction,
    },
    Applied {
        applied: usize,
    },
    Ops {
        from_organ: String,
        ops: Vec<WireOp>,
        head: i64,
    },
    /// A refusal the peer may act on. Never leaks whether a contact row exists
    /// beyond what the ALPN gate already revealed by accepting the connection.
    Error {
        message: String,
    },
}

/// The Cell's iroh endpoint plus the engine it serves.
#[derive(Clone)]
pub struct Wire {
    endpoint: Endpoint,
    engine: Arc<Engine>,
}

impl Wire {
    /// Bind an endpoint on `secret`, serving both ALPNs.
    ///
    /// Discovery is an Endpoint BUILDER option fixed at construction, so
    /// changing `reach` means rebinding rather than mutating — the caller is
    /// expected to restart the endpoint the way the File Sync live supervisor
    /// restarts watchers on a config Fact, not to demand a reboot.
    pub async fn bind(
        engine: Arc<Engine>,
        secret: SecretKey,
        reach: Reach,
    ) -> Result<Wire, EngineError> {
        let alpns = vec![ALPN_SYNC.to_vec(), ALPN_THREAD.to_vec()];
        let endpoint = match reach {
            Reach::Internet => Endpoint::builder(presets::N0),
            Reach::Local => Endpoint::builder(presets::Minimal),
        }
        .secret_key(secret)
        .alpns(alpns)
        .bind()
        .await
        .map_err(|error| EngineError::Consequence(format!("iroh bind failed: {error}")))?;
        Ok(Wire { endpoint, engine })
    }

    /// This Cell's NodeId — the one string a contact saves, and the thing a QR
    /// encodes.
    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Accept connections until the endpoint closes. Each connection is served
    /// on its own task so one slow peer cannot stall the others.
    pub async fn serve(&self) {
        while let Some(incoming) = self.endpoint.accept().await {
            let wire = self.clone();
            tokio::spawn(async move {
                let connection = match incoming.await {
                    Ok(connection) => connection,
                    // A failed handshake is normal background noise (a probe, a
                    // half-open NAT path). Nothing was authenticated, so there
                    // is nothing to report to a user.
                    Err(error) => {
                        tracing_debug(&format!("iroh handshake failed: {error}"));
                        return;
                    }
                };
                if let Err(error) = wire.serve_connection(connection).await {
                    tracing_debug(&format!("iroh connection ended: {error}"));
                }
            });
        }
    }

    /// Gate by contact state, then serve request frames until the peer hangs
    /// up. The gate is the whole of the accept-side authorization decision.
    async fn serve_connection(&self, connection: Connection) -> Result<(), EngineError> {
        let peer = connection.remote_id();
        let alpn = connection.alpn().to_vec();
        let contact =
            store::organs::contact_by_node_id(&self.engine.store.pool, &peer.to_string()).await?;

        // `blocked` is terminal everywhere (Ontology §2): close without
        // answering. Checked BEFORE the ALPN split so a blocked Organ cannot
        // reach the thread door either.
        if contact.as_ref().is_some_and(|c| c.trust == "blocked") {
            connection.close(0u32.into(), b"blocked");
            return Ok(());
        }

        // `known` is what opens sync — NOT merely having a contact row. A row
        // with `trust='unknown'` is someone added but not yet vetted, and the
        // policy is explicit that they get the thread door and nothing else.
        let known = contact.as_ref().is_some_and(|c| c.trust == "known");

        match (alpn.as_slice(), known) {
            (ALPN_SYNC, true) => {}
            (ALPN_SYNC, false) => {
                // Not known: never reaches the sync protocol. They may knock on
                // `lince/thread/1` instead — that is the invite door.
                //
                // One reason for both "no row" and "row, not yet known", so the
                // refusal does not tell a prober which of the two they are.
                connection.close(0u32.into(), b"unknown organ");
                return Ok(());
            }
            (ALPN_THREAD, _) => {
                // The invite/thread door lands with the Threads stage; until
                // then, refuse rather than half-serve.
                connection.close(0u32.into(), b"threads not yet served");
                return Ok(());
            }
            _ => {
                connection.close(0u32.into(), b"unsupported alpn");
                return Ok(());
            }
        }

        let from_organ = contact
            .map(|contact| contact.record_uid)
            .unwrap_or_default();

        loop {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(streams) => streams,
                // Clean hang-up, or the peer went away. Either way we are done.
                Err(_) => return Ok(()),
            };
            let raw = recv
                .read_to_end(MAX_FRAME_BYTES)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer frame: {error}")))?;
            let response = match serde_json::from_slice::<WireRequest>(&raw) {
                Ok(request) => self.handle(&from_organ, request).await,
                Err(error) => WireResponse::Error {
                    message: format!("unreadable request: {error}"),
                },
            };
            let bytes = serde_json::to_vec(&response)
                .map_err(|error| EngineError::Consequence(error.to_string()))?;
            send.write_all(&bytes)
                .await
                .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
            send.finish()
                .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        }
    }

    /// Serve one request. `authenticated` is the contact uid iroh proved on
    /// this connection — never a value read out of the request body.
    async fn handle(&self, authenticated: &str, request: WireRequest) -> WireResponse {
        match request {
            WireRequest::Introduction => match self.engine.introduction().await {
                Ok(intro) => WireResponse::Introduction { intro },
                Err(error) => WireResponse::Error {
                    message: error.to_string(),
                },
            },
            WireRequest::PushOps { batch } => {
                // The batch must belong to the Organ the HANDSHAKE proved, not
                // to whoever the body claims. This is the same check the HTTP
                // path made against `verify_signed_request`'s return, and it
                // is what stops an authenticated contact from writing ops
                // attributed to a third Organ.
                if batch.from_organ != authenticated {
                    return WireResponse::Error {
                        message: "batch/peer mismatch".into(),
                    };
                }
                match self.engine.import_op_batch(&batch).await {
                    Ok(applied) => WireResponse::Applied { applied },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
            WireRequest::FetchOps { after, limit } => {
                let limit = limit.clamp(1, 2000);
                let local = match store::organs::local(&self.engine.store.pool).await {
                    Ok(organ) => organ.map(|organ| organ.uid).unwrap_or_default(),
                    Err(error) => {
                        return WireResponse::Error {
                            message: error.to_string(),
                        };
                    }
                };
                match self.engine.ops_after(after, limit).await {
                    Ok((ops, head)) => WireResponse::Ops {
                        from_organ: local,
                        ops,
                        head,
                    },
                    Err(error) => WireResponse::Error {
                        message: error.to_string(),
                    },
                }
            }
        }
    }

    /// Dial `addr` and run one request/response exchange.
    ///
    /// No response signature is checked, and none is needed: the handshake
    /// already proved the answering endpoint holds the private half of the
    /// NodeId we dialed. Reaching that NodeId reaches that keypair or nothing,
    /// so there is no wire left to substitute on.
    pub async fn request(
        &self,
        addr: impl Into<EndpointAddr>,
        alpn: &[u8],
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let connection = self
            .endpoint
            .connect(addr, alpn)
            .await
            .map_err(|error| EngineError::Consequence(format!("iroh connect failed: {error}")))?;
        let response = self.exchange(&connection, request).await;
        connection.close(0u32.into(), b"done");
        response
    }

    async fn exchange(
        &self,
        connection: &Connection,
        request: &WireRequest,
    ) -> Result<WireResponse, EngineError> {
        let (mut send, mut recv) = connection
            .open_bi()
            .await
            .map_err(|error| EngineError::Consequence(format!("peer open: {error}")))?;
        let bytes =
            serde_json::to_vec(request).map_err(|error| EngineError::Consequence(error.to_string()))?;
        send.write_all(&bytes)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer write: {error}")))?;
        send.finish()
            .map_err(|error| EngineError::Consequence(format!("peer finish: {error}")))?;
        let raw = recv
            .read_to_end(MAX_FRAME_BYTES)
            .await
            .map_err(|error| EngineError::Consequence(format!("peer read: {error}")))?;
        serde_json::from_slice(&raw)
            .map_err(|error| EngineError::Consequence(format!("unreadable response: {error}")))
    }
}

fn tracing_debug(message: &str) {
    // The engine crate carries no tracing dependency; peer-connection noise is
    // expected and non-actionable, so it stays out of the Ledger entirely.
    let _ = message;
}
