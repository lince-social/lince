use {
    crate::{
        application::transfer_identity::is_supported_transfer_package_filename,
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, organ_requires_auth},
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
                "duplicate-proposal",
                "sign-agreement",
                "confirm-delivery",
                "confirm-receipt",
                "settle-local",
                "inactivate-transfer",
                "delete-transfer",
                "post-transfer",
                "import-package",
                "set-ingress-policy",
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
                                base_url: Some(target_base_url),
                            },
                        )
                        .await
                    {
                        Ok(()) => {
                            format!("Proposal duplicated into Transfer #{transfer_id} and posted.")
                        }
                        Err(error) => {
                            let error_message = error.message();
                            format!(
                                "Proposal duplicated into Transfer #{transfer_id}, but posting failed: {error_message}"
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
            "post-transfer" => {
                let request = parse_payload::<PostTransferRequest>(payload)?;
                self.post_transfer_package(session_token, request).await?;
                "Transfer package posted to the selected Organ.".to_string()
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
        if !self
            .transfer_public_proposals_enabled()
            .await
            .map_err(TransferWidgetError::from_io)?
        {
            return Err(TransferWidgetError::Invalid(
                "This node is not accepting public Transfer proposals.".into(),
            ));
        }
        let package = parse_transfer_package_value(value)?;
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
    ) -> Result<Value, TransferWidgetError> {
        let packages = self
            .load_transfer_packages_since(since)
            .await
            .map_err(TransferWidgetError::from_io)?;
        Ok(json!({
            "ok": true,
            "packages": packages,
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
        let public_proposals_enabled = self
            .transfer_public_proposals_enabled()
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(json!({
            "localIdentity": LocalIdentityView::from(local_identity),
            "ingressPolicy": {
                "publicProposalsEnabled": public_proposals_enabled,
                "copy": "When enabled, this node accepts unauthenticated initial proposal packages only. Replies still require login or manual import."
            },
            "records": records,
            "organs": organs,
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
        })
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.insert_transfer_item(transfer_id, &contribution, &need, target_organ.as_ref())
            .await
            .map_err(TransferWidgetError::from_io)?;

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
                "target_organ_name": target_organ.as_ref().map(|organ| organ.name.clone())
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

        Ok(transfer_id)
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
        })
        .await
        .map_err(TransferWidgetError::from_io)?;
        self.insert_transfer_item(transfer_id, &contribution, &need, None)
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

        let (id_column, head_column, quantity_column) = match role {
            TransferSide::Contribution => (
                "contribution_id",
                "contribution_head",
                "contribution_quantity",
            ),
            TransferSide::Need => ("need_id", "need_head", "need_quantity"),
        };

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

        self.services
            .writer
            .execute_statement(
                format!(
                    "UPDATE transfer_item
                     SET {id_column} = ?,
                         {head_column} = ?,
                         {quantity_column} = ?
                     WHERE transfer_id = ?"
                ),
                vec![
                    SqlParameter::Integer(record.id),
                    SqlParameter::Text(item_title.clone()),
                    SqlParameter::Real(quantity),
                    SqlParameter::Integer(transfer.id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;

        let identity = self
            .load_transfer_identity_by_id(transfer.id)
            .await
            .map_err(TransferWidgetError::from_io)?;
        self.append_signed_event(
            &identity,
            &local_identity,
            EventKind::ItemCreated,
            json!({
                "event_type": "item_updated",
                "role": role.as_str(),
                "title": title,
                "record": record,
                "item_title": item_title,
                "quantity": quantity
            }),
        )
        .await
        .map_err(TransferWidgetError::from_io)?;

        Ok(())
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
        let column = role.agreement_column();
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
        self.services
            .writer
            .execute_statement(
                format!("UPDATE transfer_item SET {column} = ? WHERE transfer_id = ?"),
                vec![
                    SqlParameter::Integer(next_agreement),
                    SqlParameter::Integer(transfer.id),
                ],
            )
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
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_item
                 SET first_agreement = 0,
                     second_agreement = 0
                 WHERE transfer_id = ?"
                    .to_string(),
                vec![SqlParameter::Integer(transfer.id)],
            )
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
            .event_exists(transfer.id, EventKind::DeliveryConfirmed)
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
        if !agreements_complete(&transfer) {
            return Err(TransferWidgetError::Invalid(
                "Both parties must sign agreement before settlement.".into(),
            ));
        }
        if !self
            .event_exists(transfer.id, EventKind::DeliveryConfirmed)
            .await
            .map_err(TransferWidgetError::from_io)?
            || !self
                .event_exists(transfer.id, EventKind::ReceiptConfirmed)
                .await
                .map_err(TransferWidgetError::from_io)?
        {
            return Err(TransferWidgetError::Invalid(
                "Delivery and receipt signatures are required before settlement.".into(),
            ));
        }
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
                &local_identity,
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
                    SqlParameter::Text(local_identity.label),
                    SqlParameter::Real(delta),
                    SqlParameter::Integer(event_id),
                ],
            )
            .await
            .map_err(TransferWidgetError::from_io)?;

        Ok(())
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

    async fn receive_transfer_package(
        &self,
        package: TransferPackage,
    ) -> Result<TransferImportOutcome, TransferWidgetError> {
        if package.version != PACKAGE_VERSION {
            return Err(TransferWidgetError::Invalid(
                "Unsupported Transfer package version.".into(),
            ));
        }
        validate_package(&package)?;

        let existing_id = self
            .find_transfer_id_by_uid(&package.identity.transfer_uid)
            .await
            .map_err(TransferWidgetError::from_io)?;
        let transfer_id = match existing_id {
            Some(transfer_id) => {
                self.update_transfer_identity(transfer_id, &package.identity)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                self.upsert_transfer_item(transfer_id, &package.item)
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
                self.insert_packaged_transfer_item(transfer_id, &package.item)
                    .await
                    .map_err(TransferWidgetError::from_io)?;
                transfer_id
            }
        };

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

        Ok(TransferImportOutcome {
            transfer_id,
            events_imported,
        })
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
                    source_base_url
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                    source_base_url
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
                    SqlParameter::Integer(transfer_id),
                ],
            )
            .await?;
        Ok(())
    }

    async fn insert_transfer_item(
        &self,
        transfer_id: i64,
        contribution: &TransferSideInput,
        need: &TransferSideInput,
        organ: Option<&Organ>,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_item(
                    transfer_id,
                    contribution_user_id,
                    contribution_server_id,
                    contribution_id,
                    contribution_head,
                    contribution_quantity,
                    need_user_id,
                    need_server_id,
                    need_id,
                    need_head,
                    need_quantity,
                    location
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Integer(0),
                    SqlParameter::Integer(organ.map(|value| value.id).unwrap_or(0)),
                    SqlParameter::Integer(contribution.record_id),
                    SqlParameter::Text(contribution.head.clone()),
                    SqlParameter::Real(contribution.quantity.abs()),
                    SqlParameter::Integer(0),
                    SqlParameter::Integer(organ.map(|value| value.id).unwrap_or(0)),
                    SqlParameter::Integer(need.record_id),
                    SqlParameter::Text(need.head.clone()),
                    SqlParameter::Real(need.quantity.abs()),
                    SqlParameter::Text(
                        organ
                            .map(|value| value.base_url.clone())
                            .unwrap_or_else(|| COORDINATOR_LABEL.to_string()),
                    ),
                ],
            )
            .await?;
        Ok(())
    }

    async fn insert_packaged_transfer_item(
        &self,
        transfer_id: i64,
        item: &TransferItemPackage,
    ) -> Result<(), Error> {
        self.services
            .writer
            .execute_statement(
                "INSERT INTO transfer_item(
                    transfer_id,
                    contribution_user_id,
                    contribution_server_id,
                    contribution_id,
                    contribution_head,
                    contribution_quantity,
                    need_user_id,
                    need_server_id,
                    need_id,
                    need_head,
                    need_quantity,
                    first_agreement,
                    second_agreement,
                    location
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
                    .to_string(),
                item.insert_params(transfer_id),
            )
            .await?;
        Ok(())
    }

    async fn upsert_transfer_item(
        &self,
        transfer_id: i64,
        item: &TransferItemPackage,
    ) -> Result<(), Error> {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM transfer_item WHERE transfer_id = ?",
        )
        .bind(transfer_id)
        .fetch_one(&*self.services.db)
        .await
        .map_err(Error::other)?;
        if count == 0 {
            return self.insert_packaged_transfer_item(transfer_id, item).await;
        }
        let mut params = item.update_params();
        params.push(SqlParameter::Integer(transfer_id));
        self.services
            .writer
            .execute_statement(
                "UPDATE transfer_item
                 SET contribution_user_id = ?,
                     contribution_server_id = ?,
                     contribution_id = ?,
                     contribution_head = ?,
                     contribution_quantity = ?,
                     need_user_id = ?,
                     need_server_id = ?,
                     need_id = ?,
                     need_head = ?,
                     need_quantity = ?,
                     first_agreement = MAX(first_agreement, ?),
                     second_agreement = MAX(second_agreement, ?),
                     location = ?
                 WHERE transfer_id = ?"
                    .to_string(),
                params,
            )
            .await?;
        Ok(())
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
        let mut views = Vec::with_capacity(transfers.len());
        for transfer in transfers {
            let events = self.load_transfer_events(transfer.id).await?;
            let cursors = self.load_transfer_cursors(transfer.id).await?;
            let settlements = self.load_transfer_settlements(transfer.id).await?;
            views.push(TransferView::from_rows(
                transfer,
                events,
                cursors,
                settlements,
                local_identity,
            ));
        }
        Ok(views)
    }

    async fn build_transfer_package(&self, transfer_id: i64) -> Result<TransferPackage, Error> {
        let transfer = self.load_transfer_summary(transfer_id).await?;
        let events = self.load_transfer_events(transfer_id).await?;
        Ok(TransferPackage {
            version: PACKAGE_VERSION,
            identity: TransferIdentityPackage::from(&transfer),
            item: TransferItemPackage::from(&transfer),
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
                 WHERE ident.updated_at >= ? OR event.created_at >= ?
                 ORDER BY t.id DESC
                 LIMIT 100",
            )
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
            "SELECT id, transfer_id, target_base_url, attempts, last_error, last_attempt_at
             FROM transfer_sync_outbox
             ORDER BY updated_at, id
             LIMIT 25",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;

        for row in rows {
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
        let outcome = self
            .services
            .writer
            .execute_statement(
                "DELETE FROM transfer_item WHERE transfer_id = ?".to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        let transfer_outcome = self
            .services
            .writer
            .execute_statement(
                "DELETE FROM transfer WHERE id = ?".to_string(),
                vec![SqlParameter::Integer(transfer_id)],
            )
            .await?;
        if outcome.rows_affected == 0 && transfer_outcome.rows_affected == 0 {
            return Err(Error::new(ErrorKind::NotFound, "Transfer not found"));
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
        if let Err(error) = self.flush_transfer_sync_outbox().await {
            tracing::warn!("transfer startup outbox flush failed: {error}");
        }
    }

    async fn pull_recent_transfer_packages(&self, since: &str) -> Result<(), Error> {
        let targets = self.known_transfer_sync_targets().await?;
        let path = format!(
            "/transfer/packages/since?since={}",
            encode_query_value(since)
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

    async fn known_transfer_sync_targets(&self) -> Result<Vec<String>, Error> {
        let mut targets = self
            .organs
            .list()
            .await
            .map_err(Error::other)?
            .into_iter()
            .map(|organ| organ.base_url)
            .collect::<Vec<_>>();
        let transfer_targets = sqlx::query_as::<_, TransferSyncTargetRow>(
            "SELECT source_base_url, target_base_url FROM transfer_identity",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        for row in transfer_targets {
            if let Some(base_url) = normalize_optional_text(row.source_base_url) {
                targets.push(base_url);
            }
            if let Some(base_url) = normalize_optional_text(row.target_base_url) {
                targets.push(base_url);
            }
        }
        let gossip_targets = sqlx::query_as::<_, GossipSyncTargetRow>(
            "SELECT source_base_url, target_base_url, observed_from_base_url
             FROM transfer_gossip_package",
        )
        .fetch_all(&*self.services.db)
        .await
        .map_err(Error::other)?;
        for row in gossip_targets {
            if let Some(base_url) = normalize_optional_text(row.source_base_url) {
                targets.push(base_url);
            }
            if let Some(base_url) = normalize_optional_text(row.target_base_url) {
                targets.push(base_url);
            }
            if let Some(base_url) = normalize_optional_text(row.observed_from_base_url) {
                targets.push(base_url);
            }
        }
        targets.retain(|target| !same_base_url(target, &self.local_base_url));
        targets.sort();
        targets.dedup();
        Ok(targets)
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
                    signature
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"
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
                    SqlParameter::Text(signature),
                ],
            )
            .await?;
        let event_id = outcome
            .last_insert_rowid
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Transfer event returned no id"))?;
        self.update_sync_cursor(identity.transfer_id, &local_identity.label, event_id)
            .await?;
        self.enqueue_transfer_sync(identity.transfer_id).await?;
        self.flush_transfer_sync_outbox().await?;
        Ok(event_id)
    }

    async fn insert_packaged_event(
        &self,
        transfer_id: i64,
        event: &TransferEventPackage,
    ) -> Result<i64, Error> {
        let previous_event_id = if let Some(previous_uid) = event.previous_event_uid.as_deref() {
            sqlx::query_scalar::<_, i64>(
                "SELECT id FROM transfer_event WHERE event_uid = ? LIMIT 1",
            )
            .bind(previous_uid)
            .fetch_optional(&*self.services.db)
            .await
            .map_err(Error::other)?
        } else {
            None
        };
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
                    signature,
                    created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id"
                    .to_string(),
                vec![
                    SqlParameter::Integer(transfer_id),
                    SqlParameter::Text(event.transfer_uid.clone()),
                    SqlParameter::Text(event.event_uid.clone()),
                    SqlParameter::Text(event.actor_label.clone()),
                    SqlParameter::Text(event.actor_public_key.clone()),
                    SqlParameter::Text(event.event_kind.clone()),
                    SqlParameter::Text(event.payload_json.clone()),
                    previous_event_id
                        .map(SqlParameter::Integer)
                        .unwrap_or(SqlParameter::Null),
                    optional_text_parameter(event.previous_event_uid.clone()),
                    SqlParameter::Text(event.signature.clone()),
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
            "SELECT id, event_uid
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
                signature,
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
    fn as_str(self) -> &'static str {
        match self {
            Self::Contribution => "contribution",
            Self::Need => "need",
        }
    }

    fn agreement_column(self) -> &'static str {
        match self {
            Self::Contribution => "first_agreement",
            Self::Need => "second_agreement",
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

#[derive(Debug, Clone, Copy)]
enum EventKind {
    ProposalCreated,
    ProposalDuplicated,
    ItemCreated,
    AgreementSigned,
    DeliveryConfirmed,
    ReceiptConfirmed,
    SettlementApplied,
    TransferInactivated,
}

impl EventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProposalCreated | Self::ProposalDuplicated => "transfer_created",
            Self::ItemCreated => "item_created",
            Self::AgreementSigned => "agreement_changed",
            Self::DeliveryConfirmed => "delivery_confirmed",
            Self::ReceiptConfirmed => "receipt_confirmed",
            Self::SettlementApplied => "settlement_applied",
            Self::TransferInactivated => "transfer_inactivated",
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
struct ImportPackageRequest {
    package: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetIngressPolicyRequest {
    public_proposals_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganOption {
    id: i64,
    name: String,
    base_url: String,
    requires_auth: bool,
    authenticated: bool,
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
    created_at: String,
    updated_at: String,
    contribution_user_id: i64,
    contribution_server_id: i64,
    contribution_id: i64,
    contribution_head: String,
    contribution_quantity: f64,
    need_user_id: i64,
    need_server_id: i64,
    need_id: i64,
    need_head: String,
    need_quantity: f64,
    first_agreement: i64,
    second_agreement: i64,
    location: String,
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
    signature: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone, Default, FromRow)]
struct EventPointer {
    id: Option<i64>,
    event_uid: Option<String>,
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
struct TransferSyncTargetRow {
    source_base_url: Option<String>,
    target_base_url: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct GossipSyncTargetRow {
    source_base_url: Option<String>,
    target_base_url: Option<String>,
    observed_from_base_url: Option<String>,
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
            contribution: TransferSideView {
                actor_label: identity.contribution_actor_label.clone(),
                public_key: identity.contribution_public_key.clone(),
                record_id: package.item.contribution_id,
                head: package.item.contribution_head.clone(),
                quantity: package.item.contribution_quantity.abs(),
            },
            need: TransferSideView {
                actor_label: identity.need_actor_label.clone(),
                public_key: identity.need_public_key.clone(),
                record_id: package.item.need_id,
                head: package.item.need_head.clone(),
                quantity: package.item.need_quantity.abs(),
            },
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
    controls: ControlView,
    events: Vec<EventView>,
    sync_cursors: Vec<CursorRow>,
    settlements: Vec<SettlementRow>,
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
        let can_inactivate = local_role.is_some() && !inactive;
        let event_views = events.iter().map(EventView::from).collect::<Vec<_>>();
        let package = TransferPackage {
            version: PACKAGE_VERSION,
            identity: TransferIdentityPackage::from(&transfer),
            item: TransferItemPackage::from(&transfer),
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
            controls: ControlView {
                can_duplicate,
                can_sign_agreement,
                can_confirm_delivery,
                can_confirm_receipt,
                can_settle_local,
                can_inactivate,
            },
            events: event_views,
            sync_cursors: cursors,
            settlements,
            package,
            created_at: sql_to_iso8601(&transfer.created_at),
            updated_at: sql_to_iso8601(&transfer.updated_at),
        }
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
    signature: Option<String>,
    signature_valid: bool,
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
            signature: row.signature.clone(),
            signature_valid: verify_event_row(row),
            created_at: row.created_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferPackage {
    version: u32,
    identity: TransferIdentityPackage,
    item: TransferItemPackage,
    events: Vec<TransferEventPackage>,
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferItemPackage {
    contribution_user_id: i64,
    contribution_server_id: i64,
    contribution_id: i64,
    contribution_head: String,
    contribution_quantity: f64,
    need_user_id: i64,
    need_server_id: i64,
    need_id: i64,
    need_head: String,
    need_quantity: f64,
    first_agreement: i64,
    second_agreement: i64,
    location: String,
}

impl TransferItemPackage {
    fn insert_params(&self, transfer_id: i64) -> Vec<SqlParameter> {
        let mut params = vec![SqlParameter::Integer(transfer_id)];
        params.extend(self.update_params());
        params
    }

    fn update_params(&self) -> Vec<SqlParameter> {
        vec![
            SqlParameter::Integer(self.contribution_user_id),
            SqlParameter::Integer(self.contribution_server_id),
            SqlParameter::Integer(self.contribution_id),
            SqlParameter::Text(self.contribution_head.clone()),
            SqlParameter::Real(self.contribution_quantity.abs()),
            SqlParameter::Integer(self.need_user_id),
            SqlParameter::Integer(self.need_server_id),
            SqlParameter::Integer(self.need_id),
            SqlParameter::Text(self.need_head.clone()),
            SqlParameter::Real(self.need_quantity.abs()),
            SqlParameter::Integer(self.first_agreement),
            SqlParameter::Integer(self.second_agreement),
            SqlParameter::Text(self.location.clone()),
        ]
    }
}

impl From<&TransferSummaryRow> for TransferItemPackage {
    fn from(row: &TransferSummaryRow) -> Self {
        Self {
            contribution_user_id: row.contribution_user_id,
            contribution_server_id: row.contribution_server_id,
            contribution_id: row.contribution_id,
            contribution_head: row.contribution_head.clone(),
            contribution_quantity: row.contribution_quantity.abs(),
            need_user_id: row.need_user_id,
            need_server_id: row.need_server_id,
            need_id: row.need_id,
            need_head: row.need_head.clone(),
            need_quantity: row.need_quantity.abs(),
            first_agreement: row.first_agreement,
            second_agreement: row.second_agreement,
            location: row.location.clone(),
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
            "Invalid Transfer package: /transfer/packages received a create-proposal action payload. Send widget action create-proposal to /host/widgets/{instance_id}/actions/create-proposal, or send a Transfer package with version, identity, item, and events to /transfer/packages.".into(),
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

fn optional_i64_parameter(value: Option<i64>) -> SqlParameter {
    value
        .map(SqlParameter::Integer)
        .unwrap_or(SqlParameter::Null)
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

fn validate_package(package: &TransferPackage) -> Result<(), TransferWidgetError> {
    normalize_package_identity(&package.identity)?;
    if package.events.is_empty() {
        return Err(TransferWidgetError::Invalid(
            "Transfer package has no signed events.".into(),
        ));
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
    if package.item.first_agreement != 0 || package.item.second_agreement != 0 {
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

fn transfer_summary_sql(tail: &str) -> String {
    format!(
        "SELECT
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
            ident.created_at,
            ident.updated_at,
            ti.contribution_user_id,
            ti.contribution_server_id,
            ti.contribution_id,
            ti.contribution_head,
            ti.contribution_quantity,
            ti.need_user_id,
            ti.need_server_id,
            ti.need_id,
            ti.need_head,
            ti.need_quantity,
            ti.first_agreement,
            ti.second_agreement,
            ti.location
         FROM transfer t
         JOIN transfer_identity ident ON ident.transfer_id = t.id
         JOIN transfer_item ti ON ti.transfer_id = t.id
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
