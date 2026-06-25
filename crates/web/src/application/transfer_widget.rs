use {
    crate::{
        application::transfer_identity::is_supported_transfer_package_filename,
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, is_default_local_organ, organ_requires_auth},
        },
    },
    ::application::write,
    base64::{Engine as _, engine::general_purpose::STANDARD as BASE64},
    chrono::{Duration as ChronoDuration, Utc},
    ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey},
    injection::cross_cutting::InjectedServices,
    persistence::write_coordinator::SqlParameter,
    reqwest::Method,
    serde::{Deserialize, Serialize},
    serde_json::{Value, json},
    sha2::{Digest, Sha256},
    sqlx::FromRow,
    std::{
        fs,
        io::{Error, ErrorKind},
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::Duration,
    },
    tokio::sync::broadcast,
    uuid::Uuid,
};

const COORDINATOR_LABEL: &str = "local_lince";
const PACKAGE_VERSION: u32 = 1;
const MAX_GOSSIP_EVENTS: usize = 200;
const MAX_GOSSIP_PACKAGE_BYTES: usize = 256 * 1024;
const MAX_GOSSIP_PACKAGES: i64 = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferChangeEvent {
    pub version: u64,
    pub reason: String,
}

#[derive(Clone)]
struct TransferChangeBus {
    version: Arc<AtomicU64>,
    sender: broadcast::Sender<TransferChangeEvent>,
}

#[derive(Clone)]
pub struct TransferWidgetService {
    auth: AppAuth,
    board_state: BoardStateStore,
    local_auth_required: bool,
    local_base_url: String,
    manas: ManasGateway,
    organs: OrganStore,
    services: InjectedServices,
    changes: TransferChangeBus,
}

impl TransferWidgetService {
    pub fn new(
        auth: AppAuth,
        board_state: BoardStateStore,
        local_auth_required: bool,
        local_base_url: String,
        manas: ManasGateway,
        organs: OrganStore,
        services: InjectedServices,
    ) -> Self {
        let (changes, _) = broadcast::channel(1024);
        Self {
            auth,
            board_state,
            local_auth_required,
            local_base_url,
            manas,
            organs,
            services,
            changes: TransferChangeBus {
                version: Arc::new(AtomicU64::new(0)),
                sender: changes,
            },
        }
    }

