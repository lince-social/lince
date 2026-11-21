// non-lean-yet

//! Outbound mail, carrier discovery, and collection.

use iroh::{EndpointAddr, EndpointId};

use super::{
    ALPN_MAILBOX, ALPN_THREAD, ExpiredMail, MailboxBundle, Wire, WireRequest, WireResponse,
};
use crate::{
    error::EngineError,
    roster::SignedRoster,
    sync::{Delivery, OpBatch},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailLeft {
    Left {
        carrier: String,
        uid: String,
        copies: usize,
        requested_copies: usize,
    },
    NoRoster,
    NoPickupPoints,
    NoneAccepted {
        refusals: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierProbe {
    Carrying(store::mailbox::Waiting),
    Refused,
    Unreachable,
}

impl Wire {
    pub const MAIL_AFTER: chrono::Duration = chrono::Duration::minutes(10);

    pub const NOTICE_LINGER_DAYS: i64 = 15;

    pub(super) async fn mail_if_due(
        &self,
        contact: &store::organs::Contact,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Delivery {
        if self.is_conversation_delivery(root).await {
            return self.mail_now(contact, root, batch).await;
        }
        let unreachable = format!("{} unreachable", contact.record_uid);
        let now = nucleus::execution::now();
        let since = match store::organs::contact(&self.engine.store.pool, &contact.record_uid).await
        {
            Ok(Some(fresh)) => fresh,
            _ => return Delivery::Failed(unreachable),
        };
        let waited = |stamp: &Option<String>| {
            stamp
                .as_deref()
                .and_then(|when| chrono::DateTime::parse_from_rfc3339(when).ok())
                .map(|when| now - when.with_timezone(&chrono::Utc))
        };
        match waited(&since.unreachable_since) {
            Some(elapsed) if elapsed >= Self::MAIL_AFTER => {}
            _ => return Delivery::Failed(unreachable),
        }
        if let Some(elapsed) = waited(&since.mailed_at) {
            if elapsed < Self::MAIL_AFTER {
                return Delivery::Failed(unreachable);
            }
        }
        self.mail_now(contact, root, batch).await
    }

    pub(super) async fn mail_now(
        &self,
        contact: &store::organs::Contact,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Delivery {
        let unreachable = format!("{} unreachable", contact.record_uid);
        match self.leave_mail(&contact.record_uid, root, batch).await {
            Ok(MailLeft::Left {
                carrier,
                uid,
                copies,
                requested_copies,
            }) => {
                tracing::info!(
                    contact = %contact.record_uid, %carrier, %uid,
                    "batch left as mail"
                );
                if copies >= requested_copies {
                    Delivery::Mailed
                } else {
                    Delivery::Failed(format!(
                        "{copies} of {requested_copies} mailbox servers accepted the envelope"
                    ))
                }
            }
            Ok(_) => Delivery::Failed(unreachable),
            Err(error) => Delivery::Failed(error.to_string()),
        }
    }

    pub async fn deposit_bundle(
        &self,
        addr: impl Into<EndpointAddr>,
        body: &str,
    ) -> Result<Result<String, String>, EngineError> {
        match self
            .request(
                addr,
                ALPN_MAILBOX,
                &WireRequest::MailboxDeposit {
                    body: body.to_string(),
                },
            )
            .await?
        {
            WireResponse::MailboxAccepted { uid } => Ok(Ok(uid)),
            WireResponse::Refused { code, .. } => Ok(Err(code)),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered a deposit with something else".into(),
            )),
        }
    }

    pub async fn collect_mail(
        &self,
        addr: impl Into<EndpointAddr>,
        organ_uid: &str,
        roster: &SignedRoster,
        limit: i64,
    ) -> Result<Vec<MailboxBundle>, EngineError> {
        let addr = addr.into();
        let response = self
            .request(
                addr.clone(),
                ALPN_MAILBOX,
                &WireRequest::MailboxCollect {
                    organ_uid: organ_uid.to_string(),
                    roster: roster.clone(),
                    limit,
                },
            )
            .await?;
        let bundles = match response {
            WireResponse::MailboxBundles { bundles } => bundles,
            WireResponse::Refused { message, .. } => {
                return Err(EngineError::Consequence(message));
            }
            WireResponse::Error { message } => return Err(EngineError::Consequence(message)),
            _ => {
                return Err(EngineError::Consequence(
                    "carrier answered a collection with something else".into(),
                ));
            }
        };
        if bundles.len() > limit.clamp(1, 256) as usize {
            return Err(EngineError::Consequence(
                "The mailbox exceeded the requested collection page".into(),
            ));
        }
        if !bundles.is_empty() {
            for bundle in &bundles {
                let envelope: crate::seal::SealedBundle =
                    serde_json::from_str(&bundle.body).map_err(EngineError::Json)?;
                crate::seal::validate_recoverable(&envelope, nucleus::execution::now().timestamp())
                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                if crate::seal::delivery_id(&envelope) != bundle.uid
                    || envelope.to_organ != organ_uid
                {
                    return Err(EngineError::Forbidden(
                        "The mailbox returned a different envelope identity or recipient".into(),
                    ));
                }
                let body = serde_json::to_string(&envelope).map_err(EngineError::Json)?;
                let expiry = chrono::DateTime::from_timestamp(envelope.expires_at, 0)
                    .ok_or_else(|| EngineError::Consequence("Invalid envelope expiry".into()))?
                    .to_rfc3339();
                store::mailbox::delivery::receive(
                    &self.engine.store.pool,
                    &bundle.uid,
                    &addr.id.to_string(),
                    &body,
                    &crate::seal::envelope_hash(&body),
                    &expiry,
                )
                .await?;
            }
            let uids = bundles.iter().map(|b| b.uid.clone()).collect::<Vec<_>>();
            let _ = self
                .request(
                    addr,
                    ALPN_MAILBOX,
                    &WireRequest::MailboxCollected {
                        organ_uid: organ_uid.to_string(),
                        roster: roster.clone(),
                        uids,
                    },
                )
                .await;
        }
        Ok(bundles)
    }

    pub async fn leave_mail(
        &self,
        to_organ: &str,
        root: Option<&str>,
        batch: &OpBatch,
    ) -> Result<MailLeft, EngineError> {
        let Some(their_roster) = self.engine.roster_of(to_organ).await? else {
            return Ok(MailLeft::NoRoster);
        };
        if their_roster.roster.pickup.is_empty() {
            return Ok(MailLeft::NoPickupPoints);
        }
        let queued = self
            .engine
            .prepare_outgoing_mail(to_organ, root, batch)
            .await?;
        self.deliver_saved_mail(&queued).await
    }

    pub async fn collect_own_mail(&self) -> Result<usize, EngineError> {
        let mut imported = self.engine.process_recovered_mail().await?;
        let points = self.engine.own_pickup_points().await?;
        if points.is_empty() {
            return Ok(imported);
        }
        let Some(local) = store::organs::local(&self.engine.store.pool).await? else {
            return Ok(imported);
        };
        let Some(ours) = self.engine.roster_of(&local.uid).await? else {
            return Ok(imported);
        };
        for point in points {
            let Ok(id) = point.node_id.parse::<EndpointId>() else {
                continue;
            };
            let _ = self
                .collect_mail(EndpointAddr::new(id), &local.uid, &ours, 50)
                .await;
            imported += self.engine.process_recovered_mail().await?;
        }
        Ok(imported)
    }

    pub async fn publish_own_sealing_key(&self) -> Result<bool, EngineError> {
        let pool = &self.engine.store.pool;
        let (Some(local_organ), Some(local_cell)) = (
            store::organs::local(pool).await?,
            store::cells::local(pool).await?,
        ) else {
            return Ok(false);
        };
        let Some(current) = self.engine.published_sealing_key().await? else {
            return Ok(false);
        };
        let Some(held) = self.engine.roster_of(&local_organ.uid).await? else {
            return Ok(false);
        };
        let published = held
            .roster
            .cells
            .iter()
            .find(|cell| cell.cell_uid == local_cell.uid)
            .and_then(|cell| cell.sealing_key.clone());
        if published.as_ref() == Some(&current) {
            return Ok(true);
        }
        let ours = self.node_id().to_string();
        if self
            .engine
            .republish_sealing_key(&ours, &local_cell.uid, current.clone())
            .await
            .is_ok()
        {
            return Ok(true);
        }
        for sibling in &held.roster.cells {
            if sibling.cell_uid == local_cell.uid {
                continue;
            }
            let Ok(id) = sibling.node_id.parse::<EndpointId>() else {
                continue;
            };
            let asked = self
                .request(
                    EndpointAddr::new(id),
                    ALPN_THREAD,
                    &WireRequest::PublishSealingKey {
                        cell_uid: local_cell.uid.clone(),
                        sealing_key: current.clone(),
                    },
                )
                .await;
            if matches!(asked, Ok(WireResponse::Applied { .. })) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn hear_about_expired_mail(&self) -> Result<usize, EngineError> {
        let pool = &self.engine.store.pool;
        let _ = store::mail_left::prune(
            pool,
            crate::seal::RETENTION_DAYS + crate::seal::GRACE_DAYS + Self::NOTICE_LINGER_DAYS,
        )
        .await;
        let carriers = store::mail_left::carriers_to_ask(pool).await?;
        let mut believed = 0usize;
        for node in carriers {
            let Ok(id) = node.parse::<EndpointId>() else {
                continue;
            };
            let Ok(expired) = self.ask_expiries(EndpointAddr::new(id)).await else {
                continue;
            };
            if expired.is_empty() {
                continue;
            }
            let reports: Vec<(String, String)> = expired
                .iter()
                .map(|one| (one.uid.clone(), one.expired_at.clone()))
                .collect();
            match self.engine.note_expired_mail(&node, &reports).await {
                Ok(ours) => believed += ours.len(),
                Err(error) => tracing::warn!(%error, "an expiry report could not be recorded"),
            }
            let uids: Vec<String> = expired.into_iter().map(|one| one.uid).collect();
            let _ = self
                .request(
                    EndpointAddr::new(id),
                    ALPN_MAILBOX,
                    &WireRequest::MailboxExpiriesHeard { uids },
                )
                .await;
        }
        Ok(believed)
    }

    async fn ask_expiries(
        &self,
        addr: impl Into<EndpointAddr>,
    ) -> Result<Vec<ExpiredMail>, EngineError> {
        match self
            .request(addr, ALPN_MAILBOX, &WireRequest::MailboxExpiries)
            .await?
        {
            WireResponse::MailboxExpired { expired } => Ok(expired),
            WireResponse::Refused { .. } => Ok(Vec::new()),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered an expiry question with something else".into(),
            )),
        }
    }

    pub async fn carrier_probe(&self, node_id: &str) -> CarrierProbe {
        let Ok(id) = node_id.parse::<EndpointId>() else {
            return CarrierProbe::Unreachable;
        };
        let pool = &self.engine.store.pool;
        let Ok(Some(local)) = store::organs::local(pool).await else {
            return CarrierProbe::Unreachable;
        };
        let Ok(Some(ours)) = self.engine.roster_of(&local.uid).await else {
            return CarrierProbe::Unreachable;
        };
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxWaiting {
                    organ_uid: local.uid.clone(),
                    roster: ours,
                },
            )
            .await
        {
            Ok(WireResponse::MailboxWaiting {
                bundles,
                bytes,
                oldest_expires_at,
            }) => CarrierProbe::Carrying(store::mailbox::Waiting {
                bundles,
                bytes,
                oldest_expires_at,
            }),
            Ok(WireResponse::Refused { .. }) => CarrierProbe::Refused,
            _ => CarrierProbe::Unreachable,
        }
    }

    pub async fn ask_to_be_carried(&self, node_id: &str) -> Result<(), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("that is not a usable device address".into()))?;
        let pool = &self.engine.store.pool;
        let local = store::organs::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        let ours = self.engine.roster_of(&local.uid).await?.ok_or_else(|| {
            EngineError::Consequence(
                "this Organ has published no device list, so a carrier would have no way to \
                 check who is collecting later."
                    .into(),
            )
        })?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxAskToCarry { roster: ours },
            )
            .await?
        {
            WireResponse::MailboxAsked => Ok(()),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            other => Err(EngineError::Consequence(format!("{other:?}"))),
        }
    }

    pub async fn redeem_mailbox_invite(
        &self,
        node_id: &str,
        token: &str,
    ) -> Result<(String, i64), EngineError> {
        let id = node_id
            .parse::<EndpointId>()
            .map_err(|_| EngineError::Consequence("that is not a usable device address".into()))?;
        let pool = &self.engine.store.pool;
        let local = store::organs::local(pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("this Cell has no Organ".into()))?;
        let ours = self.engine.roster_of(&local.uid).await?.ok_or_else(|| {
            EngineError::Consequence(
                "this Organ has published no device list, so there is nothing to register.".into(),
            )
        })?;
        match self
            .request(
                EndpointAddr::new(id),
                ALPN_MAILBOX,
                &WireRequest::MailboxRedeemInvite {
                    token: token.to_string(),
                    roster: ours,
                },
            )
            .await?
        {
            WireResponse::MailboxCarrying { label, quota_bytes } => Ok((label, quota_bytes)),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            other => Err(EngineError::Consequence(format!("{other:?}"))),
        }
    }

    pub async fn mail_waiting(
        &self,
        addr: impl Into<EndpointAddr>,
        organ_uid: &str,
        roster: &SignedRoster,
    ) -> Result<store::mailbox::Waiting, EngineError> {
        match self
            .request(
                addr,
                ALPN_MAILBOX,
                &WireRequest::MailboxWaiting {
                    organ_uid: organ_uid.to_string(),
                    roster: roster.clone(),
                },
            )
            .await?
        {
            WireResponse::MailboxWaiting {
                bundles,
                bytes,
                oldest_expires_at,
            } => Ok(store::mailbox::Waiting {
                bundles,
                bytes,
                oldest_expires_at,
            }),
            WireResponse::Refused { message, .. } => Err(EngineError::Consequence(message)),
            WireResponse::Error { message } => Err(EngineError::Consequence(message)),
            _ => Err(EngineError::Consequence(
                "carrier answered with something else".into(),
            )),
        }
    }
}
