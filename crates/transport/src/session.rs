//! The session state machine (blueprint VII.3). Transport-agnostic: a real
//! socket driver calls `handle` for inbound client messages and `on_fact` for
//! each fact off the engine's `fact_bus`, forwarding every returned
//! `ServerMessage` to the client. No socket is needed to test it.

use std::collections::HashMap;
use std::sync::Arc;

use engine::Engine;
use nucleus::Fact;
use protein::Protein;

use crate::lane::{LaneEvent, LaneHub};
use crate::protocol::{ClientMessage, ServerMessage};

pub struct Session {
    engine: Arc<Engine>,
    hub: Arc<LaneHub>,
    /// This connection's identity — used as the Protein visibility subject
    /// (None = the local Cell, sees everything) and the lane `from` label.
    subject: Option<String>,
    connection_id: String,
    /// Active subscriptions: subscription id -> the Protein to re-run.
    subscriptions: HashMap<String, Protein>,
    joined_rooms: Vec<String>,
}

impl Session {
    pub fn new(
        engine: Arc<Engine>,
        hub: Arc<LaneHub>,
        connection_id: impl Into<String>,
        subject: Option<String>,
    ) -> Session {
        Session {
            engine,
            hub,
            subject,
            connection_id: connection_id.into(),
            subscriptions: HashMap::new(),
            joined_rooms: Vec::new(),
        }
    }

    pub fn joined_rooms(&self) -> &[String] {
        &self.joined_rooms
    }

    /// Handle one inbound client message, producing the responses to send back.
    pub async fn handle(&mut self, msg: ClientMessage) -> Vec<ServerMessage> {
        match msg {
            ClientMessage::Subscribe { id, protein } => self.subscribe(id, protein).await,
            ClientMessage::SubscribeSaved { id, name } => self.subscribe_saved(id, name).await,
            ClientMessage::Unsubscribe { id } => {
                self.subscriptions.remove(&id);
                vec![]
            }
            ClientMessage::Act { id, action } => vec![self.act(id, action).await],
            ClientMessage::LaneJoin { room } => {
                if !self.joined_rooms.contains(&room) {
                    self.joined_rooms.push(room);
                }
                vec![] // the driver wires the room receiver into the outbound loop
            }
            ClientMessage::LaneLeave { room } => {
                self.joined_rooms.retain(|r| r != &room);
                self.hub.prune(&room);
                vec![]
            }
            ClientMessage::LaneSend { room, payload } => {
                if self.joined_rooms.contains(&room) {
                    self.hub.send(LaneEvent {
                        room,
                        from: self.connection_id.clone(),
                        payload,
                    });
                }
                vec![] // presence is fire-and-forget; senders don't echo to self
            }
        }
    }

    async fn subscribe(&mut self, id: String, protein: Protein) -> Vec<ServerMessage> {
        match protein::execute_for(&self.engine.store, &protein, self.subject.as_deref()).await {
            Ok(rows) => {
                self.subscriptions.insert(id.clone(), protein);
                vec![ServerMessage::Snapshot { id, rows }]
            }
            Err(e) => vec![ServerMessage::Error {
                id,
                message: e.to_string(),
            }],
        }
    }

    async fn subscribe_saved(&mut self, id: String, name: String) -> Vec<ServerMessage> {
        match protein::execute_saved(&self.engine.store, &name, self.subject.as_deref()).await {
            Ok(rows) => {
                // materialize the saved AST so future `on_fact` recomputes it
                match load_saved(&self.engine, &name).await {
                    Ok(protein) => {
                        self.subscriptions.insert(id.clone(), protein);
                    }
                    Err(e) => return vec![ServerMessage::Error { id, message: e }],
                }
                vec![ServerMessage::Snapshot { id, rows }]
            }
            Err(e) => vec![ServerMessage::Error {
                id,
                message: e.to_string(),
            }],
        }
    }

    async fn act(&self, id: String, action: engine::actions::Action) -> ServerMessage {
        match self.engine.act(action, self.subject.clone()).await {
            Ok(outcome) => ServerMessage::ActionOk {
                id,
                created: outcome.created,
                facts: outcome.facts.len(),
            },
            Err(e) => ServerMessage::Error {
                id,
                message: e.to_string(),
            },
        }
    }

    /// A fact was committed (off `fact_bus`): push an Update for every
    /// subscription it may have changed (blueprint VII.1 live subscriptions).
    /// v1 invalidation is coarse via `protein::affects`; re-execution is a full
    /// re-send, correct if not yet minimal.
    pub async fn on_fact(&self, fact: &Fact) -> Vec<ServerMessage> {
        let mut out = Vec::new();
        for (id, protein) in &self.subscriptions {
            if !protein::affects(protein, fact) {
                continue;
            }
            match protein::execute_for(&self.engine.store, protein, self.subject.as_deref()).await {
                Ok(rows) => out.push(ServerMessage::Update {
                    id: id.clone(),
                    rows,
                }),
                Err(e) => out.push(ServerMessage::Error {
                    id: id.clone(),
                    message: e.to_string(),
                }),
            }
        }
        out
    }
}

async fn load_saved(engine: &Engine, name: &str) -> Result<Protein, String> {
    let record = store::records::resolve(&engine.store.pool, name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no saved protein {name}"))?;
    let ast = store::records::get_extension(&engine.store.pool, &record.uid, "lince.protein")
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{name} has no protein AST"))?;
    serde_json::from_value(ast).map_err(|e| e.to_string())
}
