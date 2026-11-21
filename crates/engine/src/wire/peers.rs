// non-lean-yet

//! Peer identity, pairing, direct connections, and roster reachability.

use std::{net::SocketAddr, sync::Arc};

use iroh::{EndpointAddr, EndpointId, endpoint::Connection};

use super::{
    ALPN_HELLO, ALPN_LIVE, ALPN_SYNC, ALPN_THREAD, DIAL_STAGGER, DIAL_TIMEOUT, LiveSessions,
    PeerAddr, Reach, StaleSibling, WIRE_EPOCH, Wire, WireRequest, WireResponse,
};
use crate::{
    error::EngineError, pairing::EnrolmentInvite, roster::SignedRoster, sync::Introduction,
};

impl Wire {
    pub fn set_live_handler(&self, handler: Arc<dyn LiveSessions>) {
        *self.live.lock().expect("live handler") = Some(handler);
    }

    pub fn remember_live_connection(&self, contact_organ: &str, connection: &Connection) {
        self.live_connections
            .lock()
            .expect("live connections")
            .insert(contact_organ.to_string(), connection.clone());
    }

    pub async fn open_live(&self, contact_organ: &str) -> Result<Connection, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, contact_organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("no address for this contact".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(node_id), ALPN_LIVE),
        )
        .await
        .map_err(|_| EngineError::Consequence("they did not answer".into()))?
        .map_err(|error| EngineError::Consequence(format!("live dial failed: {error}")))
    }

    pub async fn open_live_at(
        &self,
        invite: &crate::pairing::PairingInvite,
    ) -> Result<Connection, EngineError> {
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<std::net::SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }
        tokio::time::timeout(DIAL_TIMEOUT, self.endpoint.connect(addr, ALPN_LIVE))
            .await
            .map_err(|_| EngineError::Consequence("they did not answer".into()))?
            .map_err(|error| EngineError::Consequence(format!("live dial failed: {error}")))
    }

    pub async fn enrol(&self, invite: &EnrolmentInvite) -> Result<SignedRoster, EngineError> {
        self.engine.may_enrol().await?;
        let cell = store::cells::local(&self.engine.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Cell Record".into()))?;
        let node_id: EndpointId = invite.node_id.parse().map_err(|_| {
            EngineError::Consequence("the code carries an unreadable node id".into())
        })?;
        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }
        let operational = self.engine.operational_key_for(&invite.organ_uid).await?;
        let operational_key = operational.public_key_b64();
        let sealing_key = self.engine.published_sealing_key().await?;
        let request = WireRequest::Enrol {
            token: invite.token.clone(),
            cell_uid: cell.uid.clone(),
            node_id: self.node_id().to_string(),
            label: cell.label.clone(),
            operational_key,
            sealing_key,
        };
        let response = tokio::time::timeout(
            DIAL_TIMEOUT * 2,
            self.request(addr, ALPN_THREAD, &request),
        )
        .await
        .map_err(|_| EngineError::Consequence("The other device did not answer within 12 seconds. Check its network connection and allow Lince's UDP port through its firewall, then try again.".into()))?
        .map_err(|error| {
            EngineError::Consequence(format!(
                "Could not connect to the other device. Check its network connection and firewall. If the code was already used or expired, create a new one. ({error})"
            ))
        })?;
        let signed = match response {
            WireResponse::Roster {
                roster: Some(signed),
            } => signed,
            WireResponse::Roster { roster: None } => {
                return Err(EngineError::Consequence(
                    "the other Cell accepted the code but returned no roster".into(),
                ));
            }
            WireResponse::Refused { code, message } => {
                return Err(EngineError::Consequence(format!("{code}: {message}")));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected answer to enrolment: {other:?}"
                )));
            }
        };
        if !signed.roster.cells.iter().any(|member| {
            member.cell_uid == cell.uid && member.node_id == self.node_id().to_string()
        }) {
            return Err(EngineError::Consequence(
                "The returned roster does not name this network endpoint".into(),
            ));
        }
        self.engine.join_organ(invite, &signed, operational).await?;
        Ok(signed)
    }

    pub fn remember_addr(&self, addr: PeerAddr) {
        self.known_addrs.add_endpoint_info(addr);
    }

    pub async fn pairing_invite(&self) -> Result<crate::pairing::PairingInvite, EngineError> {
        let organ = store::organs::local(&self.engine.store.pool).await?;
        let root_key = match &organ {
            Some(organ) => self.engine.current_root_key(&organ.uid).await?,
            None => None,
        };
        Ok(crate::pairing::PairingInvite {
            node_id: self.node_id().to_string(),
            root_key,
            label: organ.map(|organ| organ.head),
            addrs: if self
                .private
                .is_some_and(|config| !config.listen_addr.ip().is_unspecified())
            {
                self.endpoint
                    .bound_sockets()
                    .into_iter()
                    .map(|addr| addr.to_string())
                    .collect()
            } else {
                self.endpoint
                    .addr()
                    .ip_addrs()
                    .map(|addr| addr.to_string())
                    .collect()
            },
        })
    }

    pub async fn pair_with(
        &self,
        invite: &crate::pairing::PairingInvite,
        name: &str,
    ) -> Result<String, EngineError> {
        let node_id: EndpointId = invite
            .node_id
            .parse()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;

        let mut addr = EndpointAddr::new(node_id);
        for text in &invite.addrs {
            if let Ok(socket) = text.parse::<std::net::SocketAddr>() {
                addr = addr.with_ip_addr(socket);
            }
        }
        if !invite.addrs.is_empty() {
            self.remember_addr(addr.clone());
        }

        let ours = self.engine.introduction().await?;
        let response = self
            .request(
                addr.clone(),
                ALPN_THREAD,
                &WireRequest::Introduce { intro: ours },
            )
            .await?;
        let intro = match response {
            WireResponse::Introduction { intro } => intro,
            other => {
                return Err(EngineError::Consequence(format!(
                    "pairing refused: {other:?}"
                )));
            }
        };
        self.engine.adopt_introduction(&intro, 1).await?;

        if let Ok(WireResponse::Roster {
            roster: Some(roster),
        }) = self
            .request(addr, ALPN_THREAD, &WireRequest::FetchRoster)
            .await
        {
            self.adopt_fetched_roster(&roster, &intro.organ_uid).await;
        }

        let pool = &self.engine.store.pool;
        if !name.trim().is_empty() {
            store::records::set_text(pool, &intro.organ_uid, Some(name.trim()), None).await?;
        }
        store::organs::set_node_id(pool, &intro.organ_uid, Some(&invite.node_id)).await?;
        store::organs::set_trust(pool, &intro.organ_uid, "known").await?;
        Ok(intro.organ_uid)
    }

    pub(super) async fn bind_unknown_peer(
        &self,
        node_id: &str,
        intro: &Introduction,
    ) -> Result<String, EngineError> {
        let pool = &self.engine.store.pool;
        if store::organs::local(pool)
            .await?
            .is_some_and(|local| local.uid == intro.organ_uid)
        {
            return Err(EngineError::Forbidden(
                "a remote Cell claimed this Cell's Organ uid".into(),
            ));
        }
        if let Some(by_node) = store::organs::contact_by_node_id(pool, node_id).await? {
            if by_node.record_uid != intro.organ_uid {
                return Err(EngineError::Forbidden(
                    "this NodeId is already bound to another Organ".into(),
                ));
            }
            return Ok(by_node.record_uid);
        }
        if let Some(by_uid) = store::organs::contact(pool, &intro.organ_uid).await? {
            if by_uid
                .node_id
                .as_deref()
                .is_some_and(|held| held != node_id)
            {
                return Err(EngineError::Forbidden(
                    "this Organ is already bound to another NodeId".into(),
                ));
            }
            if by_uid.trust == "blocked" {
                return Err(EngineError::Forbidden("this Organ is blocked".into()));
            }
            store::organs::set_node_id(pool, &intro.organ_uid, Some(node_id)).await?;
            return Ok(intro.organ_uid.clone());
        }

        let organ_uid = self.engine.adopt_introduction(intro, 1).await?;
        store::organs::set_node_id(pool, &organ_uid, Some(node_id)).await?;
        store::organs::set_trust(pool, &organ_uid, "unknown").await?;
        Ok(organ_uid)
    }

    pub async fn offer_conversation_to_node(
        &self,
        node_id: &str,
        title: &str,
    ) -> Result<(String, String), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed NodeId".into()))?;
        let remote = self
            .request(
                EndpointAddr::new(id),
                ALPN_THREAD,
                &WireRequest::Introduction,
            )
            .await?;
        let WireResponse::Introduction {
            intro: remote_intro,
        } = remote
        else {
            return Err(EngineError::Consequence(
                "the nearby Cell refused to introduce itself".into(),
            ));
        };
        let contact = self.bind_unknown_peer(node_id, &remote_intro).await?;
        let (conversation, thread) = self.engine.start_conversation(&contact, title).await?;
        let intro = self.engine.introduction().await?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_THREAD,
                &WireRequest::OfferGrant {
                    root: conversation.clone(),
                    title: title.to_string(),
                    intro,
                },
            )
            .await?
        {
            WireResponse::Applied { .. } => Ok((conversation, thread)),
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                Err(EngineError::Consequence(message))
            }
            other => Err(EngineError::Consequence(format!(
                "unexpected conversation-offer response: {other:?}"
            ))),
        }
    }

    pub async fn fetch_reference(
        &self,
        owner_organ: &str,
        root: &str,
        record: &str,
    ) -> Result<serde_json::Value, EngineError> {
        let contact = store::organs::contact(&self.engine.store.pool, owner_organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("not a contact".into()))?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("no address for this contact".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("malformed node id".into()))?;
        let request = WireRequest::FetchReference {
            root: root.to_string(),
            record: record.to_string(),
        };
        match self
            .request(EndpointAddr::new(node_id), ALPN_THREAD, &request)
            .await?
        {
            WireResponse::Reference { row } => Ok(row),
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                Err(EngineError::Consequence(message))
            }
            other => Err(EngineError::Consequence(format!(
                "unexpected reference response: {other:?}"
            ))),
        }
    }

    pub async fn answer_conversation_invite(
        &self,
        invite_uid: &str,
        accept: bool,
    ) -> Result<String, EngineError> {
        let invite = store::invites::get(&self.engine.store.pool, invite_uid)
            .await?
            .ok_or_else(|| EngineError::Consequence("no such invite".into()))?;
        let contact = store::organs::contact(&self.engine.store.pool, &invite.from_organ)
            .await?
            .ok_or_else(|| {
                EngineError::Consequence("invite sender has no identity binding".into())
            })?;
        let node_id = contact
            .node_id
            .as_deref()
            .ok_or_else(|| EngineError::Consequence("invite sender has no NodeId".into()))?
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("invite sender has a malformed NodeId".into()))?;
        let request = if accept {
            WireRequest::AcceptGrant {
                root: invite.root.clone(),
            }
        } else {
            WireRequest::DeclineGrant {
                root: invite.root.clone(),
            }
        };
        match self
            .request(EndpointAddr::new(node_id), ALPN_THREAD, &request)
            .await?
        {
            WireResponse::Applied { .. } => {}
            WireResponse::Refused { message, .. } | WireResponse::Error { message } => {
                return Err(EngineError::Consequence(message));
            }
            other => {
                return Err(EngineError::Consequence(format!(
                    "unexpected invite response: {other:?}"
                )));
            }
        }
        if accept {
            self.engine.accept_invite(invite_uid).await?;
        } else {
            self.engine.decline_invite(invite_uid).await?;
        }
        Ok(invite.root)
    }

    pub async fn roster_names_node(&self, node_id: &str) -> bool {
        let Ok(Some(organ)) = store::organs::local(&self.engine.store.pool).await else {
            return false;
        };
        let Ok(Some(signed)) = self.engine.roster_of(&organ.uid).await else {
            return false;
        };
        if !crate::roster::roster_signature_is_valid(&signed) {
            return false;
        }
        signed
            .roster
            .cells
            .iter()
            .any(|member| member.node_id == node_id)
    }

    pub async fn sibling_organ(&self, node_id: &str) -> Option<String> {
        let organ = store::organs::local(&self.engine.store.pool).await.ok()??;
        let signed = self.engine.roster_of(&organ.uid).await.ok()??;
        if !crate::roster::roster_signature_is_valid(&signed) {
            return None;
        }
        let ours = store::cells::local(&self.engine.store.pool)
            .await
            .ok()??
            .uid;
        signed
            .roster
            .cells
            .iter()
            .find(|member| {
                member.node_id == node_id
                    && member.cell_uid != ours
                    && member.may(crate::roster::CAP_WRITE)
            })
            .map(|_| organ.uid)
    }

    pub(crate) async fn may_represent(&self) -> bool {
        let pool = &self.engine.store.pool;
        let (Ok(Some(organ)), Ok(Some(cell))) = (
            store::organs::local(pool).await,
            store::cells::local(pool).await,
        ) else {
            return false;
        };
        if self.engine.identity_in_flux() {
            return false;
        }
        match self.engine.roster_of(&organ.uid).await {
            Ok(None) => true,
            Ok(Some(_)) => self
                .engine
                .cell_may(&organ.uid, &cell.uid, crate::roster::CAP_REPRESENT)
                .await
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    pub async fn reconnect_contact(&self, organ: &str) -> Result<(), EngineError> {
        let pool = &self.engine.store.pool;
        let contact = store::organs::contact(pool, organ)
            .await?
            .ok_or_else(|| EngineError::Consequence("Choose an existing contact".into()))?;
        if contact.trust == "blocked" {
            return Err(EngineError::Forbidden("This contact is blocked".into()));
        }
        let connection = self.dial(&contact).await.ok_or_else(|| {
            EngineError::Consequence(
                "Contact unreachable. Ask them to open Lince, then reconnect.".into(),
            )
        })?;
        let ours = self.engine.introduction().await?;
        match self
            .exchange(&connection, &WireRequest::Introduce { intro: ours })
            .await?
        {
            WireResponse::Introduction { intro } if intro.organ_uid == organ => {
                self.engine
                    .adopt_introduction(&intro, contact.proximity)
                    .await?;
            }
            _ => {
                return Err(EngineError::Forbidden(
                    "Reconnect returned a different contact identity".into(),
                ));
            }
        }
        self.refresh_roster(&connection, organ).await;
        self.refresh_organ_access(&connection, organ).await;
        let roster = self.engine.roster_of(organ).await?.ok_or_else(|| EngineError::Consequence("The contact has no device list yet. Ask them to create or renew it, then reconnect.".into()))?;
        if !crate::roster::roster_signature_is_valid(&roster) {
            return Err(EngineError::Consequence(
                "The contact's device list has expired. Ask them to renew it, then reconnect."
                    .into(),
            ));
        }
        let eligible = |cell: &&crate::roster::CellEntry| {
            cell.may(crate::roster::CAP_WRITE) && cell.may(crate::roster::CAP_REPRESENT)
        };
        if !roster
            .roster
            .cells
            .iter()
            .filter(eligible)
            .any(|cell| cell.node_id == connection.remote_id().to_string())
            && let Some(cell) = roster.roster.cells.iter().find(eligible)
        {
            store::organs::set_node_id(pool, organ, Some(&cell.node_id)).await?;
        }
        store::organs::clear_awaiting_roster(pool, organ).await?;
        store::mailbox::outbox::retry_recipient(pool, organ).await?;
        self.engine.notify_query_changed();
        Ok(())
    }

    pub async fn dial(&self, contact: &store::organs::Contact) -> Option<Connection> {
        let connection = self.dial_candidates(contact).await;
        let pool = &self.engine.store.pool;
        let noted = match connection {
            Some(_) => store::organs::mark_reachable(pool, &contact.record_uid).await,
            None => store::organs::mark_unreachable(pool, &contact.record_uid).await,
        };
        if let Err(error) = noted {
            tracing::debug!(%error, contact = %contact.record_uid, "reachability not recorded");
        }
        connection
    }

    async fn dial_candidates(&self, contact: &store::organs::Contact) -> Option<Connection> {
        let mut candidates: Vec<String> = Vec::new();
        if let Some(node_id) = &contact.node_id {
            candidates.push(node_id.clone());
        }
        if let Ok(from_roster) = self.engine.roster_node_ids(&contact.record_uid).await {
            for node_id in from_roster {
                if !candidates.contains(&node_id) {
                    candidates.push(node_id);
                }
            }
        }
        candidates = self.in_preference_order(candidates);
        if let Some(connection) = self.try_candidates(contact, &candidates).await {
            return Some(connection);
        }
        if self.reach == Reach::Local {
            return None;
        }
        let resolved = match self.engine.resolve_public_record(&contact.record_uid).await {
            Ok(Some(record)) => record,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(
                    contact = %contact.record_uid,
                    %error,
                    "the directory record for this contact was refused"
                );
                return None;
            }
        };
        let fresh: Vec<String> = resolved
            .node_ids
            .into_iter()
            .filter(|node_id| !candidates.contains(node_id))
            .collect();
        if fresh.is_empty() {
            return None;
        }
        tracing::info!(
            contact = %contact.record_uid,
            doors = fresh.len(),
            "no known Cell answered; dialing the front door from the directory"
        );
        self.try_candidates(contact, &fresh).await
    }

    fn in_preference_order(&self, candidates: Vec<String>) -> Vec<String> {
        let on_the_lan: Vec<String> = self
            .nearby
            .current()
            .into_iter()
            .map(|peer| peer.node_id)
            .collect();
        let mut ordered = candidates;
        ordered.sort_by_key(|node_id| !on_the_lan.contains(node_id));
        ordered
    }

    async fn try_candidates(
        &self,
        contact: &store::organs::Contact,
        candidates: &[String],
    ) -> Option<Connection> {
        use n0_future::StreamExt as _;

        let alpn = if contact.trust == "known" {
            ALPN_SYNC
        } else {
            ALPN_THREAD
        };
        let mut racing = n0_future::FuturesUnordered::new();
        for (position, candidate) in candidates.iter().enumerate() {
            let Ok(id) = candidate.parse::<EndpointId>() else {
                continue;
            };
            let endpoint = self.endpoint.clone();
            racing.push(async move {
                tokio::time::sleep(DIAL_STAGGER * position as u32).await;
                tokio::time::timeout(DIAL_TIMEOUT, endpoint.connect(EndpointAddr::new(id), alpn))
                    .await
            });
        }
        while let Some(finished) = racing.next().await {
            if let Ok(Ok(connection)) = finished {
                return Some(connection);
            }
        }
        None
    }

    pub async fn hello(&self, addr: EndpointAddr) -> Option<u32> {
        let connection =
            tokio::time::timeout(DIAL_TIMEOUT, self.endpoint.connect(addr, ALPN_HELLO))
                .await
                .ok()?
                .ok()?;
        let mut recv = tokio::time::timeout(DIAL_TIMEOUT, connection.accept_uni())
            .await
            .ok()?
            .ok()?;
        let raw = tokio::time::timeout(DIAL_TIMEOUT, recv.read_to_end(4096))
            .await
            .ok()?
            .ok()?;
        serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()?
            .get("epoch")
            .and_then(serde_json::Value::as_u64)
            .map(|epoch| epoch as u32)
    }

    pub(super) async fn note_if_stale(&self, member: &crate::roster::CellEntry, id: EndpointId) {
        let Ok(Ok(connection)) = tokio::time::timeout(
            DIAL_TIMEOUT,
            self.endpoint.connect(EndpointAddr::new(id), ALPN_HELLO),
        )
        .await
        else {
            return;
        };
        let Ok(Ok(mut recv)) = tokio::time::timeout(DIAL_TIMEOUT, connection.accept_uni()).await
        else {
            return;
        };
        let Ok(Ok(raw)) = tokio::time::timeout(DIAL_TIMEOUT, recv.read_to_end(4096)).await else {
            return;
        };
        let epoch = serde_json::from_slice::<serde_json::Value>(&raw)
            .ok()
            .and_then(|body| body.get("epoch").and_then(serde_json::Value::as_u64));
        let Some(epoch) = epoch else { return };
        if epoch as u32 == WIRE_EPOCH {
            return;
        }
        tracing::warn!(
            cell = %member.label,
            node_id = %member.node_id,
            theirs = epoch,
            ours = WIRE_EPOCH,
            "this device of your Organ speaks a different wire epoch and cannot \
             sync until it is updated"
        );
        let mut stale = self.engine.stale_siblings.lock().expect("stale siblings");
        stale.retain(|other: &StaleSibling| other.node_id != member.node_id);
        stale.push(StaleSibling {
            cell_uid: member.cell_uid.clone(),
            node_id: member.node_id.clone(),
            label: member.label.clone(),
            their_epoch: epoch as u32,
            our_epoch: WIRE_EPOCH,
        });
    }

    pub fn stale_siblings(&self) -> Vec<StaleSibling> {
        self.engine
            .stale_siblings
            .lock()
            .expect("stale siblings")
            .clone()
    }
}