    pub async fn contract(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<Value, TransferWidgetError> {
        let card = self.resolve_card(instance_id).await?;

        Ok(json!({
            "widget": {
                "instanceId": card.id,
                "title": card.title,
                "description": card.description,
                "packageName": card.package_name,
            },
            "coordinator": {
                "kind": "local_lince",
                "label": COORDINATOR_LABEL,
                "copy": "This local Lince signs its own events and can post Transfer packages to selected Organs."
            },
            "actions": [
                "configure-local-party",
                "reset-local-party",
                "create-record",
                "create-proposal",
                "update-transfer-local-item",
                "update-transfer-work",
                "update-transfer-item-work",
                "update-transfer-interaction-work",
                "duplicate-proposal",
                "sign-agreement",
                "confirm-delivery",
                "confirm-receipt",
                "settle-local",
                "settle-full",
                "inactivate-transfer",
                "delete-transfer",
                "create-child-transfer",
                "create-transfer-tree-from-record",
                "sync-transfer-tree",
                "set-transfer-branch-mode",
                "set-transfer-tree-sync-mode",
                "set-transfer-reservation-policy",
                "set-transfer-visibility",
                "post-transfer",
                "poll-transfer-peer",
                "mark-transfer-seen",
                "import-package",
                "set-ingress-policy",
                "set-network-policy",
                "set-receipt-policy",
                "set-organ-proximity",
                "set-organ-receipt-policy",
                "set-quantity-projection-sharing",
                "apply-visibility-wave",
                "refresh"
            ],
            "snapshot": self.snapshot(session_token).await?,
        }))
    }

    pub async fn action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, TransferWidgetError> {
        self.resolve_card(instance_id).await?;

        let message = match action {
            "configure-local-party" => {
                let request = parse_payload::<ConfigureLocalPartyRequest>(payload)?;
                self.configure_local_party(&request.label).await?;
                "Local Transfer signing identity saved in this node database.".to_string()
            }
            "reset-local-party" => {
                let request = parse_payload::<ConfigureLocalPartyRequest>(payload)?;
                self.reset_local_party(&request.label).await?;
                "Local Transfer signing identity reset.".to_string()
            }
            "create-record" => {
                let request = parse_payload::<CreateRecordRequest>(payload)?;
                let record = self
                    .create_record(request)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                format!("Record #{} created.", record.id)
            }
            "create-proposal" => {
                let request = parse_payload::<CreateProposalRequest>(payload)?;
                let target_organ_id = request.target_organ_id;
                let transfer_id = self.create_proposal(request).await?;
                if let Some(organ_id) = target_organ_id {
                    match self
                        .post_transfer_package(
                            session_token,
                            PostTransferRequest {
                                transfer_id,
                                organ_id: Some(organ_id),
                                base_url: None,
                            },
                        )
                        .await
                    {
                        Ok(()) => format!("Transfer proposal #{transfer_id} created and posted."),
                        Err(error) => {
                            let error_message = error.message();
                            format!(
                                "Transfer proposal #{transfer_id} was created locally, but posting failed: {error_message}"
                            )
                        }
                    }
                } else {
                    format!("Transfer proposal #{transfer_id} created locally.")
                }
            }
            "duplicate-proposal" => {
                let request = parse_payload::<DuplicateProposalRequest>(payload)?;
                let transfer_id = self.duplicate_proposal(request).await?;
                let transfer = self
                    .load_transfer_summary(transfer_id)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                if let Some(target_base_url) = normalize_optional_text(transfer.target_base_url)
                    .or_else(|| normalize_optional_text(transfer.source_base_url))
                {
                    match self
                        .post_transfer_package(
                            session_token,
                            PostTransferRequest {
                                transfer_id,
                                organ_id: None,
                                base_url: Some(target_base_url.clone()),
                            },
                        )
                        .await
                    {
                        Ok(()) => format!(
                            "Proposal duplicated into Transfer #{transfer_id} and posted to {target_base_url}."
                        ),
                        Err(error) => {
                            let error_message = error.message();
                            format!(
                                "Proposal duplicated into Transfer #{transfer_id}, but posting to {target_base_url} failed: {error_message}"
                            )
                        }
                    }
                } else {
                    format!("Proposal duplicated into Transfer #{transfer_id}.")
                }
            }
            "update-transfer-local-item" => {
                let request = parse_payload::<UpdateTransferLocalItemRequest>(payload)?;
                self.update_transfer_local_item(request).await?;
                "Transfer local terms updated.".to_string()
            }
            "update-transfer-work" => {
                let request = parse_payload::<UpdateTransferWorkRequest>(payload)?;
                self.update_transfer_work(request).await?;
                "Transfer work metadata updated.".to_string()
            }
            "update-transfer-item-work" => {
                let request = parse_payload::<UpdateTransferItemWorkRequest>(payload)?;
                self.update_transfer_item_work(request).await?;
                "Transfer item work metadata updated.".to_string()
            }
            "update-transfer-interaction-work" => {
                let request = parse_payload::<UpdateTransferInteractionWorkRequest>(payload)?;
                self.update_transfer_interaction_work(request).await?;
                "Transfer interaction work metadata updated.".to_string()
            }
            "sign-agreement" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.sign_agreement(request.transfer_id).await?;
                "Local agreement signed.".to_string()
            }
            "confirm-delivery" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.confirm_delivery(request.transfer_id).await?;
                "Delivery signed by the contribution side.".to_string()
            }
            "confirm-receipt" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.confirm_receipt(request.transfer_id).await?;
                "Receipt signed by the need side.".to_string()
            }
            "settle-local" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.settle_local(request.transfer_id).await?;
                "Local Record quantity settled.".to_string()
            }
            "settle-full" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.settle_full(request.transfer_id).await?;
                "Full Transfer quantity settled.".to_string()
            }
            "inactivate-transfer" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.inactivate_transfer(request.transfer_id).await?;
                "Transfer inactivated and progress reset.".to_string()
            }
            "delete-transfer" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.delete_transfer(request.transfer_id)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                "Transfer deleted from this node.".to_string()
            }
            "create-child-transfer" => {
                let request = parse_payload::<CreateChildTransferRequest>(payload)?;
                let transfer_id = self.create_child_transfer(request).await?;
                format!("Child Transfer #{transfer_id} created.")
            }
            "create-transfer-tree-from-record" => {
                let request = parse_payload::<CreateTransferTreeFromRecordRequest>(payload)?;
                let created = self.create_transfer_tree_from_record(request).await?;
                format!("Transfer tree created with {created} Transfers.")
            }
            "sync-transfer-tree" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                let created = self.sync_transfer_tree(request.transfer_id).await?;
                format!("Transfer tree sync created {created} missing children.")
            }
            "set-transfer-branch-mode" => {
                let request = parse_payload::<SetTransferBranchModeRequest>(payload)?;
                self.set_transfer_branch_mode(request).await?;
                "Transfer branch mode updated.".to_string()
            }
            "set-transfer-tree-sync-mode" => {
                let request = parse_payload::<SetTransferTreeSyncModeRequest>(payload)?;
                self.set_transfer_tree_sync_mode(request).await?;
                "Transfer tree sync mode updated.".to_string()
            }
            "set-transfer-reservation-policy" => {
                let request = parse_payload::<SetTransferReservationPolicyRequest>(payload)?;
                self.set_transfer_reservation_policy(request).await?;
                "Transfer reservation policy updated.".to_string()
            }
            "set-transfer-visibility" => {
                let request = parse_payload::<SetTransferVisibilityRequest>(payload)?;
                self.set_transfer_visibility(request).await?;
                "Transfer visibility updated.".to_string()
            }
            "post-transfer" => {
                let request = parse_payload::<PostTransferRequest>(payload)?;
                self.post_transfer_package(session_token, request).await?;
                "Transfer package posted to the selected Organ.".to_string()
            }
            "poll-transfer-peer" => {
                let request = parse_payload::<PollTransferPeerRequest>(payload)?;
                let target = self.poll_transfer_peer(request).await?;
                format!("Transfer updates requested from {target}.")
            }
            "mark-transfer-seen" => {
                let request = parse_payload::<TransferIdRequest>(payload)?;
                self.mark_transfer_seen(request.transfer_id).await?;
                "Transfer marked seen.".to_string()
            }
            "import-package" => {
                let request = parse_payload::<ImportPackageRequest>(payload)?;
                self.receive_transfer_package_value(request.package).await?;
                "Transfer package imported.".to_string()
            }
            "set-ingress-policy" => {
                let request = parse_payload::<SetIngressPolicyRequest>(payload)?;
                self.set_transfer_public_proposals_enabled(request.public_proposals_enabled)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                "Transfer ingress policy updated.".to_string()
            }
            "set-network-policy" => {
                let request = parse_payload::<SetNetworkPolicyRequest>(payload)?;
                self.set_transfer_known_peer_polling_enabled(request.known_peer_polling_enabled)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.set_transfer_share_quantity_projections(request.share_quantity_projections)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                "Transfer network policy updated.".to_string()
            }
            "set-receipt-policy" => {
                let request = parse_payload::<SetReceiptPolicyRequest>(payload)?;
                self.set_transfer_receipt_policy(request)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                "Transfer receipt policy updated.".to_string()
            }
            "set-organ-proximity" => {
                let request = parse_payload::<SetOrganProximityRequest>(payload)?;
                self.organs
                    .set_proximity(request.organ_id, request.proximity)
                    .await
                    .map_err(TransferWidgetError::Invalid)?;
                "Organ proximity updated.".to_string()
            }
            "set-organ-receipt-policy" => {
                let request = parse_payload::<SetOrganReceiptPolicyRequest>(payload)?;
                self.organs
                    .set_transfer_receipts(
                        request.organ_id,
                        request.send_received_receipts,
                        request.send_seen_receipts,
                    )
                    .await
                    .map_err(TransferWidgetError::Invalid)?;
                "Organ Transfer receipt policy updated.".to_string()
            }
            "set-quantity-projection-sharing" => {
                let request = parse_payload::<SetQuantityProjectionSharingRequest>(payload)?;
                self.set_transfer_share_quantity_projections(request.enabled)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                "Transfer quantity projection sharing updated.".to_string()
            }
            "apply-visibility-wave" => {
                let request = parse_payload::<ApplyVisibilityWaveRequest>(payload)?;
                self.apply_visibility_wave(request).await?;
                "Transfer visibility wave applied.".to_string()
            }
            "refresh" => {
                self.pulse_transfer_mesh().await?;
                "Transfer mesh pulse completed.".to_string()
            }
            _ => {
                return Err(TransferWidgetError::Invalid(
                    "Unknown Transfer action.".into(),
                ));
            }
        };

        if matches!(
            action,
            "update-transfer-local-item"
                | "update-transfer-work"
                | "update-transfer-item-work"
                | "update-transfer-interaction-work"
                | "sign-agreement"
                | "confirm-delivery"
                | "confirm-receipt"
                | "settle-local"
                | "settle-full"
                | "inactivate-transfer"
                | "create-child-transfer"
                | "create-transfer-tree-from-record"
                | "sync-transfer-tree"
                | "set-transfer-branch-mode"
                | "set-transfer-tree-sync-mode"
        ) {
            self.flush_transfer_sync_outbox()
                .await
                .map_err(TransferWidgetError::from_io)?;
        }

        let response = json!({
            "ok": true,
            "action": action,
            "message": message,
            "snapshot": self.snapshot(session_token).await?,
        });
        self.notify_changed(action);
        Ok(response)
    }

    pub fn subscribe_changes(&self) -> broadcast::Receiver<TransferChangeEvent> {
        self.changes.sender.subscribe()
    }

    fn notify_changed(&self, reason: impl Into<String>) {
        let version = self.changes.version.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.changes.sender.send(TransferChangeEvent {
            version,
            reason: reason.into(),
        });
    }

    pub async fn receive_transfer_package_value(
        &self,
        value: Value,
    ) -> Result<Value, TransferWidgetError> {
        let package = parse_transfer_package_value(value)?;
        self.reject_blocked_package(&package).await?;
        let imported = self.receive_transfer_package(package).await?;
        if imported.events_imported > 0 {
            self.notify_changed("package_received");
        }

        Ok(json!({
            "ok": true,
            "transferId": imported.transfer_id,
            "eventsImported": imported.events_imported,
        }))
    }

    pub async fn receive_public_transfer_package_value(
        &self,
        value: Value,
    ) -> Result<Value, TransferWidgetError> {
        let package = parse_transfer_package_value(value)?;
        self.reject_blocked_package(&package).await?;
        let known_peer = self.package_has_known_peer(&package).await?;
        if !known_peer
            && !self
                .transfer_public_proposals_enabled()
                .await
                .map_err(TransferWidgetError::from_io)?
        {
            return Err(TransferWidgetError::Invalid(
                "This node is not accepting public Transfer proposals.".into(),
            ));
        }
        validate_public_transfer_package(self, &package).await?;
        let known = self
            .find_transfer_id_by_uid(&package.identity.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?
            .is_some();
        let addressed = self.package_is_addressed_to_local_node(&package);
        let initial = is_public_proposal_package(&package);
        if known || addressed || initial {
            let imported = self.receive_transfer_package(package).await?;
            if imported.events_imported > 0 {
                self.notify_changed("public_package_received");
            }
            return Ok(json!({
                "ok": true,
                "transferId": imported.transfer_id,
                "eventsImported": imported.events_imported,
            }));
        }
        let transfer_uid = package.identity.transfer_uid.clone();
        self.store_gossip_package(&package, None)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.notify_changed("gossip_package_received");

        Ok(json!({
            "ok": true,
            "gossiped": true,
            "transferUid": transfer_uid,
        }))
    }

    pub async fn transfer_packages_since_value(
        &self,
        since: Option<&str>,
        requester_base_url: Option<&str>,
    ) -> Result<Value, TransferWidgetError> {
        let requester_organ = if let Some(base_url) = requester_base_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            self.organs
                .find_by_base_url(base_url)
                .await
                .map_err(TransferWidgetError::Invalid)?
        } else {
            None
        };
        let mut visible_packages = Vec::new();
        for package in self
            .load_transfer_packages_since(since)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            if self
                .transfer_package_visible_for_export(&package, requester_organ.as_ref())
                .await?
            {
                visible_packages.push(package);
            }
        }
        Ok(json!({
            "ok": true,
            "packages": visible_packages,
        }))
    }

    pub fn spawn_sync_tasks(self) {
        let cache_service = self.clone();
        tokio::spawn(async move {
            cache_service.sync_on_startup().await;
        });

        let heartbeat_service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = heartbeat_service.write_sync_cache_now() {
                    tracing::warn!("transfer sync cache write failed: {error}");
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });

        tokio::spawn(async move {
            loop {
                if let Err(error) = self.flush_transfer_sync_outbox().await {
                    tracing::warn!("transfer sync outbox flush failed: {error}");
                }
                if let Err(error) = self.poll_due_known_transfer_peers().await {
                    tracing::warn!("transfer known-peer poll failed: {error}");
                }
                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        });
    }

    async fn resolve_card(&self, instance_id: &str) -> Result<BoardCard, TransferWidgetError> {
        let instance_id = instance_id.trim();
        if instance_id.is_empty() {
            return Err(TransferWidgetError::NotFound(
                "Widget instance ausente.".into(),
            ));
        }

        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            TransferWidgetError::NotFound("Nao encontrei esse widget no board.".into())
        })?;

        if !is_supported_transfer_package_filename(&card.package_name) {
            return Err(TransferWidgetError::Misconfigured(
                "Esse widget nao e o Transfer oficial.".into(),
            ));
        }

        Ok(card)
    }

    async fn snapshot(&self, session_token: Option<&str>) -> Result<Value, TransferWidgetError> {
        let local_identity = self
            .ensure_local_identity()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let records = self
            .load_records()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let transfers = self
            .load_transfer_views(Some(&local_identity))
            .await
            .map_err(TransferWidgetError::from_io)?;
        let gossip_transfers = self
            .load_gossip_views()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let organs = self.load_organ_options(session_token).await?;
        let work_assignee_organs = organs.clone();
        let local_users = self
            .load_app_user_options()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let public_proposals_enabled = self
            .transfer_public_proposals_enabled()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let known_peer_polling_enabled = self
            .transfer_known_peer_polling_enabled()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let receipt_policy = self
            .transfer_receipt_policy()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let share_quantity_projections = self
            .transfer_share_quantity_projections()
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(json!({
            "localIdentity": LocalIdentityView::from(local_identity),
            "ingressPolicy": {
                "publicProposalsEnabled": public_proposals_enabled,
                "copy": "When enabled, this node accepts unauthenticated initial proposal packages only. Replies still require login or manual import."
            },
            "networkPolicy": {
                "knownPeerPollingEnabled": known_peer_polling_enabled,
                "shareQuantityProjections": share_quantity_projections,
            },
            "receiptPolicy": receipt_policy,
            "records": records,
            "organs": organs,
            "workAssigneeOptions": {
                "localUsers": local_users,
                "organs": work_assignee_organs,
            },
            "transfers": transfers,
            "gossipTransfers": gossip_transfers,
        }))
    }

    async fn transfer_public_proposals_enabled(&self) -> Result<bool, Error> {
        let value = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(
                (SELECT transfer_public_proposals_enabled
                 FROM configuration
                 WHERE quantity = 1
                 ORDER BY id
                 LIMIT 1),
                0
             )",
        )
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(value != 0)
    }

    async fn transfer_known_peer_polling_enabled(&self) -> Result<bool, Error> {
        let value = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(
                (SELECT transfer_known_peer_polling_enabled
                 FROM configuration
                 WHERE quantity = 1
                 ORDER BY id
                 LIMIT 1),
                1
             )",
        )
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(value != 0)
    }

    async fn transfer_receipt_policy(&self) -> Result<TransferReceiptPolicyView, Error> {
        let row = sqlx::query_as::<_, TransferReceiptPolicyRow>(
            "SELECT
                COALESCE(transfer_send_received_receipts, 1) AS send_received_receipts,
                COALESCE(transfer_send_seen_receipts, 1) AS send_seen_receipts,
                COALESCE(transfer_anonymous_package_viewing, 0) AS anonymous_package_viewing
             FROM configuration
             WHERE quantity = 1
             ORDER BY id
             LIMIT 1",
        )
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .unwrap_or(TransferReceiptPolicyRow {
            send_received_receipts: 1,
            send_seen_receipts: 1,
            anonymous_package_viewing: 0,
        });
        Ok(TransferReceiptPolicyView::from(row))
    }

    async fn transfer_share_quantity_projections(&self) -> Result<bool, Error> {
        let value = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(transfer_share_quantity_projections, 0)
             FROM configuration
             WHERE quantity = 1
             ORDER BY id
             LIMIT 1",
        )
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .unwrap_or(0);
        Ok(value != 0)
    }

    async fn set_transfer_share_quantity_projections(&self, enabled: bool) -> Result<(), Error> {
        let enabled = bool_i64(enabled);
        let outcome = self
            .services
            .writer
            .execute_statement(
                "UPDATE configuration
                 SET transfer_share_quantity_projections = ?
                 WHERE quantity = 1"
                    .to_string(),
                vec![SqlParameter::Integer(enabled)],
            )
            .await?;
        if outcome.rows_affected == 0 {
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO configuration(
                        quantity,
                        name,
                        language,
                        timezone,
                        style,
                        transfer_share_quantity_projections
                    ) VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato', ?)"
                        .to_string(),
                    vec![SqlParameter::Integer(enabled)],
                )
                .await?;
        }
        Ok(())
    }

    async fn set_transfer_known_peer_polling_enabled(&self, enabled: bool) -> Result<(), Error> {
        let enabled = if enabled { 1_i64 } else { 0_i64 };
        let outcome = self
            .services
            .writer
            .execute_statement(
                "UPDATE configuration
                 SET transfer_known_peer_polling_enabled = ?
                 WHERE quantity = 1"
                    .to_string(),
                vec![SqlParameter::Integer(enabled)],
            )
            .await?;
        if outcome.rows_affected == 0 {
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO configuration(
                        quantity,
                        name,
                        language,
                        timezone,
                        style,
                        transfer_known_peer_polling_enabled
                    ) VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato', ?)"
                        .to_string(),
                    vec![SqlParameter::Integer(enabled)],
                )
                .await?;
        }
        Ok(())
    }

    async fn set_transfer_public_proposals_enabled(&self, enabled: bool) -> Result<(), Error> {
        let enabled = if enabled { 1_i64 } else { 0_i64 };
        let outcome = self
            .services
            .writer
            .execute_statement(
                "UPDATE configuration
                 SET transfer_public_proposals_enabled = ?
                 WHERE quantity = 1"
                    .to_string(),
                vec![SqlParameter::Integer(enabled)],
            )
            .await?;
        if outcome.rows_affected == 0 {
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO configuration(
                        quantity,
                        name,
                        language,
                        timezone,
                        style,
                        transfer_public_proposals_enabled
                    ) VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato', ?)"
                        .to_string(),
                    vec![SqlParameter::Integer(enabled)],
                )
                .await?;
        }
        Ok(())
    }

    async fn set_transfer_receipt_policy(
        &self,
        request: SetReceiptPolicyRequest,
    ) -> Result<(), Error> {
        let send_received = bool_i64(request.send_received_receipts);
        let send_seen = bool_i64(request.send_seen_receipts);
        let anonymous = bool_i64(request.anonymous_package_viewing);
        let outcome = self
            .services
            .writer
            .execute_statement(
                "UPDATE configuration
                 SET transfer_send_received_receipts = ?,
                     transfer_send_seen_receipts = ?,
                     transfer_anonymous_package_viewing = ?
                 WHERE quantity = 1"
                    .to_string(),
                vec![
                    SqlParameter::Integer(send_received),
                    SqlParameter::Integer(send_seen),
                    SqlParameter::Integer(anonymous),
                ],
            )
            .await?;
        if outcome.rows_affected == 0 {
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO configuration(
                        quantity,
                        name,
                        language,
                        timezone,
                        style,
                        transfer_send_received_receipts,
                        transfer_send_seen_receipts,
                        transfer_anonymous_package_viewing
                    ) VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato', ?, ?, ?)"
                        .to_string(),
                    vec![
                        SqlParameter::Integer(send_received),
                        SqlParameter::Integer(send_seen),
                        SqlParameter::Integer(anonymous),
                    ],
                )
                .await?;
        }
        Ok(())
    }

    async fn load_organ_options(
        &self,
        session_token: Option<&str>,
    ) -> Result<Vec<OrganOption>, TransferWidgetError> {
        let statuses = self.auth.remote_server_snapshots(session_token).await;
        let organs = self
            .organs
            .list()
            .await
            .map_err(TransferWidgetError::Invalid)?;
        Ok(organs
            .into_iter()
            .map(|organ| {
                let status = statuses.get(&organ.id.to_string());
                let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
                OrganOption {
                    id: organ.id,
                    name: organ.name,
                    base_url: organ.base_url,
                    trust_state: organ.trust_state,
                    contact_discovery_enabled: organ.contact_discovery_enabled != 0,
                    last_seen_at: organ.last_seen_at,
                    last_transfer_polled_at: organ.last_transfer_polled_at,
                    proximity: organ.proximity,
                    transfer_send_received_receipts: organ.transfer_send_received_receipts != 0,
                    transfer_send_seen_receipts: organ.transfer_send_seen_receipts != 0,
                    requires_auth,
                    authenticated: !requires_auth || status.is_some(),
                }
            })
            .collect())
    }

    async fn configure_local_party(&self, label: &str) -> Result<(), TransferWidgetError> {
        let label = normalize_nonempty(label, "Local party label")?;
        if let Some(identity) = self
            .load_local_identity()
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            self.services
                .writer
                .execute_statement(
                    "UPDATE transfer_node_identity
                     SET label = ?, updated_at = CURRENT_TIMESTAMP
                     WHERE id = ?"
                        .to_string(),
                    vec![
                        SqlParameter::Text(label),
                        SqlParameter::Integer(identity.id),
                    ],
                )
                .await
                .map_err(TransferWidgetError::from_io)?;
            return Ok(());
        }

        self.insert_local_identity(&label)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
    }

    async fn reset_local_party(&self, label: &str) -> Result<(), TransferWidgetError> {
        let label = normalize_nonempty(label, "Local party label")?;
        self.services
            .writer
            .execute_statement("DELETE FROM transfer_node_identity".to_string(), Vec::new())
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.insert_local_identity(&label)
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn insert_local_identity(&self, label: &str) -> Result<(), Error> {
        let signing_key = new_signing_key();
        let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
        let secret_key = BASE64.encode(signing_key.to_bytes());

        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_node_identity(id, label, public_key, secret_key)
                 VALUES (1, ?, ?, ?)"
                    .to_string(),
                vec![
                    SqlParameter::Text(label.to_string()),
                    SqlParameter::Text(public_key),
                    SqlParameter::Text(secret_key),
                ],
            )
            .await?;

        Ok(())
    }

    async fn create_record(&self, request: CreateRecordRequest) -> Result<RecordView, Error> {
        let head = normalize_optional_text(request.head);
        let body = normalize_optional_text(request.body);
        let outcome = write::execute_record_insert_returning_id(
            self.services.clone(),
            "INSERT INTO record(quantity, head, body) VALUES (?, ?, ?) RETURNING id",
            vec![
                SqlParameter::Real(request.quantity),
                optional_text_parameter(head),
                optional_text_parameter(body),
            ],
        )
        .await?;
        let id = outcome.last_insert_rowid.ok_or_else(|| {
            Error::new(ErrorKind::InvalidData, "Record insert did not return an id")
        })?;
        self.load_record_by_id(id).await
    }

    async fn create_proposal(
        &self,
        request: CreateProposalRequest,
    ) -> Result<i64, TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let role = request.role;
        let title = normalize_nonempty(&request.title, "Transfer title")?;
        let topic_text = normalize_optional_text(request.topic_text);
        let quantity = positive_quantity(request.quantity)?;
        let record = self
            .load_record_by_id(request.record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let target_organ = self.load_optional_organ(request.target_organ_id).await?;
        let counterparty_label =
            normalize_counterparty(request.counterparty_label, target_organ.as_ref())?;
        let transfer_uid = Uuid::new_v4().to_string();
        let transfer_id = self
            .insert_transfer()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let record_head = record
            .head
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Record #{}", record.id));

        let (contribution, need) = match role {
            TransferSide::Contribution => (
                TransferSideInput {
                    actor_label: local_identity.label.clone(),
                    public_key: Some(local_identity.public_key.clone()),
                    record_id: record.id,
                    head: record_head.clone(),
                    quantity,
                },
                TransferSideInput {
                    actor_label: counterparty_label.clone(),
                    public_key: None,
                    record_id: 0,
                    head: record_head.clone(),
                    quantity,
                },
            ),
            TransferSide::Need => (
                TransferSideInput {
                    actor_label: counterparty_label.clone(),
                    public_key: None,
                    record_id: 0,
                    head: record_head.clone(),
                    quantity,
                },
                TransferSideInput {
                    actor_label: local_identity.label.clone(),
                    public_key: Some(local_identity.public_key.clone()),
                    record_id: record.id,
                    head: record_head.clone(),
                    quantity,
                },
            ),
        };

        self.insert_transfer_identity(TransferIdentityInput {
            transfer_id,
            transfer_uid: transfer_uid.clone(),
            parent_transfer_uid: None,
            source_transfer_uid: None,
            state: TransferState::PublicProposal.as_str().to_string(),
            title: title.clone(),
            coordinator_label: COORDINATOR_LABEL.to_string(),
            proposer_label: local_identity.label.clone(),
            counterparty_label: counterparty_label.clone(),
            contribution_actor_label: contribution.actor_label.clone(),
            contribution_public_key: contribution.public_key.clone(),
            need_actor_label: need.actor_label.clone(),
            need_public_key: need.public_key.clone(),
            target_organ: target_organ.clone(),
            target_base_url: None,
            source_base_url: Some(self.local_base_url.clone()),
            topic_text: topic_text.clone(),
        })
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.upsert_structured_transfer_sides(
            transfer_id,
            &contribution,
            &need,
            target_organ.as_ref().map(|organ| organ.id),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.ensure_transfer_visibility_policy(transfer_id, "hidden", None)
            .await
            .map_err(TransferWidgetError::from_io)?;
        if let Some(organ) = target_organ.as_ref() {
            self.allow_transfer_visibility_for_organ(transfer_id, organ.id)
                .await
                .map_err(TransferWidgetError::from_io)?;
        }
        if let Some(parent_transfer_id) = request.parent_transfer_id {
            let parent = self
                .load_transfer_summary(parent_transfer_id)
                .await
                .map_err(TransferWidgetError::from_io)?;
            self.upsert_transfer_relation(
                &transfer_uid,
                TransferRelationType::Parent.as_str(),
                &parent.transfer_uid,
                None,
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }

        let identity = self
            .load_transfer_identity_by_id(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &identity,
            &local_identity,
            EventKind::ProposalCreated,
            json!({
                "event_type": "proposal_created",
                "title": title,
                "local_role": role.as_str(),
                "record": record,
                "quantity": quantity,
                "counterparty_label": counterparty_label,
                "target_organ_id": target_organ.as_ref().map(|organ| organ.id),
                "target_organ_name": target_organ.as_ref().map(|organ| organ.name.clone()),
                "topic_text": topic_text
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &identity,
            &local_identity,
            EventKind::ItemCreated,
            json!({
                "contribution": contribution,
                "need": need
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.refresh_transfer_reservation(transfer_id, ReservationRefreshTrigger::ProposalCreated)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(transfer_id)
    }

    async fn create_child_transfer(
        &self,
        request: CreateChildTransferRequest,
    ) -> Result<i64, TransferWidgetError> {
        self.load_transfer_summary(request.parent_transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.create_proposal(CreateProposalRequest {
            title: request.title,
            role: request.role,
            record_id: request.record_id,
            quantity: request.quantity,
            counterparty_label: request.counterparty_label,
            target_organ_id: request.target_organ_id,
            parent_transfer_id: Some(request.parent_transfer_id),
            topic_text: None,
        })
        .await
    }

    async fn duplicate_proposal(
        &self,
        request: DuplicateProposalRequest,
    ) -> Result<i64, TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let source = self
            .load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let local_record = self
            .load_record_by_id(request.local_record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        if local_role_for(&source, Some(&local_identity)).is_some() {
            return Err(TransferWidgetError::Invalid(
                "This local party is already one of the parties on that Transfer.".into(),
            ));
        }

        let transfer_uid = Uuid::new_v4().to_string();
        let transfer_id = self
            .insert_transfer()
            .await
            .map_err(TransferWidgetError::from_io)?;
        let local_head = local_record
            .head
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Record #{}", local_record.id));
        let quantity = match request.local_role {
            TransferSide::Contribution => source.contribution_quantity.abs(),
            TransferSide::Need => source.need_quantity.abs(),
        };

        let contribution = if request.local_role == TransferSide::Contribution {
            TransferSideInput {
                actor_label: local_identity.label.clone(),
                public_key: Some(local_identity.public_key.clone()),
                record_id: local_record.id,
                head: local_head,
                quantity,
            }
        } else {
            TransferSideInput {
                actor_label: source.contribution_actor_label.clone(),
                public_key: source.contribution_public_key.clone(),
                record_id: source.contribution_id,
                head: source.contribution_head.clone(),
                quantity: source.contribution_quantity.abs(),
            }
        };
        let need = if request.local_role == TransferSide::Need {
            TransferSideInput {
                actor_label: local_identity.label.clone(),
                public_key: Some(local_identity.public_key.clone()),
                record_id: local_record.id,
                head: local_record
                    .head
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("Record #{}", local_record.id)),
                quantity,
            }
        } else {
            TransferSideInput {
                actor_label: source.need_actor_label.clone(),
                public_key: source.need_public_key.clone(),
                record_id: source.need_id,
                head: source.need_head.clone(),
                quantity: source.need_quantity.abs(),
            }
        };

        if request.local_role == TransferSide::Contribution && need.public_key.is_none() {
            return Err(TransferWidgetError::Invalid(
                "The need side is not signed on the source proposal yet.".into(),
            ));
        }
        if request.local_role == TransferSide::Need && contribution.public_key.is_none() {
            return Err(TransferWidgetError::Invalid(
                "The contribution side is not signed on the source proposal yet.".into(),
            ));
        }

        let target_organ = self.load_optional_organ(source.target_organ_id).await?;
        let reply_base_url = normalize_optional_text(source.source_base_url.clone())
            .filter(|base_url| !same_base_url(base_url, &self.local_base_url))
            .or_else(|| {
                normalize_optional_text(source.target_base_url.clone())
                    .filter(|base_url| !same_base_url(base_url, &self.local_base_url))
            });
        self.insert_transfer_identity(TransferIdentityInput {
            transfer_id,
            transfer_uid: transfer_uid.clone(),
            parent_transfer_uid: Some(source.transfer_uid.clone()),
            source_transfer_uid: Some(source.transfer_uid.clone()),
            state: TransferState::Negotiation.as_str().to_string(),
            title: source.title.clone(),
            coordinator_label: COORDINATOR_LABEL.to_string(),
            proposer_label: source.proposer_label.clone(),
            counterparty_label: local_identity.label.clone(),
            contribution_actor_label: contribution.actor_label.clone(),
            contribution_public_key: contribution.public_key.clone(),
            need_actor_label: need.actor_label.clone(),
            need_public_key: need.public_key.clone(),
            target_organ,
            target_base_url: reply_base_url,
            source_base_url: Some(self.local_base_url.clone()),
            topic_text: source.topic_text.clone(),
        })
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.upsert_structured_transfer_sides(transfer_id, &contribution, &need, None)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_transfer_relation(
            &transfer_uid,
            TransferRelationType::Parent.as_str(),
            &source.transfer_uid,
            None,
        )
        .await
        .map_err(TransferWidgetError::from_io)?;

        let identity = self
            .load_transfer_identity_by_id(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &identity,
            &local_identity,
            EventKind::ProposalDuplicated,
            json!({
                "event_type": "proposal_duplicated",
                "source_transfer_uid": source.transfer_uid,
                "local_role": request.local_role.as_str(),
                "local_record": local_record,
                "contribution": contribution,
                "need": need
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.refresh_transfer_reservation(transfer_id, ReservationRefreshTrigger::ProposalConsumed)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(transfer_id)
    }

    async fn update_transfer_local_item(
        &self,
        request: UpdateTransferLocalItemRequest,
    ) -> Result<(), TransferWidgetError> {
        let title = normalize_nonempty(&request.title, "Transfer title")?;
        let item_title = normalize_nonempty(&request.item_title, "Transfer item title")?;
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        let record = self
            .load_record_by_id(request.record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let quantity = positive_quantity(request.quantity)?;

        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_identity
                 SET title = ?, updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?"
                    .to_string(),
                vec![
                    SqlParameter::Text(title.clone()),
                    SqlParameter::Integer(transfer.id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;

        self.update_structured_side_item(transfer.id, role, record.id, &item_title, quantity)
            .await
            .map_err(TransferWidgetError::from_io)?;

        let identity = self
            .load_transfer_identity_by_id(transfer.id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &identity,
            &local_identity,
            EventKind::ItemEdited,
            json!({
                "event_type": "item_edited",
                "role": role.as_str(),
                "title": title,
                "record": record,
                "item_title": item_title,
                "quantity": quantity
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.invalidate_structured_agreements_for_transfer_edit(transfer.id, None)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.refresh_transfer_reservation(transfer.id, ReservationRefreshTrigger::ProposalCreated)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
    }

    async fn invalidate_structured_agreements_for_transfer_edit(
        &self,
        transfer_id: i64,
        event_id: Option<i64>,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_agreement
                 SET agreement_level = 0,
                     invalidated_at = CURRENT_TIMESTAMP,
                     invalidated_by_event_id = ?,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND invalidated_at IS NULL"
                    .to_string(),
                vec![
                    optional_i64_parameter(event_id),
                    SqlParameter::Integer(transfer_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn update_transfer_work(
        &self,
        request: UpdateTransferWorkRequest,
    ) -> Result<(), TransferWidgetError> {
        self.load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_work_metadata("transfer", request.transfer_id, &request.work)
            .await?;
        Ok(())
    }

    async fn update_transfer_item_work(
        &self,
        request: UpdateTransferItemWorkRequest,
    ) -> Result<(), TransferWidgetError> {
        let belongs_to_transfer = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_structured_item
             WHERE id = ? AND transfer_id = ?",
        )
        .bind(request.structured_item_id)
        .bind(request.transfer_id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(|error| TransferWidgetError::Invalid(error.to_string()))?;
        if belongs_to_transfer <= 0 {
            return Err(TransferWidgetError::Invalid(
                "Transfer item does not belong to this Transfer.".into(),
            ));
        }
        self.upsert_work_metadata(
            "transfer_structured_item",
            request.structured_item_id,
            &request.work,
        )
        .await?;
        Ok(())
    }

    async fn update_transfer_interaction_work(
        &self,
        request: UpdateTransferInteractionWorkRequest,
    ) -> Result<(), TransferWidgetError> {
        let belongs_to_transfer = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_interaction
             WHERE id = ? AND transfer_id = ?",
        )
        .bind(request.interaction_id)
        .bind(request.transfer_id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(|error| TransferWidgetError::Invalid(error.to_string()))?;
        if belongs_to_transfer <= 0 {
            return Err(TransferWidgetError::Invalid(
                "Transfer interaction does not belong to this Transfer.".into(),
            ));
        }
        self.upsert_work_metadata(
            "transfer_interaction",
            request.interaction_id,
            &request.work,
        )
        .await?;
        Ok(())
    }

    async fn upsert_work_metadata(
        &self,
        owner_kind: &str,
        owner_id: i64,
        input: &WorkMetadataInput,
    ) -> Result<i64, TransferWidgetError> {
        if owner_id <= 0 {
            return Err(TransferWidgetError::Invalid(
                "Work metadata owner id must be positive.".into(),
            ));
        }
        let estimate_seconds = match input.estimate_seconds {
            Some(value) if value < 0 => {
                return Err(TransferWidgetError::Invalid(
                    "Estimate seconds cannot be negative.".into(),
                ));
            }
            value => value,
        };
        let metadata_id = self
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO work_metadata(
                    owner_kind,
                    owner_id,
                    start_at,
                    end_at,
                    estimate_seconds,
                    completion_notes,
                    metadata_json,
                    updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, '{}', CURRENT_TIMESTAMP)
                 ON CONFLICT(owner_kind, owner_id) DO UPDATE SET
                    start_at = excluded.start_at,
                    end_at = excluded.end_at,
                    estimate_seconds = excluded.estimate_seconds,
                    completion_notes = excluded.completion_notes,
                    updated_at = CURRENT_TIMESTAMP
                 RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Text(owner_kind.to_string()),
                    SqlParameter::Integer(owner_id),
                    optional_text_param(input.start_at.as_deref()),
                    optional_text_param(input.end_at.as_deref()),
                    optional_i64_param(estimate_seconds),
                    optional_text_param(input.completion_notes.as_deref()),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?
            .last_insert_rowid
            .ok_or_else(|| {
                TransferWidgetError::Invalid("Work metadata upsert returned no id.".into())
            })?;

        self.replace_work_assignments(metadata_id, &input.assignees)
            .await?;
        Ok(metadata_id)
    }

    async fn replace_work_assignments(
        &self,
        work_metadata_id: i64,
        assignees: &[WorkAssigneeInput],
    ) -> Result<(), TransferWidgetError> {
        self.services
            .writer
            .execute_statement(
                "DELETE FROM work_assignment
                 WHERE work_metadata_id = ? AND assignment_kind = 'responsible'"
                    .to_string(),
                vec![SqlParameter::Integer(work_metadata_id)],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;

        for assignee in assignees {
            let subject_id = self.ensure_work_subject(assignee).await?;
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO work_assignment(
                        work_metadata_id,
                        work_subject_id,
                        assignment_kind
                     ) VALUES (?, ?, 'responsible')
                     ON CONFLICT(work_metadata_id, work_subject_id, assignment_kind) DO NOTHING"
                        .to_string(),
                    vec![
                        SqlParameter::Integer(work_metadata_id),
                        SqlParameter::Integer(subject_id),
                    ],
                )
                .await
                .map_err(TransferWidgetError::from_io)?;
        }
        Ok(())
    }

    async fn ensure_work_subject(
        &self,
        input: &WorkAssigneeInput,
    ) -> Result<i64, TransferWidgetError> {
        match input.kind.as_str() {
            "appUser" => {
                let app_user_id =
                    input
                        .app_user_id
                        .filter(|value| *value > 0)
                        .ok_or_else(|| {
                            TransferWidgetError::Invalid(
                                "Local assignee is missing app_user id.".into(),
                            )
                        })?;
                self.services
                    .writer
                    .execute_statement_returning_id(
                        "INSERT INTO work_subject(
                            subject_kind,
                            app_user_id,
                            display_name_snapshot
                         )
                         SELECT
                            'app_user',
                            id,
                            COALESCE(NULLIF(trim(name), ''), NULLIF(trim(username), ''), 'user ' || id)
                         FROM app_user
                         WHERE id = ?
                         ON CONFLICT(app_user_id) WHERE subject_kind = 'app_user' AND app_user_id IS NOT NULL
                         DO UPDATE SET
                            display_name_snapshot = excluded.display_name_snapshot,
                            updated_at = CURRENT_TIMESTAMP
                         RETURNING id"
                            .to_string(),
                        vec![SqlParameter::Integer(app_user_id)],
                    )
                    .await
                    .map_err(TransferWidgetError::from_io)?
                    .last_insert_rowid
                    .ok_or_else(|| {
                        TransferWidgetError::Invalid("Local assignee does not exist.".into())
                    })
            }
            "externalActor" => {
                let display_name = normalize_nonempty(
                    input.display_name.as_deref().unwrap_or(""),
                    "External assignee name",
                )?;
                if has_stable_remote_subject(
                    input.remote_base_url.as_deref(),
                    input.remote_subject_uid.as_deref(),
                ) {
                    return self
                        .services
                        .writer
                        .execute_statement_returning_id(
                            "INSERT INTO work_subject(
                                subject_kind,
                                remote_base_url,
                                remote_public_key,
                                remote_subject_uid,
                                display_name_snapshot,
                                organ_name_snapshot
                             ) VALUES ('external_actor', ?, ?, ?, ?, ?)
                             ON CONFLICT(subject_kind, remote_base_url, remote_subject_uid)
                             WHERE remote_base_url IS NOT NULL AND remote_subject_uid IS NOT NULL
                             DO UPDATE SET
                                remote_public_key = COALESCE(excluded.remote_public_key, work_subject.remote_public_key),
                                display_name_snapshot = excluded.display_name_snapshot,
                                organ_name_snapshot = COALESCE(excluded.organ_name_snapshot, work_subject.organ_name_snapshot),
                                updated_at = CURRENT_TIMESTAMP
                             RETURNING id"
                                .to_string(),
                            vec![
                                optional_text_param(input.remote_base_url.as_deref()),
                                optional_text_param(input.remote_public_key.as_deref()),
                                optional_text_param(input.remote_subject_uid.as_deref()),
                                SqlParameter::Text(display_name),
                                optional_text_param(input.organ_name.as_deref()),
                            ],
                        )
                        .await
                        .map_err(TransferWidgetError::from_io)?
                        .last_insert_rowid
                        .ok_or_else(|| {
                            TransferWidgetError::Invalid(
                                "External assignee upsert returned no id.".into(),
                            )
                        });
                }
                self.services
                    .writer
                    .execute_statement_returning_id(
                        "INSERT INTO work_subject(
                            subject_kind,
                            remote_base_url,
                            remote_public_key,
                            remote_subject_uid,
                            display_name_snapshot,
                            organ_name_snapshot
                         ) VALUES ('external_actor', ?, ?, ?, ?, ?)
                         RETURNING id"
                            .to_string(),
                        vec![
                            optional_text_param(input.remote_base_url.as_deref()),
                            optional_text_param(input.remote_public_key.as_deref()),
                            optional_text_param(input.remote_subject_uid.as_deref()),
                            SqlParameter::Text(display_name),
                            optional_text_param(input.organ_name.as_deref()),
                        ],
                    )
                    .await
                    .map_err(TransferWidgetError::from_io)?
                    .last_insert_rowid
                    .ok_or_else(|| {
                        TransferWidgetError::Invalid(
                            "External assignee insert returned no id.".into(),
                        )
                    })
            }
            _ => Err(TransferWidgetError::Invalid(
                "Unknown work assignee kind.".into(),
            )),
        }
    }

    async fn upsert_work_metadata_package(
        &self,
        owner_kind: &str,
        owner_id: i64,
        package: &WorkMetadataPackage,
    ) -> Result<(), TransferWidgetError> {
        let metadata_id = self
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO work_metadata(
                    owner_kind,
                    owner_id,
                    task_type,
                    status,
                    start_at,
                    end_at,
                    estimate_seconds,
                    completion_notes,
                    metadata_json,
                    updated_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
                 ON CONFLICT(owner_kind, owner_id) DO UPDATE SET
                    task_type = excluded.task_type,
                    status = excluded.status,
                    start_at = excluded.start_at,
                    end_at = excluded.end_at,
                    estimate_seconds = excluded.estimate_seconds,
                    completion_notes = excluded.completion_notes,
                    metadata_json = excluded.metadata_json,
                    updated_at = CURRENT_TIMESTAMP
                 RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Text(owner_kind.to_string()),
                    SqlParameter::Integer(owner_id),
                    optional_text_parameter(package.task_type.clone()),
                    optional_text_parameter(package.status.clone()),
                    optional_text_parameter(package.start_at.clone()),
                    optional_text_parameter(package.end_at.clone()),
                    optional_i64_parameter(package.estimate_seconds),
                    optional_text_parameter(package.completion_notes.clone()),
                    SqlParameter::Text(package.metadata_json()),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?
            .last_insert_rowid
            .ok_or_else(|| {
                TransferWidgetError::Invalid("Packaged work metadata returned no id.".into())
            })?;

        self.services
            .writer
            .execute_statement(
                "DELETE FROM work_assignment WHERE work_metadata_id = ?".to_string(),
                vec![SqlParameter::Integer(metadata_id)],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;

        for assignment in &package.assignments {
            let subject_id = self.ensure_packaged_work_subject(assignment).await?;
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO work_assignment(
                        work_metadata_id,
                        work_subject_id,
                        assignment_kind
                     ) VALUES (?, ?, ?)
                     ON CONFLICT(work_metadata_id, work_subject_id, assignment_kind) DO NOTHING"
                        .to_string(),
                    vec![
                        SqlParameter::Integer(metadata_id),
                        SqlParameter::Integer(subject_id),
                        SqlParameter::Text(assignment.assignment_kind.clone()),
                    ],
                )
                .await
                .map_err(TransferWidgetError::from_io)?;
        }
        Ok(())
    }

    async fn ensure_packaged_work_subject(
        &self,
        assignment: &WorkAssignmentPackage,
    ) -> Result<i64, TransferWidgetError> {
        if assignment.subject_kind == "app_user" {
            if let Some(app_user_id) = assignment.app_user_id.filter(|value| *value > 0) {
                let exists =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM app_user WHERE id = ?")
                        .bind(app_user_id)
                        .fetch_one(&*self.services.db)
                        .await
                        .map_err(|error| TransferWidgetError::Invalid(error.to_string()))?;
                if exists > 0 {
                    return self
                        .ensure_work_subject(&WorkAssigneeInput {
                            kind: "appUser".to_string(),
                            app_user_id: Some(app_user_id),
                            display_name: None,
                            organ_name: None,
                            remote_base_url: None,
                            remote_public_key: None,
                            remote_subject_uid: None,
                        })
                        .await;
                }
            }
        }
        let display_name = assignment
            .display_name
            .as_deref()
            .or(assignment.remote_subject_uid.as_deref())
            .unwrap_or("Remote assignee")
            .to_string();
        self.ensure_work_subject(&WorkAssigneeInput {
            kind: "externalActor".to_string(),
            app_user_id: None,
            display_name: Some(display_name),
            organ_name: assignment.organ_name.clone(),
            remote_base_url: assignment.remote_base_url.clone(),
            remote_public_key: assignment.remote_public_key.clone(),
            remote_subject_uid: assignment.remote_subject_uid.clone(),
        })
        .await
    }

    async fn sign_agreement(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        let (local_agreement, remote_agreement) = match role {
            TransferSide::Contribution => (transfer.first_agreement, transfer.second_agreement),
            TransferSide::Need => (transfer.second_agreement, transfer.first_agreement),
        };
        let next_agreement = if local_agreement <= 0 {
            1
        } else if local_agreement < 2 && remote_agreement >= 1 {
            2
        } else {
            return Err(TransferWidgetError::Invalid(
                "The other side must lock terms before you can accept them.".into(),
            ));
        };
        if transfer.state == TransferState::Inactive.as_str() {
            self.services
                .writer
                .execute_statement(
                    "UPDATE transfer_identity
                     SET state = ?, updated_at = CURRENT_TIMESTAMP
                     WHERE transfer_id = ?"
                        .to_string(),
                    vec![
                        SqlParameter::Text(TransferState::Negotiation.as_str().to_string()),
                        SqlParameter::Integer(transfer.id),
                    ],
                )
                .await
                .map_err(TransferWidgetError::from_io)?;
        }
        self.upsert_structured_agreement_level(transfer.id, role, next_agreement)
            .await
            .map_err(TransferWidgetError::from_io)?;

        self.append_signed_event(
            &transfer.identity_row(),
            &local_identity,
            EventKind::AgreementSigned,
            json!({
                "role": role.as_str(),
                "agreement_type": "full",
                "agreement_level": next_agreement
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.refresh_transfer_reservation(transfer.id, ReservationRefreshTrigger::AgreementLocked)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
    }

    async fn inactivate_transfer(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_identity
                 SET state = ?, updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?"
                    .to_string(),
                vec![
                    SqlParameter::Text(TransferState::Inactive.as_str().to_string()),
                    SqlParameter::Integer(transfer.id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.invalidate_structured_agreements_for_transfer_edit(transfer.id, None)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &transfer.identity_row(),
            &local_identity,
            EventKind::TransferInactivated,
            json!({
                "event_type": "transfer_inactivated",
                "role": role.as_str(),
                "progress_reset": true
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.refresh_transfer_reservation(transfer.id, ReservationRefreshTrigger::Released)
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn append_package_receipt_event(
        &self,
        transfer_id: i64,
        kind: EventKind,
        payload: Value,
    ) -> Result<(), Error> {
        let Some(local_identity) = self.load_local_identity().await? else {
            return Ok(());
        };
        let identity = self.load_transfer_identity_by_id(transfer_id).await?;
        let already_signed = sqlx::query_scalar::<_, i64>(
            "SELECT 1
             FROM transfer_event
             WHERE transfer_id = ?
               AND event_kind = ?
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(kind.as_str())
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .is_some();
        if already_signed {
            return Ok(());
        }
        self.append_signed_event(&identity, &local_identity, kind, payload)
            .await?;
        Ok(())
    }

    async fn confirm_delivery(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        if role != TransferSide::Contribution {
            return Err(TransferWidgetError::Invalid(
                "Only the contribution side can sign delivery.".into(),
            ));
        }
        if !agreements_complete(&transfer) {
            return Err(TransferWidgetError::Invalid(
                "Both parties must sign agreement before delivery.".into(),
            ));
        }
        self.append_event_once(
            &transfer.identity_row(),
            &local_identity,
            EventKind::DeliveryConfirmed,
            json!({
                "role": role.as_str(),
                "record_id": transfer.contribution_id,
                "quantity": transfer.contribution_quantity.abs()
            }),
        )
        .await?;
        self.record_structured_confirmation(
            transfer.id,
            TransferSide::Contribution,
            "delivery",
            EventKind::DeliveryConfirmed,
        )
        .await?;
        Ok(())
    }

    async fn confirm_receipt(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        if role != TransferSide::Need {
            return Err(TransferWidgetError::Invalid(
                "Only the need side can sign receipt.".into(),
            ));
        }
        if !self
            .structured_confirmation_exists(transfer.id, "delivery")
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            return Err(TransferWidgetError::Invalid(
                "Delivery must be signed before receipt.".into(),
            ));
        }
        self.append_event_once(
            &transfer.identity_row(),
            &local_identity,
            EventKind::ReceiptConfirmed,
            json!({
                "role": role.as_str(),
                "record_id": transfer.need_id,
                "quantity": transfer.need_quantity.abs()
            }),
        )
        .await?;
        self.record_structured_confirmation(
            transfer.id,
            TransferSide::Need,
            "receipt",
            EventKind::ReceiptConfirmed,
        )
        .await?;
        self.settle_local(transfer_id).await?;
        Ok(())
    }

    async fn settle_local(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let role = self.require_local_role(&transfer, &local_identity)?;
        self.ensure_settlement_ready(&transfer).await?;
        if self
            .local_settlement_exists(transfer.id, &local_identity.label)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            return Ok(());
        }

        let (record_id, delta) = match role {
            TransferSide::Contribution => (
                transfer.contribution_id,
                -transfer.contribution_quantity.abs(),
            ),
            TransferSide::Need => (transfer.need_id, transfer.need_quantity.abs()),
        };
        if record_id <= 0 {
            return Err(TransferWidgetError::Invalid(
                "This side does not have a local Record selected.".into(),
            ));
        }
        self.apply_settlement_effect(&transfer, &local_identity, role, record_id, delta)
            .await?;
        self.refresh_transfer_reservation(transfer.id, ReservationRefreshTrigger::Settled)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
    }

    async fn settle_full(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let local_identity = self.require_local_identity().await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.ensure_settlement_ready(&transfer).await?;
        if transfer.contribution_id <= 0 || transfer.need_id <= 0 {
            return Err(TransferWidgetError::Invalid(
                "Full settlement requires local Records on both sides.".into(),
            ));
        }
        self.load_record_by_id(transfer.contribution_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.load_record_by_id(transfer.need_id)
            .await
            .map_err(TransferWidgetError::from_io)?;

        if !self
            .local_settlement_exists(transfer.id, &transfer.contribution_actor_label)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            self.apply_settlement_effect(
                &transfer,
                &local_identity,
                TransferSide::Contribution,
                transfer.contribution_id,
                -transfer.contribution_quantity.abs(),
            )
            .await?;
        }

        if !self
            .local_settlement_exists(transfer.id, &transfer.need_actor_label)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            self.apply_settlement_effect(
                &transfer,
                &local_identity,
                TransferSide::Need,
                transfer.need_id,
                transfer.need_quantity.abs(),
            )
            .await?;
        }

        self.refresh_transfer_reservation(transfer.id, ReservationRefreshTrigger::Settled)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
    }

    async fn ensure_settlement_ready(
        &self,
        transfer: &TransferSummaryRow,
    ) -> Result<(), TransferWidgetError> {
        if !self.structured_settlement_ready(transfer).await? {
            return Err(TransferWidgetError::Invalid(
                "Both parties must sign agreement before settlement.".into(),
            ));
        }
        if !self
            .structured_confirmation_exists(transfer.id, "delivery")
            .await
            .map_err(TransferWidgetError::from_io)?
            || !self
                .structured_confirmation_exists(transfer.id, "receipt")
                .await
                .map_err(TransferWidgetError::from_io)?
        {
            return Err(TransferWidgetError::Invalid(
                "Delivery and receipt signatures are required before settlement.".into(),
            ));
        }
        Ok(())
    }

    async fn structured_settlement_ready(
        &self,
        transfer: &TransferSummaryRow,
    ) -> Result<bool, TransferWidgetError> {
        if !agreements_complete(transfer) {
            return Ok(false);
        }
        let blocking_dependencies = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_interaction interaction
             WHERE interaction.transfer_id = ?
               AND interaction.dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle')
               AND interaction.state NOT IN ('settled', 'satisfied', 'complete', 'completed', 'inactive')",
        )
        .bind(transfer.id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(|error| TransferWidgetError::Internal(error.to_string()))?;
        Ok(blocking_dependencies == 0)
    }

    async fn apply_settlement_effect(
        &self,
        transfer: &TransferSummaryRow,
        signer: &LocalIdentityRow,
        role: TransferSide,
        record_id: i64,
        delta: f64,
    ) -> Result<(), TransferWidgetError> {
        let record = self
            .load_record_by_id(record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let next_quantity = record.quantity + delta;

        write::execute_record_update(
            self.services.clone(),
            [record.id as u32],
            "UPDATE record SET quantity = ? WHERE id = ?",
            vec![
                SqlParameter::Real(next_quantity),
                SqlParameter::Integer(record.id),
            ],
        )
        .await
        .map_err(TransferWidgetError::from_io)?;

        let event_id = self
            .append_signed_event(
                &transfer.identity_row(),
                signer,
                EventKind::SettlementApplied,
                json!({
                    "role": role.as_str(),
                    "record_id": record.id,
                    "quantity_delta": delta,
                    "next_quantity": next_quantity
                }),
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_local_settlement(
                    transfer_id,
                    local_record_id,
                    local_actor_label,
                    local_quantity_delta,
                    event_id
                ) VALUES (?, ?, ?, ?, ?)"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer.id),
                    SqlParameter::Integer(record.id),
                    SqlParameter::Text(match role {
                        TransferSide::Contribution => transfer.contribution_actor_label.clone(),
                        TransferSide::Need => transfer.need_actor_label.clone(),
                    }),
                    SqlParameter::Real(delta),
                    SqlParameter::Integer(event_id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.record_structured_settlement(transfer.id, role, record.id, delta, event_id)
            .await?;
        Ok(())
    }

    async fn record_structured_confirmation(
        &self,
        transfer_id: i64,
        role: TransferSide,
        confirmation_kind: &str,
        event_kind: EventKind,
    ) -> Result<(), TransferWidgetError> {
        let party_id = self
            .structured_party_id(transfer_id, role)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let item_id = self
            .structured_item_id(transfer_id, role)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let event_id = self
            .latest_event_id(transfer_id, event_kind)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_confirmation(
                    transfer_id,
                    party_id,
                    scope_kind,
                    scope_id,
                    confirmation_kind,
                    event_id
                ) VALUES (?, ?, 'item', ?, ?, ?)
                ON CONFLICT(transfer_id, party_id, scope_kind, scope_id, confirmation_kind)
                DO UPDATE SET event_id = excluded.event_id,
                              confirmed_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    optional_i64_parameter(party_id),
                    optional_i64_parameter(item_id),
                    SqlParameter::Text(confirmation_kind.to_string()),
                    optional_i64_parameter(event_id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn record_structured_settlement(
        &self,
        transfer_id: i64,
        role: TransferSide,
        record_id: i64,
        delta: f64,
        event_id: i64,
    ) -> Result<(), TransferWidgetError> {
        let party_id = self
            .structured_party_id(transfer_id, role)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let item_id = self
            .structured_item_id(transfer_id, role)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_structured_settlement(
                    transfer_id,
                    party_id,
                    item_id,
                    scope_kind,
                    scope_id,
                    local_record_id,
                    quantity_delta,
                    event_id
                ) VALUES (?, ?, ?, 'item', ?, ?, ?, ?)
                ON CONFLICT(transfer_id, party_id, local_record_id, scope_kind, scope_id)
                DO UPDATE SET quantity_delta = excluded.quantity_delta,
                              event_id = excluded.event_id,
                              settled_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    optional_i64_parameter(party_id),
                    optional_i64_parameter(item_id),
                    optional_i64_parameter(item_id),
                    SqlParameter::Integer(record_id),
                    SqlParameter::Real(delta),
                    SqlParameter::Integer(event_id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn structured_confirmation_exists(
        &self,
        transfer_id: i64,
        confirmation_kind: &str,
    ) -> Result<bool, Error> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_confirmation
             WHERE transfer_id = ? AND confirmation_kind = ?",
        )
        .bind(transfer_id)
        .bind(confirmation_kind)
        .fetch_one(&*self.services.db)
        .await
        .map_err(|error| Error::other(error.to_string()))?
            > 0)
    }

    async fn structured_party_id(
        &self,
        transfer_id: i64,
        role: TransferSide,
    ) -> Result<Option<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_party
             WHERE transfer_id = ? AND role_hint = ?
             ORDER BY id
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(role.as_str())
        .fetch_optional(&*self.services.db)
        .await
        .map_err(|error| Error::other(error.to_string()))
    }

    async fn structured_item_id(
        &self,
        transfer_id: i64,
        role: TransferSide,
    ) -> Result<Option<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_structured_item
             WHERE transfer_id = ? AND role = ?
             ORDER BY id
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(role.as_str())
        .fetch_optional(&*self.services.db)
        .await
        .map_err(|error| Error::other(error.to_string()))
    }

    async fn latest_event_id(
        &self,
        transfer_id: i64,
        event_kind: EventKind,
    ) -> Result<Option<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_event
             WHERE transfer_id = ? AND event_kind = ?
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(event_kind.as_str())
        .fetch_optional(&*self.services.db)
        .await
        .map_err(|error| Error::other(error.to_string()))
    }

    async fn post_transfer_package(
        &self,
        session_token: Option<&str>,
        request: PostTransferRequest,
    ) -> Result<(), TransferWidgetError> {
        let package = self
            .build_transfer_package(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let package_value = serde_json::to_value(&package)
            .map_err(|error| TransferWidgetError::Internal(error.to_string()))?;

        if let Some(base_url) = normalize_optional_text(request.base_url) {
            self.reject_blocked_base_url(&base_url).await?;
            if let Some(organ) = self
                .organs
                .find_by_base_url(&base_url)
                .await
                .map_err(TransferWidgetError::Invalid)?
            {
                self.ensure_transfer_visible_to_organ(request.transfer_id, &organ)
                    .await?;
            }
            let response = self
                .manas
                .send_public_backend_request(
                    &base_url,
                    Method::POST,
                    "/transfer/packages",
                    Some(package_value),
                )
                .await
                .map_err(TransferWidgetError::Invalid)?;
            if !response.status().is_success() {
                let status = response.status();
                let url = response.url().to_string();
                let body = response.text().await.unwrap_or_default();
                return Err(TransferWidgetError::Invalid(
                    describe_remote_transfer_error("Remote node", status.as_u16(), &url, &body),
                ));
            }
            return Ok(());
        }

        let organ_id = request.organ_id.ok_or_else(|| {
            TransferWidgetError::Invalid("Select an Organ or reply target.".into())
        })?;
        let organ = self
            .organs
            .get(organ_id)
            .await
            .map_err(TransferWidgetError::Invalid)?
            .ok_or_else(|| TransferWidgetError::Invalid("Organ not found.".into()))?;
        if organ.trust_state == "blocked" {
            return Err(TransferWidgetError::Invalid(
                "Blocked Organs cannot receive Transfer packages.".into(),
            ));
        }
        self.ensure_transfer_visible_to_organ(request.transfer_id, &organ)
            .await?;

        if !organ_requires_auth(&organ, self.local_auth_required) {
            self.receive_transfer_package_value(package_value).await?;
            return Ok(());
        }

        let response =
            if let Some(session) = self.auth.server_session(session_token, organ.id).await {
                self.manas
                    .send_backend_request(
                        &organ.base_url,
                        &session.bearer_token,
                        Method::POST,
                        "/transfer/packages",
                        Some(package_value),
                    )
                    .await
                    .map_err(TransferWidgetError::Invalid)?
            } else {
                self.manas
                    .send_public_backend_request(
                        &organ.base_url,
                        Method::POST,
                        "/transfer/packages",
                        Some(package_value),
                    )
                    .await
                    .map_err(TransferWidgetError::Invalid)?
            };
        if !response.status().is_success() {
            let status = response.status();
            let url = response.url().to_string();
            let body = response.text().await.unwrap_or_default();
            return Err(TransferWidgetError::Invalid(
                describe_remote_transfer_error("Remote Organ", status.as_u16(), &url, &body),
            ));
        }

        Ok(())
    }

    async fn ensure_transfer_visibility_policy(
        &self,
        transfer_id: i64,
        visibility_mode: &str,
        max_visible_proximity: Option<i64>,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_visibility_policy(
                    transfer_id,
                    visibility_mode,
                    max_visible_proximity
                 ) VALUES (?, ?, ?)
                 ON CONFLICT(transfer_id) DO UPDATE SET
                    visibility_mode = excluded.visibility_mode,
                    max_visible_proximity = excluded.max_visible_proximity,
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(visibility_mode.to_string()),
                    optional_i64_parameter(max_visible_proximity),
                ],
            )
            .await?;
        Ok(())
    }

    async fn set_transfer_visibility(
        &self,
        request: SetTransferVisibilityRequest,
    ) -> Result<(), TransferWidgetError> {
        let mode = request.visibility_mode.trim();
        let max_visible_proximity = match request.max_visible_proximity {
            Some(value) if value < 0 => {
                return Err(TransferWidgetError::Invalid(
                    "Visibility proximity cannot be negative.".into(),
                ));
            }
            value => value,
        };
        match mode {
            "hidden" | "public" => {
                self.ensure_transfer_visibility_policy(request.transfer_id, mode, None)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.clear_transfer_visibility_rules(request.transfer_id)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
            }
            "restricted" => {
                self.ensure_transfer_visibility_policy(
                    request.transfer_id,
                    "restricted",
                    max_visible_proximity,
                )
                .await
                .map_err(TransferWidgetError::from_io)?;
                self.clear_transfer_visibility_rules(request.transfer_id)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                for organ_id in request.organ_ids {
                    self.allow_transfer_visibility_for_organ(request.transfer_id, organ_id)
                        .await
                        .map_err(TransferWidgetError::from_io)?;
                }
            }
            _ => {
                return Err(TransferWidgetError::Invalid(
                    "Visibility mode must be hidden, public, or restricted.".into(),
                ));
            }
        }
        Ok(())
    }

    async fn apply_visibility_wave(
        &self,
        request: ApplyVisibilityWaveRequest,
    ) -> Result<(), TransferWidgetError> {
        let next = request.max_visible_proximity.ok_or_else(|| {
            TransferWidgetError::Invalid("Set the next max proximity for this wave.".into())
        })?;
        if next < 0 {
            return Err(TransferWidgetError::Invalid(
                "Visibility proximity cannot be negative.".into(),
            ));
        }
        let current = self
            .load_transfer_visibility_view(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        if current.visibility_mode != "restricted" {
            return Err(TransferWidgetError::Invalid(
                "Visibility waves only apply to restricted Transfers.".into(),
            ));
        }
        if current
            .max_visible_proximity
            .is_some_and(|existing| next <= existing)
        {
            return Err(TransferWidgetError::Invalid(
                "Visibility waves must widen the current proximity threshold.".into(),
            ));
        }

        self.ensure_transfer_visibility_policy(request.transfer_id, "restricted", Some(next))
            .await
            .map_err(TransferWidgetError::from_io)?;
        if let Some(local_identity) = self
            .load_local_identity()
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            let identity = self
                .load_transfer_identity_by_id(request.transfer_id)
                .await
                .map_err(TransferWidgetError::from_io)?;
            self.append_signed_event(
                &identity,
                &local_identity,
                EventKind::VisibilityChanged,
                json!({
                    "event_type": "visibility_wave",
                    "previous_max_visible_proximity": current.max_visible_proximity,
                    "max_visible_proximity": next,
                    "reason": normalize_optional_text(request.reason)
                        .unwrap_or_else(|| "karma_wave".to_string()),
                }),
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }
        Ok(())
    }

    async fn clear_transfer_visibility_rules(&self, transfer_id: i64) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "DELETE FROM transfer_visibility_rule
                 WHERE transfer_id = ?
                   AND scope_kind = 'transfer'
                   AND scope_id IS NULL"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        Ok(())
    }

    async fn allow_transfer_visibility_for_organ(
        &self,
        transfer_id: i64,
        organ_id: i64,
    ) -> Result<(), Error> {
        let subject_id = self.ensure_visibility_organ_subject(organ_id).await?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_visibility_policy(transfer_id, visibility_mode)
                 VALUES (?, 'restricted')
                 ON CONFLICT(transfer_id) DO NOTHING"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_visibility_policy
                 SET visibility_mode = CASE
                        WHEN visibility_mode = 'public' THEN 'public'
                        ELSE 'restricted'
                     END,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_visibility_rule(
                    transfer_id,
                    subject_id,
                    scope_kind,
                    scope_id,
                    can_discover,
                    can_view
                 ) VALUES (?, ?, 'transfer', NULL, 1, 1)
                 ON CONFLICT DO NOTHING"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(subject_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn ensure_visibility_organ_subject(&self, organ_id: i64) -> Result<i64, Error> {
        if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_visibility_subject
             WHERE subject_kind = 'organ'
               AND organ_id = ?
             ORDER BY id
             LIMIT 1",
        )
        .bind(organ_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        {
            return Ok(id);
        }
        self.services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_visibility_subject(subject_kind, organ_id)
                 VALUES ('organ', ?)
                 RETURNING id"
                    .to_string(),
                vec![SqlParameter::Integer(organ_id)],
            )
            .await?
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Visibility subject returned no id"))
    }

    async fn ensure_transfer_visible_to_organ(
        &self,
        transfer_id: i64,
        organ: &Organ,
    ) -> Result<(), TransferWidgetError> {
        if self
            .transfer_visible_to_organ(transfer_id, organ)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            return Ok(());
        }
        self.allow_transfer_visibility_for_organ(transfer_id, organ.id)
            .await
            .map_err(TransferWidgetError::from_io)
    }

    async fn transfer_visible_to_organ(
        &self,
        transfer_id: i64,
        organ: &Organ,
    ) -> Result<bool, Error> {
        if organ.trust_state == "blocked" {
            return Ok(false);
        }
        let policy = sqlx::query_as::<_, TransferVisibilityPolicyRow>(
            "SELECT visibility_mode, max_visible_proximity
             FROM transfer_visibility_policy
             WHERE transfer_id = ?
             LIMIT 1",
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .unwrap_or_else(|| TransferVisibilityPolicyRow {
            visibility_mode: "hidden".to_string(),
            max_visible_proximity: None,
        });
        if policy.visibility_mode == "public" {
            return Ok(true);
        }
        if policy.visibility_mode == "hidden" {
            return Ok(false);
        }
        if policy
            .max_visible_proximity
            .is_some_and(|max| organ.proximity <= max)
        {
            return Ok(true);
        }
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_visibility_rule rule
             JOIN transfer_visibility_subject subject ON subject.id = rule.subject_id
             WHERE rule.transfer_id = ?
               AND rule.scope_kind = 'transfer'
               AND rule.scope_id IS NULL
               AND rule.can_view = 1
               AND subject.subject_kind = 'organ'
               AND subject.organ_id = ?",
        )
        .bind(transfer_id)
        .bind(organ.id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(count > 0)
    }

    async fn transfer_package_visible_for_export(
        &self,
        package: &TransferPackage,
        requester_organ: Option<&Organ>,
    ) -> Result<bool, TransferWidgetError> {
        let Some(transfer_id) = self
            .find_transfer_id_by_uid(&package.identity.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?
        else {
            return Ok(is_public_proposal_package(package));
        };
        if let Some(organ) = requester_organ {
            return self
                .transfer_visible_to_organ(transfer_id, organ)
                .await
                .map_err(TransferWidgetError::from_io);
        }
        self.transfer_publicly_visible(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)
    }

    async fn transfer_publicly_visible(&self, transfer_id: i64) -> Result<bool, Error> {
        let mode = sqlx::query_scalar::<_, String>(
            "SELECT visibility_mode
             FROM transfer_visibility_policy
             WHERE transfer_id = ?
             LIMIT 1",
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(mode.as_deref() == Some("public"))
    }

    async fn mark_transfer_received(
        &self,
        transfer_id: i64,
        source_base_url: Option<String>,
    ) -> Result<(), Error> {
        let policy = self.transfer_receipt_policy().await?;
        if policy.anonymous_package_viewing {
            return Ok(());
        }
        let source_base_url = source_base_url.unwrap_or_default();
        let send_received_receipt = self
            .organ_allows_transfer_receipt(&source_base_url, policy.send_received_receipts, true)
            .await?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_package_receipt(
                    transfer_id,
                    source_base_url,
                    received_at,
                    received_receipt_generated,
                    seen_receipt_generated
                 ) VALUES (?, ?, CURRENT_TIMESTAMP, ?, 0)
                 ON CONFLICT(transfer_id, source_base_url) DO UPDATE SET
                    received_at = COALESCE(transfer_package_receipt.received_at, CURRENT_TIMESTAMP),
                    received_receipt_generated = CASE
                        WHEN transfer_package_receipt.received_receipt_generated = 1 THEN 1
                        ELSE excluded.received_receipt_generated
                    END"
                .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(source_base_url.clone()),
                    SqlParameter::Integer(bool_i64(send_received_receipt)),
                ],
            )
            .await?;
        if send_received_receipt {
            self.append_package_receipt_event(
                transfer_id,
                EventKind::PackageReceived,
                json!({
                    "event_type": "package_received",
                    "source_base_url": source_base_url,
                }),
            )
            .await?;
        }
        Ok(())
    }

    async fn mark_transfer_seen(&self, transfer_id: i64) -> Result<(), TransferWidgetError> {
        let policy = self
            .transfer_receipt_policy()
            .await
            .map_err(TransferWidgetError::from_io)?;
        if policy.anonymous_package_viewing {
            return Ok(());
        }
        let source_base_url = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)
            .ok()
            .and_then(|transfer| {
                transfer
                    .source_base_url
                    .filter(|value| !value.trim().is_empty())
                    .or(transfer.target_base_url)
            })
            .unwrap_or_default();
        let send_seen_receipt = self
            .organ_allows_transfer_receipt(&source_base_url, policy.send_seen_receipts, false)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_package_receipt(
                    transfer_id,
                    source_base_url,
                    received_at,
                    seen_at,
                    received_receipt_generated,
                    seen_receipt_generated
                 ) VALUES (?, ?, NULL, CURRENT_TIMESTAMP, 0, ?)
                 ON CONFLICT(transfer_id, source_base_url) DO UPDATE SET
                    seen_at = COALESCE(transfer_package_receipt.seen_at, CURRENT_TIMESTAMP),
                    seen_receipt_generated = CASE
                        WHEN transfer_package_receipt.seen_receipt_generated = 1 THEN 1
                        ELSE excluded.seen_receipt_generated
                    END"
                .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(source_base_url.clone()),
                    SqlParameter::Integer(bool_i64(send_seen_receipt)),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        if send_seen_receipt {
            self.append_package_receipt_event(
                transfer_id,
                EventKind::PackageSeen,
                json!({
                    "event_type": "package_seen",
                    "source_base_url": source_base_url,
                }),
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }
        Ok(())
    }

    async fn organ_allows_transfer_receipt(
        &self,
        source_base_url: &str,
        global_enabled: bool,
        received: bool,
    ) -> Result<bool, Error> {
        if !global_enabled {
            return Ok(false);
        }
        let Some(source_base_url) = normalize_optional_text(Some(source_base_url.to_string()))
        else {
            return Ok(global_enabled);
        };
        let Some(organ) = self
            .organs
            .find_by_base_url(&source_base_url)
            .await
            .map_err(Error::other)?
        else {
            return Ok(global_enabled);
        };
        if organ.trust_state == "blocked" {
            return Ok(false);
        }
        Ok(if received {
            organ.transfer_send_received_receipts != 0
        } else {
            organ.transfer_send_seen_receipts != 0
        })
    }

    async fn receive_transfer_package(
        &self,
        package: TransferPackage,
    ) -> Result<TransferImportOutcome, TransferWidgetError> {
        self.reject_blocked_package(&package).await?;
        if package.version != PACKAGE_VERSION {
            return Err(TransferWidgetError::Invalid(
                "Unsupported Transfer package version.".into(),
            ));
        }
        validate_package(&package)?;
        let receipt_source_base_url = package
            .identity
            .source_base_url
            .clone()
            .or_else(|| package.identity.target_base_url.clone());
        let has_structured_rows = structured_package_has_rows(&package.structured);
        if !has_structured_rows {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has no structured rows.".into(),
            ));
        }

        let existing_id = self
            .find_transfer_id_by_uid(&package.identity.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let transfer_id = match existing_id {
            Some(transfer_id) => {
                self.update_transfer_identity(transfer_id, &package.identity)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.upsert_structured_transfer_package(transfer_id, &package.structured)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                transfer_id
            }
            None => {
                let transfer_id = self
                    .insert_transfer()
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.insert_packaged_transfer_identity(transfer_id, &package.identity)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.upsert_structured_transfer_package(transfer_id, &package.structured)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.ensure_transfer_visibility_policy(transfer_id, "hidden", None)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                transfer_id
            }
        };
        for relation in &package.relations {
            self.upsert_transfer_relation(
                &relation.transfer_uid,
                &relation.relation_type,
                &relation.target_transfer_uid,
                relation.position,
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }
        if let Some(config) = &package.tree_config {
            self.upsert_transfer_tree_config(
                &config.transfer_uid,
                TransferBranchMode::parse_storage(&config.branch_mode)
                    .unwrap_or(TransferBranchMode::Inherit),
                TransferRecordSyncMode::parse_storage(&config.record_sync_mode)
                    .unwrap_or(TransferRecordSyncMode::None),
                config
                    .reservation_policy
                    .as_deref()
                    .and_then(TransferReservationPolicy::parse_storage),
                config.source_record_id,
                config.sync_enabled,
                TransferSide::parse_storage(config.sync_role.as_deref()),
                config.sync_quantity,
                config.sync_counterparty_label.clone(),
                config.sync_target_organ_id,
                config.last_synced_record_head.clone(),
                false,
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }
        if let Some(work) = &package.work {
            self.upsert_work_metadata_package("transfer", transfer_id, work)
                .await?;
        }
        for item_work in &package.item_work {
            let item_id = self
                .ensure_packaged_structured_item(transfer_id, item_work)
                .await
                .map_err(TransferWidgetError::from_io)?;
            self.upsert_work_metadata_package("transfer_structured_item", item_id, &item_work.work)
                .await?;
        }
        for interaction_work in &package.interaction_work {
            let interaction_id = self
                .ensure_packaged_interaction(transfer_id, interaction_work)
                .await
                .map_err(TransferWidgetError::from_io)?;
            self.upsert_work_metadata_package(
                "transfer_interaction",
                interaction_id,
                &interaction_work.work,
            )
            .await?;
        }

        let mut events_imported = 0;
        for event in package.events {
            if self
                .event_uid_exists(&event.event_uid)
                .await
                .map_err(TransferWidgetError::from_io)?
            {
                continue;
            }
            self.insert_packaged_event(transfer_id, &event)
                .await
                .map_err(TransferWidgetError::from_io)?;
            events_imported += 1;
        }
        self.mark_transfer_received(transfer_id, receipt_source_base_url)
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(TransferImportOutcome {
            transfer_id,
            events_imported,
        })
    }

    async fn reject_blocked_package(
        &self,
        package: &TransferPackage,
    ) -> Result<(), TransferWidgetError> {
        for base_url in package_peer_base_urls(package) {
            self.reject_blocked_base_url(&base_url).await?;
        }
        Ok(())
    }

    async fn reject_blocked_base_url(&self, base_url: &str) -> Result<(), TransferWidgetError> {
        if let Some(organ) = self
            .organs
            .find_by_base_url(base_url)
            .await
            .map_err(TransferWidgetError::Invalid)?
            && organ.trust_state == "blocked"
        {
            return Err(TransferWidgetError::Invalid(
                "Blocked Organs cannot send or receive Transfer packages.".into(),
            ));
        }
        Ok(())
    }

    async fn package_has_known_peer(
        &self,
        package: &TransferPackage,
    ) -> Result<bool, TransferWidgetError> {
        for base_url in package_peer_base_urls(package) {
            if let Some(organ) = self
                .organs
                .find_by_base_url(&base_url)
                .await
                .map_err(TransferWidgetError::Invalid)?
                && organ.trust_state == "known"
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn package_is_addressed_to_local_node(&self, package: &TransferPackage) -> bool {
        package
            .identity
            .target_base_url
            .as_deref()
            .is_some_and(|value| same_base_url(value, &self.local_base_url))
            || package
                .identity
                .source_base_url
                .as_deref()
                .is_some_and(|value| same_base_url(value, &self.local_base_url))
    }

    fn require_local_role(
        &self,
        transfer: &TransferSummaryRow,
        identity: &LocalIdentityRow,
    ) -> Result<TransferSide, TransferWidgetError> {
        local_role_for(transfer, Some(identity)).ok_or_else(|| {
            TransferWidgetError::Invalid(
                "This local node does not own either signing key for this Transfer.".into(),
            )
        })
    }

    async fn require_local_identity(&self) -> Result<LocalIdentityRow, TransferWidgetError> {
        self.ensure_local_identity()
            .await
            .map_err(TransferWidgetError::from_io)
    }

    async fn ensure_local_identity(&self) -> Result<LocalIdentityRow, Error> {
        if let Some(identity) = self.load_local_identity().await? {
            return Ok(identity);
        }
        self.insert_local_identity("local-cell").await?;
        self.load_local_identity()
            .await?
            .ok_or_else(|| Error::other("Local Transfer identity was not created"))
    }

    async fn load_local_identity(&self) -> Result<Option<LocalIdentityRow>, Error> {
        sqlx::query_as::<_, LocalIdentityRow>(
            "SELECT id, label, public_key, secret_key, created_at, updated_at
             FROM transfer_node_identity
             ORDER BY id
             LIMIT 1",
        )
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_records(&self) -> Result<Vec<RecordView>, Error> {
        sqlx::query_as::<_, RecordView>(
            "SELECT id, quantity, head, body
             FROM record
             ORDER BY id DESC
             LIMIT 100",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_app_user_options(&self) -> Result<Vec<AppUserOption>, Error> {
        sqlx::query_as::<_, AppUserOption>(
            "SELECT id, name, username
             FROM app_user
             ORDER BY lower(COALESCE(NULLIF(name, ''), username)), id
             LIMIT 100",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_record_by_id(&self, id: i64) -> Result<RecordView, Error> {
        sqlx::query_as::<_, RecordView>(
            "SELECT id, quantity, head, body FROM record WHERE id = ? LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Record not found"))
    }

    async fn record_exists(&self, id: i64) -> Result<bool, Error> {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM record WHERE id = ?")
            .bind(id)
            .fetch_one(&*self.services.db)
            .await
            .map(|count| count > 0)
            .map_err(Error::other)
    }

    async fn load_optional_organ(
        &self,
        organ_id: Option<i64>,
    ) -> Result<Option<Organ>, TransferWidgetError> {
        let Some(organ_id) = organ_id.filter(|value| *value > 0) else {
            return Ok(None);
        };
        self.organs
            .get(organ_id)
            .await
            .map_err(TransferWidgetError::Invalid)
    }

    async fn insert_transfer(&self) -> Result<i64, Error> {
        let outcome = self
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer(quantity) VALUES (0) RETURNING id".to_string(),
                vec![],
            )
            .await?;
        outcome
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Transfer insert returned no id"))
    }

    async fn insert_transfer_identity(&self, input: TransferIdentityInput) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_identity(
                    transfer_id,
                    transfer_uid,
                    parent_transfer_uid,
                    source_transfer_uid,
                    state,
                    title,
                    coordinator_label,
                    proposer_label,
                    counterparty_label,
                    contribution_actor_label,
                    contribution_public_key,
                    need_actor_label,
                    need_public_key,
                    target_organ_id,
                    target_organ_name,
                    target_base_url,
                    source_base_url,
                    topic_text
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                    .to_string(),
                vec![
                    SqlParameter::Integer(input.transfer_id),
                    SqlParameter::Text(input.transfer_uid),
                    optional_text_parameter(input.parent_transfer_uid),
                    optional_text_parameter(input.source_transfer_uid),
                    SqlParameter::Text(input.state),
                    SqlParameter::Text(input.title),
                    SqlParameter::Text(input.coordinator_label),
                    SqlParameter::Text(input.proposer_label),
                    SqlParameter::Text(input.counterparty_label),
                    SqlParameter::Text(input.contribution_actor_label),
                    optional_text_parameter(input.contribution_public_key),
                    SqlParameter::Text(input.need_actor_label),
                    optional_text_parameter(input.need_public_key),
                    optional_i64_parameter(input.target_organ.as_ref().map(|organ| organ.id)),
                    optional_text_parameter(
                        input.target_organ.as_ref().map(|organ| organ.name.clone()),
                    ),
                    optional_text_parameter(input.target_base_url.or_else(|| {
                        input
                            .target_organ
                            .as_ref()
                            .map(|organ| organ.base_url.clone())
                    })),
                    optional_text_parameter(input.source_base_url),
                    optional_text_parameter(input.topic_text),
                ],
            )
            .await?;
        Ok(())
    }

    async fn insert_packaged_transfer_identity(
        &self,
        transfer_id: i64,
        identity: &TransferIdentityPackage,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_identity(
                    transfer_id,
                    transfer_uid,
                    parent_transfer_uid,
                    source_transfer_uid,
                    state,
                    title,
                    coordinator_label,
                    proposer_label,
                    counterparty_label,
                    contribution_actor_label,
                    contribution_public_key,
                    need_actor_label,
                    need_public_key,
                    target_organ_id,
                    target_organ_name,
                    target_base_url,
                    source_base_url,
                    topic_text
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(identity.transfer_uid.clone()),
                    optional_text_parameter(identity.parent_transfer_uid.clone()),
                    optional_text_parameter(identity.source_transfer_uid.clone()),
                    SqlParameter::Text(identity.state.clone()),
                    SqlParameter::Text(identity.title.clone()),
                    SqlParameter::Text(identity.coordinator_label.clone()),
                    SqlParameter::Text(identity.proposer_label.clone()),
                    SqlParameter::Text(identity.counterparty_label.clone()),
                    SqlParameter::Text(identity.contribution_actor_label.clone()),
                    optional_text_parameter(identity.contribution_public_key.clone()),
                    SqlParameter::Text(identity.need_actor_label.clone()),
                    optional_text_parameter(identity.need_public_key.clone()),
                    optional_i64_parameter(identity.target_organ_id),
                    optional_text_parameter(identity.target_organ_name.clone()),
                    optional_text_parameter(identity.target_base_url.clone()),
                    optional_text_parameter(identity.source_base_url.clone()),
                    optional_text_parameter(identity.topic_text.clone()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn update_transfer_identity(
        &self,
        transfer_id: i64,
        identity: &TransferIdentityPackage,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_identity
                 SET parent_transfer_uid = ?,
                     source_transfer_uid = ?,
                     state = ?,
                     title = ?,
                     coordinator_label = ?,
                     proposer_label = ?,
                     counterparty_label = ?,
                     contribution_actor_label = ?,
                     contribution_public_key = ?,
                     need_actor_label = ?,
                     need_public_key = ?,
                     target_organ_id = ?,
                     target_organ_name = ?,
                     target_base_url = ?,
                     source_base_url = ?,
                     topic_text = ?,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?"
                    .to_string(),
                vec![
                    optional_text_parameter(identity.parent_transfer_uid.clone()),
                    optional_text_parameter(identity.source_transfer_uid.clone()),
                    SqlParameter::Text(identity.state.clone()),
                    SqlParameter::Text(identity.title.clone()),
                    SqlParameter::Text(identity.coordinator_label.clone()),
                    SqlParameter::Text(identity.proposer_label.clone()),
                    SqlParameter::Text(identity.counterparty_label.clone()),
                    SqlParameter::Text(identity.contribution_actor_label.clone()),
                    optional_text_parameter(identity.contribution_public_key.clone()),
                    SqlParameter::Text(identity.need_actor_label.clone()),
                    optional_text_parameter(identity.need_public_key.clone()),
                    optional_i64_parameter(identity.target_organ_id),
                    optional_text_parameter(identity.target_organ_name.clone()),
                    optional_text_parameter(identity.target_base_url.clone()),
                    optional_text_parameter(identity.source_base_url.clone()),
                    optional_text_parameter(identity.topic_text.clone()),
                    SqlParameter::Integer(transfer_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn upsert_structured_transfer_sides(
        &self,
        transfer_id: i64,
        contribution: &TransferSideInput,
        need: &TransferSideInput,
        organ_id: Option<i64>,
    ) -> Result<(), Error> {
        self.upsert_structured_side(
            transfer_id,
            TransferSide::Contribution,
            contribution,
            organ_id,
        )
        .await?;
        self.upsert_structured_side(transfer_id, TransferSide::Need, need, organ_id)
            .await?;
        self.upsert_structured_interaction(transfer_id).await?;
        self.upsert_structured_agreement_level(transfer_id, TransferSide::Contribution, 0)
            .await?;
        self.upsert_structured_agreement_level(transfer_id, TransferSide::Need, 0)
            .await?;
        Ok(())
    }

    async fn upsert_structured_side(
        &self,
        transfer_id: i64,
        role: TransferSide,
        side: &TransferSideInput,
        organ_id: Option<i64>,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_party(
                    transfer_id,
                    party_uid,
                    participation_kind,
                    role_hint,
                    actor_label,
                    public_key,
                    organ_id,
                    placeholder
                 ) VALUES (?, ?, 'participant', ?, ?, ?, ?, ?)
                 ON CONFLICT(transfer_id, party_uid) DO NOTHING"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(format!("party-{transfer_id}-{}", role.as_str())),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Text(side.actor_label.clone()),
                    optional_text_parameter(side.public_key.clone()),
                    optional_i64_parameter(organ_id),
                    SqlParameter::Integer(if side.public_key.is_some() { 0 } else { 1 }),
                ],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_party
                 SET actor_label = ?,
                     public_key = ?,
                     organ_id = ?,
                     placeholder = ?,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND role_hint = ?"
                    .to_string(),
                vec![
                    SqlParameter::Text(side.actor_label.clone()),
                    optional_text_parameter(side.public_key.clone()),
                    optional_i64_parameter(organ_id),
                    SqlParameter::Integer(if side.public_key.is_some() { 0 } else { 1 }),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                ],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_structured_item(
                    transfer_id,
                    item_uid,
                    role,
                    source_record_id,
                    owner_party_id,
                    title,
                    record_head_snapshot,
                    quantity,
                    location
                 ) VALUES (
                    ?,
                    ?,
                    ?,
                    CASE WHEN EXISTS (SELECT 1 FROM record WHERE id = ?) THEN ? ELSE NULL END,
                    (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = ? ORDER BY id LIMIT 1),
                    ?,
                    ?,
                    ?,
                    ?
                 )
                 ON CONFLICT(transfer_id, item_uid) DO NOTHING"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(format!("item-{transfer_id}-{}", role.as_str())),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Integer(side.record_id),
                    SqlParameter::Integer(side.record_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Text(side.head.clone()),
                    SqlParameter::Text(side.head.clone()),
                    SqlParameter::Real(side.quantity.abs()),
                    optional_text_parameter(organ_id.map(|id| format!("organ:{id}"))),
                ],
            )
            .await?;
        self.update_structured_side_item(
            transfer_id,
            role,
            side.record_id,
            &side.head,
            side.quantity.abs(),
        )
        .await?;
        Ok(())
    }

    async fn update_structured_side_item(
        &self,
        transfer_id: i64,
        role: TransferSide,
        record_id: i64,
        title: &str,
        quantity: f64,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_structured_item
                 SET source_record_id = CASE
                        WHEN EXISTS (SELECT 1 FROM record WHERE id = ?) THEN ? ELSE NULL
                     END,
                     owner_party_id = (
                        SELECT id
                        FROM transfer_party
                        WHERE transfer_id = ?
                          AND role_hint = ?
                        ORDER BY id
                        LIMIT 1
                     ),
                     title = ?,
                     record_head_snapshot = ?,
                     quantity = ?,
                     version = version + 1,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND role = ?"
                    .to_string(),
                vec![
                    SqlParameter::Integer(record_id),
                    SqlParameter::Integer(record_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Text(title.to_string()),
                    SqlParameter::Text(title.to_string()),
                    SqlParameter::Real(quantity.abs()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn upsert_structured_interaction(&self, transfer_id: i64) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_interaction(
                    transfer_id,
                    interaction_uid,
                    interaction_kind,
                    direction,
                    from_item_id,
                    to_item_id,
                    from_party_id,
                    to_party_id,
                    quantity,
                    state
                 ) VALUES (
                    ?,
                    ?,
                    'contributes_to',
                    'outgoing',
                    (SELECT id FROM transfer_structured_item WHERE transfer_id = ? AND role = 'contribution' ORDER BY id LIMIT 1),
                    (SELECT id FROM transfer_structured_item WHERE transfer_id = ? AND role = 'need' ORDER BY id LIMIT 1),
                    (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = 'contribution' ORDER BY id LIMIT 1),
                    (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = 'need' ORDER BY id LIMIT 1),
                    (SELECT ABS(COALESCE(quantity, 0)) FROM transfer_structured_item WHERE transfer_id = ? AND role = 'contribution' ORDER BY id LIMIT 1),
                    'proposed'
                 )
                 ON CONFLICT(transfer_id, interaction_uid) DO NOTHING"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(format!("interaction-{transfer_id}-contributes-to")),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                ],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_interaction
                 SET from_item_id = (SELECT id FROM transfer_structured_item WHERE transfer_id = ? AND role = 'contribution' ORDER BY id LIMIT 1),
                     to_item_id = (SELECT id FROM transfer_structured_item WHERE transfer_id = ? AND role = 'need' ORDER BY id LIMIT 1),
                     from_party_id = (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = 'contribution' ORDER BY id LIMIT 1),
                     to_party_id = (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = 'need' ORDER BY id LIMIT 1),
                     quantity = (SELECT ABS(COALESCE(quantity, 0)) FROM transfer_structured_item WHERE transfer_id = ? AND role = 'contribution' ORDER BY id LIMIT 1),
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND interaction_kind = 'contributes_to'"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn upsert_structured_agreement_level(
        &self,
        transfer_id: i64,
        role: TransferSide,
        level: i64,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_agreement(
                    transfer_id,
                    party_id,
                    scope_kind,
                    scope_id,
                    agreement_level,
                    agreed_item_version,
                    agreed_at,
                    invalidated_at
                 ) VALUES (
                    ?,
                    (SELECT id FROM transfer_party WHERE transfer_id = ? AND role_hint = ? ORDER BY id LIMIT 1),
                    'item',
                    (SELECT id FROM transfer_structured_item WHERE transfer_id = ? AND role = ? ORDER BY id LIMIT 1),
                    ?,
                    (SELECT version FROM transfer_structured_item WHERE transfer_id = ? AND role = ? ORDER BY id LIMIT 1),
                    CASE WHEN ? > 0 THEN CURRENT_TIMESTAMP ELSE NULL END,
                    CASE WHEN ? = 0 THEN CURRENT_TIMESTAMP ELSE NULL END
                 )
                 ON CONFLICT(transfer_id, party_id, scope_kind, scope_id) DO UPDATE SET
                    agreement_level = excluded.agreement_level,
                    agreed_item_version = excluded.agreed_item_version,
                    agreed_at = excluded.agreed_at,
                    invalidated_at = excluded.invalidated_at,
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Integer(level),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(role.as_str().to_string()),
                    SqlParameter::Integer(level),
                    SqlParameter::Integer(level),
                ],
            )
            .await?;
        Ok(())
    }

    async fn ensure_packaged_structured_item(
        &self,
        transfer_id: i64,
        item_work: &TransferItemWorkPackage,
    ) -> Result<i64, Error> {
        if let Some(source_record_id) = item_work.source_record_id {
            if let Some(id) = sqlx::query_scalar::<_, i64>(
                "SELECT id
                 FROM transfer_structured_item
                 WHERE transfer_id = ? AND role = ? AND source_record_id = ?
                 ORDER BY id
                 LIMIT 1",
            )
            .bind(transfer_id)
            .bind(&item_work.role)
            .bind(source_record_id)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?
            {
                return Ok(id);
            }
        }
        if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_structured_item
             WHERE transfer_id = ? AND role = ? AND title = ?
             ORDER BY id
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(&item_work.role)
        .bind(&item_work.title)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        {
            return Ok(id);
        }
        self.services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_structured_item(
                    transfer_id,
                    item_uid,
                    role,
                    source_record_id,
                    title,
                    description,
                    quantity,
                    unit,
                    version
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                 RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(structured_row_uid("item")),
                    SqlParameter::Text(item_work.role.clone()),
                    optional_i64_parameter(item_work.source_record_id),
                    SqlParameter::Text(item_work.title.clone()),
                    optional_text_parameter(item_work.description.clone()),
                    item_work
                        .quantity
                        .map(SqlParameter::Real)
                        .unwrap_or(SqlParameter::Null),
                    optional_text_parameter(item_work.unit.clone()),
                    SqlParameter::Integer(item_work.version.unwrap_or(1)),
                ],
            )
            .await?
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Structured item returned no id"))
    }

    async fn ensure_packaged_interaction(
        &self,
        transfer_id: i64,
        interaction_work: &TransferInteractionWorkPackage,
    ) -> Result<i64, Error> {
        if let Some(id) = sqlx::query_scalar::<_, i64>(
            "SELECT id
             FROM transfer_interaction
             WHERE transfer_id = ?
               AND interaction_kind = ?
               AND direction = ?
               AND COALESCE(from_item_id, -1) = COALESCE(?, -1)
               AND COALESCE(to_item_id, -1) = COALESCE(?, -1)
               AND COALESCE(from_party_id, -1) = COALESCE(?, -1)
               AND COALESCE(to_party_id, -1) = COALESCE(?, -1)
             ORDER BY id
             LIMIT 1",
        )
        .bind(transfer_id)
        .bind(&interaction_work.interaction_kind)
        .bind(&interaction_work.direction)
        .bind(interaction_work.from_item_id)
        .bind(interaction_work.to_item_id)
        .bind(interaction_work.from_party_id)
        .bind(interaction_work.to_party_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        {
            return Ok(id);
        }
        self.services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_interaction(
                    transfer_id,
                    interaction_uid,
                    interaction_kind,
                    direction,
                    from_item_id,
                    to_item_id,
                    from_party_id,
                    to_party_id,
                    quantity,
                    state,
                    dependency_kind,
                    version
                 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(structured_row_uid("interaction")),
                    SqlParameter::Text(interaction_work.interaction_kind.clone()),
                    SqlParameter::Text(interaction_work.direction.clone()),
                    optional_i64_parameter(interaction_work.from_item_id),
                    optional_i64_parameter(interaction_work.to_item_id),
                    optional_i64_parameter(interaction_work.from_party_id),
                    optional_i64_parameter(interaction_work.to_party_id),
                    interaction_work
                        .quantity
                        .map(SqlParameter::Real)
                        .unwrap_or(SqlParameter::Null),
                    SqlParameter::Text(interaction_work.state.clone()),
                    optional_text_parameter(interaction_work.dependency_kind.clone()),
                    SqlParameter::Integer(interaction_work.version.unwrap_or(1)),
                ],
            )
            .await?
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Interaction returned no id"))
    }

    async fn upsert_transfer_relation(
        &self,
        transfer_uid: &str,
        relation_type: &str,
        target_transfer_uid: &str,
        position: Option<f64>,
    ) -> Result<(), Error> {
        if transfer_uid == target_transfer_uid {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "A Transfer cannot relate to itself",
            ));
        }
        if relation_type == TransferRelationType::Parent.as_str()
            && self
                .would_create_parent_cycle(transfer_uid, target_transfer_uid)
                .await?
        {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Transfer parent relation would create a cycle",
            ));
        }
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_relation(
                    transfer_uid,
                    relation_type,
                    target_transfer_uid,
                    position
                ) VALUES (?, ?, ?, ?)
                ON CONFLICT(transfer_uid, relation_type, target_transfer_uid)
                DO UPDATE SET
                    position = excluded.position,
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Text(transfer_uid.to_string()),
                    SqlParameter::Text(relation_type.to_string()),
                    SqlParameter::Text(target_transfer_uid.to_string()),
                    optional_f64_parameter(position),
                ],
            )
            .await?;
        Ok(())
    }

    async fn would_create_parent_cycle(
        &self,
        child_uid: &str,
        parent_uid: &str,
    ) -> Result<bool, Error> {
        let mut current = Some(parent_uid.to_string());
        let mut seen = std::collections::BTreeSet::new();
        while let Some(uid) = current {
            if uid == child_uid {
                return Ok(true);
            }
            if !seen.insert(uid.clone()) {
                return Ok(true);
            }
            current = sqlx::query_scalar::<_, String>(
                "SELECT target_transfer_uid
                 FROM transfer_relation
                 WHERE transfer_uid = ?
                   AND relation_type = 'parent'
                 LIMIT 1",
            )
            .bind(uid)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?;
        }
        Ok(false)
    }

    async fn upsert_transfer_tree_config(
        &self,
        transfer_uid: &str,
        branch_mode: TransferBranchMode,
        record_sync_mode: TransferRecordSyncMode,
        reservation_policy: Option<TransferReservationPolicy>,
        source_record_id: Option<i64>,
        sync_enabled: bool,
        sync_role: Option<TransferSide>,
        sync_quantity: Option<f64>,
        sync_counterparty_label: Option<String>,
        sync_target_organ_id: Option<i64>,
        last_synced_record_head: Option<String>,
        mark_synced: bool,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_tree_config(
                    transfer_uid,
                    branch_mode,
                    record_sync_mode,
                    reservation_policy,
                    source_record_id,
                    sync_role,
                    sync_quantity,
                    sync_counterparty_label,
                    sync_target_organ_id,
                    last_synced_record_head,
                    sync_enabled,
                    last_synced_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CASE WHEN ? THEN CURRENT_TIMESTAMP ELSE NULL END)
                ON CONFLICT(transfer_uid)
                DO UPDATE SET
                    branch_mode = excluded.branch_mode,
                    record_sync_mode = excluded.record_sync_mode,
                    reservation_policy = excluded.reservation_policy,
                    source_record_id = COALESCE(excluded.source_record_id, transfer_tree_config.source_record_id),
                    sync_role = COALESCE(excluded.sync_role, transfer_tree_config.sync_role),
                    sync_quantity = COALESCE(excluded.sync_quantity, transfer_tree_config.sync_quantity),
                    sync_counterparty_label = COALESCE(excluded.sync_counterparty_label, transfer_tree_config.sync_counterparty_label),
                    sync_target_organ_id = COALESCE(excluded.sync_target_organ_id, transfer_tree_config.sync_target_organ_id),
                    last_synced_record_head = COALESCE(excluded.last_synced_record_head, transfer_tree_config.last_synced_record_head),
                    sync_enabled = excluded.sync_enabled,
                    last_synced_at = CASE
                        WHEN ? THEN CURRENT_TIMESTAMP
                        ELSE transfer_tree_config.last_synced_at
                    END,
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Text(transfer_uid.to_string()),
                    SqlParameter::Text(branch_mode.as_str().to_string()),
                    SqlParameter::Text(record_sync_mode.as_str().to_string()),
                    optional_text_parameter(
                        reservation_policy.map(|policy| policy.as_str().to_string()),
                    ),
                    optional_i64_parameter(source_record_id),
                    optional_text_parameter(sync_role.map(|role| role.as_str().to_string())),
                    optional_f64_parameter(sync_quantity),
                    optional_text_parameter(sync_counterparty_label),
                    optional_i64_parameter(sync_target_organ_id),
                    optional_text_parameter(last_synced_record_head),
                    SqlParameter::Integer(if sync_enabled { 1 } else { 0 }),
                    SqlParameter::Integer(if mark_synced { 1 } else { 0 }),
                    SqlParameter::Integer(if mark_synced { 1 } else { 0 }),
                ],
            )
            .await?;
        Ok(())
    }

    async fn set_transfer_branch_mode(
        &self,
        request: SetTransferBranchModeRequest,
    ) -> Result<(), TransferWidgetError> {
        let transfer = self
            .load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let mode = TransferBranchMode::parse(&request.branch_mode)?;
        let existing = self
            .load_transfer_tree_config(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_transfer_tree_config(
            &transfer.transfer_uid,
            mode,
            existing
                .as_ref()
                .and_then(|config| TransferRecordSyncMode::parse_storage(&config.record_sync_mode))
                .unwrap_or(TransferRecordSyncMode::None),
            existing
                .as_ref()
                .and_then(|config| config.reservation_policy.as_deref())
                .and_then(TransferReservationPolicy::parse_storage),
            existing.as_ref().and_then(|config| config.source_record_id),
            existing
                .as_ref()
                .is_some_and(|config| config.sync_enabled != 0),
            existing
                .as_ref()
                .and_then(|config| TransferSide::parse_storage(config.sync_role.as_deref())),
            existing.as_ref().and_then(|config| config.sync_quantity),
            existing
                .as_ref()
                .and_then(|config| config.sync_counterparty_label.clone()),
            existing
                .as_ref()
                .and_then(|config| config.sync_target_organ_id),
            existing
                .as_ref()
                .and_then(|config| config.last_synced_record_head.clone()),
            false,
        )
        .await
        .map_err(TransferWidgetError::from_io)
    }

    async fn set_transfer_tree_sync_mode(
        &self,
        request: SetTransferTreeSyncModeRequest,
    ) -> Result<(), TransferWidgetError> {
        let transfer = self
            .load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let mode = TransferRecordSyncMode::parse(&request.record_sync_mode)?;
        let existing = self
            .load_transfer_tree_config(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_transfer_tree_config(
            &transfer.transfer_uid,
            existing
                .as_ref()
                .and_then(|config| TransferBranchMode::parse_storage(&config.branch_mode))
                .unwrap_or(TransferBranchMode::Inherit),
            mode,
            existing
                .as_ref()
                .and_then(|config| config.reservation_policy.as_deref())
                .and_then(TransferReservationPolicy::parse_storage),
            existing.as_ref().and_then(|config| config.source_record_id),
            mode == TransferRecordSyncMode::Live,
            existing
                .as_ref()
                .and_then(|config| TransferSide::parse_storage(config.sync_role.as_deref())),
            existing.as_ref().and_then(|config| config.sync_quantity),
            existing
                .as_ref()
                .and_then(|config| config.sync_counterparty_label.clone()),
            existing
                .as_ref()
                .and_then(|config| config.sync_target_organ_id),
            existing
                .as_ref()
                .and_then(|config| config.last_synced_record_head.clone()),
            false,
        )
        .await
        .map_err(TransferWidgetError::from_io)
    }

    async fn set_transfer_reservation_policy(
        &self,
        request: SetTransferReservationPolicyRequest,
    ) -> Result<(), TransferWidgetError> {
        let transfer = self
            .load_transfer_summary(request.transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let policy = request
            .reservation_policy
            .as_deref()
            .map(TransferReservationPolicy::parse)
            .transpose()?;
        let existing = self
            .load_transfer_tree_config(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_transfer_tree_config(
            &transfer.transfer_uid,
            existing
                .as_ref()
                .and_then(|config| TransferBranchMode::parse_storage(&config.branch_mode))
                .unwrap_or(TransferBranchMode::Inherit),
            existing
                .as_ref()
                .and_then(|config| TransferRecordSyncMode::parse_storage(&config.record_sync_mode))
                .unwrap_or(TransferRecordSyncMode::None),
            policy,
            existing.as_ref().and_then(|config| config.source_record_id),
            existing
                .as_ref()
                .is_some_and(|config| config.sync_enabled != 0),
            existing
                .as_ref()
                .and_then(|config| TransferSide::parse_storage(config.sync_role.as_deref())),
            existing.as_ref().and_then(|config| config.sync_quantity),
            existing
                .as_ref()
                .and_then(|config| config.sync_counterparty_label.clone()),
            existing
                .as_ref()
                .and_then(|config| config.sync_target_organ_id),
            existing
                .as_ref()
                .and_then(|config| config.last_synced_record_head.clone()),
            false,
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        let mut transfer_ids = vec![transfer.id];
        transfer_ids.extend(
            self.load_descendant_transfer_ids(&transfer.transfer_uid)
                .await
                .map_err(TransferWidgetError::from_io)?,
        );
        for transfer_id in transfer_ids {
            self.refresh_transfer_reservation(
                transfer_id,
                ReservationRefreshTrigger::PolicyChanged,
            )
            .await
            .map_err(TransferWidgetError::from_io)?;
        }
        Ok(())
    }

    async fn refresh_transfer_reservation(
        &self,
        transfer_id: i64,
        trigger: ReservationRefreshTrigger,
    ) -> Result<(), Error> {
        let transfer = self.load_transfer_summary(transfer_id).await?;
        let mut affected_record_ids = self
            .load_transfer_influence_record_ids(transfer.id)
            .await?
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if transfer.contribution_id > 0 {
            affected_record_ids.insert(transfer.contribution_id);
        }

        match trigger {
            ReservationRefreshTrigger::Settled => {
                self.services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_quantity_influence
                         SET influence_state = 'consumed',
                             consumed_at = CURRENT_TIMESTAMP
                         WHERE transfer_id = ?
                           AND influence_state IN ('planned', 'active')"
                            .to_string(),
                        vec![SqlParameter::Integer(transfer.id)],
                    )
                    .await?;
            }
            ReservationRefreshTrigger::Released => {
                self.services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_quantity_influence
                         SET influence_state = 'released',
                             consumed_at = CURRENT_TIMESTAMP
                         WHERE transfer_id = ?
                           AND influence_state IN ('planned', 'active')"
                            .to_string(),
                        vec![SqlParameter::Integer(transfer.id)],
                    )
                    .await?;
            }
            _ => {
                self.services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_quantity_influence
                         SET influence_state = 'released',
                             consumed_at = CURRENT_TIMESTAMP
                         WHERE transfer_id = ?
                           AND influence_state IN ('planned', 'active')"
                            .to_string(),
                        vec![SqlParameter::Integer(transfer.id)],
                    )
                    .await?;

                let Some(local_identity) = self.load_local_identity().await? else {
                    self.refresh_record_transfer_availability_for_records(&affected_record_ids)
                        .await?;
                    return Ok(());
                };
                if self
                    .local_settlement_exists(transfer.id, &local_identity.label)
                    .await?
                {
                    self.refresh_record_transfer_availability_for_records(&affected_record_ids)
                        .await?;
                    return Ok(());
                }
                let Some(local_role) = local_role_for(&transfer, Some(&local_identity)) else {
                    self.refresh_record_transfer_availability_for_records(&affected_record_ids)
                        .await?;
                    return Ok(());
                };
                let (record_id, influence) = match local_role {
                    TransferSide::Contribution => (
                        transfer.contribution_id,
                        -transfer.contribution_quantity.abs(),
                    ),
                    TransferSide::Need => (transfer.need_id, transfer.need_quantity.abs()),
                };
                if record_id <= 0 {
                    self.refresh_record_transfer_availability_for_records(&affected_record_ids)
                        .await?;
                    return Ok(());
                }
                affected_record_ids.insert(record_id);

                let policy = self
                    .effective_transfer_reservation_policy(&transfer.transfer_uid)
                    .await?;
                if policy != TransferReservationPolicy::None {
                    let influence_state = match policy {
                        TransferReservationPolicy::None => None,
                        TransferReservationPolicy::Soft => Some("planned"),
                        TransferReservationPolicy::HardOnProposal => Some("active"),
                        TransferReservationPolicy::HardOnConsume => {
                            if transfer.source_transfer_uid.is_some()
                                || trigger == ReservationRefreshTrigger::ProposalConsumed
                            {
                                Some("active")
                            } else {
                                Some("planned")
                            }
                        }
                        TransferReservationPolicy::HardOnLock => {
                            if agreements_complete(&transfer) {
                                Some("active")
                            } else {
                                Some("planned")
                            }
                        }
                    };

                    if let Some(influence_state) = influence_state {
                        self.services
                            .writer
                            .execute_statement(
                                "INSERT INTO transfer_quantity_influence(
                                    transfer_id,
                                    item_id,
                                    interaction_id,
                                    record_id,
                                    influence,
                                    influence_state,
                                    policy
                                ) VALUES (?, NULL, NULL, ?, ?, ?, 'manual')"
                                    .to_string(),
                                vec![
                                    SqlParameter::Integer(transfer.id),
                                    SqlParameter::Integer(record_id),
                                    SqlParameter::Real(influence),
                                    SqlParameter::Text(influence_state.to_string()),
                                ],
                            )
                            .await?;
                    }
                }
            }
        }

        self.refresh_record_transfer_availability_for_records(&affected_record_ids)
            .await
    }

    async fn effective_transfer_reservation_policy(
        &self,
        transfer_uid: &str,
    ) -> Result<TransferReservationPolicy, Error> {
        let mut current = Some(transfer_uid.to_string());
        let mut seen = std::collections::BTreeSet::new();
        while let Some(uid) = current {
            if !seen.insert(uid.clone()) {
                break;
            }
            if let Some(policy) = sqlx::query_scalar::<_, String>(
                "SELECT reservation_policy
                 FROM transfer_tree_config
                 WHERE transfer_uid = ?
                   AND reservation_policy IS NOT NULL
                 LIMIT 1",
            )
            .bind(&uid)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?
                && let Some(parsed) = TransferReservationPolicy::parse_storage(&policy)
            {
                return Ok(parsed);
            }
            current = sqlx::query_scalar::<_, String>(
                "SELECT target_transfer_uid
                 FROM transfer_relation
                 WHERE transfer_uid = ?
                   AND relation_type = 'parent'
                 LIMIT 1",
            )
            .bind(&uid)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?;
        }

        let configured = sqlx::query_scalar::<_, String>(
            "SELECT COALESCE(
                (SELECT transfer_reservation_policy
                 FROM configuration
                 WHERE quantity = 1
                 ORDER BY id
                 LIMIT 1),
                'soft'
             )",
        )
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(TransferReservationPolicy::parse_storage(&configured)
            .unwrap_or(TransferReservationPolicy::Soft))
    }

    async fn load_transfer_influence_record_ids(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT DISTINCT record_id
             FROM transfer_quantity_influence
             WHERE transfer_id = ?",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_descendant_transfer_ids(&self, transfer_uid: &str) -> Result<Vec<i64>, Error> {
        let mut descendant_ids = Vec::new();
        let mut stack = vec![transfer_uid.to_string()];
        let mut seen = std::collections::BTreeSet::new();
        while let Some(parent_uid) = stack.pop() {
            if !seen.insert(parent_uid.clone()) {
                continue;
            }
            let children = sqlx::query_as::<_, (i64, String)>(
                "SELECT ident.transfer_id, ident.transfer_uid
                 FROM transfer_identity ident
                 JOIN transfer_relation rel ON rel.transfer_uid = ident.transfer_uid
                 WHERE rel.target_transfer_uid = ?
                   AND rel.relation_type = 'parent'",
            )
            .bind(&parent_uid)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?;
            for (transfer_id, child_uid) in children {
                descendant_ids.push(transfer_id);
                stack.push(child_uid);
            }
        }
        Ok(descendant_ids)
    }

    async fn refresh_record_transfer_availability_for_records(
        &self,
        record_ids: &std::collections::BTreeSet<i64>,
    ) -> Result<(), Error> {
        for record_id in record_ids
            .iter()
            .copied()
            .filter(|record_id| *record_id > 0)
        {
            self.services
                .writer
                .execute_statement(
                    "DELETE FROM record_transfer_availability WHERE record_id = ?".to_string(),
                    vec![SqlParameter::Integer(record_id)],
                )
                .await?;
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO record_transfer_availability(
                        record_id,
                        actual_quantity,
                        proposed_outgoing_quantity,
                        proposed_incoming_quantity,
                        reserved_quantity,
                        reserved_incoming_quantity,
                        available_quantity,
                        planned_quantity,
                        updated_at
                     )
                     WITH projection AS (
                        SELECT
                            record.id AS record_id,
                            record.quantity AS actual_quantity,
                            COALESCE(SUM(CASE
                                WHEN transfer_quantity_influence.influence_state = 'planned'
                                     AND transfer_quantity_influence.influence < 0
                                THEN ABS(transfer_quantity_influence.influence)
                                ELSE 0
                            END), 0) AS proposed_outgoing_quantity,
                            COALESCE(SUM(CASE
                                WHEN transfer_quantity_influence.influence_state = 'planned'
                                     AND transfer_quantity_influence.influence > 0
                                THEN transfer_quantity_influence.influence
                                ELSE 0
                            END), 0) AS proposed_incoming_quantity,
                            COALESCE(SUM(CASE
                                WHEN transfer_quantity_influence.influence_state = 'active'
                                     AND transfer_quantity_influence.influence < 0
                                THEN ABS(transfer_quantity_influence.influence)
                                ELSE 0
                            END), 0) AS reserved_outgoing_quantity,
                            COALESCE(SUM(CASE
                                WHEN transfer_quantity_influence.influence_state = 'active'
                                     AND transfer_quantity_influence.influence > 0
                                THEN transfer_quantity_influence.influence
                                ELSE 0
                            END), 0) AS reserved_incoming_quantity
                        FROM record
                        LEFT JOIN transfer_quantity_influence
                            ON transfer_quantity_influence.record_id = record.id
                           AND transfer_quantity_influence.influence_state IN ('planned', 'active')
                        WHERE record.id = ?
                        GROUP BY record.id
                     )
                     SELECT
                        record_id,
                        actual_quantity,
                        proposed_outgoing_quantity,
                        proposed_incoming_quantity,
                        reserved_outgoing_quantity,
                        reserved_incoming_quantity,
                        actual_quantity - reserved_outgoing_quantity,
                        actual_quantity
                            + proposed_incoming_quantity
                            + reserved_incoming_quantity
                            - proposed_outgoing_quantity
                            - reserved_outgoing_quantity,
                        CURRENT_TIMESTAMP
                     FROM projection"
                        .to_string(),
                    vec![SqlParameter::Integer(record_id)],
                )
                .await?;
        }
        Ok(())
    }
    async fn create_transfer_tree_from_record(
        &self,
        request: CreateTransferTreeFromRecordRequest,
    ) -> Result<usize, TransferWidgetError> {
        self.load_transfer_summary(request.parent_transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let sync_mode = TransferRecordSyncMode::parse(&request.record_sync_mode)?;
        let created = self
            .create_transfer_tree_node(
                request.parent_transfer_id,
                request.root_record_id,
                request.role,
                request.quantity,
                request.counterparty_label,
                request.target_organ_id,
                sync_mode,
            )
            .await?;
        Ok(created)
    }

    async fn create_transfer_tree_node(
        &self,
        parent_transfer_id: i64,
        record_id: i64,
        role: TransferSide,
        quantity: f64,
        counterparty_label: String,
        target_organ_id: Option<i64>,
        sync_mode: TransferRecordSyncMode,
    ) -> Result<usize, TransferWidgetError> {
        let record = self
            .load_record_by_id(record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let title = record
            .head
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Record #{}", record.id));
        let transfer_id = self
            .create_child_transfer(CreateChildTransferRequest {
                parent_transfer_id,
                title,
                role,
                record_id,
                quantity,
                counterparty_label: counterparty_label.clone(),
                target_organ_id,
            })
            .await?;
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.upsert_transfer_tree_config(
            &transfer.transfer_uid,
            TransferBranchMode::Inherit,
            sync_mode,
            None,
            Some(record_id),
            sync_mode == TransferRecordSyncMode::Live,
            Some(role),
            Some(quantity),
            Some(counterparty_label.clone()),
            target_organ_id,
            record.head.clone(),
            true,
        )
        .await
        .map_err(TransferWidgetError::from_io)?;

        let mut created = 1;
        for child_record_id in self
            .load_child_record_ids(record_id)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            created += Box::pin(self.create_transfer_tree_node(
                transfer_id,
                child_record_id,
                role,
                quantity,
                counterparty_label.clone(),
                target_organ_id,
                sync_mode,
            ))
            .await?;
        }
        Ok(created)
    }

    async fn sync_transfer_tree(&self, transfer_id: i64) -> Result<usize, TransferWidgetError> {
        let transfer = self
            .load_transfer_summary(transfer_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let Some(config) = self
            .load_transfer_tree_config(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?
        else {
            return Err(TransferWidgetError::Invalid(
                "This Transfer was not created from a Record tree.".into(),
            ));
        };
        if config.record_sync_mode != TransferRecordSyncMode::Live.as_str()
            || config.sync_enabled == 0
        {
            return Ok(0);
        }
        let Some(source_record_id) = config.source_record_id else {
            return Ok(0);
        };
        self.sync_transfer_title_from_record(&transfer, &config)
            .await
            .map_err(TransferWidgetError::from_io)?;

        let children = self
            .load_child_record_ids(source_record_id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let existing_child_source_ids = self
            .load_child_transfer_source_record_ids(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let mut created = 0;
        for child_record_id in children {
            if existing_child_source_ids.contains(&child_record_id) {
                continue;
            }
            let role = TransferSide::parse_storage(config.sync_role.as_deref())
                .unwrap_or(TransferSide::Need);
            let quantity = config.sync_quantity.unwrap_or(1.0);
            let counterparty_label = config
                .sync_counterparty_label
                .clone()
                .unwrap_or_else(|| transfer.counterparty_label.clone());
            created += self
                .create_transfer_tree_node(
                    transfer_id,
                    child_record_id,
                    role,
                    quantity,
                    counterparty_label,
                    config.sync_target_organ_id.or(transfer.target_organ_id),
                    TransferRecordSyncMode::Live,
                )
                .await?;
        }
        for child_transfer_id in self
            .load_child_transfer_ids(&transfer.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            created += Box::pin(self.sync_transfer_tree(child_transfer_id)).await?;
        }
        self.upsert_transfer_tree_config(
            &transfer.transfer_uid,
            TransferBranchMode::parse_storage(
                self.load_transfer_tree_config(&transfer.transfer_uid)
                    .await
                    .map_err(TransferWidgetError::from_io)?
                    .as_ref()
                    .map(|config| config.branch_mode.as_str())
                    .unwrap_or(TransferBranchMode::Inherit.as_str()),
            )
            .unwrap_or(TransferBranchMode::Inherit),
            TransferRecordSyncMode::Live,
            config
                .reservation_policy
                .as_deref()
                .and_then(TransferReservationPolicy::parse_storage),
            Some(source_record_id),
            true,
            TransferSide::parse_storage(config.sync_role.as_deref()),
            config.sync_quantity,
            config.sync_counterparty_label.clone(),
            config.sync_target_organ_id,
            config.last_synced_record_head.clone(),
            true,
        )
        .await
        .map_err(TransferWidgetError::from_io)?;
        Ok(created)
    }

    async fn sync_transfer_title_from_record(
        &self,
        transfer: &TransferSummaryRow,
        config: &TransferTreeConfigRow,
    ) -> Result<(), Error> {
        let Some(record_id) = config.source_record_id else {
            return Ok(());
        };
        let record = self.load_record_by_id(record_id).await?;
        let next_head = record
            .head
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("Record #{}", record.id));
        let should_update = config
            .last_synced_record_head
            .as_deref()
            .is_none_or(|last| transfer.title == last);
        if should_update && transfer.title != next_head {
            self.services
                .writer
                .execute_statement(
                    "UPDATE transfer_identity
                     SET title = ?, updated_at = CURRENT_TIMESTAMP
                     WHERE transfer_id = ?"
                        .to_string(),
                    vec![
                        SqlParameter::Text(next_head.clone()),
                        SqlParameter::Integer(transfer.id),
                    ],
                )
                .await?;
            if let Some(role) = TransferSide::parse_storage(config.sync_role.as_deref()) {
                self.update_structured_side_item(
                    transfer.id,
                    role,
                    config.source_record_id.unwrap_or(0),
                    &next_head,
                    config.sync_quantity.unwrap_or_else(|| match role {
                        TransferSide::Contribution => transfer.contribution_quantity.abs(),
                        TransferSide::Need => transfer.need_quantity.abs(),
                    }),
                )
                .await?;
            }
        }
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_tree_config
                 SET last_synced_record_head = ?,
                     last_synced_at = CURRENT_TIMESTAMP,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_uid = ?"
                    .to_string(),
                vec![
                    SqlParameter::Text(next_head),
                    SqlParameter::Text(transfer.transfer_uid.clone()),
                ],
            )
            .await?;
        Ok(())
    }

    async fn load_child_record_ids(&self, record_id: i64) -> Result<Vec<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT record_id
             FROM record_link
             WHERE link_type = 'parent'
               AND target_table = 'record'
               AND target_id = ?
             ORDER BY COALESCE(position, id), id",
        )
        .bind(record_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_child_transfer_source_record_ids(
        &self,
        parent_transfer_uid: &str,
    ) -> Result<std::collections::BTreeSet<i64>, Error> {
        let values = sqlx::query_scalar::<_, i64>(
            "SELECT config.source_record_id
             FROM transfer_relation rel
             JOIN transfer_tree_config config ON config.transfer_uid = rel.transfer_uid
             WHERE rel.relation_type = 'parent'
               AND rel.target_transfer_uid = ?
               AND config.source_record_id IS NOT NULL",
        )
        .bind(parent_transfer_uid)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(values.into_iter().collect())
    }

    async fn load_child_transfer_ids(&self, parent_transfer_uid: &str) -> Result<Vec<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT ident.transfer_id
             FROM transfer_relation rel
             JOIN transfer_identity ident ON ident.transfer_uid = rel.transfer_uid
             WHERE rel.relation_type = 'parent'
               AND rel.target_transfer_uid = ?
             ORDER BY COALESCE(rel.position, rel.id), rel.id",
        )
        .bind(parent_transfer_uid)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_transfer_identity_by_id(
        &self,
        transfer_id: i64,
    ) -> Result<IdentityOnlyRow, Error> {
        sqlx::query_as::<_, IdentityOnlyRow>(
            "SELECT
                transfer_id,
                transfer_uid
             FROM transfer_identity
             WHERE transfer_id = ?
             LIMIT 1",
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Transfer identity not found"))
    }

    async fn load_transfer_summary(&self, transfer_id: i64) -> Result<TransferSummaryRow, Error> {
        sqlx::query_as::<_, TransferSummaryRow>(
            transfer_summary_sql("WHERE t.id = ? LIMIT 1").as_str(),
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .ok_or_else(|| Error::new(ErrorKind::NotFound, "Transfer not found"))
    }

    async fn load_transfer_views(
        &self,
        local_identity: Option<&LocalIdentityRow>,
    ) -> Result<Vec<TransferView>, Error> {
        let transfers = sqlx::query_as::<_, TransferSummaryRow>(
            transfer_summary_sql("ORDER BY t.id DESC LIMIT 100").as_str(),
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let relations = self.load_transfer_relations().await?;
        let configs = self.load_transfer_tree_configs().await?;
        let transfer_lookup = transfers
            .iter()
            .map(|transfer| (transfer.transfer_uid.clone(), transfer.id))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut views = Vec::with_capacity(transfers.len());
        for transfer in transfers {
            let events = self.load_transfer_events(transfer.id).await?;
            let cursors = self.load_transfer_cursors(transfer.id).await?;
            let settlements = self.load_transfer_settlements(transfer.id).await?;
            let work = self.load_work_metadata("transfer", transfer.id).await?;
            let receipt = self.load_transfer_receipt_view(transfer.id).await?;
            let visibility = self.load_transfer_visibility_view(transfer.id).await?;
            let items = self.load_transfer_item_work_views(transfer.id).await?;
            let interactions = self
                .load_transfer_interaction_work_views(transfer.id)
                .await?;
            views.push(TransferView::from_rows(
                transfer,
                events,
                cursors,
                settlements,
                work,
                receipt,
                visibility,
                items,
                interactions,
                &relations,
                &configs,
                &transfer_lookup,
                local_identity,
            ));
        }
        Ok(views)
    }

    async fn load_transfer_receipt_view(
        &self,
        transfer_id: i64,
    ) -> Result<TransferReceiptView, Error> {
        let row = sqlx::query_as::<_, TransferReceiptSummaryRow>(
            "SELECT
                MIN(received_at) AS received_at,
                MIN(seen_at) AS seen_at
             FROM transfer_package_receipt
             WHERE transfer_id = ?",
        )
        .bind(transfer_id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let events = sqlx::query_as::<_, TransferReceiptEventRow>(
            "SELECT actor_label, event_kind, created_at
             FROM transfer_event
             WHERE transfer_id = ?
               AND event_kind IN ('package_received', 'package_seen')
             ORDER BY created_at DESC, id DESC
             LIMIT 20",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?
        .into_iter()
        .map(TransferReceiptEventView::from)
        .collect();
        Ok(TransferReceiptView::from_row(row, events))
    }

    async fn load_transfer_visibility_view(
        &self,
        transfer_id: i64,
    ) -> Result<TransferVisibilityView, Error> {
        let policy = sqlx::query_as::<_, TransferVisibilityPolicyRow>(
            "SELECT visibility_mode, max_visible_proximity
             FROM transfer_visibility_policy
             WHERE transfer_id = ?
             LIMIT 1",
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        .unwrap_or_else(|| TransferVisibilityPolicyRow {
            visibility_mode: "hidden".to_string(),
            max_visible_proximity: None,
        });
        let organ_ids = sqlx::query_scalar::<_, i64>(
            "SELECT subject.organ_id
             FROM transfer_visibility_rule rule
             JOIN transfer_visibility_subject subject ON subject.id = rule.subject_id
             WHERE rule.transfer_id = ?
               AND rule.scope_kind = 'transfer'
               AND rule.scope_id IS NULL
               AND rule.can_view = 1
               AND subject.subject_kind = 'organ'
               AND subject.organ_id IS NOT NULL
             ORDER BY subject.organ_id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(TransferVisibilityView {
            visibility_mode: policy.visibility_mode,
            max_visible_proximity: policy.max_visible_proximity,
            organ_ids,
        })
    }

    async fn load_transfer_interaction_work_views(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<TransferInteractionWorkView>, Error> {
        let rows = sqlx::query_as::<_, TransferInteractionWorkRow>(
            "SELECT
                id,
                transfer_id,
                interaction_kind,
                direction,
                from_item_id,
                to_item_id,
                from_party_id,
                to_party_id,
                quantity,
                state,
                dependency_kind,
                version
             FROM transfer_interaction
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let mut views = Vec::with_capacity(rows.len());
        for row in rows {
            let work = self
                .load_work_metadata("transfer_interaction", row.id)
                .await?;
            views.push(TransferInteractionWorkView::from_row(row, work));
        }
        Ok(views)
    }

    async fn load_work_metadata(
        &self,
        owner_kind: &str,
        owner_id: i64,
    ) -> Result<WorkMetadataView, Error> {
        let Some(row) = sqlx::query_as::<_, WorkMetadataRow>(
            "SELECT id, status, start_at, end_at, estimate_seconds, completion_notes, metadata_json
             FROM work_metadata
             WHERE owner_kind = ? AND owner_id = ?
             LIMIT 1",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        else {
            return Ok(WorkMetadataView::empty());
        };
        let assignments = sqlx::query_as::<_, WorkAssignmentJoinRow>(
            "SELECT
                assignment.id,
                subject.id AS work_subject_id,
                assignment.assignment_kind,
                subject.subject_kind,
                subject.app_user_id,
                subject.organ_id,
                subject.transfer_party_id,
                subject.remote_base_url,
                subject.remote_public_key,
                subject.remote_subject_uid,
                subject.display_name_snapshot,
                subject.organ_name_snapshot
             FROM work_assignment assignment
             JOIN work_subject subject ON subject.id = assignment.work_subject_id
             WHERE assignment.work_metadata_id = ?
             ORDER BY assignment.id",
        )
        .bind(row.id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(WorkMetadataView::from_row(row, assignments))
    }

    async fn load_transfer_item_work_views(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<TransferItemWorkView>, Error> {
        let rows = sqlx::query_as::<_, TransferStructuredItemWorkRow>(
            "SELECT id, transfer_id, role, title, description, source_record_id, quantity, unit, version
             FROM transfer_structured_item
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let mut views = Vec::with_capacity(rows.len());
        for row in rows {
            let work = self
                .load_work_metadata("transfer_structured_item", row.id)
                .await?;
            views.push(TransferItemWorkView::from_row(row, work));
        }
        Ok(views)
    }

    async fn load_work_metadata_package(
        &self,
        owner_kind: &str,
        owner_id: i64,
    ) -> Result<Option<WorkMetadataPackage>, Error> {
        let Some(row) = sqlx::query_as::<_, WorkMetadataPackageRow>(
            "SELECT id, task_type, status, start_at, end_at, estimate_seconds, completion_notes, metadata_json
             FROM work_metadata
             WHERE owner_kind = ? AND owner_id = ?
             LIMIT 1",
        )
        .bind(owner_kind)
        .bind(owner_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)?
        else {
            return Ok(None);
        };
        let assignments = sqlx::query_as::<_, WorkAssignmentJoinRow>(
            "SELECT
                assignment.id,
                subject.id AS work_subject_id,
                assignment.assignment_kind,
                subject.subject_kind,
                subject.app_user_id,
                subject.organ_id,
                subject.transfer_party_id,
                subject.remote_base_url,
                subject.remote_public_key,
                subject.remote_subject_uid,
                subject.display_name_snapshot,
                subject.organ_name_snapshot
             FROM work_assignment assignment
             JOIN work_subject subject ON subject.id = assignment.work_subject_id
             WHERE assignment.work_metadata_id = ?
             ORDER BY assignment.id",
        )
        .bind(row.id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(Some(WorkMetadataPackage::from_row(row, assignments)))
    }

    async fn load_transfer_item_work_packages(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<TransferItemWorkPackage>, Error> {
        let rows = sqlx::query_as::<_, TransferStructuredItemWorkRow>(
            "SELECT id, transfer_id, role, title, description, source_record_id, quantity, unit, version
             FROM transfer_structured_item
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let mut packages = Vec::new();
        for row in rows {
            if let Some(work) = self
                .load_work_metadata_package("transfer_structured_item", row.id)
                .await?
            {
                packages.push(TransferItemWorkPackage::from_row(row, work));
            }
        }
        Ok(packages)
    }

    async fn load_transfer_interaction_work_packages(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<TransferInteractionWorkPackage>, Error> {
        let rows = sqlx::query_as::<_, TransferInteractionWorkRow>(
            "SELECT
                id,
                transfer_id,
                interaction_kind,
                direction,
                from_item_id,
                to_item_id,
                from_party_id,
                to_party_id,
                quantity,
                state,
                dependency_kind,
                version
             FROM transfer_interaction
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        let mut packages = Vec::new();
        for row in rows {
            if let Some(work) = self
                .load_work_metadata_package("transfer_interaction", row.id)
                .await?
            {
                packages.push(TransferInteractionWorkPackage::from_row(row, work));
            }
        }
        Ok(packages)
    }

    async fn load_structured_transfer_package(
        &self,
        transfer_id: i64,
    ) -> Result<StructuredTransferPackage, Error> {
        self.ensure_structured_transfer_row_uids(transfer_id)
            .await?;
        let quantity_influences = if self.transfer_share_quantity_projections().await? {
            sqlx::query_as::<_, QuantityInfluencePackage>(
                "SELECT record_id, influence, influence_state, policy, consumed_at
                 FROM transfer_quantity_influence WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?
        } else {
            Vec::new()
        };
        Ok(StructuredTransferPackage {
            parties: sqlx::query_as::<_, TransferPartyPackage>(
                "SELECT party_uid, participation_kind, role_hint, actor_label, public_key, placeholder
                 FROM transfer_party WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            items: sqlx::query_as::<_, StructuredItemPackage>(
                "SELECT item_uid, role, source_record_id, title, description, record_head_snapshot, record_body_snapshot, quantity, unit, location, metadata_json, version
                 FROM transfer_structured_item WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            interactions: sqlx::query_as::<_, StructuredInteractionPackage>(
                "SELECT interaction_uid, interaction_kind, direction, quantity, state, dependency_kind, metadata_json, version
                 FROM transfer_interaction WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            agreements: sqlx::query_as::<_, StructuredAgreementPackage>(
                "SELECT scope_kind, agreement_level, agreed_item_version, agreed_interaction_version, agreed_at, invalidated_at
                 FROM transfer_agreement WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            confirmations: sqlx::query_as::<_, StructuredConfirmationPackage>(
                "SELECT scope_kind, confirmation_kind, confirmed_at
                 FROM transfer_confirmation WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            settlements: sqlx::query_as::<_, StructuredSettlementPackage>(
                "SELECT scope_kind, local_record_id, quantity_delta, settled_at
                 FROM transfer_structured_settlement WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
            quantity_influences,
            messages: sqlx::query_as::<_, TransferMessagePackage>(
                "SELECT body, created_at
                 FROM transfer_message WHERE transfer_id = ? ORDER BY id",
            )
            .bind(transfer_id)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?,
        })
    }

    async fn ensure_structured_transfer_row_uids(&self, transfer_id: i64) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_party
                 SET party_uid = 'party-' || transfer_id || '-' || id,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND party_uid IS NULL"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_structured_item
                 SET item_uid = 'item-' || transfer_id || '-' || id,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND item_uid IS NULL"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_interaction
                 SET interaction_uid = 'interaction-' || transfer_id || '-' || id,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE transfer_id = ?
                   AND interaction_uid IS NULL"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        Ok(())
    }

    async fn upsert_structured_transfer_package(
        &self,
        transfer_id: i64,
        package: &StructuredTransferPackage,
    ) -> Result<(), Error> {
        for party in &package.parties {
            if let Some(party_uid) = normalize_optional_text(party.party_uid.clone()) {
                let outcome = self
                    .services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_party
                     SET participation_kind = ?,
                         role_hint = ?,
                         actor_label = ?,
                         public_key = ?,
                         placeholder = ?,
                         updated_at = CURRENT_TIMESTAMP
                     WHERE transfer_id = ?
                       AND party_uid = ?"
                            .to_string(),
                        vec![
                            SqlParameter::Text(party.participation_kind.clone()),
                            optional_text_parameter(party.role_hint.clone()),
                            SqlParameter::Text(party.actor_label.clone()),
                            optional_text_parameter(party.public_key.clone()),
                            SqlParameter::Integer(party.placeholder),
                            SqlParameter::Integer(transfer_id),
                            SqlParameter::Text(party_uid.clone()),
                        ],
                    )
                    .await?;
                if outcome.rows_affected > 0 {
                    continue;
                }
            }
            let party_uid = normalize_optional_text(party.party_uid.clone())
                .unwrap_or_else(|| structured_row_uid("party"));
            self.services.writer.execute_statement(
                "INSERT INTO transfer_party(transfer_id, party_uid, participation_kind, role_hint, actor_label, public_key, placeholder)
                 SELECT ?, ?, ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_party
                    WHERE transfer_id = ?
                      AND party_uid IS ?
                      AND participation_kind = ?
                      AND role_hint IS ?
                      AND actor_label = ?
                      AND public_key IS ?
                      AND placeholder = ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(party_uid.clone()),
                    SqlParameter::Text(party.participation_kind.clone()),
                    optional_text_parameter(party.role_hint.clone()),
                    SqlParameter::Text(party.actor_label.clone()),
                    optional_text_parameter(party.public_key.clone()),
                    SqlParameter::Integer(party.placeholder),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(party_uid),
                    SqlParameter::Text(party.participation_kind.clone()),
                    optional_text_parameter(party.role_hint.clone()),
                    SqlParameter::Text(party.actor_label.clone()),
                    optional_text_parameter(party.public_key.clone()),
                    SqlParameter::Integer(party.placeholder),
                ],
            ).await?;
        }
        for item in &package.items {
            let source_record_id = match item.source_record_id {
                Some(id) if self.record_exists(id).await? => Some(id),
                _ => None,
            };
            let metadata_json = valid_json_or_empty_object(&item.metadata_json);
            if let Some(item_uid) = normalize_optional_text(item.item_uid.clone()) {
                let outcome = self
                    .services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_structured_item
                     SET role = ?,
                         source_record_id = ?,
                         title = ?,
                         description = ?,
                         record_head_snapshot = ?,
                         record_body_snapshot = ?,
                         quantity = ?,
                         unit = ?,
                         location = ?,
                         metadata_json = ?,
                         version = ?,
                         updated_at = CURRENT_TIMESTAMP
                     WHERE transfer_id = ?
                       AND item_uid = ?"
                            .to_string(),
                        vec![
                            SqlParameter::Text(item.role.clone()),
                            optional_i64_parameter(source_record_id),
                            SqlParameter::Text(item.title.clone()),
                            optional_text_parameter(item.description.clone()),
                            optional_text_parameter(item.record_head_snapshot.clone()),
                            optional_text_parameter(item.record_body_snapshot.clone()),
                            optional_f64_parameter(item.quantity),
                            optional_text_parameter(item.unit.clone()),
                            optional_text_parameter(item.location.clone()),
                            SqlParameter::Text(metadata_json.clone()),
                            SqlParameter::Integer(item.version.unwrap_or(1)),
                            SqlParameter::Integer(transfer_id),
                            SqlParameter::Text(item_uid.clone()),
                        ],
                    )
                    .await?;
                if outcome.rows_affected > 0 {
                    continue;
                }
            }
            let item_uid = normalize_optional_text(item.item_uid.clone())
                .unwrap_or_else(|| structured_row_uid("item"));
            self.services.writer.execute_statement(
                "INSERT INTO transfer_structured_item(transfer_id, item_uid, role, source_record_id, title, description, record_head_snapshot, record_body_snapshot, quantity, unit, location, metadata_json, version)
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_structured_item
                    WHERE transfer_id = ?
                      AND item_uid IS ?
                      AND role = ?
                      AND source_record_id IS ?
                      AND title = ?
                      AND description IS ?
                      AND record_head_snapshot IS ?
                      AND record_body_snapshot IS ?
                      AND quantity IS ?
                      AND unit IS ?
                      AND location IS ?
                      AND metadata_json = ?
                      AND version = ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(item_uid.clone()),
                    SqlParameter::Text(item.role.clone()),
                    optional_i64_parameter(source_record_id),
                    SqlParameter::Text(item.title.clone()),
                    optional_text_parameter(item.description.clone()),
                    optional_text_parameter(item.record_head_snapshot.clone()),
                    optional_text_parameter(item.record_body_snapshot.clone()),
                    optional_f64_parameter(item.quantity),
                    optional_text_parameter(item.unit.clone()),
                    optional_text_parameter(item.location.clone()),
                    SqlParameter::Text(metadata_json.clone()),
                    SqlParameter::Integer(item.version.unwrap_or(1)),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(item_uid),
                    SqlParameter::Text(item.role.clone()),
                    optional_i64_parameter(source_record_id),
                    SqlParameter::Text(item.title.clone()),
                    optional_text_parameter(item.description.clone()),
                    optional_text_parameter(item.record_head_snapshot.clone()),
                    optional_text_parameter(item.record_body_snapshot.clone()),
                    optional_f64_parameter(item.quantity),
                    optional_text_parameter(item.unit.clone()),
                    optional_text_parameter(item.location.clone()),
                    SqlParameter::Text(metadata_json),
                    SqlParameter::Integer(item.version.unwrap_or(1)),
                ],
            ).await?;
        }
        for interaction in &package.interactions {
            let metadata_json = valid_json_or_empty_object(&interaction.metadata_json);
            if let Some(interaction_uid) =
                normalize_optional_text(interaction.interaction_uid.clone())
            {
                let outcome = self
                    .services
                    .writer
                    .execute_statement(
                        "UPDATE transfer_interaction
                     SET interaction_kind = ?,
                         direction = ?,
                         quantity = ?,
                         state = ?,
                         dependency_kind = ?,
                         metadata_json = ?,
                         version = ?,
                         updated_at = CURRENT_TIMESTAMP
                     WHERE transfer_id = ?
                       AND interaction_uid = ?"
                            .to_string(),
                        vec![
                            SqlParameter::Text(interaction.interaction_kind.clone()),
                            SqlParameter::Text(interaction.direction.clone()),
                            optional_f64_parameter(interaction.quantity),
                            SqlParameter::Text(interaction.state.clone()),
                            optional_text_parameter(interaction.dependency_kind.clone()),
                            SqlParameter::Text(metadata_json.clone()),
                            SqlParameter::Integer(interaction.version.unwrap_or(1)),
                            SqlParameter::Integer(transfer_id),
                            SqlParameter::Text(interaction_uid.clone()),
                        ],
                    )
                    .await?;
                if outcome.rows_affected > 0 {
                    continue;
                }
            }
            let interaction_uid = normalize_optional_text(interaction.interaction_uid.clone())
                .unwrap_or_else(|| structured_row_uid("interaction"));
            self.services.writer.execute_statement(
                "INSERT INTO transfer_interaction(transfer_id, interaction_uid, interaction_kind, direction, quantity, state, dependency_kind, metadata_json, version)
                 SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_interaction
                    WHERE transfer_id = ?
                      AND interaction_uid IS ?
                      AND interaction_kind = ?
                      AND direction = ?
                      AND quantity IS ?
                      AND state = ?
                      AND dependency_kind IS ?
                      AND metadata_json = ?
                      AND version = ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(interaction_uid.clone()),
                    SqlParameter::Text(interaction.interaction_kind.clone()),
                    SqlParameter::Text(interaction.direction.clone()),
                    optional_f64_parameter(interaction.quantity),
                    SqlParameter::Text(interaction.state.clone()),
                    optional_text_parameter(interaction.dependency_kind.clone()),
                    SqlParameter::Text(metadata_json.clone()),
                    SqlParameter::Integer(interaction.version.unwrap_or(1)),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(interaction_uid),
                    SqlParameter::Text(interaction.interaction_kind.clone()),
                    SqlParameter::Text(interaction.direction.clone()),
                    optional_f64_parameter(interaction.quantity),
                    SqlParameter::Text(interaction.state.clone()),
                    optional_text_parameter(interaction.dependency_kind.clone()),
                    SqlParameter::Text(metadata_json),
                    SqlParameter::Integer(interaction.version.unwrap_or(1)),
                ],
            ).await?;
        }
        for agreement in &package.agreements {
            self.services.writer.execute_statement(
                "INSERT INTO transfer_agreement(transfer_id, scope_kind, agreement_level, agreed_item_version, agreed_interaction_version, agreed_at, invalidated_at)
                 SELECT ?, ?, ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_agreement
                    WHERE transfer_id = ?
                      AND scope_kind = ?
                      AND agreement_level = ?
                      AND agreed_item_version IS ?
                      AND agreed_interaction_version IS ?
                      AND agreed_at IS ?
                      AND invalidated_at IS ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(agreement.scope_kind.clone()),
                    SqlParameter::Integer(agreement.agreement_level),
                    optional_i64_parameter(agreement.agreed_item_version),
                    optional_i64_parameter(agreement.agreed_interaction_version),
                    optional_text_parameter(agreement.agreed_at.clone()),
                    optional_text_parameter(agreement.invalidated_at.clone()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(agreement.scope_kind.clone()),
                    SqlParameter::Integer(agreement.agreement_level),
                    optional_i64_parameter(agreement.agreed_item_version),
                    optional_i64_parameter(agreement.agreed_interaction_version),
                    optional_text_parameter(agreement.agreed_at.clone()),
                    optional_text_parameter(agreement.invalidated_at.clone()),
                ],
            ).await?;
        }
        for confirmation in &package.confirmations {
            self.services.writer.execute_statement(
                "INSERT INTO transfer_confirmation(transfer_id, scope_kind, confirmation_kind, confirmed_at)
                 SELECT ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_confirmation
                    WHERE transfer_id = ?
                      AND scope_kind = ?
                      AND confirmation_kind = ?
                      AND confirmed_at = ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(confirmation.scope_kind.clone()),
                    SqlParameter::Text(confirmation.confirmation_kind.clone()),
                    SqlParameter::Text(confirmation.confirmed_at.clone()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(confirmation.scope_kind.clone()),
                    SqlParameter::Text(confirmation.confirmation_kind.clone()),
                    SqlParameter::Text(confirmation.confirmed_at.clone()),
                ],
            ).await?;
        }
        for settlement in &package.settlements {
            if !self.record_exists(settlement.local_record_id).await? {
                continue;
            }
            self.services.writer.execute_statement(
                "INSERT INTO transfer_structured_settlement(transfer_id, scope_kind, local_record_id, quantity_delta, settled_at)
                 SELECT ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_structured_settlement
                    WHERE transfer_id = ?
                      AND scope_kind = ?
                      AND local_record_id = ?
                      AND quantity_delta = ?
                      AND settled_at = ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(settlement.scope_kind.clone()),
                    SqlParameter::Integer(settlement.local_record_id),
                    SqlParameter::Real(settlement.quantity_delta),
                    SqlParameter::Text(settlement.settled_at.clone()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(settlement.scope_kind.clone()),
                    SqlParameter::Integer(settlement.local_record_id),
                    SqlParameter::Real(settlement.quantity_delta),
                    SqlParameter::Text(settlement.settled_at.clone()),
                ],
            ).await?;
        }
        for influence in &package.quantity_influences {
            if !self.record_exists(influence.record_id).await? {
                continue;
            }
            self.services.writer.execute_statement(
                "INSERT INTO transfer_quantity_influence(transfer_id, record_id, influence, influence_state, policy, consumed_at)
                 SELECT ?, ?, ?, ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_quantity_influence
                    WHERE transfer_id = ?
                      AND record_id = ?
                      AND influence = ?
                      AND influence_state = ?
                      AND policy = ?
                      AND consumed_at IS ?
                 )"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(influence.record_id),
                    SqlParameter::Real(influence.influence),
                    SqlParameter::Text(influence.influence_state.clone()),
                    SqlParameter::Text(influence.policy.clone()),
                    optional_text_parameter(influence.consumed_at.clone()),
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(influence.record_id),
                    SqlParameter::Real(influence.influence),
                    SqlParameter::Text(influence.influence_state.clone()),
                    SqlParameter::Text(influence.policy.clone()),
                    optional_text_parameter(influence.consumed_at.clone()),
                ],
            ).await?;
        }
        for message in &package.messages {
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO transfer_message(transfer_id, body, created_at)
                 SELECT ?, ?, ?
                 WHERE NOT EXISTS (
                    SELECT 1 FROM transfer_message
                    WHERE transfer_id = ?
                      AND body = ?
                      AND created_at = ?
                 )"
                    .to_string(),
                    vec![
                        SqlParameter::Integer(transfer_id),
                        SqlParameter::Text(message.body.clone()),
                        SqlParameter::Text(message.created_at.clone()),
                        SqlParameter::Integer(transfer_id),
                        SqlParameter::Text(message.body.clone()),
                        SqlParameter::Text(message.created_at.clone()),
                    ],
                )
                .await?;
        }
        Ok(())
    }

    async fn load_transfer_relations(&self) -> Result<Vec<TransferRelationRow>, Error> {
        sqlx::query_as::<_, TransferRelationRow>(
            "SELECT
                id,
                transfer_uid,
                relation_type,
                target_transfer_uid,
                position,
                created_at,
                updated_at
             FROM transfer_relation
             ORDER BY COALESCE(position, id), id",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_transfer_package_relations(
        &self,
        transfer_uid: &str,
    ) -> Result<Vec<TransferRelationPackage>, Error> {
        let rows = sqlx::query_as::<_, TransferRelationRow>(
            "SELECT
                id,
                transfer_uid,
                relation_type,
                target_transfer_uid,
                position,
                created_at,
                updated_at
             FROM transfer_relation
             WHERE transfer_uid = ? OR target_transfer_uid = ?
             ORDER BY COALESCE(position, id), id",
        )
        .bind(transfer_uid)
        .bind(transfer_uid)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(rows
            .into_iter()
            .map(TransferRelationPackage::from)
            .collect())
    }

    async fn load_transfer_tree_configs(&self) -> Result<Vec<TransferTreeConfigRow>, Error> {
        sqlx::query_as::<_, TransferTreeConfigRow>(
            "SELECT
                id,
                transfer_uid,
                branch_mode,
                record_sync_mode,
                reservation_policy,
                source_record_id,
                sync_role,
                sync_quantity,
                sync_counterparty_label,
                sync_target_organ_id,
                last_synced_record_head,
                sync_enabled,
                last_synced_at,
                created_at,
                updated_at
             FROM transfer_tree_config",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_transfer_tree_config(
        &self,
        transfer_uid: &str,
    ) -> Result<Option<TransferTreeConfigRow>, Error> {
        sqlx::query_as::<_, TransferTreeConfigRow>(
            "SELECT
                id,
                transfer_uid,
                branch_mode,
                record_sync_mode,
                reservation_policy,
                source_record_id,
                sync_role,
                sync_quantity,
                sync_counterparty_label,
                sync_target_organ_id,
                last_synced_record_head,
                sync_enabled,
                last_synced_at,
                created_at,
                updated_at
             FROM transfer_tree_config
             WHERE transfer_uid = ?
             LIMIT 1",
        )
        .bind(transfer_uid)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn build_transfer_package(&self, transfer_id: i64) -> Result<TransferPackage, Error> {
        let transfer = self.load_transfer_summary(transfer_id).await?;
        let events = self.load_transfer_events(transfer_id).await?;
        let work = self
            .load_work_metadata_package("transfer", transfer_id)
            .await?;
        let item_work = self.load_transfer_item_work_packages(transfer_id).await?;
        let interaction_work = self
            .load_transfer_interaction_work_packages(transfer_id)
            .await?;
        let structured = self.load_structured_transfer_package(transfer_id).await?;
        Ok(TransferPackage {
            version: PACKAGE_VERSION,
            identity: TransferIdentityPackage::from(&transfer),
            structured,
            work,
            item_work,
            interaction_work,
            relations: self
                .load_transfer_package_relations(&transfer.transfer_uid)
                .await?,
            tree_config: self
                .load_transfer_tree_config(&transfer.transfer_uid)
                .await?
                .map(TransferTreeConfigPackage::from),
            events: events.into_iter().map(TransferEventPackage::from).collect(),
        })
    }

    async fn load_transfer_packages_since(
        &self,
        since: Option<&str>,
    ) -> Result<Vec<TransferPackage>, Error> {
        let transfer_ids = if let Some(since) = since.filter(|value| !value.trim().is_empty()) {
            sqlx::query_scalar::<_, i64>(
                "SELECT DISTINCT t.id
                 FROM transfer t
                 JOIN transfer_identity ident ON ident.transfer_id = t.id
                 LEFT JOIN transfer_event event ON event.transfer_id = t.id
                 LEFT JOIN work_metadata transfer_work
                    ON transfer_work.owner_kind = 'transfer'
                   AND transfer_work.owner_id = t.id
                 LEFT JOIN transfer_structured_item structured_item
                    ON structured_item.transfer_id = t.id
                 LEFT JOIN work_metadata item_work
                    ON item_work.owner_kind = 'transfer_structured_item'
                   AND item_work.owner_id = structured_item.id
                 LEFT JOIN transfer_interaction interaction
                    ON interaction.transfer_id = t.id
                 LEFT JOIN work_metadata interaction_work
                    ON interaction_work.owner_kind = 'transfer_interaction'
                   AND interaction_work.owner_id = interaction.id
                 WHERE ident.updated_at >= ?
                    OR event.created_at >= ?
                    OR transfer_work.updated_at >= ?
                    OR item_work.updated_at >= ?
                    OR interaction_work.updated_at >= ?
                 ORDER BY t.id DESC
                 LIMIT 100",
            )
            .bind(since)
            .bind(since)
            .bind(since)
            .bind(since)
            .bind(since)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?
        } else {
            sqlx::query_scalar::<_, i64>(
                "SELECT t.id
                 FROM transfer t
                 JOIN transfer_identity ident ON ident.transfer_id = t.id
                 ORDER BY t.id DESC
                 LIMIT 100",
            )
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?
        };

        let mut packages = Vec::with_capacity(transfer_ids.len());
        for transfer_id in transfer_ids {
            packages.push(self.build_transfer_package(transfer_id).await?);
        }
        packages.extend(self.load_gossip_packages_since(since).await?);
        Ok(packages)
    }

    async fn load_gossip_packages_since(
        &self,
        since: Option<&str>,
    ) -> Result<Vec<TransferPackage>, Error> {
        let rows = if let Some(since) = since.filter(|value| !value.trim().is_empty()) {
            sqlx::query_as::<_, GossipPackageJsonRow>(
                "SELECT package_json
                 FROM transfer_gossip_package
                 WHERE updated_at >= ?
                 ORDER BY updated_at DESC, id DESC
                 LIMIT 100",
            )
            .bind(since)
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?
        } else {
            sqlx::query_as::<_, GossipPackageJsonRow>(
                "SELECT package_json
                 FROM transfer_gossip_package
                 ORDER BY updated_at DESC, id DESC
                 LIMIT 100",
            )
            .fetch_all(&*self.services.db)
            .await
            .map_err(Error::other)?
        };

        let mut packages = Vec::with_capacity(rows.len());
        for row in rows {
            let package =
                serde_json::from_str::<TransferPackage>(&row.package_json).map_err(Error::other)?;
            packages.push(package);
        }
        Ok(packages)
    }

    async fn load_gossip_views(&self) -> Result<Vec<GossipTransferView>, Error> {
        let rows = sqlx::query_as::<_, GossipPackageRow>(
            "SELECT
                id,
                transfer_uid,
                package_json,
                source_base_url,
                target_base_url,
                observed_from_base_url,
                event_count,
                latest_event_created_at,
                first_seen_at,
                updated_at,
                last_pulsed_at
             FROM transfer_gossip_package
             ORDER BY updated_at DESC, id DESC
             LIMIT 100",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;

        let mut views = Vec::with_capacity(rows.len());
        for row in rows {
            if let Ok(package) = serde_json::from_str::<TransferPackage>(&row.package_json) {
                views.push(GossipTransferView::from_row(row, package));
            }
        }
        Ok(views)
    }

    async fn store_gossip_package(
        &self,
        package: &TransferPackage,
        observed_from_base_url: Option<String>,
    ) -> Result<(), Error> {
        let package_json = serde_json::to_string(package).map_err(Error::other)?;
        if package_json.len() > MAX_GOSSIP_PACKAGE_BYTES || package.events.len() > MAX_GOSSIP_EVENTS
        {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Transfer gossip package too large",
            ));
        }
        let latest_event_created_at = package
            .events
            .iter()
            .map(|event| event.created_at.as_str())
            .max()
            .map(ToOwned::to_owned);
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_gossip_package(
                    transfer_uid,
                    package_json,
                    source_base_url,
                    target_base_url,
                    observed_from_base_url,
                    event_count,
                    latest_event_created_at
                 ) VALUES (?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(transfer_uid)
                 DO UPDATE SET
                    package_json = excluded.package_json,
                    source_base_url = COALESCE(excluded.source_base_url, transfer_gossip_package.source_base_url),
                    target_base_url = COALESCE(excluded.target_base_url, transfer_gossip_package.target_base_url),
                    observed_from_base_url = COALESCE(excluded.observed_from_base_url, transfer_gossip_package.observed_from_base_url),
                    event_count = MAX(transfer_gossip_package.event_count, excluded.event_count),
                    latest_event_created_at = COALESCE(excluded.latest_event_created_at, transfer_gossip_package.latest_event_created_at),
                    updated_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Text(package.identity.transfer_uid.clone()),
                    SqlParameter::Text(package_json),
                    optional_text_parameter(package.identity.source_base_url.clone()),
                    optional_text_parameter(package.identity.target_base_url.clone()),
                    optional_text_parameter(observed_from_base_url),
                    SqlParameter::Integer(package.events.len() as i64),
                    optional_text_parameter(latest_event_created_at),
                ],
            )
            .await?;
        self.prune_gossip_packages().await?;
        Ok(())
    }

    async fn prune_gossip_packages(&self) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "DELETE FROM transfer_gossip_package
                 WHERE id NOT IN (
                    SELECT id FROM transfer_gossip_package
                    ORDER BY updated_at DESC, id DESC
                    LIMIT ?
                 )"
                .to_string(),
                vec![SqlParameter::Integer(MAX_GOSSIP_PACKAGES)],
            )
            .await?;
        Ok(())
    }

    async fn pulse_transfer_mesh(&self) -> Result<(), TransferWidgetError> {
        let since = sync_since_with_lookback(&current_sql_timestamp());
        self.pull_recent_transfer_packages(&since)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.flush_transfer_sync_outbox()
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn poll_transfer_peer(
        &self,
        request: PollTransferPeerRequest,
    ) -> Result<String, TransferWidgetError> {
        let mut organ_id = None;
        let (target, since) = if let Some(id) = request.organ_id {
            let organ = self
                .organs
                .get(id)
                .await
                .map_err(TransferWidgetError::Invalid)?
                .ok_or_else(|| TransferWidgetError::Invalid("Organ not found.".into()))?;
            if organ.trust_state == "blocked" {
                return Err(TransferWidgetError::Invalid(
                    "Blocked Organs cannot be polled.".into(),
                ));
            }
            organ_id = Some(organ.id);
            let since = organ
                .last_transfer_polled_at
                .as_deref()
                .map(sync_since_with_lookback)
                .unwrap_or_else(|| sync_since_with_lookback(&current_sql_timestamp()));
            (organ.base_url, since)
        } else if let Some(base_url) = normalize_optional_text(request.base_url) {
            self.reject_blocked_base_url(&base_url).await?;
            let since = sync_since_with_lookback(&current_sql_timestamp());
            (base_url, since)
        } else {
            return Err(TransferWidgetError::Invalid(
                "Select an Organ or base URL to poll.".into(),
            ));
        };
        self.pull_recent_transfer_packages_from_targets(&since, vec![target.clone()])
            .await
            .map_err(TransferWidgetError::from_io)?;
        if let Some(id) = organ_id {
            self.organs
                .mark_transfer_polled(id)
                .await
                .map_err(TransferWidgetError::Invalid)?;
        }
        Ok(target)
    }

    async fn enqueue_transfer_sync(&self, transfer_id: i64) -> Result<(), Error> {
        let transfer = self.load_transfer_summary(transfer_id).await?;
        let mut targets = Vec::new();
        if let Some(base_url) = normalize_optional_text(transfer.target_base_url) {
            targets.push(base_url);
        }
        if let Some(base_url) = normalize_optional_text(transfer.source_base_url) {
            targets.push(base_url);
        }
        targets.sort();
        targets.dedup();

        for target in targets {
            if same_base_url(&target, &self.local_base_url) {
                continue;
            }
            if self
                .organs
                .find_by_base_url(&target)
                .await
                .map_err(Error::other)?
                .is_some_and(|organ| organ.trust_state == "blocked")
            {
                continue;
            }
            self.services
                .writer
                .execute_statement(
                    "INSERT INTO transfer_sync_outbox(transfer_id, target_base_url)
                     VALUES (?, ?)
                     ON CONFLICT(transfer_id, target_base_url)
                     DO UPDATE SET updated_at = CURRENT_TIMESTAMP"
                        .to_string(),
                    vec![
                        SqlParameter::Integer(transfer_id),
                        SqlParameter::Text(target),
                    ],
                )
                .await?;
        }
        Ok(())
    }

    async fn flush_transfer_sync_outbox(&self) -> Result<(), Error> {
        let rows = sqlx::query_as::<_, TransferSyncOutboxRow>(
            "SELECT outbox.id, outbox.transfer_id, outbox.target_base_url, outbox.attempts, outbox.last_error, outbox.last_attempt_at
             FROM transfer_sync_outbox outbox
             LEFT JOIN organ ON organ.base_url = outbox.target_base_url
             ORDER BY COALESCE(organ.proximity, 100), outbox.updated_at, outbox.id
             LIMIT 25",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;

        for row in rows {
            if self
                .organs
                .find_by_base_url(&row.target_base_url)
                .await
                .map_err(Error::other)?
                .is_some_and(|organ| organ.trust_state == "blocked")
            {
                self.delete_outbox_row(row.id).await?;
                continue;
            }
            let package_value = match self.build_transfer_package(row.transfer_id).await {
                Ok(package) => serde_json::to_value(package).map_err(Error::other)?,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    self.delete_outbox_row(row.id).await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            match self
                .manas
                .send_public_backend_request(
                    &row.target_base_url,
                    Method::POST,
                    "/transfer/packages",
                    Some(package_value),
                )
                .await
            {
                Ok(response) if response.status().is_success() => {
                    self.delete_outbox_row(row.id).await?;
                }
                Ok(response) => {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    self.mark_outbox_attempt(
                        row.id,
                        format!("Remote rejected sync with {status}: {body}"),
                    )
                    .await?;
                }
                Err(error) => {
                    self.mark_outbox_attempt(row.id, error).await?;
                }
            }
        }
        Ok(())
    }

    async fn delete_outbox_row(&self, id: i64) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "DELETE FROM transfer_sync_outbox WHERE id = ?".to_string(),
                vec![SqlParameter::Integer(id)],
            )
            .await?;
        Ok(())
    }

    async fn mark_outbox_attempt(&self, id: i64, error: String) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_sync_outbox
                 SET attempts = attempts + 1,
                     last_error = ?,
                     last_attempt_at = CURRENT_TIMESTAMP,
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?"
                    .to_string(),
                vec![SqlParameter::Text(error), SqlParameter::Integer(id)],
            )
            .await?;
        Ok(())
    }

    async fn delete_transfer(&self, transfer_id: i64) -> Result<(), Error> {
        let transfer_uid = self
            .load_transfer_summary(transfer_id)
            .await
            .ok()
            .map(|transfer| transfer.transfer_uid);
        let transfer_outcome = self
            .services
            .writer
            .execute_statement(
                "DELETE FROM transfer WHERE id = ?".to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        if transfer_outcome.rows_affected == 0 {
            return Err(Error::new(ErrorKind::NotFound, "Transfer not found"));
        }
        if let Some(transfer_uid) = transfer_uid {
            self.services
                .writer
                .execute_statement(
                    "DELETE FROM transfer_relation
                     WHERE transfer_uid = ? OR target_transfer_uid = ?"
                        .to_string(),
                    vec![
                        SqlParameter::Text(transfer_uid.clone()),
                        SqlParameter::Text(transfer_uid.clone()),
                    ],
                )
                .await?;
            self.services
                .writer
                .execute_statement(
                    "DELETE FROM transfer_tree_config WHERE transfer_uid = ?".to_string(),
                    vec![SqlParameter::Text(transfer_uid)],
                )
                .await?;
        }
        Ok(())
    }

    async fn sync_on_startup(&self) {
        let previous_cache = self.read_sync_cache().unwrap_or_default();
        if let Err(error) = self.write_sync_cache_now() {
            tracing::warn!("transfer sync cache write failed on startup: {error}");
        }

        let since = previous_cache
            .last_online_at
            .as_deref()
            .map(sync_since_with_lookback);
        if let Some(since) = since.as_deref() {
            if let Err(error) = self.pull_recent_transfer_packages(since).await {
                tracing::warn!("transfer startup pull failed: {error}");
            }
        }
        if let Err(error) = self.announce_online_to_known_peers().await {
            tracing::warn!("transfer startup online announcement failed: {error}");
        }
        if let Err(error) = self.flush_transfer_sync_outbox().await {
            tracing::warn!("transfer startup outbox flush failed: {error}");
        }
    }

    async fn pull_recent_transfer_packages(&self, since: &str) -> Result<(), Error> {
        let targets = self
            .known_transfer_sync_targets()
            .await?
            .into_iter()
            .map(|organ| organ.base_url)
            .collect::<Vec<_>>();
        self.pull_recent_transfer_packages_from_targets(since, targets)
            .await
    }

    async fn pull_recent_transfer_packages_from_targets(
        &self,
        since: &str,
        targets: Vec<String>,
    ) -> Result<(), Error> {
        let path = format!(
            "/transfer/packages/since?since={}&requesterBaseUrl={}",
            encode_query_value(since),
            encode_query_value(&self.local_base_url)
        );
        for target in targets {
            let response = match self
                .manas
                .send_public_backend_request(&target, Method::GET, &path, None)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    tracing::warn!("transfer startup pull skipped {target}: {error}");
                    continue;
                }
            };
            if !response.status().is_success() {
                tracing::warn!(
                    "transfer startup pull rejected by {target}: {}",
                    response.status()
                );
                continue;
            }
            let value = response.json::<Value>().await.map_err(Error::other)?;
            let packages = value
                .get("packages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for value in packages {
                let package = match parse_transfer_package_value(value) {
                    Ok(package) => package,
                    Err(error) => {
                        tracing::warn!(
                            "transfer startup package parse failed from {target}: {error:?}"
                        );
                        continue;
                    }
                };
                if let Err(error) = validate_package(&package) {
                    tracing::warn!(
                        "transfer startup package validation failed from {target}: {error:?}"
                    );
                    continue;
                }
                let changed = if self
                    .find_transfer_id_by_uid(&package.identity.transfer_uid)
                    .await?
                    .is_some()
                    || self.package_is_addressed_to_local_node(&package)
                {
                    match self.receive_transfer_package(package).await {
                        Ok(outcome) => outcome.events_imported > 0,
                        Err(error) => {
                            tracing::warn!(
                                "transfer startup package import failed from {target}: {error:?}"
                            );
                            false
                        }
                    }
                } else {
                    match self
                        .store_gossip_package(&package, Some(target.clone()))
                        .await
                    {
                        Ok(()) => true,
                        Err(error) => {
                            tracing::warn!(
                                "transfer startup package import failed from {target}: {error:?}"
                            );
                            false
                        }
                    }
                };
                if changed {
                    self.notify_changed("mesh_pull");
                }
            }
        }
        Ok(())
    }

    async fn known_transfer_sync_targets(&self) -> Result<Vec<Organ>, Error> {
        let mut targets = self
            .organs
            .known_transfer_poll_targets()
            .await
            .map_err(Error::other)?;
        targets.retain(|organ| {
            !same_base_url(&organ.base_url, &self.local_base_url)
                && !is_default_local_organ(organ.id)
                && organ.trust_state == "known"
        });
        Ok(targets)
    }

    async fn poll_due_known_transfer_peers(&self) -> Result<(), Error> {
        if !self.transfer_known_peer_polling_enabled().await? {
            return Ok(());
        }
        let targets = self.known_transfer_sync_targets().await?;
        for target in targets {
            if !transfer_poll_due(target.last_transfer_polled_at.as_deref()) {
                continue;
            }
            let since = target
                .last_transfer_polled_at
                .as_deref()
                .map(sync_since_with_lookback)
                .unwrap_or_else(|| sync_since_with_lookback(&current_sql_timestamp()));
            self.pull_recent_transfer_packages_from_targets(&since, vec![target.base_url.clone()])
                .await?;
            self.organs
                .mark_transfer_polled(target.id)
                .await
                .map_err(Error::other)?;
        }
        Ok(())
    }

    async fn announce_online_to_known_peers(&self) -> Result<(), Error> {
        let local = self.organs.get(1).await.map_err(Error::other)?;
        let local_name = local
            .as_ref()
            .map(|organ| organ.name.clone())
            .unwrap_or_else(|| "Local Lince".to_string());
        let payload = json!({
            "name": local_name,
            "baseUrl": self.local_base_url,
        });
        for target in self.known_transfer_sync_targets().await? {
            match self
                .manas
                .send_public_backend_request(
                    &target.base_url,
                    Method::POST,
                    "/transfer/peers/online",
                    Some(payload.clone()),
                )
                .await
            {
                Ok(response) if response.status().is_success() => {
                    self.organs
                        .mark_seen_by_base_url(&target.base_url)
                        .await
                        .map_err(Error::other)?;
                }
                Ok(response) => {
                    tracing::warn!(
                        "transfer online announcement rejected by {}: {}",
                        target.base_url,
                        response.status()
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        "transfer online announcement skipped {}: {error}",
                        target.base_url
                    );
                }
            }
        }
        Ok(())
    }

    fn read_sync_cache(&self) -> Result<TransferSyncCache, Error> {
        let path = transfer_sync_cache_path()?;
        if !path.exists() {
            return Ok(TransferSyncCache::default());
        }
        let raw = fs::read_to_string(path)?;
        serde_json::from_str(&raw).map_err(Error::other)
    }

    fn write_sync_cache_now(&self) -> Result<(), Error> {
        let path = transfer_sync_cache_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let cache = TransferSyncCache {
            last_online_at: Some(current_sql_timestamp()),
            local_base_url: Some(self.local_base_url.clone()),
        };
        let raw = serde_json::to_string_pretty(&cache).map_err(Error::other)?;
        fs::write(path, raw)?;
        Ok(())
    }

    async fn append_event_once(
        &self,
        identity: &IdentityOnlyRow,
        local_identity: &LocalIdentityRow,
        kind: EventKind,
        payload: Value,
    ) -> Result<(), TransferWidgetError> {
        if self
            .event_exists(identity.transfer_id, kind)
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            return Ok(());
        }
        self.append_signed_event(identity, local_identity, kind, payload)
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(())
    }

    async fn append_signed_event(
        &self,
        identity: &IdentityOnlyRow,
        local_identity: &LocalIdentityRow,
        kind: EventKind,
        payload: Value,
    ) -> Result<i64, Error> {
        let signing_key = signing_key_from_base64(&local_identity.secret_key)?;
        let event_uid = Uuid::new_v4().to_string();
        let previous = self.latest_event_pointer(identity.transfer_id).await?;
        let payload_json = serde_json::to_string(&payload)
            .map_err(|error| Error::new(ErrorKind::InvalidData, error.to_string()))?;
        let message = event_signing_message(
            &event_uid,
            &identity.transfer_uid,
            &local_identity.label,
            &local_identity.public_key,
            kind.as_str(),
            previous.event_uid.as_deref(),
            &payload_json,
        );
        let signature = BASE64.encode(signing_key.sign(message.as_bytes()).to_bytes());
        let previous_event_hash = previous.event_hash.clone();
        let event_hash = transfer_event_hash(
            &event_uid,
            &identity.transfer_uid,
            &local_identity.label,
            &local_identity.public_key,
            kind.as_str(),
            previous.event_uid.as_deref(),
            previous_event_hash.as_deref(),
            &payload_json,
            &signature,
        );

        let outcome = self
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_event(
                    transfer_id,
                    transfer_uid,
                    event_uid,
                    actor_label,
                    actor_public_key,
                    event_kind,
                    payload_json,
                    previous_event_id,
                    previous_event_uid,
                    previous_event_hash,
                    event_hash,
                    signature,
                    validation_state
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(identity.transfer_id),
                    SqlParameter::Text(identity.transfer_uid.clone()),
                    SqlParameter::Text(event_uid.clone()),
                    SqlParameter::Text(local_identity.label.clone()),
                    SqlParameter::Text(local_identity.public_key.clone()),
                    SqlParameter::Text(kind.as_str().to_string()),
                    SqlParameter::Text(payload_json),
                    previous
                        .id
                        .map(SqlParameter::Integer)
                        .unwrap_or(SqlParameter::Null),
                    optional_text_parameter(previous.event_uid),
                    optional_text_parameter(previous_event_hash),
                    SqlParameter::Text(event_hash),
                    SqlParameter::Text(signature),
                    SqlParameter::Text("valid".to_string()),
                ],
            )
            .await?;
        let event_id = outcome
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Transfer event returned no id"))?;
        self.update_sync_cursor(identity.transfer_id, &local_identity.label, event_id)
            .await?;
        self.enqueue_transfer_sync(identity.transfer_id).await?;
        Ok(event_id)
    }

    async fn insert_packaged_event(
        &self,
        transfer_id: i64,
        event: &TransferEventPackage,
    ) -> Result<i64, Error> {
        let previous = if let Some(previous_uid) = event.previous_event_uid.as_deref() {
            sqlx::query_as::<_, EventPointer>(
                "SELECT id, event_uid, event_hash FROM transfer_event WHERE event_uid = ? LIMIT 1",
            )
            .bind(previous_uid)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?
        } else {
            None
        };
        let validation = validate_packaged_event(event, previous.as_ref());
        let outcome = self
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_event(
                    transfer_id,
                    transfer_uid,
                    event_uid,
                    actor_label,
                    actor_public_key,
                    event_kind,
                    payload_json,
                    previous_event_id,
                    previous_event_uid,
                    previous_event_hash,
                    event_hash,
                    signature,
                    validation_state,
                    validation_error,
                    created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(event.transfer_uid.clone()),
                    SqlParameter::Text(event.event_uid.clone()),
                    SqlParameter::Text(event.actor_label.clone()),
                    SqlParameter::Text(event.actor_public_key.clone()),
                    SqlParameter::Text(event.event_kind.clone()),
                    SqlParameter::Text(event.payload_json.clone()),
                    previous
                        .as_ref()
                        .and_then(|value| value.id)
                        .map(SqlParameter::Integer)
                        .unwrap_or(SqlParameter::Null),
                    optional_text_parameter(event.previous_event_uid.clone()),
                    optional_text_parameter(event.previous_event_hash.clone()),
                    optional_text_parameter(event.event_hash.clone()),
                    SqlParameter::Text(event.signature.clone()),
                    SqlParameter::Text(validation.state),
                    optional_text_parameter(validation.error),
                    SqlParameter::Text(event.created_at.clone()),
                ],
            )
            .await?;
        let event_id = outcome
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Transfer event returned no id"))?;
        self.update_sync_cursor(transfer_id, &event.actor_label, event_id)
            .await?;
        Ok(event_id)
    }

    async fn latest_event_pointer(&self, transfer_id: i64) -> Result<EventPointer, Error> {
        sqlx::query_as::<_, EventPointer>(
            "SELECT id, event_uid, event_hash
             FROM transfer_event
             WHERE transfer_id = ?
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(transfer_id)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)
        .map(|value| value.unwrap_or_default())
    }

    async fn update_sync_cursor(
        &self,
        transfer_id: i64,
        peer_label: &str,
        event_id: i64,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_sync_cursor(transfer_id, peer_label, last_event_id)
                 VALUES (?, ?, ?)
                 ON CONFLICT(transfer_id, peer_label)
                 DO UPDATE SET
                    last_event_id = excluded.last_event_id,
                    last_synced_at = CURRENT_TIMESTAMP"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(peer_label.to_string()),
                    SqlParameter::Integer(event_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn event_exists(&self, transfer_id: i64, kind: EventKind) -> Result<bool, Error> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM transfer_event WHERE transfer_id = ? AND event_kind = ?",
        )
        .bind(transfer_id)
        .bind(kind.as_str())
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(count > 0)
    }

    async fn event_uid_exists(&self, event_uid: &str) -> Result<bool, Error> {
        let count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM transfer_event WHERE event_uid = ?")
                .bind(event_uid)
                .fetch_one(&*self.services.db)
                .await
                .map_err(Error::other)?;
        Ok(count > 0)
    }

    async fn find_transfer_id_by_uid(&self, transfer_uid: &str) -> Result<Option<i64>, Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT transfer_id FROM transfer_identity WHERE transfer_uid = ? LIMIT 1",
        )
        .bind(transfer_uid)
        .fetch_optional(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn local_settlement_exists(
        &self,
        transfer_id: i64,
        local_actor_label: &str,
    ) -> Result<bool, Error> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_local_settlement
             WHERE transfer_id = ? AND local_actor_label = ?",
        )
        .bind(transfer_id)
        .bind(local_actor_label)
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        Ok(count > 0)
    }

    async fn load_transfer_events(&self, transfer_id: i64) -> Result<Vec<EventRow>, Error> {
        sqlx::query_as::<_, EventRow>(
            "SELECT
                id,
                transfer_uid,
                event_uid,
                actor_label,
                actor_public_key,
                event_kind,
                payload_json,
                previous_event_id,
                previous_event_uid,
                previous_event_hash,
                event_hash,
                signature,
                validation_state,
                validation_error,
                created_at
             FROM transfer_event
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_transfer_cursors(&self, transfer_id: i64) -> Result<Vec<CursorRow>, Error> {
        sqlx::query_as::<_, CursorRow>(
            "SELECT peer_label, last_event_id, last_synced_at
             FROM transfer_sync_cursor
             WHERE transfer_id = ?
             ORDER BY peer_label",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }

    async fn load_transfer_settlements(
        &self,
        transfer_id: i64,
    ) -> Result<Vec<SettlementRow>, Error> {
        sqlx::query_as::<_, SettlementRow>(
            "SELECT
                local_actor_label,
                local_record_id,
                local_quantity_delta,
                event_id,
                settled_at
             FROM transfer_local_settlement
             WHERE transfer_id = ?
             ORDER BY id",
        )
        .bind(transfer_id)
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransferSide {
    Contribution,
    Need,
}

impl TransferSide {
    fn parse_storage(value: Option<&str>) -> Option<Self> {
        match value {
            Some("contribution") => Some(Self::Contribution),
            Some("need") => Some(Self::Need),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Contribution => "contribution",
            Self::Need => "need",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum TransferState {
    PublicProposal,
    Negotiation,
    Inactive,
}

impl TransferState {
    fn as_str(self) -> &'static str {
        match self {
            Self::PublicProposal => "public_proposal",
            Self::Negotiation => "negotiation",
            Self::Inactive => "inactive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferRelationType {
    Parent,
}

impl TransferRelationType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Parent => "parent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferBranchMode {
    Inherit,
    Duplicated,
    Greedy,
}

impl TransferBranchMode {
    fn parse(value: &str) -> Result<Self, TransferWidgetError> {
        Self::parse_storage(value)
            .ok_or_else(|| TransferWidgetError::Invalid("Unknown Transfer branch mode.".into()))
    }

    fn parse_storage(value: &str) -> Option<Self> {
        match value {
            "inherit" => Some(Self::Inherit),
            "duplicated" => Some(Self::Duplicated),
            "greedy" => Some(Self::Greedy),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Inherit => "inherit",
            Self::Duplicated => "duplicated",
            Self::Greedy => "greedy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferReservationPolicy {
    None,
    Soft,
    HardOnProposal,
    HardOnConsume,
    HardOnLock,
}

impl TransferReservationPolicy {
    fn parse(value: &str) -> Result<Self, TransferWidgetError> {
        Self::parse_storage(value).ok_or_else(|| {
            TransferWidgetError::Invalid("Unknown Transfer reservation policy.".into())
        })
    }

    fn parse_storage(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "soft" => Some(Self::Soft),
            "hard_on_proposal" => Some(Self::HardOnProposal),
            "hard_on_consume" => Some(Self::HardOnConsume),
            "hard_on_lock" => Some(Self::HardOnLock),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Soft => "soft",
            Self::HardOnProposal => "hard_on_proposal",
            Self::HardOnConsume => "hard_on_consume",
            Self::HardOnLock => "hard_on_lock",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReservationRefreshTrigger {
    ProposalCreated,
    ProposalConsumed,
    AgreementLocked,
    PolicyChanged,
    Released,
    Settled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransferRecordSyncMode {
    None,
    CopyOnce,
    Live,
}

impl TransferRecordSyncMode {
    fn parse(value: &str) -> Result<Self, TransferWidgetError> {
        Self::parse_storage(value)
            .ok_or_else(|| TransferWidgetError::Invalid("Unknown Transfer tree sync mode.".into()))
    }

    fn parse_storage(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "copy_once" => Some(Self::CopyOnce),
            "live" => Some(Self::Live),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CopyOnce => "copy_once",
            Self::Live => "live",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EventKind {
    ProposalCreated,
    ProposalDuplicated,
    ItemCreated,
    ItemEdited,
    AgreementSigned,
    DeliveryConfirmed,
    ReceiptConfirmed,
    PackageReceived,
    PackageSeen,
    SettlementApplied,
    TransferInactivated,
    VisibilityChanged,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProposalCreated | Self::ProposalDuplicated => "transfer_created",
            Self::ItemCreated => "item_created",
            Self::ItemEdited => "item_edited",
            Self::AgreementSigned => "agreement_changed",
            Self::DeliveryConfirmed => "delivery_confirmed",
            Self::ReceiptConfirmed => "receipt_confirmed",
            Self::PackageReceived => "package_received",
            Self::PackageSeen => "package_seen",
            Self::SettlementApplied => "settlement_applied",
            Self::TransferInactivated => "transfer_inactivated",
            Self::VisibilityChanged => "visibility_changed",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigureLocalPartyRequest {
    label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateRecordRequest {
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProposalRequest {
    title: String,
    role: TransferSide,
    record_id: i64,
    quantity: f64,
    counterparty_label: String,
    target_organ_id: Option<i64>,
    parent_transfer_id: Option<i64>,
    topic_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateChildTransferRequest {
    parent_transfer_id: i64,
    title: String,
    role: TransferSide,
    record_id: i64,
    quantity: f64,
    counterparty_label: String,
    target_organ_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTransferTreeFromRecordRequest {
    parent_transfer_id: i64,
    root_record_id: i64,
    role: TransferSide,
    quantity: f64,
    counterparty_label: String,
    target_organ_id: Option<i64>,
    record_sync_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTransferBranchModeRequest {
    transfer_id: i64,
    branch_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTransferTreeSyncModeRequest {
    transfer_id: i64,
    record_sync_mode: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTransferReservationPolicyRequest {
    transfer_id: i64,
    reservation_policy: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateProposalRequest {
    transfer_id: i64,
    local_role: TransferSide,
    local_record_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransferLocalItemRequest {
    transfer_id: i64,
    title: String,
    item_title: String,
    record_id: i64,
    quantity: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransferWorkRequest {
    transfer_id: i64,
    work: WorkMetadataInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransferItemWorkRequest {
    transfer_id: i64,
    structured_item_id: i64,
    work: WorkMetadataInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransferInteractionWorkRequest {
    transfer_id: i64,
    interaction_id: i64,
    work: WorkMetadataInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkMetadataInput {
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    completion_notes: Option<String>,
    #[serde(default)]
    assignees: Vec<WorkAssigneeInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkAssigneeInput {
    kind: String,
    app_user_id: Option<i64>,
    display_name: Option<String>,
    organ_name: Option<String>,
    remote_base_url: Option<String>,
    remote_public_key: Option<String>,
    remote_subject_uid: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferIdRequest {
    transfer_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostTransferRequest {
    transfer_id: i64,
    organ_id: Option<i64>,
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PollTransferPeerRequest {
    organ_id: Option<i64>,
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportPackageRequest {
    package: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetIngressPolicyRequest {
    public_proposals_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetNetworkPolicyRequest {
    known_peer_polling_enabled: bool,
    #[serde(default)]
    share_quantity_projections: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetReceiptPolicyRequest {
    send_received_receipts: bool,
    send_seen_receipts: bool,
    anonymous_package_viewing: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetOrganProximityRequest {
    organ_id: i64,
    proximity: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetOrganReceiptPolicyRequest {
    organ_id: i64,
    send_received_receipts: bool,
    send_seen_receipts: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetQuantityProjectionSharingRequest {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTransferVisibilityRequest {
    transfer_id: i64,
    visibility_mode: String,
    max_visible_proximity: Option<i64>,
    #[serde(default)]
    organ_ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyVisibilityWaveRequest {
    transfer_id: i64,
    max_visible_proximity: Option<i64>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganOption {
    id: i64,
    name: String,
    base_url: String,
    trust_state: String,
    contact_discovery_enabled: bool,
    last_seen_at: Option<String>,
    last_transfer_polled_at: Option<String>,
    proximity: i64,
    transfer_send_received_receipts: bool,
    transfer_send_seen_receipts: bool,
    requires_auth: bool,
    authenticated: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct AppUserOption {
    id: i64,
    name: String,
    username: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct RecordView {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct LocalIdentityRow {
    id: i64,
    label: String,
    public_key: String,
    secret_key: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalIdentityView {
    id: i64,
    label: String,
    public_key: String,
    created_at: String,
    updated_at: String,
}

impl From<LocalIdentityRow> for LocalIdentityView {
    fn from(row: LocalIdentityRow) -> Self {
        Self {
            id: row.id,
            label: row.label,
            public_key: row.public_key,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferSideInput {
    actor_label: String,
    public_key: Option<String>,
    record_id: i64,
    head: String,
    quantity: f64,
}

#[derive(Debug, Clone)]
struct TransferIdentityInput {
    transfer_id: i64,
    transfer_uid: String,
    parent_transfer_uid: Option<String>,
    source_transfer_uid: Option<String>,
    state: String,
    title: String,
    coordinator_label: String,
    proposer_label: String,
    counterparty_label: String,
    contribution_actor_label: String,
    contribution_public_key: Option<String>,
    need_actor_label: String,
    need_public_key: Option<String>,
    target_organ: Option<Organ>,
    target_base_url: Option<String>,
    source_base_url: Option<String>,
    topic_text: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct IdentityOnlyRow {
    transfer_id: i64,
    transfer_uid: String,
}

#[derive(Debug, Clone, FromRow)]
struct TransferSummaryRow {
    id: i64,
    quantity: f64,
    transfer_uid: String,
    parent_transfer_uid: Option<String>,
    source_transfer_uid: Option<String>,
    state: String,
    title: String,
    coordinator_label: String,
    proposer_label: String,
    counterparty_label: String,
    contribution_actor_label: String,
    contribution_public_key: Option<String>,
    need_actor_label: String,
    need_public_key: Option<String>,
    target_organ_id: Option<i64>,
    target_organ_name: Option<String>,
    target_base_url: Option<String>,
    source_base_url: Option<String>,
    topic_text: Option<String>,
    created_at: String,
    updated_at: String,
    contribution_id: i64,
    contribution_head: String,
    contribution_quantity: f64,
    need_id: i64,
    need_head: String,
    need_quantity: f64,
    first_agreement: i64,
    second_agreement: i64,
}

#[derive(Debug, Clone, FromRow)]
struct TransferVisibilityPolicyRow {
    visibility_mode: String,
    max_visible_proximity: Option<i64>,
}

#[derive(Debug, Clone, FromRow)]
struct TransferReceiptPolicyRow {
    send_received_receipts: i64,
    send_seen_receipts: i64,
    anonymous_package_viewing: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferReceiptPolicyView {
    send_received_receipts: bool,
    send_seen_receipts: bool,
    anonymous_package_viewing: bool,
}

impl From<TransferReceiptPolicyRow> for TransferReceiptPolicyView {
    fn from(row: TransferReceiptPolicyRow) -> Self {
        Self {
            send_received_receipts: row.send_received_receipts != 0,
            send_seen_receipts: row.send_seen_receipts != 0,
            anonymous_package_viewing: row.anonymous_package_viewing != 0,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TransferReceiptSummaryRow {
    received_at: Option<String>,
    seen_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TransferReceiptEventRow {
    actor_label: String,
    event_kind: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferReceiptEventView {
    actor_label: String,
    event_kind: String,
    created_at: String,
}

impl From<TransferReceiptEventRow> for TransferReceiptEventView {
    fn from(row: TransferReceiptEventRow) -> Self {
        Self {
            actor_label: row.actor_label,
            event_kind: row.event_kind,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferReceiptView {
    received_at: Option<String>,
    seen_at: Option<String>,
    events: Vec<TransferReceiptEventView>,
}

impl TransferReceiptView {
    fn from_row(row: TransferReceiptSummaryRow, events: Vec<TransferReceiptEventView>) -> Self {
        Self {
            received_at: row.received_at,
            seen_at: row.seen_at,
            events,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferVisibilityView {
    visibility_mode: String,
    max_visible_proximity: Option<i64>,
    organ_ids: Vec<i64>,
}

impl TransferSummaryRow {
    fn identity_row(&self) -> IdentityOnlyRow {
        IdentityOnlyRow {
            transfer_id: self.id,
            transfer_uid: self.transfer_uid.clone(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct EventRow {
    id: i64,
    transfer_uid: Option<String>,
    event_uid: Option<String>,
    actor_label: String,
    actor_public_key: Option<String>,
    event_kind: String,
    payload_json: String,
    previous_event_id: Option<i64>,
    previous_event_uid: Option<String>,
    previous_event_hash: Option<String>,
    event_hash: Option<String>,
    signature: Option<String>,
    validation_state: String,
    validation_error: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Default, FromRow)]
struct EventPointer {
    id: Option<i64>,
    event_uid: Option<String>,
    event_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct CursorRow {
    peer_label: String,
    last_event_id: Option<i64>,
    last_synced_at: String,
}

#[derive(Debug, Clone, FromRow)]
struct TransferSyncOutboxRow {
    id: i64,
    transfer_id: i64,
    target_base_url: String,
}

#[derive(Debug, Clone, FromRow)]
struct GossipPackageJsonRow {
    package_json: String,
}

#[derive(Debug, Clone, FromRow)]
struct GossipPackageRow {
    id: i64,
    transfer_uid: String,
    package_json: String,
    source_base_url: Option<String>,
    target_base_url: Option<String>,
    observed_from_base_url: Option<String>,
    event_count: i64,
    latest_event_created_at: Option<String>,
    first_seen_at: String,
    updated_at: String,
    last_pulsed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GossipTransferView {
    id: i64,
    transfer_uid: String,
    title: String,
    state: String,
    proposer_label: String,
    counterparty_label: String,
    source_base_url: Option<String>,
    target_base_url: Option<String>,
    observed_from_base_url: Option<String>,
    event_count: i64,
    latest_event_created_at: Option<String>,
    first_seen_at: String,
    updated_at: String,
    last_pulsed_at: Option<String>,
    contribution: TransferSideView,
    need: TransferSideView,
    package: TransferPackage,
}

impl GossipTransferView {
    fn from_row(row: GossipPackageRow, package: TransferPackage) -> Self {
        let identity = &package.identity;
        Self {
            id: row.id,
            transfer_uid: row.transfer_uid,
            title: identity.title.clone(),
            state: identity.state.clone(),
            proposer_label: identity.proposer_label.clone(),
            counterparty_label: identity.counterparty_label.clone(),
            source_base_url: row.source_base_url,
            target_base_url: row.target_base_url,
            observed_from_base_url: row.observed_from_base_url,
            event_count: row.event_count,
            latest_event_created_at: row
                .latest_event_created_at
                .map(|value| sql_to_iso8601(&value)),
            first_seen_at: sql_to_iso8601(&row.first_seen_at),
            updated_at: sql_to_iso8601(&row.updated_at),
            last_pulsed_at: row.last_pulsed_at.map(|value| sql_to_iso8601(&value)),
            contribution: package_side_view(
                &package,
                TransferSide::Contribution,
                identity.contribution_actor_label.clone(),
                identity.contribution_public_key.clone(),
            ),
            need: package_side_view(
                &package,
                TransferSide::Need,
                identity.need_actor_label.clone(),
                identity.need_public_key.clone(),
            ),
            package,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferSyncCache {
    last_online_at: Option<String>,
    local_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct SettlementRow {
    local_actor_label: String,
    local_record_id: i64,
    local_quantity_delta: f64,
    event_id: Option<i64>,
    settled_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct TransferRelationRow {
    id: i64,
    transfer_uid: String,
    relation_type: String,
    target_transfer_uid: String,
    position: Option<f64>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct TransferTreeConfigRow {
    id: i64,
    transfer_uid: String,
    branch_mode: String,
    record_sync_mode: String,
    reservation_policy: Option<String>,
    source_record_id: Option<i64>,
    sync_role: Option<String>,
    sync_quantity: Option<f64>,
    sync_counterparty_label: Option<String>,
    sync_target_organ_id: Option<i64>,
    last_synced_record_head: Option<String>,
    sync_enabled: i64,
    last_synced_at: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferTreeView {
    parent_uid: Option<String>,
    parent_id: Option<i64>,
    child_uids: Vec<String>,
    child_ids: Vec<i64>,
    relations: Vec<TransferRelationRow>,
    config: Option<TransferTreeConfigRow>,
    effective_branch_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferView {
    id: i64,
    quantity: f64,
    transfer_uid: String,
    parent_transfer_uid: Option<String>,
    source_transfer_uid: Option<String>,
    state: String,
    status: String,
    title: String,
    coordinator_label: String,
    proposer_label: String,
    counterparty_label: String,
    local_role: Option<String>,
    target_organ_id: Option<i64>,
    target_organ_name: Option<String>,
    target_base_url: Option<String>,
    source_base_url: Option<String>,
    contribution: TransferSideView,
    need: TransferSideView,
    agreement: AgreementView,
    confirmations: ConfirmationView,
    tree: TransferTreeView,
    controls: ControlView,
    events: Vec<EventView>,
    sync_cursors: Vec<CursorRow>,
    settlements: Vec<SettlementRow>,
    work: WorkMetadataView,
    receipt: TransferReceiptView,
    visibility: TransferVisibilityView,
    items: Vec<TransferItemWorkView>,
    interactions: Vec<TransferInteractionWorkView>,
    package: TransferPackage,
    created_at: String,
    updated_at: String,
}

impl TransferView {
    fn from_rows(
        transfer: TransferSummaryRow,
        events: Vec<EventRow>,
        cursors: Vec<CursorRow>,
        settlements: Vec<SettlementRow>,
        work: WorkMetadataView,
        receipt: TransferReceiptView,
        visibility: TransferVisibilityView,
        items: Vec<TransferItemWorkView>,
        interactions: Vec<TransferInteractionWorkView>,
        relations: &[TransferRelationRow],
        configs: &[TransferTreeConfigRow],
        transfer_lookup: &std::collections::BTreeMap<String, i64>,
        local_identity: Option<&LocalIdentityRow>,
    ) -> Self {
        let local_role = local_role_for(&transfer, local_identity);
        let reset_after_index = events
            .iter()
            .rposition(|event| event.event_kind == EventKind::TransferInactivated.as_str())
            .map(|index| index + 1)
            .unwrap_or(0);
        let event_kinds = events[reset_after_index..]
            .iter()
            .map(|event| event.event_kind.as_str())
            .collect::<Vec<_>>();
        let delivery_confirmed = event_kinds.contains(&EventKind::DeliveryConfirmed.as_str());
        let receipt_confirmed = event_kinds.contains(&EventKind::ReceiptConfirmed.as_str());
        let complete_agreement = agreements_complete(&transfer);
        let local_actor_label = local_identity.map(|identity| identity.label.as_str());
        let local_settled = local_actor_label.is_some_and(|label| {
            settlements
                .iter()
                .any(|settlement| settlement.local_actor_label == label)
        });
        let inactive = transfer.state == TransferState::Inactive.as_str();
        let status = if inactive {
            "inactive"
        } else if local_settled {
            "local_settled"
        } else if delivery_confirmed && receipt_confirmed && complete_agreement {
            "ready_to_settle"
        } else if complete_agreement {
            "agreed"
        } else if transfer.state == TransferState::PublicProposal.as_str() {
            "public_proposal"
        } else {
            "negotiation"
        }
        .to_string();
        let contribution_signed = transfer.contribution_public_key.is_some();
        let need_signed = transfer.need_public_key.is_some();
        let can_duplicate =
            local_identity.is_some() && local_role.is_none() && (contribution_signed ^ need_signed);
        let can_sign_agreement = match local_role {
            Some(TransferSide::Contribution) => {
                transfer.first_agreement <= 0
                    || (transfer.first_agreement < 2 && transfer.second_agreement >= 1)
            }
            Some(TransferSide::Need) => {
                transfer.second_agreement <= 0
                    || (transfer.second_agreement < 2 && transfer.first_agreement >= 1)
            }
            None => false,
        };
        let can_confirm_delivery = !inactive
            && local_role == Some(TransferSide::Contribution)
            && complete_agreement
            && !delivery_confirmed;
        let can_confirm_receipt = !inactive
            && local_role == Some(TransferSide::Need)
            && delivery_confirmed
            && !receipt_confirmed;
        let can_settle_local = !inactive
            && local_role.is_some()
            && complete_agreement
            && delivery_confirmed
            && receipt_confirmed
            && !local_settled;
        let can_settle_full = !inactive
            && complete_agreement
            && delivery_confirmed
            && receipt_confirmed
            && transfer.contribution_id > 0
            && transfer.need_id > 0;
        let can_inactivate = local_role.is_some() && !inactive;
        let event_views = events.iter().map(EventView::from).collect::<Vec<_>>();
        let tree =
            build_transfer_tree_view(&transfer.transfer_uid, relations, configs, transfer_lookup);
        let package_work = WorkMetadataPackage::from_view(&work);
        let package_item_work = items
            .iter()
            .filter_map(TransferItemWorkPackage::from_view)
            .collect::<Vec<_>>();
        let package_interaction_work = interactions
            .iter()
            .filter_map(TransferInteractionWorkPackage::from_view)
            .collect::<Vec<_>>();
        let package = TransferPackage {
            version: PACKAGE_VERSION,
            identity: TransferIdentityPackage::from(&transfer),
            structured: StructuredTransferPackage::default(),
            work: package_work,
            item_work: package_item_work,
            interaction_work: package_interaction_work,
            relations: tree
                .relations
                .iter()
                .cloned()
                .map(TransferRelationPackage::from)
                .collect(),
            tree_config: tree.config.clone().map(TransferTreeConfigPackage::from),
            events: events.into_iter().map(TransferEventPackage::from).collect(),
        };
        Self {
            id: transfer.id,
            quantity: transfer.quantity,
            transfer_uid: transfer.transfer_uid,
            parent_transfer_uid: transfer.parent_transfer_uid,
            source_transfer_uid: transfer.source_transfer_uid,
            state: transfer.state,
            status,
            title: transfer.title,
            coordinator_label: transfer.coordinator_label,
            proposer_label: transfer.proposer_label,
            counterparty_label: transfer.counterparty_label,
            local_role: local_role.map(|role| role.as_str().to_string()),
            target_organ_id: transfer.target_organ_id,
            target_organ_name: transfer.target_organ_name,
            target_base_url: transfer.target_base_url,
            source_base_url: transfer.source_base_url,
            contribution: TransferSideView {
                actor_label: transfer.contribution_actor_label,
                public_key: transfer.contribution_public_key,
                record_id: transfer.contribution_id,
                head: transfer.contribution_head,
                quantity: transfer.contribution_quantity.abs(),
            },
            need: TransferSideView {
                actor_label: transfer.need_actor_label,
                public_key: transfer.need_public_key,
                record_id: transfer.need_id,
                head: transfer.need_head,
                quantity: transfer.need_quantity.abs(),
            },
            agreement: AgreementView {
                contribution: transfer.first_agreement,
                need: transfer.second_agreement,
            },
            confirmations: ConfirmationView {
                delivery: delivery_confirmed,
                receipt: receipt_confirmed,
            },
            tree,
            controls: ControlView {
                can_duplicate,
                can_sign_agreement,
                can_confirm_delivery,
                can_confirm_receipt,
                can_settle_local,
                can_settle_full,
                can_inactivate,
            },
            events: event_views,
            sync_cursors: cursors,
            settlements,
            work,
            receipt,
            visibility,
            items,
            interactions,
            package,
            created_at: sql_to_iso8601(&transfer.created_at),
            updated_at: sql_to_iso8601(&transfer.updated_at),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkMetadataView {
    status: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    completion_notes: Option<String>,
    metadata: Value,
    assignments: Vec<WorkAssignmentView>,
}

impl WorkMetadataView {
    fn empty() -> Self {
        Self {
            status: None,
            start_at: None,
            end_at: None,
            estimate_seconds: None,
            completion_notes: None,
            metadata: json!({}),
            assignments: Vec::new(),
        }
    }

    fn from_row(row: WorkMetadataRow, assignments: Vec<WorkAssignmentJoinRow>) -> Self {
        Self {
            status: row.status,
            start_at: row.start_at.as_deref().map(sql_to_iso8601),
            end_at: row.end_at.as_deref().map(sql_to_iso8601),
            estimate_seconds: row.estimate_seconds,
            completion_notes: row.completion_notes,
            metadata: row
                .metadata_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({})),
            assignments: assignments
                .into_iter()
                .map(WorkAssignmentView::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct WorkMetadataRow {
    id: i64,
    status: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    completion_notes: Option<String>,
    metadata_json: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct WorkMetadataPackageRow {
    id: i64,
    task_type: Option<String>,
    status: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    completion_notes: Option<String>,
    metadata_json: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct WorkAssignmentJoinRow {
    id: i64,
    work_subject_id: i64,
    assignment_kind: String,
    subject_kind: String,
    app_user_id: Option<i64>,
    organ_id: Option<i64>,
    transfer_party_id: Option<i64>,
    remote_base_url: Option<String>,
    remote_public_key: Option<String>,
    remote_subject_uid: Option<String>,
    display_name_snapshot: Option<String>,
    organ_name_snapshot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkAssignmentView {
    id: i64,
    subject_id: i64,
    assignment_kind: String,
    subject_kind: String,
    app_user_id: Option<i64>,
    organ_id: Option<i64>,
    transfer_party_id: Option<i64>,
    remote_base_url: Option<String>,
    remote_public_key: Option<String>,
    remote_subject_uid: Option<String>,
    display_name: Option<String>,
    organ_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkMetadataPackage {
    #[serde(default)]
    task_type: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    start_at: Option<String>,
    #[serde(default)]
    end_at: Option<String>,
    #[serde(default)]
    estimate_seconds: Option<i64>,
    #[serde(default)]
    completion_notes: Option<String>,
    #[serde(default)]
    metadata: Value,
    #[serde(default)]
    assignments: Vec<WorkAssignmentPackage>,
}

impl WorkMetadataPackage {
    fn from_view(view: &WorkMetadataView) -> Option<Self> {
        if view.status.is_none()
            && view.start_at.is_none()
            && view.end_at.is_none()
            && view.estimate_seconds.is_none()
            && view.completion_notes.is_none()
            && view.metadata == json!({})
            && view.assignments.is_empty()
        {
            return None;
        }
        Some(Self {
            task_type: None,
            status: view.status.clone(),
            start_at: view.start_at.clone(),
            end_at: view.end_at.clone(),
            estimate_seconds: view.estimate_seconds,
            completion_notes: view.completion_notes.clone(),
            metadata: view.metadata.clone(),
            assignments: view
                .assignments
                .iter()
                .cloned()
                .map(WorkAssignmentPackage::from)
                .collect(),
        })
    }

    fn from_row(row: WorkMetadataPackageRow, assignments: Vec<WorkAssignmentJoinRow>) -> Self {
        Self {
            task_type: row.task_type,
            status: row.status,
            start_at: row.start_at.as_deref().map(sql_to_iso8601),
            end_at: row.end_at.as_deref().map(sql_to_iso8601),
            estimate_seconds: row.estimate_seconds,
            completion_notes: row.completion_notes,
            metadata: row
                .metadata_json
                .as_deref()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or_else(|| json!({})),
            assignments: assignments
                .into_iter()
                .map(WorkAssignmentPackage::from)
                .collect(),
        }
    }

    fn metadata_json(&self) -> String {
        serde_json::to_string(&self.metadata).unwrap_or_else(|_| "{}".to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkAssignmentPackage {
    assignment_kind: String,
    subject_kind: String,
    #[serde(default)]
    app_user_id: Option<i64>,
    #[serde(default)]
    organ_id: Option<i64>,
    #[serde(default)]
    transfer_party_id: Option<i64>,
    #[serde(default)]
    remote_base_url: Option<String>,
    #[serde(default)]
    remote_public_key: Option<String>,
    #[serde(default)]
    remote_subject_uid: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    organ_name: Option<String>,
}

impl From<WorkAssignmentJoinRow> for WorkAssignmentPackage {
    fn from(row: WorkAssignmentJoinRow) -> Self {
        Self {
            assignment_kind: row.assignment_kind,
            subject_kind: row.subject_kind,
            app_user_id: row.app_user_id,
            organ_id: row.organ_id,
            transfer_party_id: row.transfer_party_id,
            remote_base_url: row.remote_base_url,
            remote_public_key: row.remote_public_key,
            remote_subject_uid: row.remote_subject_uid,
            display_name: row.display_name_snapshot,
            organ_name: row.organ_name_snapshot,
        }
    }
}

impl From<WorkAssignmentView> for WorkAssignmentPackage {
    fn from(row: WorkAssignmentView) -> Self {
        Self {
            assignment_kind: row.assignment_kind,
            subject_kind: row.subject_kind,
            app_user_id: row.app_user_id,
            organ_id: row.organ_id,
            transfer_party_id: row.transfer_party_id,
            remote_base_url: row.remote_base_url,
            remote_public_key: row.remote_public_key,
            remote_subject_uid: row.remote_subject_uid,
            display_name: row.display_name,
            organ_name: row.organ_name,
        }
    }
}

impl From<WorkAssignmentJoinRow> for WorkAssignmentView {
    fn from(row: WorkAssignmentJoinRow) -> Self {
        Self {
            id: row.id,
            subject_id: row.work_subject_id,
            assignment_kind: row.assignment_kind,
            subject_kind: row.subject_kind,
            app_user_id: row.app_user_id,
            organ_id: row.organ_id,
            transfer_party_id: row.transfer_party_id,
            remote_base_url: row.remote_base_url,
            remote_public_key: row.remote_public_key,
            remote_subject_uid: row.remote_subject_uid,
            display_name: row.display_name_snapshot,
            organ_name: row.organ_name_snapshot,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TransferStructuredItemWorkRow {
    id: i64,
    transfer_id: i64,
    role: String,
    title: String,
    description: Option<String>,
    source_record_id: Option<i64>,
    quantity: Option<f64>,
    unit: Option<String>,
    version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferItemWorkView {
    id: i64,
    transfer_id: i64,
    role: String,
    title: String,
    description: Option<String>,
    source_record_id: Option<i64>,
    quantity: Option<f64>,
    unit: Option<String>,
    version: i64,
    work: WorkMetadataView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferItemWorkPackage {
    role: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    source_record_id: Option<i64>,
    #[serde(default)]
    quantity: Option<f64>,
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    version: Option<i64>,
    work: WorkMetadataPackage,
}

impl TransferItemWorkPackage {
    fn from_row(row: TransferStructuredItemWorkRow, work: WorkMetadataPackage) -> Self {
        Self {
            role: row.role,
            title: row.title,
            description: row.description,
            source_record_id: row.source_record_id,
            quantity: row.quantity,
            unit: row.unit,
            version: Some(row.version),
            work,
        }
    }

    fn from_view(view: &TransferItemWorkView) -> Option<Self> {
        WorkMetadataPackage::from_view(&view.work).map(|work| Self {
            role: view.role.clone(),
            title: view.title.clone(),
            description: view.description.clone(),
            source_record_id: view.source_record_id,
            quantity: view.quantity,
            unit: view.unit.clone(),
            version: Some(view.version),
            work,
        })
    }
}

impl TransferItemWorkView {
    fn from_row(row: TransferStructuredItemWorkRow, work: WorkMetadataView) -> Self {
        Self {
            id: row.id,
            transfer_id: row.transfer_id,
            role: row.role,
            title: row.title,
            description: row.description,
            source_record_id: row.source_record_id,
            quantity: row.quantity,
            unit: row.unit,
            version: row.version,
            work,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
struct TransferInteractionWorkRow {
    id: i64,
    transfer_id: i64,
    interaction_kind: String,
    direction: String,
    from_item_id: Option<i64>,
    to_item_id: Option<i64>,
    from_party_id: Option<i64>,
    to_party_id: Option<i64>,
    quantity: Option<f64>,
    state: String,
    dependency_kind: Option<String>,
    version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferInteractionWorkView {
    id: i64,
    transfer_id: i64,
    interaction_kind: String,
    direction: String,
    from_item_id: Option<i64>,
    to_item_id: Option<i64>,
    from_party_id: Option<i64>,
    to_party_id: Option<i64>,
    quantity: Option<f64>,
    state: String,
    dependency_kind: Option<String>,
    version: i64,
    work: WorkMetadataView,
}

impl TransferInteractionWorkView {
    fn from_row(row: TransferInteractionWorkRow, work: WorkMetadataView) -> Self {
        Self {
            id: row.id,
            transfer_id: row.transfer_id,
            interaction_kind: row.interaction_kind,
            direction: row.direction,
            from_item_id: row.from_item_id,
            to_item_id: row.to_item_id,
            from_party_id: row.from_party_id,
            to_party_id: row.to_party_id,
            quantity: row.quantity,
            state: row.state,
            dependency_kind: row.dependency_kind,
            version: row.version,
            work,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferInteractionWorkPackage {
    interaction_kind: String,
    direction: String,
    #[serde(default)]
    from_item_id: Option<i64>,
    #[serde(default)]
    to_item_id: Option<i64>,
    #[serde(default)]
    from_party_id: Option<i64>,
    #[serde(default)]
    to_party_id: Option<i64>,
    #[serde(default)]
    quantity: Option<f64>,
    state: String,
    #[serde(default)]
    dependency_kind: Option<String>,
    #[serde(default)]
    version: Option<i64>,
    work: WorkMetadataPackage,
}

impl TransferInteractionWorkPackage {
    fn from_row(row: TransferInteractionWorkRow, work: WorkMetadataPackage) -> Self {
        Self {
            interaction_kind: row.interaction_kind,
            direction: row.direction,
            from_item_id: row.from_item_id,
            to_item_id: row.to_item_id,
            from_party_id: row.from_party_id,
            to_party_id: row.to_party_id,
            quantity: row.quantity,
            state: row.state,
            dependency_kind: row.dependency_kind,
            version: Some(row.version),
            work,
        }
    }

    fn from_view(view: &TransferInteractionWorkView) -> Option<Self> {
        WorkMetadataPackage::from_view(&view.work).map(|work| Self {
            interaction_kind: view.interaction_kind.clone(),
            direction: view.direction.clone(),
            from_item_id: view.from_item_id,
            to_item_id: view.to_item_id,
            from_party_id: view.from_party_id,
            to_party_id: view.to_party_id,
            quantity: view.quantity,
            state: view.state.clone(),
            dependency_kind: view.dependency_kind.clone(),
            version: Some(view.version),
            work,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferSideView {
    actor_label: String,
    public_key: Option<String>,
    record_id: i64,
    head: String,
    quantity: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgreementView {
    contribution: i64,
    need: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmationView {
    delivery: bool,
    receipt: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControlView {
    can_duplicate: bool,
    can_sign_agreement: bool,
    can_confirm_delivery: bool,
    can_confirm_receipt: bool,
    can_settle_local: bool,
    can_settle_full: bool,
    can_inactivate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventView {
    id: i64,
    transfer_uid: Option<String>,
    event_uid: Option<String>,
    actor_label: String,
    actor_public_key: Option<String>,
    event_kind: String,
    payload: Value,
    previous_event_id: Option<i64>,
    previous_event_uid: Option<String>,
    previous_event_hash: Option<String>,
    event_hash: Option<String>,
    signature: Option<String>,
    signature_valid: bool,
    validation_state: String,
    validation_error: Option<String>,
    created_at: String,
}

impl From<&EventRow> for EventView {
    fn from(row: &EventRow) -> Self {
        Self {
            id: row.id,
            transfer_uid: row.transfer_uid.clone(),
            event_uid: row.event_uid.clone(),
            actor_label: row.actor_label.clone(),
            actor_public_key: row.actor_public_key.clone(),
            event_kind: row.event_kind.clone(),
            payload: serde_json::from_str(&row.payload_json).unwrap_or(Value::Null),
            previous_event_id: row.previous_event_id,
            previous_event_uid: row.previous_event_uid.clone(),
            previous_event_hash: row.previous_event_hash.clone(),
            event_hash: row.event_hash.clone(),
            signature: row.signature.clone(),
            signature_valid: verify_event_row(row),
            validation_state: row.validation_state.clone(),
            validation_error: row.validation_error.clone(),
            created_at: row.created_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferPackage {
    version: u32,
    identity: TransferIdentityPackage,
    #[serde(default)]
    structured: StructuredTransferPackage,
    #[serde(default)]
    work: Option<WorkMetadataPackage>,
    #[serde(default)]
    item_work: Vec<TransferItemWorkPackage>,
    #[serde(default)]
    interaction_work: Vec<TransferInteractionWorkPackage>,
    #[serde(default)]
    relations: Vec<TransferRelationPackage>,
    #[serde(default)]
    tree_config: Option<TransferTreeConfigPackage>,
    events: Vec<TransferEventPackage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructuredTransferPackage {
    #[serde(default)]
    parties: Vec<TransferPartyPackage>,
    #[serde(default)]
    items: Vec<StructuredItemPackage>,
    #[serde(default)]
    interactions: Vec<StructuredInteractionPackage>,
    #[serde(default)]
    agreements: Vec<StructuredAgreementPackage>,
    #[serde(default)]
    confirmations: Vec<StructuredConfirmationPackage>,
    #[serde(default)]
    settlements: Vec<StructuredSettlementPackage>,
    #[serde(default)]
    quantity_influences: Vec<QuantityInfluencePackage>,
    #[serde(default)]
    messages: Vec<TransferMessagePackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct TransferPartyPackage {
    party_uid: Option<String>,
    participation_kind: String,
    role_hint: Option<String>,
    actor_label: String,
    public_key: Option<String>,
    placeholder: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct StructuredItemPackage {
    item_uid: Option<String>,
    role: String,
    source_record_id: Option<i64>,
    title: String,
    description: Option<String>,
    record_head_snapshot: Option<String>,
    record_body_snapshot: Option<String>,
    quantity: Option<f64>,
    unit: Option<String>,
    location: Option<String>,
    metadata_json: String,
    version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct StructuredInteractionPackage {
    interaction_uid: Option<String>,
    interaction_kind: String,
    direction: String,
    quantity: Option<f64>,
    state: String,
    dependency_kind: Option<String>,
    metadata_json: String,
    version: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct StructuredAgreementPackage {
    scope_kind: String,
    agreement_level: i64,
    agreed_item_version: Option<i64>,
    agreed_interaction_version: Option<i64>,
    agreed_at: Option<String>,
    invalidated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct StructuredConfirmationPackage {
    scope_kind: String,
    confirmation_kind: String,
    confirmed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct StructuredSettlementPackage {
    scope_kind: String,
    local_record_id: i64,
    quantity_delta: f64,
    settled_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct QuantityInfluencePackage {
    record_id: i64,
    influence: f64,
    influence_state: String,
    policy: String,
    consumed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
struct TransferMessagePackage {
    body: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferIdentityPackage {
    transfer_uid: String,
    parent_transfer_uid: Option<String>,
    source_transfer_uid: Option<String>,
    state: String,
    title: String,
    coordinator_label: String,
    proposer_label: String,
    counterparty_label: String,
    contribution_actor_label: String,
    contribution_public_key: Option<String>,
    need_actor_label: String,
    need_public_key: Option<String>,
    target_organ_id: Option<i64>,
    target_organ_name: Option<String>,
    target_base_url: Option<String>,
    source_base_url: Option<String>,
    topic_text: Option<String>,
}

impl From<&TransferSummaryRow> for TransferIdentityPackage {
    fn from(row: &TransferSummaryRow) -> Self {
        Self {
            transfer_uid: row.transfer_uid.clone(),
            parent_transfer_uid: row.parent_transfer_uid.clone(),
            source_transfer_uid: row.source_transfer_uid.clone(),
            state: row.state.clone(),
            title: row.title.clone(),
            coordinator_label: row.coordinator_label.clone(),
            proposer_label: row.proposer_label.clone(),
            counterparty_label: row.counterparty_label.clone(),
            contribution_actor_label: row.contribution_actor_label.clone(),
            contribution_public_key: row.contribution_public_key.clone(),
            need_actor_label: row.need_actor_label.clone(),
            need_public_key: row.need_public_key.clone(),
            target_organ_id: row.target_organ_id,
            target_organ_name: row.target_organ_name.clone(),
            target_base_url: row.target_base_url.clone(),
            source_base_url: row.source_base_url.clone(),
            topic_text: row.topic_text.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferRelationPackage {
    transfer_uid: String,
    relation_type: String,
    target_transfer_uid: String,
    position: Option<f64>,
}

impl From<TransferRelationRow> for TransferRelationPackage {
    fn from(row: TransferRelationRow) -> Self {
        Self {
            transfer_uid: row.transfer_uid,
            relation_type: row.relation_type,
            target_transfer_uid: row.target_transfer_uid,
            position: row.position,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferTreeConfigPackage {
    transfer_uid: String,
    branch_mode: String,
    record_sync_mode: String,
    #[serde(default)]
    reservation_policy: Option<String>,
    source_record_id: Option<i64>,
    #[serde(default)]
    sync_role: Option<String>,
    #[serde(default)]
    sync_quantity: Option<f64>,
    #[serde(default)]
    sync_counterparty_label: Option<String>,
    #[serde(default)]
    sync_target_organ_id: Option<i64>,
    #[serde(default)]
    last_synced_record_head: Option<String>,
    sync_enabled: bool,
}

impl From<TransferTreeConfigRow> for TransferTreeConfigPackage {
    fn from(row: TransferTreeConfigRow) -> Self {
        Self {
            transfer_uid: row.transfer_uid,
            branch_mode: row.branch_mode,
            record_sync_mode: row.record_sync_mode,
            reservation_policy: row.reservation_policy,
            source_record_id: row.source_record_id,
            sync_role: row.sync_role,
            sync_quantity: row.sync_quantity,
            sync_counterparty_label: row.sync_counterparty_label,
            sync_target_organ_id: row.sync_target_organ_id,
            last_synced_record_head: row.last_synced_record_head,
            sync_enabled: row.sync_enabled != 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferEventPackage {
    transfer_uid: String,
    event_uid: String,
    actor_label: String,
    actor_public_key: String,
    event_kind: String,
    payload_json: String,
    previous_event_uid: Option<String>,
    previous_event_hash: Option<String>,
    event_hash: Option<String>,
    signature: String,
    created_at: String,
}

impl From<EventRow> for TransferEventPackage {
    fn from(row: EventRow) -> Self {
        Self {
            transfer_uid: row.transfer_uid.unwrap_or_default(),
            event_uid: row.event_uid.unwrap_or_default(),
            actor_label: row.actor_label,
            actor_public_key: row.actor_public_key.unwrap_or_default(),
            event_kind: row.event_kind,
            payload_json: row.payload_json,
            previous_event_uid: row.previous_event_uid,
            previous_event_hash: row.previous_event_hash,
            event_hash: row.event_hash,
            signature: row.signature.unwrap_or_default(),
            created_at: row.created_at,
        }
    }
}

struct TransferImportOutcome {
    transfer_id: i64,
    events_imported: i64,
}

#[derive(Debug, Clone)]
pub enum TransferWidgetError {
    NotFound(String),
    Misconfigured(String),
    Invalid(String),
    Internal(String),
}

impl TransferWidgetError {
    fn message(&self) -> &str {
        match self {
            Self::NotFound(message)
            | Self::Misconfigured(message)
            | Self::Invalid(message)
            | Self::Internal(message) => message,
        }
    }

    fn from_io(error: Error) -> Self {
        match error.kind() {
            ErrorKind::NotFound => Self::NotFound(error.to_string()),
            ErrorKind::InvalidInput | ErrorKind::InvalidData => Self::Invalid(error.to_string()),
            _ => Self::Internal(error.to_string()),
        }
    }
}

fn parse_payload<T: for<'de> Deserialize<'de>>(payload: Value) -> Result<T, TransferWidgetError> {
    serde_json::from_value::<T>(payload)
        .map_err(|error| TransferWidgetError::Invalid(format!("Invalid payload: {error}")))
}

fn parse_transfer_package_value(value: Value) -> Result<TransferPackage, TransferWidgetError> {
    if looks_like_create_proposal_payload(&value) {
        return Err(TransferWidgetError::Invalid(
            "Invalid Transfer package: /transfer/packages received a create-proposal action payload. Send widget action create-proposal to /host/widgets/{instance_id}/actions/create-proposal, or send a Transfer package with version, identity, structured, and events to /transfer/packages.".into(),
        ));
    }
    if let Some(text) = value.as_str() {
        serde_json::from_str::<TransferPackage>(text).map_err(|error| {
            TransferWidgetError::Invalid(format!("Invalid Transfer package JSON: {error}"))
        })
    } else {
        serde_json::from_value::<TransferPackage>(value).map_err(|error| {
            TransferWidgetError::Invalid(format!("Invalid Transfer package: {error}"))
        })
    }
}

fn structured_package_has_rows(package: &StructuredTransferPackage) -> bool {
    !package.parties.is_empty()
        || !package.items.is_empty()
        || !package.interactions.is_empty()
        || !package.agreements.is_empty()
        || !package.confirmations.is_empty()
        || !package.settlements.is_empty()
        || !package.quantity_influences.is_empty()
        || !package.messages.is_empty()
}

fn package_side_view(
    package: &TransferPackage,
    role: TransferSide,
    actor_label: String,
    public_key: Option<String>,
) -> TransferSideView {
    let item = package
        .structured
        .items
        .iter()
        .find(|item| item.role == role.as_str());
    TransferSideView {
        actor_label,
        public_key,
        record_id: item.and_then(|item| item.source_record_id).unwrap_or(0),
        head: item
            .map(|item| item.title.clone().trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                item.and_then(|item| item.record_head_snapshot.clone())
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| role.as_str().to_string()),
        quantity: item.and_then(|item| item.quantity).unwrap_or(0.0).abs(),
    }
}

fn describe_remote_transfer_error(source: &str, status: u16, url: &str, body: &str) -> String {
    let detail = remote_error_detail(body);
    match status {
        400 => format!("{source} rejected {url}: bad Transfer payload. {detail}"),
        401 => format!("{source} rejected {url}: authentication is required or expired. {detail}"),
        403 => format!("{source} rejected {url}: authenticated user is not allowed. {detail}"),
        404 => format!(
            "{source} rejected {url}: endpoint not found. Check that the Organ base URL points to the Lince server root, not /host or another app path. {detail}"
        ),
        413 => format!("{source} rejected {url}: Transfer package is too large. {detail}"),
        415 => format!("{source} rejected {url}: unsupported content type. {detail}"),
        422 => format!("{source} rejected {url}: Transfer package validation failed. {detail}"),
        500..=599 => {
            format!("{source} failed while handling {url}: server error {status}. {detail}")
        }
        _ => format!("{source} rejected {url} with status {status}. {detail}"),
    }
}

fn remote_error_detail(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "The response body was empty.".to_string();
    }
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return error.to_string();
        }
        if looks_like_create_proposal_payload(&value) {
            return "The response body looked like a create-proposal action payload, not a Transfer package error. Check browser/network logs for a request hitting /transfer/packages with the wrong body.".to_string();
        }
    }
    trimmed.to_string()
}

fn looks_like_create_proposal_payload(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object.contains_key("title")
            && object.contains_key("role")
            && object.contains_key("recordId")
            && object.contains_key("quantity")
            && object.contains_key("counterpartyLabel")
            && !object.contains_key("version")
            && !object.contains_key("identity")
            && !object.contains_key("events")
    })
}

fn normalize_nonempty(value: &str, field: &str) -> Result<String, TransferWidgetError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(TransferWidgetError::Invalid(format!(
            "{field} is required."
        )));
    }
    Ok(value.to_string())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn package_peer_base_urls(package: &TransferPackage) -> Vec<String> {
    let mut base_urls = Vec::new();
    if let Some(base_url) = normalize_optional_text(package.identity.source_base_url.clone()) {
        base_urls.push(base_url);
    }
    if let Some(base_url) = normalize_optional_text(package.identity.target_base_url.clone()) {
        base_urls.push(base_url);
    }
    base_urls.sort();
    base_urls.dedup_by(|left, right| same_base_url(left, right));
    base_urls
}

fn normalize_counterparty(
    raw: String,
    target_organ: Option<&Organ>,
) -> Result<String, TransferWidgetError> {
    let value = raw.trim();
    if !value.is_empty() {
        return Ok(value.to_string());
    }
    target_organ
        .map(|organ| organ.name.clone())
        .ok_or_else(|| TransferWidgetError::Invalid("Counterparty label is required.".into()))
}

fn positive_quantity(value: f64) -> Result<f64, TransferWidgetError> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(TransferWidgetError::Invalid(
            "Transfer quantity must be greater than zero.".into(),
        ))
    }
}

fn optional_text_parameter(value: Option<String>) -> SqlParameter {
    value.map(SqlParameter::Text).unwrap_or(SqlParameter::Null)
}

fn optional_text_param(value: Option<&str>) -> SqlParameter {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| SqlParameter::Text(value.to_string()))
        .unwrap_or(SqlParameter::Null)
}

fn optional_i64_parameter(value: Option<i64>) -> SqlParameter {
    value
        .map(SqlParameter::Integer)
        .unwrap_or(SqlParameter::Null)
}

fn bool_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn optional_i64_param(value: Option<i64>) -> SqlParameter {
    value
        .map(SqlParameter::Integer)
        .unwrap_or(SqlParameter::Null)
}

fn has_stable_remote_subject(
    remote_base_url: Option<&str>,
    remote_subject_uid: Option<&str>,
) -> bool {
    remote_base_url.is_some_and(|value| !value.trim().is_empty())
        && remote_subject_uid.is_some_and(|value| !value.trim().is_empty())
}

fn optional_f64_parameter(value: Option<f64>) -> SqlParameter {
    value.map(SqlParameter::Real).unwrap_or(SqlParameter::Null)
}

fn structured_row_uid(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4())
}

fn valid_json_or_empty_object(value: &str) -> String {
    serde_json::from_str::<Value>(value)
        .ok()
        .filter(|value| value.is_object())
        .map(|value| value.to_string())
        .unwrap_or_else(|| "{}".to_string())
}

fn transfer_sync_cache_path() -> Result<PathBuf, Error> {
    let dir = utils::config::lince_data_dir()
        .ok_or_else(|| Error::other("Unable to resolve Lince data directory"))?;
    Ok(dir.join("state").join("transfer-sync-cache.json"))
}

fn current_sql_timestamp() -> String {
    Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn sql_to_iso8601(value: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|time| format!("{}Z", time.format("%Y-%m-%dT%H:%M:%S")))
        .unwrap_or_else(|_| value.to_string())
}

fn sync_since_with_lookback(value: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|time| time - ChronoDuration::seconds(60))
        .unwrap_or_else(|_| Utc::now().naive_utc() - ChronoDuration::seconds(60))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn transfer_poll_due(last_polled_at: Option<&str>) -> bool {
    let Some(value) = last_polled_at else {
        return true;
    };
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map(|time| time + ChronoDuration::hours(1) <= Utc::now().naive_utc())
        .unwrap_or(true)
}

fn same_base_url(left: &str, right: &str) -> bool {
    let left_parts = normalize_base_url_parts(left);
    let right_parts = normalize_base_url_parts(right);
    match (left_parts, right_parts) {
        (Some(left), Some(right)) => left == right,
        _ => left.trim().trim_end_matches('/') == right.trim().trim_end_matches('/'),
    }
}

fn normalize_base_url_parts(value: &str) -> Option<(String, Option<u16>)> {
    let value = value
        .trim()
        .trim_end_matches('/')
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_port = value.split('/').next().unwrap_or(value);
    let (host, port) = host_port
        .rsplit_once(':')
        .map(|(host, port)| (host, port.parse::<u16>().ok()))
        .unwrap_or((host_port, None));
    if host.trim().is_empty() {
        return None;
    }
    let host = match host {
        "localhost" | "0.0.0.0" | "::1" => "127.0.0.1",
        other => other,
    };
    Some((host.to_string(), port))
}

fn encode_query_value(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace(':', "%3A")
}

fn new_signing_key() -> SigningKey {
    let first = Uuid::new_v4();
    let second = Uuid::new_v4();
    let mut seed = [0_u8; 32];
    seed[..16].copy_from_slice(first.as_bytes());
    seed[16..].copy_from_slice(second.as_bytes());
    SigningKey::from_bytes(&seed)
}

fn signing_key_from_base64(secret_key: &str) -> Result<SigningKey, Error> {
    let bytes = BASE64
        .decode(secret_key)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error.to_string()))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Error::new(ErrorKind::InvalidData, "Invalid signing secret length"))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn event_signing_message(
    event_uid: &str,
    transfer_uid: &str,
    actor_label: &str,
    actor_public_key: &str,
    event_kind: &str,
    previous_event_uid: Option<&str>,
    payload_json: &str,
) -> String {
    format!(
        "transfer-event-v1\n{event_uid}\n{transfer_uid}\n{actor_label}\n{actor_public_key}\n{event_kind}\n{}\n{payload_json}",
        previous_event_uid.unwrap_or_default()
    )
}

fn verify_event_row(row: &EventRow) -> bool {
    let Some(transfer_uid) = row.transfer_uid.as_deref() else {
        return false;
    };
    let Some(event_uid) = row.event_uid.as_deref() else {
        return false;
    };
    let Some(actor_public_key) = row.actor_public_key.as_deref() else {
        return false;
    };
    let Some(signature) = row.signature.as_deref() else {
        return false;
    };
    verify_event_signature(
        event_uid,
        transfer_uid,
        &row.actor_label,
        actor_public_key,
        &row.event_kind,
        row.previous_event_uid.as_deref(),
        &row.payload_json,
        signature,
    )
}

fn verify_event_signature(
    event_uid: &str,
    transfer_uid: &str,
    actor_label: &str,
    actor_public_key: &str,
    event_kind: &str,
    previous_event_uid: Option<&str>,
    payload_json: &str,
    signature: &str,
) -> bool {
    let Ok(public_bytes) = BASE64.decode(actor_public_key) else {
        return false;
    };
    let Ok(public_bytes) = <[u8; 32]>::try_from(public_bytes.as_slice()) else {
        return false;
    };
    let Ok(signature_bytes) = BASE64.decode(signature) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_bytes) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&signature_bytes) else {
        return false;
    };
    let message = event_signing_message(
        event_uid,
        transfer_uid,
        actor_label,
        actor_public_key,
        event_kind,
        previous_event_uid,
        payload_json,
    );
    verifying_key.verify(message.as_bytes(), &signature).is_ok()
}

fn transfer_event_hash(
    event_uid: &str,
    transfer_uid: &str,
    actor_label: &str,
    actor_public_key: &str,
    event_kind: &str,
    previous_event_uid: Option<&str>,
    previous_event_hash: Option<&str>,
    payload_json: &str,
    signature: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"transfer-event-hash-v1\n");
    hasher.update(event_uid.as_bytes());
    hasher.update(b"\n");
    hasher.update(transfer_uid.as_bytes());
    hasher.update(b"\n");
    hasher.update(actor_label.as_bytes());
    hasher.update(b"\n");
    hasher.update(actor_public_key.as_bytes());
    hasher.update(b"\n");
    hasher.update(event_kind.as_bytes());
    hasher.update(b"\n");
    hasher.update(previous_event_uid.unwrap_or_default().as_bytes());
    hasher.update(b"\n");
    hasher.update(previous_event_hash.unwrap_or_default().as_bytes());
    hasher.update(b"\n");
    hasher.update(payload_json.as_bytes());
    hasher.update(b"\n");
    hasher.update(signature.as_bytes());
    format!("{:x}", hasher.finalize())
}

struct EventValidation {
    state: String,
    error: Option<String>,
}

fn validate_packaged_event(
    event: &TransferEventPackage,
    previous: Option<&EventPointer>,
) -> EventValidation {
    let mut errors = Vec::new();
    if !payload_shape_matches_event_kind(&event.event_kind, &event.payload_json) {
        errors.push("payload shape does not match event kind".to_string());
    }
    if !verify_event_signature(
        &event.event_uid,
        &event.transfer_uid,
        &event.actor_label,
        &event.actor_public_key,
        &event.event_kind,
        event.previous_event_uid.as_deref(),
        &event.payload_json,
        &event.signature,
    ) {
        errors.push("signature verification failed".to_string());
    }
    if let Some(previous) = previous {
        if event.previous_event_hash.as_deref() != previous.event_hash.as_deref() {
            errors.push("previous event hash mismatch".to_string());
        }
    } else if event.previous_event_uid.is_some() {
        errors.push("previous event not found locally".to_string());
    }
    let expected_hash = transfer_event_hash(
        &event.event_uid,
        &event.transfer_uid,
        &event.actor_label,
        &event.actor_public_key,
        &event.event_kind,
        event.previous_event_uid.as_deref(),
        event.previous_event_hash.as_deref(),
        &event.payload_json,
        &event.signature,
    );
    if event.event_hash.as_deref() != Some(expected_hash.as_str()) {
        errors.push("event hash mismatch".to_string());
    }
    if errors.is_empty() {
        EventValidation {
            state: "valid".to_string(),
            error: None,
        }
    } else {
        EventValidation {
            state: "invalid".to_string(),
            error: Some(errors.join("; ")),
        }
    }
}

fn payload_shape_matches_event_kind(event_kind: &str, payload_json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(payload_json) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    match event_kind {
        "transfer_created" => object.contains_key("event_type"),
        "item_created" => object.contains_key("contribution") || object.contains_key("role"),
        "item_edited" => object.contains_key("event_type") || object.contains_key("role"),
        "agreement_changed" => object.contains_key("event_type") || object.contains_key("level"),
        "delivery_confirmed" | "receipt_confirmed" | "settlement_applied" => {
            object.contains_key("event_type") || object.contains_key("transfer_id")
        }
        "transfer_inactivated" => object.contains_key("event_type"),
        _ => true,
    }
}

fn validate_package(package: &TransferPackage) -> Result<(), TransferWidgetError> {
    normalize_package_identity(&package.identity)?;
    if package.events.is_empty() {
        return Err(TransferWidgetError::Invalid(
            "Transfer package has no signed events.".into(),
        ));
    }
    if !structured_package_has_rows(&package.structured) {
        return Err(TransferWidgetError::Invalid(
            "Transfer package has no structured transfer rows.".into(),
        ));
    }
    for relation in &package.relations {
        if relation.transfer_uid.trim().is_empty()
            || relation.target_transfer_uid.trim().is_empty()
            || !matches!(relation.relation_type.as_str(), "parent" | "depends_on")
        {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has an invalid relation.".into(),
            ));
        }
    }
    if let Some(config) = &package.tree_config {
        if config.transfer_uid.trim().is_empty()
            || TransferBranchMode::parse_storage(&config.branch_mode).is_none()
            || TransferRecordSyncMode::parse_storage(&config.record_sync_mode).is_none()
            || config
                .sync_role
                .as_deref()
                .is_some_and(|role| TransferSide::parse_storage(Some(role)).is_none())
        {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has an invalid tree config.".into(),
            ));
        }
    }
    if let Some(work) = &package.work {
        validate_work_metadata_package(work)?;
    }
    for item_work in &package.item_work {
        if TransferSide::parse_storage(Some(&item_work.role)).is_none()
            && !matches!(
                item_work.role.as_str(),
                "support" | "task" | "information" | "reservation"
            )
        {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has invalid item work role.".into(),
            ));
        }
        if item_work.title.trim().is_empty() {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has item work without a title.".into(),
            ));
        }
        validate_work_metadata_package(&item_work.work)?;
    }
    for interaction_work in &package.interaction_work {
        if !matches!(
            interaction_work.interaction_kind.as_str(),
            "contributes_to" | "depends_on" | "unblocks" | "replaces" | "informs"
        ) || !matches!(
            interaction_work.direction.as_str(),
            "incoming" | "outgoing" | "mutual" | "informational"
        ) || interaction_work
            .dependency_kind
            .as_deref()
            .is_some_and(|kind| {
                !matches!(
                    kind,
                    "must_agree"
                        | "must_activate"
                        | "must_deliver"
                        | "must_receive"
                        | "must_settle"
                )
            })
        {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has invalid interaction work.".into(),
            ));
        }
        validate_work_metadata_package(&interaction_work.work)?;
    }

    for event in &package.events {
        if event.transfer_uid != package.identity.transfer_uid {
            return Err(TransferWidgetError::Invalid(
                "Transfer event belongs to a different transfer_uid.".into(),
            ));
        }
        if serde_json::from_str::<Value>(&event.payload_json).is_err() {
            return Err(TransferWidgetError::Invalid(
                "Transfer event payload is not valid JSON.".into(),
            ));
        }
        if !verify_event_signature(
            &event.event_uid,
            &event.transfer_uid,
            &event.actor_label,
            &event.actor_public_key,
            &event.event_kind,
            event.previous_event_uid.as_deref(),
            &event.payload_json,
            &event.signature,
        ) {
            return Err(TransferWidgetError::Invalid(format!(
                "Transfer event {} has an invalid signature.",
                event.event_uid
            )));
        }
        validate_event_actor(&package.identity, event)?;
    }

    Ok(())
}

fn validate_work_metadata_package(work: &WorkMetadataPackage) -> Result<(), TransferWidgetError> {
    if work.estimate_seconds.is_some_and(|value| value < 0) {
        return Err(TransferWidgetError::Invalid(
            "Transfer package has invalid work estimate.".into(),
        ));
    }
    if !work.metadata.is_object() {
        return Err(TransferWidgetError::Invalid(
            "Transfer package work metadata must be a JSON object.".into(),
        ));
    }
    for assignment in &work.assignments {
        if !matches!(
            assignment.assignment_kind.as_str(),
            "responsible" | "observer" | "helper"
        ) {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has invalid work assignment kind.".into(),
            ));
        }
        if !matches!(
            assignment.subject_kind.as_str(),
            "app_user" | "organ" | "transfer_party" | "external_actor" | "placeholder"
        ) {
            return Err(TransferWidgetError::Invalid(
                "Transfer package has invalid work subject kind.".into(),
            ));
        }
    }
    Ok(())
}

fn validate_gossip_size(package: &TransferPackage) -> Result<(), TransferWidgetError> {
    if package.events.len() > MAX_GOSSIP_EVENTS {
        return Err(TransferWidgetError::Invalid(
            "Transfer gossip package has too many events.".into(),
        ));
    }
    let size = serde_json::to_string(package)
        .map_err(|error| TransferWidgetError::Invalid(error.to_string()))?
        .len();
    if size > MAX_GOSSIP_PACKAGE_BYTES {
        return Err(TransferWidgetError::Invalid(
            "Transfer gossip package is too large.".into(),
        ));
    }
    Ok(())
}

async fn validate_public_transfer_package(
    service: &TransferWidgetService,
    package: &TransferPackage,
) -> Result<(), TransferWidgetError> {
    validate_package(package)?;
    validate_gossip_size(package)?;
    if is_public_proposal_package(package) {
        return Ok(());
    }
    if service
        .find_transfer_id_by_uid(&package.identity.transfer_uid)
        .await
        .map_err(TransferWidgetError::from_io)?
        .is_some()
    {
        return Ok(());
    }
    Ok(())
}

fn is_public_proposal_package(package: &TransferPackage) -> bool {
    if package.identity.state != TransferState::PublicProposal.as_str() {
        return false;
    }
    let contribution_signed = package.identity.contribution_public_key.is_some();
    let need_signed = package.identity.need_public_key.is_some();
    if contribution_signed == need_signed {
        return false;
    }
    if package
        .structured
        .agreements
        .iter()
        .any(|agreement| agreement.agreement_level != 0 && agreement.invalidated_at.is_none())
    {
        return false;
    }
    package.events.iter().all(|event| {
        matches!(
            event.event_kind.as_str(),
            "transfer_created" | "item_created"
        )
    })
}

fn validate_event_actor(
    identity: &TransferIdentityPackage,
    event: &TransferEventPackage,
) -> Result<(), TransferWidgetError> {
    let expected_side = match event.event_kind.as_str() {
        "delivery_confirmed" => Some(TransferSide::Contribution),
        "receipt_confirmed" => Some(TransferSide::Need),
        "agreement_changed" | "settlement_applied" | "transfer_inactivated" => {
            Some(event_payload_role(&event.payload_json)?)
        }
        "transfer_created" | "item_created" => None,
        _ => {
            return Err(TransferWidgetError::Invalid(format!(
                "Unsupported Transfer event kind {}.",
                event.event_kind
            )));
        }
    };

    if let Some(side) = expected_side {
        if !event_matches_side(identity, event, side) {
            return Err(TransferWidgetError::Invalid(format!(
                "Transfer event {} is not signed by the {} side.",
                event.event_uid,
                side.as_str()
            )));
        }
        return Ok(());
    }

    if event_matches_side(identity, event, TransferSide::Contribution)
        || event_matches_side(identity, event, TransferSide::Need)
    {
        Ok(())
    } else {
        Err(TransferWidgetError::Invalid(format!(
            "Transfer event {} is not signed by a party on this Transfer.",
            event.event_uid
        )))
    }
}

fn event_payload_role(payload_json: &str) -> Result<TransferSide, TransferWidgetError> {
    let value = serde_json::from_str::<Value>(payload_json).map_err(|error| {
        TransferWidgetError::Invalid(format!("Transfer event payload is invalid JSON: {error}"))
    })?;
    value
        .get("role")
        .and_then(Value::as_str)
        .and_then(|value| match value {
            "contribution" => Some(TransferSide::Contribution),
            "need" => Some(TransferSide::Need),
            _ => None,
        })
        .ok_or_else(|| TransferWidgetError::Invalid("Transfer event role is invalid.".into()))
}

fn event_matches_side(
    identity: &TransferIdentityPackage,
    event: &TransferEventPackage,
    side: TransferSide,
) -> bool {
    match side {
        TransferSide::Contribution => {
            identity.contribution_actor_label == event.actor_label
                && identity.contribution_public_key.as_deref()
                    == Some(event.actor_public_key.as_str())
        }
        TransferSide::Need => {
            identity.need_actor_label == event.actor_label
                && identity.need_public_key.as_deref() == Some(event.actor_public_key.as_str())
        }
    }
}

fn normalize_package_identity(
    identity: &TransferIdentityPackage,
) -> Result<(), TransferWidgetError> {
    for (field, value) in [
        ("transfer_uid", identity.transfer_uid.as_str()),
        ("state", identity.state.as_str()),
        ("title", identity.title.as_str()),
        ("coordinator_label", identity.coordinator_label.as_str()),
        ("proposer_label", identity.proposer_label.as_str()),
        ("counterparty_label", identity.counterparty_label.as_str()),
        (
            "contribution_actor_label",
            identity.contribution_actor_label.as_str(),
        ),
        ("need_actor_label", identity.need_actor_label.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(TransferWidgetError::Invalid(format!(
                "{field} is required."
            )));
        }
    }
    Ok(())
}

fn local_role_for(
    transfer: &TransferSummaryRow,
    local_identity: Option<&LocalIdentityRow>,
) -> Option<TransferSide> {
    let public_key = local_identity?.public_key.as_str();
    if transfer.contribution_public_key.as_deref() == Some(public_key) {
        Some(TransferSide::Contribution)
    } else if transfer.need_public_key.as_deref() == Some(public_key) {
        Some(TransferSide::Need)
    } else {
        None
    }
}

fn agreements_complete(transfer: &TransferSummaryRow) -> bool {
    transfer.first_agreement >= 2 && transfer.second_agreement >= 2
}

fn build_transfer_tree_view(
    transfer_uid: &str,
    relations: &[TransferRelationRow],
    configs: &[TransferTreeConfigRow],
    transfer_lookup: &std::collections::BTreeMap<String, i64>,
) -> TransferTreeView {
    let parent_uid = relations
        .iter()
        .find(|relation| {
            relation.transfer_uid == transfer_uid
                && relation.relation_type == TransferRelationType::Parent.as_str()
        })
        .map(|relation| relation.target_transfer_uid.clone());
    let parent_id = parent_uid
        .as_ref()
        .and_then(|uid| transfer_lookup.get(uid).copied());
    let child_uids = relations
        .iter()
        .filter(|relation| {
            relation.target_transfer_uid == transfer_uid
                && relation.relation_type == TransferRelationType::Parent.as_str()
        })
        .map(|relation| relation.transfer_uid.clone())
        .collect::<Vec<_>>();
    let child_ids = child_uids
        .iter()
        .filter_map(|uid| transfer_lookup.get(uid).copied())
        .collect::<Vec<_>>();
    let direct_relations = relations
        .iter()
        .filter(|relation| {
            relation.transfer_uid == transfer_uid || relation.target_transfer_uid == transfer_uid
        })
        .cloned()
        .collect::<Vec<_>>();
    let config = configs
        .iter()
        .find(|config| config.transfer_uid == transfer_uid)
        .cloned();
    let effective_branch_mode = resolve_effective_branch_mode(transfer_uid, relations, configs);
    TransferTreeView {
        parent_uid,
        parent_id,
        child_uids,
        child_ids,
        relations: direct_relations,
        config,
        effective_branch_mode,
    }
}

fn resolve_effective_branch_mode(
    transfer_uid: &str,
    relations: &[TransferRelationRow],
    configs: &[TransferTreeConfigRow],
) -> String {
    let mut current = Some(transfer_uid.to_string());
    let mut seen = std::collections::BTreeSet::new();
    while let Some(uid) = current {
        if !seen.insert(uid.clone()) {
            break;
        }
        if let Some(config) = configs.iter().find(|config| config.transfer_uid == uid)
            && config.branch_mode != TransferBranchMode::Inherit.as_str()
        {
            return config.branch_mode.clone();
        }
        current = relations
            .iter()
            .find(|relation| {
                relation.transfer_uid == uid
                    && relation.relation_type == TransferRelationType::Parent.as_str()
            })
            .map(|relation| relation.target_transfer_uid.clone());
    }
    TransferBranchMode::Duplicated.as_str().to_string()
}

fn transfer_summary_sql(tail: &str) -> String {
    format!(
        "WITH
            contribution_item AS (
                SELECT
                    item.*,
                    ROW_NUMBER() OVER (PARTITION BY item.transfer_id ORDER BY item.id) AS rn
                FROM transfer_structured_item item
                WHERE item.role = 'contribution'
            ),
            need_item AS (
                SELECT
                    item.*,
                    ROW_NUMBER() OVER (PARTITION BY item.transfer_id ORDER BY item.id) AS rn
                FROM transfer_structured_item item
                WHERE item.role = 'need'
            ),
            contribution_party AS (
                SELECT
                    party.*,
                    ROW_NUMBER() OVER (PARTITION BY party.transfer_id ORDER BY party.id) AS rn
                FROM transfer_party party
                WHERE party.role_hint = 'contribution'
            ),
            need_party AS (
                SELECT
                    party.*,
                    ROW_NUMBER() OVER (PARTITION BY party.transfer_id ORDER BY party.id) AS rn
                FROM transfer_party party
                WHERE party.role_hint = 'need'
            ),
            contribution_agreement AS (
                SELECT
                    agreement.transfer_id,
                    agreement.scope_id,
                    MAX(agreement.agreement_level) AS agreement_level
                FROM transfer_agreement agreement
                JOIN transfer_party party ON party.id = agreement.party_id
                WHERE agreement.scope_kind = 'item'
                  AND party.role_hint = 'contribution'
                  AND agreement.invalidated_at IS NULL
                GROUP BY agreement.transfer_id, agreement.scope_id
            ),
            need_agreement AS (
                SELECT
                    agreement.transfer_id,
                    agreement.scope_id,
                    MAX(agreement.agreement_level) AS agreement_level
                FROM transfer_agreement agreement
                JOIN transfer_party party ON party.id = agreement.party_id
                WHERE agreement.scope_kind = 'item'
                  AND party.role_hint = 'need'
                  AND agreement.invalidated_at IS NULL
                GROUP BY agreement.transfer_id, agreement.scope_id
            )
         SELECT
            t.id,
            t.quantity,
            ident.transfer_uid,
            ident.parent_transfer_uid,
            ident.source_transfer_uid,
            ident.state,
            ident.title,
            ident.coordinator_label,
            ident.proposer_label,
            ident.counterparty_label,
            ident.contribution_actor_label,
            ident.contribution_public_key,
            ident.need_actor_label,
            ident.need_public_key,
            ident.target_organ_id,
            ident.target_organ_name,
            ident.target_base_url,
            ident.source_base_url,
            ident.topic_text,
            ident.created_at,
            ident.updated_at,
            COALESCE(contribution_item.source_record_id, 0) AS contribution_id,
            COALESCE(contribution_item.title, contribution_item.record_head_snapshot, '') AS contribution_head,
            COALESCE(contribution_item.quantity, 0) AS contribution_quantity,
            COALESCE(need_item.source_record_id, 0) AS need_id,
            COALESCE(need_item.title, need_item.record_head_snapshot, '') AS need_head,
            COALESCE(need_item.quantity, 0) AS need_quantity,
            COALESCE(contribution_agreement.agreement_level, 0) AS first_agreement,
            COALESCE(need_agreement.agreement_level, 0) AS second_agreement
         FROM transfer t
         JOIN transfer_identity ident ON ident.transfer_id = t.id
         LEFT JOIN contribution_item
           ON contribution_item.transfer_id = t.id
          AND contribution_item.rn = 1
         LEFT JOIN need_item
           ON need_item.transfer_id = t.id
          AND need_item.rn = 1
         LEFT JOIN contribution_party
           ON contribution_party.transfer_id = t.id
          AND contribution_party.rn = 1
         LEFT JOIN need_party
           ON need_party.transfer_id = t.id
          AND need_party.rn = 1
         LEFT JOIN contribution_agreement
           ON contribution_agreement.transfer_id = t.id
          AND contribution_agreement.scope_id = contribution_item.id
         LEFT JOIN need_agreement
           ON need_agreement.transfer_id = t.id
          AND need_agreement.scope_id = need_item.id
         {tail}"
    )
}

fn find_board_card(board_state: &BoardState, instance_id: &str) -> Option<BoardCard> {
    board_state
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.cards.iter())
        .find(|card| card.id == instance_id)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use {
        crate::{
            domain::board::{BoardCard, BoardWorkspace},
            infrastructure::{
                auth::AppAuth, board_state_store::BoardStateStore, manas::ManasGateway,
                organ_store::OrganStore,
            },
        },
        injection::cross_cutting::dependency_injection,
        persistence::{bootstrap_database, connection, storage::StorageService},
        std::{
            path::PathBuf,
            sync::{Mutex, OnceLock},
        },
    };

    static SERVICE_TEST_LOCK: Mutex<()> = Mutex::new(());
    static SERVICE_TEST_DIR: OnceLock<PathBuf> = OnceLock::new();

    #[test]
    fn stable_remote_subject_requires_base_url_and_subject_uid() {
        assert!(has_stable_remote_subject(
            Some("https://organ.example"),
            Some("remote-user-1")
        ));
        assert!(!has_stable_remote_subject(
            Some("https://organ.example"),
            Some(" ")
        ));
        assert!(!has_stable_remote_subject(None, Some("remote-user-1")));
        assert!(!has_stable_remote_subject(
            Some("https://organ.example"),
            None
        ));
    }

    #[test]
    fn work_package_from_view_preserves_assignment_snapshots() {
        let view = WorkMetadataView {
            status: None,
            start_at: Some("2026-06-18T10:00:00Z".to_string()),
            end_at: None,
            estimate_seconds: Some(3600),
            completion_notes: Some("done".to_string()),
            metadata: json!({"priority": "high"}),
            assignments: vec![WorkAssignmentView {
                id: 7,
                subject_id: 9,
                assignment_kind: "responsible".to_string(),
                subject_kind: "external_actor".to_string(),
                app_user_id: None,
                organ_id: None,
                transfer_party_id: None,
                remote_base_url: Some("https://organ.example".to_string()),
                remote_public_key: Some("public-key".to_string()),
                remote_subject_uid: Some("remote-user-1".to_string()),
                display_name: Some("Remote User".to_string()),
                organ_name: Some("Remote Organ".to_string()),
            }],
        };

        let package = WorkMetadataPackage::from_view(&view).expect("package");

        assert_eq!(package.start_at.as_deref(), Some("2026-06-18T10:00:00Z"));
        assert_eq!(package.estimate_seconds, Some(3600));
        assert_eq!(package.metadata, json!({"priority": "high"}));
        assert_eq!(package.assignments.len(), 1);
        assert_eq!(
            package.assignments[0].remote_subject_uid.as_deref(),
            Some("remote-user-1")
        );
        assert!(validate_work_metadata_package(&package).is_ok());
    }

    #[test]
    fn empty_work_view_is_not_packaged() {
        let view = WorkMetadataView::empty();

        assert!(WorkMetadataPackage::from_view(&view).is_none());
    }

    #[test]
    fn work_package_validation_rejects_invalid_assignment_kind() {
        let package = WorkMetadataPackage {
            task_type: None,
            status: None,
            start_at: None,
            end_at: None,
            estimate_seconds: Some(1),
            completion_notes: None,
            metadata: json!({}),
            assignments: vec![WorkAssignmentPackage {
                assignment_kind: "owner".to_string(),
                subject_kind: "external_actor".to_string(),
                app_user_id: None,
                organ_id: None,
                transfer_party_id: None,
                remote_base_url: None,
                remote_public_key: None,
                remote_subject_uid: None,
                display_name: Some("Remote User".to_string()),
                organ_name: None,
            }],
        };

        assert!(validate_work_metadata_package(&package).is_err());
    }

    #[tokio::test]
    async fn widget_actions_persist_work_metadata_in_snapshot_and_package() {
        let _guard = SERVICE_TEST_LOCK.lock().expect("service test lock");
        let service = test_service().await;
        let instance_id = "transfer-test";

        let response = service
            .action(
                None,
                instance_id,
                "create-proposal",
                json!({
                    "title": "Work metadata proposal",
                    "role": "contribution",
                    "recordId": 1,
                    "quantity": 1,
                    "counterpartyLabel": "Remote Organ",
                    "targetOrganId": null,
                    "parentTransferId": null
                }),
            )
            .await
            .expect("create proposal");
        let transfer_id = response["snapshot"]["transfers"][0]["id"]
            .as_i64()
            .expect("transfer id");
        let item_id = insert_test_structured_item(&service, transfer_id).await;
        let interaction_id = insert_test_interaction(&service, transfer_id, item_id).await;

        service
            .action(
                None,
                instance_id,
                "update-transfer-work",
                json!({
                    "transferId": transfer_id,
                    "work": {
                        "startAt": "2026-06-18T10:00:00Z",
                        "endAt": null,
                        "estimateSeconds": 3600,
                        "completionNotes": "Transfer note",
                        "assignees": [
                            { "kind": "appUser", "appUserId": 1 },
                            {
                                "kind": "externalActor",
                                "displayName": "Remote User",
                                "organName": "Remote Organ",
                                "remoteBaseUrl": "https://remote.example",
                                "remoteSubjectUid": "remote-user-1",
                                "remotePublicKey": "remote-public-key"
                            }
                        ]
                    }
                }),
            )
            .await
            .expect("update transfer work");
        service
            .action(
                None,
                instance_id,
                "update-transfer-item-work",
                json!({
                    "transferId": transfer_id,
                    "structuredItemId": item_id,
                    "work": {
                        "startAt": null,
                        "endAt": "2026-06-18T12:00:00Z",
                        "estimateSeconds": 1200,
                        "completionNotes": "Item note",
                        "assignees": []
                    }
                }),
            )
            .await
            .expect("update item work");
        let response = service
            .action(
                None,
                instance_id,
                "update-transfer-interaction-work",
                json!({
                    "transferId": transfer_id,
                    "interactionId": interaction_id,
                    "work": {
                        "startAt": null,
                        "endAt": null,
                        "estimateSeconds": 600,
                        "completionNotes": "Interaction note",
                        "assignees": []
                    }
                }),
            )
            .await
            .expect("update interaction work");

        let transfer = response["snapshot"]["transfers"]
            .as_array()
            .expect("transfers")
            .iter()
            .find(|transfer| transfer["id"].as_i64() == Some(transfer_id))
            .expect("transfer snapshot");
        assert_eq!(
            transfer["work"]["completionNotes"].as_str(),
            Some("Transfer note")
        );
        let item = transfer["items"]
            .as_array()
            .expect("items")
            .iter()
            .find(|item| item["id"].as_i64() == Some(item_id))
            .expect("item snapshot");
        let interaction = transfer["interactions"]
            .as_array()
            .expect("interactions")
            .iter()
            .find(|interaction| interaction["id"].as_i64() == Some(interaction_id))
            .expect("interaction snapshot");
        assert_eq!(item["work"]["completionNotes"].as_str(), Some("Item note"));
        assert_eq!(
            interaction["work"]["completionNotes"].as_str(),
            Some("Interaction note")
        );
        assert_eq!(
            transfer["work"]["assignments"]
                .as_array()
                .expect("assignments")
                .len(),
            2
        );

        let package = service
            .build_transfer_package(transfer_id)
            .await
            .expect("package");
        assert_eq!(
            package
                .work
                .as_ref()
                .and_then(|work| work.completion_notes.as_deref()),
            Some("Transfer note")
        );
        assert_eq!(package.item_work.len(), 1);
        assert_eq!(package.interaction_work.len(), 1);
        assert_eq!(
            package.interaction_work[0].work.completion_notes.as_deref(),
            Some("Interaction note")
        );

        let stable_subject_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM work_subject
             WHERE remote_base_url = 'https://remote.example'
               AND remote_subject_uid = 'remote-user-1'",
        )
        .fetch_one(&*service.services.db)
        .await
        .expect("stable subject count");
        assert_eq!(stable_subject_count, 1);
    }

    #[tokio::test]
    async fn structured_package_import_updates_by_uid_without_duplicates() {
        let _guard = SERVICE_TEST_LOCK.lock().expect("service test lock");
        let service = test_service().await;
        let instance_id = "transfer-test";

        let response = service
            .action(
                None,
                instance_id,
                "create-proposal",
                json!({
                    "title": "Structured package proposal",
                    "role": "contribution",
                    "recordId": 1,
                    "quantity": 1,
                    "counterpartyLabel": "Remote Organ",
                    "targetOrganId": null,
                    "parentTransferId": null
                }),
            )
            .await
            .expect("create proposal");
        let transfer_id = response["snapshot"]["transfers"][0]["id"]
            .as_i64()
            .expect("transfer id");

        let first_package = StructuredTransferPackage {
            parties: vec![TransferPartyPackage {
                party_uid: Some("party-remote-1".to_string()),
                participation_kind: "participant".to_string(),
                role_hint: Some("contribution".to_string()),
                actor_label: "Remote A".to_string(),
                public_key: None,
                placeholder: 1,
            }],
            items: vec![StructuredItemPackage {
                item_uid: Some("item-remote-1".to_string()),
                role: "contribution".to_string(),
                source_record_id: None,
                title: "Remote item A".to_string(),
                description: None,
                record_head_snapshot: None,
                record_body_snapshot: None,
                quantity: Some(1.0),
                unit: None,
                location: None,
                metadata_json: "{}".to_string(),
                version: Some(1),
            }],
            interactions: vec![StructuredInteractionPackage {
                interaction_uid: Some("interaction-remote-1".to_string()),
                interaction_kind: "depends_on".to_string(),
                direction: "outgoing".to_string(),
                quantity: Some(1.0),
                state: "open".to_string(),
                dependency_kind: Some("must_agree".to_string()),
                metadata_json: "{}".to_string(),
                version: Some(1),
            }],
            ..StructuredTransferPackage::default()
        };
        service
            .upsert_structured_transfer_package(transfer_id, &first_package)
            .await
            .expect("first import");

        let changed_package = StructuredTransferPackage {
            parties: vec![TransferPartyPackage {
                actor_label: "Remote B".to_string(),
                ..first_package.parties[0].clone()
            }],
            items: vec![StructuredItemPackage {
                title: "Remote item B".to_string(),
                quantity: Some(2.0),
                version: Some(2),
                ..first_package.items[0].clone()
            }],
            interactions: vec![StructuredInteractionPackage {
                state: "active".to_string(),
                quantity: Some(2.0),
                version: Some(2),
                ..first_package.interactions[0].clone()
            }],
            ..StructuredTransferPackage::default()
        };
        service
            .upsert_structured_transfer_package(transfer_id, &changed_package)
            .await
            .expect("changed import");

        let party_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_party
             WHERE transfer_id = ? AND party_uid = 'party-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("party count");
        let item_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_structured_item
             WHERE transfer_id = ? AND item_uid = 'item-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("item count");
        let interaction_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1)
             FROM transfer_interaction
             WHERE transfer_id = ? AND interaction_uid = 'interaction-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("interaction count");
        assert_eq!(party_count, 1);
        assert_eq!(item_count, 1);
        assert_eq!(interaction_count, 1);

        let actor_label = sqlx::query_scalar::<_, String>(
            "SELECT actor_label
             FROM transfer_party
             WHERE transfer_id = ? AND party_uid = 'party-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("actor label");
        let item_title = sqlx::query_scalar::<_, String>(
            "SELECT title
             FROM transfer_structured_item
             WHERE transfer_id = ? AND item_uid = 'item-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("item title");
        let interaction_state = sqlx::query_scalar::<_, String>(
            "SELECT state
             FROM transfer_interaction
             WHERE transfer_id = ? AND interaction_uid = 'interaction-remote-1'",
        )
        .bind(transfer_id)
        .fetch_one(&*service.services.db)
        .await
        .expect("interaction state");
        assert_eq!(actor_label, "Remote B");
        assert_eq!(item_title, "Remote item B");
        assert_eq!(interaction_state, "active");
    }

    async fn test_service() -> TransferWidgetService {
        let dir = SERVICE_TEST_DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join("lince-transfer-widget-service-test");
            let _ = std::fs::remove_dir_all(&dir);
            utils::config::set_lince_data_dir_override(dir.clone()).expect("data dir override");
            dir
        });
        std::fs::create_dir_all(dir).expect("test data dir");

        let db = Arc::new(connection::connection().await.expect("db connection"));
        bootstrap_database(&db, "http://127.0.0.1:6174")
            .await
            .expect("bootstrap db");
        seed_test_user(&db).await;
        let writer = persistence::write_coordinator::spawn_write_coordinator()
            .await
            .expect("writer");
        let storage = Arc::new(StorageService::from_database(&db).await.expect("storage"));
        let services = dependency_injection(db.clone(), storage, writer.clone());
        let board_state = BoardStateStore::new().expect("board state");
        board_state
            .replace(test_board_state())
            .await
            .expect("replace board state");
        let organs = OrganStore::new(db, writer);
        let auth = AppAuth::with_shared_remote_tokens(services.remote_organ_auth.clone());
        let manas = ManasGateway::new().expect("manas");

        TransferWidgetService::new(
            auth,
            board_state,
            false,
            "http://127.0.0.1:6174".to_string(),
            manas,
            organs,
            services,
        )
    }

    fn test_board_state() -> BoardState {
        BoardState {
            density: 4,
            global_streams_enabled: true,
            world: crate::domain::board::default_world(),
            active_workspace_id: "space-1".to_string(),
            workspaces: vec![BoardWorkspace {
                id: "space-1".to_string(),
                name: "Test".to_string(),
                camera: crate::domain::board::default_camera(),
                cards: vec![BoardCard {
                    id: "transfer-test".to_string(),
                    kind: "widget".to_string(),
                    title: "Transfer".to_string(),
                    description: String::new(),
                    text: String::new(),
                    html: String::new(),
                    author: "Lince".to_string(),
                    permissions: vec![],
                    package_name: "transfer.lince".to_string(),
                    requires_server: false,
                    server_id: String::new(),
                    view_id: None,
                    streams_enabled: true,
                    widget_state: json!({}),
                    x: 49_000.0,
                    y: 49_000.0,
                    width: 720.0,
                    height: 640.0,
                }],
            }],
        }
    }

    async fn seed_test_user(db: &sqlx::Pool<sqlx::Sqlite>) {
        sqlx::query(
            "INSERT INTO app_user(id, name, username, password_hash, role_id)
             VALUES (
                1,
                'Test User',
                'test-user',
                'hash',
                (SELECT id FROM role WHERE name = 'lince' LIMIT 1)
             )
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                username = excluded.username,
                password_hash = excluded.password_hash,
                role_id = excluded.role_id",
        )
        .execute(db)
        .await
        .expect("seed app user");
    }

    async fn insert_test_structured_item(service: &TransferWidgetService, transfer_id: i64) -> i64 {
        service
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_structured_item(
                    transfer_id,
                    role,
                    source_record_id,
                    title,
                    quantity,
                    version
                 ) VALUES (?, 'contribution', 1, 'Structured item', 1, 1)
                 RETURNING id"
                    .to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await
            .expect("insert structured item")
            .last_insert_rowid
            .expect("structured item id")
    }

    async fn insert_test_interaction(
        service: &TransferWidgetService,
        transfer_id: i64,
        item_id: i64,
    ) -> i64 {
        service
            .services
            .writer
            .execute_statement_returning_id(
                "INSERT INTO transfer_interaction(
                    transfer_id,
                    interaction_kind,
                    direction,
                    from_item_id,
                    to_item_id,
                    state,
                    version
                 ) VALUES (?, 'depends_on', 'outgoing', ?, ?, 'open', 1)
                 RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(item_id),
                    SqlParameter::Integer(item_id),
                ],
            )
            .await
            .expect("insert interaction")
            .last_insert_rowid
            .expect("interaction id")
    }
}
