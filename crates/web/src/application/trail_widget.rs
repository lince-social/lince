use {
    crate::{
        application::{
            backend_api::BackendApiService, trail_identity::is_supported_trail_package_filename,
        },
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, organ_requires_auth},
        },
    },
    ::application::{auth::AuthSubject, subscription::SubscriptionHandle},
    reqwest::Method,
    serde::{Deserialize, Serialize},
    serde_json::{Map, Number, Value, json},
    urlencoding::encode,
};

const TRAIL_RUNTIME_STATE_KEY: &str = "trail_runtime";
const TRAIL_DERIVED_VIEW_NAME_PREFIX: &str = "__lince_web_trail_";

#[derive(Clone)]
pub struct TrailWidgetService {
    auth: AppAuth,
    backend: BackendApiService,
    board_state: BoardStateStore,
    local_auth_required: bool,
    manas: ManasGateway,
    organs: OrganStore,
}

impl TrailWidgetService {
    pub fn new(
        auth: AppAuth,
        backend: BackendApiService,
        board_state: BoardStateStore,
        local_auth_required: bool,
        manas: ManasGateway,
        organs: OrganStore,
    ) -> Self {
        Self {
            auth,
            backend,
            board_state,
            local_auth_required,
            manas,
            organs,
        }
    }

    pub async fn contract(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Read)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let effective_view_id = runtime
            .derived_view_id
            .or_else(|| resolved.card.view_id.map(i64::from));
        let snapshot = if let Some(view_id) = effective_view_id {
            self.load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                u32::try_from(view_id).map_err(|_| {
                    TrailWidgetError::Internal("view_id invalido no binding da trail.".into())
                })?,
            )
            .await
            .ok()
        } else {
            None
        };
        let trail_root_record_id = runtime
            .trail_root_record_id
            .or_else(|| snapshot.as_ref().and_then(snapshot_trail_root_record_id));
        if runtime.trail_root_record_id.is_none()
            && runtime.derived_view_id.is_none()
            && let (Some(root_id), Some(view_id)) = (trail_root_record_id, effective_view_id)
        {
            let _ = self
                .persist_runtime_state(&resolved.card.id, &resolved.organ.id, root_id, view_id)
                .await;
        }
        let sync = if let Some(root_id) = trail_root_record_id {
            self.load_trail_sync_metadata(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                root_id,
            )
            .await?
        } else {
            None
        };

        Ok(json!({
            "widget": {
                "instanceId": resolved.card.id,
                "title": resolved.card.title,
                "description": resolved.card.description,
                "packageName": resolved.card.package_name,
            },
            "source": {
                "serverId": resolved.organ.id,
                "serverName": resolved.organ.name,
                "requiresAuth": resolved.requires_auth,
                "authenticated": resolved.bearer_token.is_some() || resolved.is_local,
                "streamsEnabled": resolved.card.streams_enabled && resolved.global_streams_enabled,
            },
            "permissions": {
                "declared": resolved.card.permissions,
                "readViewStream": has_permission(&resolved.card, "read_view_stream"),
                "writeRecords": has_permission(&resolved.card, "write_records"),
                "writeTable": has_permission(&resolved.card, "write_table"),
            },
            "binding": {
                "trailRootRecordId": trail_root_record_id,
                "viewId": effective_view_id,
                "sync": sync,
                "snapshot": snapshot,
            },
            "dataContract": {
                "requiredColumns": ["id", "quantity", "head", "body"],
                "optionalColumns": [
                    "depth",
                    "primary_category",
                    "categories_json",
                    "assignee_ids_json",
                    "assignee_names_json",
                    "assignee_usernames_json",
                    "parent_ids_json",
                    "parent_heads_json",
                    "children_count",
                    "children_json",
                    "sync_source_record_id",
                    "trail_root_record_id"
                ]
            },
            "search": {
                "recordEndpoint": "/api/table/record",
                "supportedFilters": ["assignee", "category", "head_contains"],
            },
            "actions": [
                "search-trails",
                "search-assignees",
                "bind-trail",
                "create-trail",
                "run-trail-sync",
                "set-trail-node-quantity",
            ],
            "diagnostics": {
                "trailRootRecordId": trail_root_record_id,
                "viewId": effective_view_id,
            }
        }))
    }

    pub async fn prepare_stream(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<PreparedTrailStream, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Read)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        if !(resolved.card.streams_enabled && resolved.global_streams_enabled) {
            return Err(TrailWidgetError::Disabled(
                "Streams desativados para esse widget.".into(),
            ));
        }
        let view_id = runtime
            .derived_view_id
            .or_else(|| resolved.card.view_id.map(i64::from))
            .ok_or_else(|| {
                TrailWidgetError::Disabled("Trail ainda nao foi vinculado a um root record.".into())
            })?;
        let snapshot = self
            .load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                u32::try_from(view_id).map_err(|_| {
                    TrailWidgetError::Internal("view_id invalido no binding da trail.".into())
                })?,
            )
            .await?;
        let trail_root_record_id = runtime
            .trail_root_record_id
            .or_else(|| snapshot_trail_root_record_id(&snapshot))
            .ok_or_else(|| {
                TrailWidgetError::Disabled("Trail ainda nao foi vinculado a um root record.".into())
            })?;
        if runtime.trail_root_record_id.is_none() && runtime.derived_view_id.is_none() {
            let _ = self
                .persist_runtime_state(
                    &resolved.card.id,
                    &resolved.organ.id,
                    trail_root_record_id,
                    view_id,
                )
                .await;
        }
        let sync = self
            .load_trail_sync_metadata(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                trail_root_record_id,
            )
            .await?;
        let binding = TrailBindingPayload {
            trail_root_record_id,
            view_id,
            sync,
        };

        if resolved.is_local {
            let handle = self
                .backend
                .subscribe_view(local_host_subject(), view_id as u32)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            Ok(PreparedTrailStream::Local { handle, binding })
        } else {
            let bearer_token = resolved
                .bearer_token
                .as_deref()
                .ok_or_else(|| TrailWidgetError::Unauthorized("Sessao remota ausente.".into()))?;
            let response = self
                .manas
                .open_view_stream(&resolved.organ.base_url, bearer_token, view_id as u64)
                .await
                .map_err(TrailWidgetError::BadGateway)?;
            Ok(PreparedTrailStream::Remote { response, binding })
        }
    }

    pub async fn action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, TrailWidgetError> {
        match action {
            "search-trails" => {
                let request =
                    serde_json::from_value::<SearchTrailRequest>(payload).map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.search_trails(session_token, instance_id, request)
                    .await
            }
            "search-assignees" => {
                let request =
                    serde_json::from_value::<SearchAssigneesRequest>(payload).map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.search_assignees(session_token, instance_id, request)
                    .await
            }
            "bind-trail" => {
                let request =
                    serde_json::from_value::<BindTrailRequest>(payload).map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.bind_trail(session_token, instance_id, request).await
            }
            "create-trail" => {
                let request =
                    serde_json::from_value::<CreateTrailRequest>(payload).map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.create_trail(session_token, instance_id, request).await
            }
            "run-trail-sync" => {
                let request =
                    serde_json::from_value::<RunTrailSyncRequest>(payload).map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.run_trail_sync(session_token, instance_id, request)
                    .await
            }
            "set-trail-node-quantity" => {
                let request = serde_json::from_value::<SetTrailNodeQuantityRequest>(payload)
                    .map_err(|error| {
                        TrailWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.set_trail_node_quantity(session_token, instance_id, request)
                    .await
            }
            _ => Err(TrailWidgetError::Invalid(
                "Acao de trail desconhecida.".into(),
            )),
        }
    }

    async fn search_trails(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: SearchTrailRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Read)
            .await?;
        let rows = self
            .list_record_rows_filtered(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                &request,
            )
            .await?;
        Ok(json!({
            "ok": true,
            "action": "search-trails",
            "results": rows,
        }))
    }

    async fn search_assignees(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: SearchAssigneesRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Read)
            .await?;
        let rows = self
            .list_app_users_filtered(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                request.query.as_deref(),
            )
            .await?;
        Ok(json!({
            "ok": true,
            "action": "search-assignees",
            "results": rows,
        }))
    }

    async fn bind_trail(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: BindTrailRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Read)
            .await?;
        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            request.trail_root_record_id,
        )
        .await?;
        let view_id = self
            .ensure_derived_view(session_token, &resolved, request.trail_root_record_id, None)
            .await?;
        self.persist_runtime_state(
            &resolved.card.id,
            &resolved.organ.id,
            request.trail_root_record_id,
            view_id,
        )
        .await?;
        let sync = self
            .load_trail_sync_metadata(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                request.trail_root_record_id,
            )
            .await?;
        let snapshot = self
            .load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                u32::try_from(view_id).map_err(|_| {
                    TrailWidgetError::Internal("view_id invalido ao vincular trail.".into())
                })?,
            )
            .await?;
        Ok(json!({
            "ok": true,
            "action": "bind-trail",
            "message": "Trail vinculado.",
            "detail": {
                "trailRootRecordId": request.trail_root_record_id,
                "viewId": view_id,
                "sync": sync,
                "snapshot": snapshot,
            }
        }))
    }

    async fn create_trail(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: CreateTrailRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Write)
            .await?;
        validate_scope(&request.scope)?;
        validate_fields(&request.fields)?;
        if request.assignee.trim().is_empty() {
            return Err(TrailWidgetError::Invalid(
                "assignee precisa ser informado.".into(),
            ));
        }

        let source = self
            .get_record_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                request.source_record_id,
            )
            .await?;
        let assignee_id = self
            .resolve_app_user_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                &request.assignee,
            )
            .await?;
        let created = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "record",
                json!({
                    "quantity": -1,
                    "head": source.head,
                    "body": source.body,
                }),
            )
            .await?;
        let copied_root_id = created.last_insert_rowid.ok_or_else(|| {
            TrailWidgetError::Internal("Criacao do root copiado nao retornou id.".into())
        })?;

        let source_categories = parse_json_strings(source.categories_json.as_deref());
        let copy_categories = normalize_categories(
            source_categories
                .into_iter()
                .chain(std::iter::once("copy".to_string()))
                .collect(),
        );
        self.sync_categories_extension(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            copied_root_id,
            &copy_categories,
        )
        .await?;
        self.sync_assignees(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            copied_root_id,
            &[assignee_id],
        )
        .await?;

        let sync = self
            .upsert_trail_sync_config(
                session_token,
                &resolved,
                copied_root_id,
                request.source_record_id,
                request.scope.as_deref().unwrap_or("t"),
                request.fields.as_deref().unwrap_or("hb"),
            )
            .await?;
        let view_id = self
            .ensure_derived_view(
                session_token,
                &resolved,
                copied_root_id,
                request.view_name.as_deref(),
            )
            .await?;
        self.execute_karma(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            sync.sync_karma_id.unwrap_or_default(),
        )
        .await?;
        self.persist_runtime_state(
            &resolved.card.id,
            &resolved.organ.id,
            copied_root_id,
            view_id,
        )
        .await?;
        let snapshot = self
            .load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                u32::try_from(view_id).map_err(|_| {
                    TrailWidgetError::Internal("view_id invalido ao criar trail.".into())
                })?,
            )
            .await?;

        Ok(json!({
            "ok": true,
            "action": "create-trail",
            "message": "Trail criado.",
            "record_id": copied_root_id,
            "await_stream_refresh": true,
            "detail": {
                "trailRootRecordId": copied_root_id,
                "viewId": view_id,
                "sync": sync,
                "snapshot": snapshot,
            }
        }))
    }

    async fn run_trail_sync(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: RunTrailSyncRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Write)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let trail_root_record_id = request
            .trail_root_record_id
            .or(runtime.trail_root_record_id)
            .ok_or_else(|| TrailWidgetError::Invalid("Nenhum trail root foi escolhido.".into()))?;
        let existing = self
            .load_trail_sync_metadata(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                trail_root_record_id,
            )
            .await?
            .ok_or_else(|| {
                TrailWidgetError::Invalid("Esse trail ainda nao tem sync configurado.".into())
            })?;
        let scope = request.scope.as_deref().unwrap_or(existing.scope.as_str());
        let fields = request
            .fields
            .as_deref()
            .unwrap_or(existing.fields.as_str());
        validate_scope(&Some(scope.to_string()))?;
        validate_fields(&Some(fields.to_string()))?;
        let sync = self
            .upsert_trail_sync_config(
                session_token,
                &resolved,
                trail_root_record_id,
                existing.sync_source_record_id,
                scope,
                fields,
            )
            .await?;
        self.execute_karma(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            sync.sync_karma_id.unwrap_or_default(),
        )
        .await?;
        let view_id = self
            .ensure_derived_view(session_token, &resolved, trail_root_record_id, None)
            .await?;
        Ok(json!({
            "ok": true,
            "action": "run-trail-sync",
            "await_stream_refresh": true,
            "detail": {
                "trailRootRecordId": trail_root_record_id,
                "viewId": view_id,
                "sync": sync,
            }
        }))
    }

    async fn set_trail_node_quantity(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: SetTrailNodeQuantityRequest,
    ) -> Result<Value, TrailWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, TrailPermission::Write)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let trail_root_record_id = runtime
            .trail_root_record_id
            .ok_or_else(|| TrailWidgetError::Invalid("Nenhum trail root foi escolhido.".into()))?;
        let view_id = match runtime.derived_view_id {
            Some(view_id) => view_id,
            None => {
                self.ensure_derived_view(session_token, &resolved, trail_root_record_id, None)
                    .await?
            }
        };
        let snapshot = self
            .load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                u32::try_from(view_id).map_err(|_| {
                    TrailWidgetError::Internal("view_id invalido ao atualizar quantidade.".into())
                })?,
            )
            .await?;
        let rows = parse_view_snapshot_rows(&snapshot)?;
        let changes = compute_trail_quantity_changes(
            &rows,
            trail_root_record_id,
            request.record_id,
            request.quantity,
        )?;
        for change in &changes {
            self.update_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "record",
                change.record_id,
                json!({
                    "quantity": change.quantity,
                }),
            )
            .await?;
        }
        Ok(json!({
            "ok": true,
            "action": "set-trail-node-quantity",
            "await_stream_refresh": true,
            "detail": {
                "recordId": request.record_id,
                "quantity": request.quantity,
                "changes": changes,
            }
        }))
    }

    async fn resolve_instance(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        permission: TrailPermission,
    ) -> Result<ResolvedTrailInstance, TrailWidgetError> {
        let instance_id = instance_id.trim();
        if instance_id.is_empty() {
            return Err(TrailWidgetError::NotFound(
                "Widget instance ausente.".into(),
            ));
        }
        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            TrailWidgetError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_trail_card(&card)?;
        permission.check(&card)?;

        let server_id = card.server_id.trim().to_string();
        if server_id.is_empty() {
            return Err(TrailWidgetError::Misconfigured(
                "Trail Relation sem server_id configurado.".into(),
            ));
        }

        let organ = self
            .organs
            .get(&server_id)
            .await
            .map_err(TrailWidgetError::Internal)?
            .ok_or_else(|| {
                TrailWidgetError::Misconfigured("O server_id configurado nao existe mais.".into())
            })?;
        let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
        let is_local = !requires_auth;
        let bearer_token = if requires_auth {
            let session = self
                .auth
                .server_session(session_token, &server_id)
                .await
                .ok_or_else(|| {
                    TrailWidgetError::Unauthorized(
                        "Essa sessao local nao esta conectada a esse servidor.".into(),
                    )
                })?;
            Some(session.bearer_token)
        } else {
            None
        };

        Ok(ResolvedTrailInstance {
            card,
            organ,
            bearer_token,
            is_local,
            requires_auth,
            global_streams_enabled: board_state.global_streams_enabled,
        })
    }

    async fn ensure_derived_view(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedTrailInstance,
        trail_root_record_id: i64,
        view_name: Option<&str>,
    ) -> Result<i64, TrailWidgetError> {
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let derived_name = derived_view_name(&resolved.card.id, view_name);
        let query = build_trail_view_query(trail_root_record_id);

        let derived_view_id = match runtime.derived_view_id {
            Some(view_id)
                if runtime.server_id.as_deref() == Some(resolved.organ.id.as_str())
                    && runtime.trail_root_record_id == Some(trail_root_record_id) =>
            {
                if self
                    .load_view_definition(
                        session_token,
                        &resolved.organ,
                        resolved.bearer_token.as_deref(),
                        view_id,
                    )
                    .await
                    .ok()
                    .is_some_and(|view| view.name == derived_name)
                {
                    self.update_view_query(
                        session_token,
                        &resolved.organ,
                        resolved.bearer_token.as_deref(),
                        view_id,
                        &derived_name,
                        &query,
                    )
                    .await?;
                    view_id
                } else {
                    self.create_or_replace_view(
                        session_token,
                        resolved,
                        trail_root_record_id,
                        &derived_name,
                        &query,
                    )
                    .await?
                }
            }
            _ => {
                self.create_or_replace_view(
                    session_token,
                    resolved,
                    trail_root_record_id,
                    &derived_name,
                    &query,
                )
                .await?
            }
        };

        self.persist_runtime_state(
            &resolved.card.id,
            &resolved.organ.id,
            trail_root_record_id,
            derived_view_id,
        )
        .await?;
        Ok(derived_view_id)
    }

    async fn create_or_replace_view(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedTrailInstance,
        trail_root_record_id: i64,
        derived_name: &str,
        query: &str,
    ) -> Result<i64, TrailWidgetError> {
        if let Some(existing_id) = self
            .find_view_id_by_name(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                derived_name,
            )
            .await?
        {
            self.update_view_query(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                existing_id,
                derived_name,
                query,
            )
            .await?;
            return Ok(existing_id);
        }

        self.create_view(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            derived_name,
            query,
        )
        .await?;
        let view_id = self
            .find_view_id_by_name(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                derived_name,
            )
            .await?
            .ok_or_else(|| {
                TrailWidgetError::Internal(format!(
                    "Nao encontrei a view derivada do trail root {trail_root_record_id}."
                ))
            })?;
        Ok(view_id)
    }

    async fn load_view_definition(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: i64,
    ) -> Result<ViewDefinition, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let value = self
                .backend
                .get_table_row(&local_host_subject(), "view", view_id)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return parse_view_definition(&value);
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::GET,
                "view",
                Some(view_id),
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        let value = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        parse_view_definition(&value)
    }

    async fn list_views(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<ViewDefinition>, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let value = self
                .backend
                .list_table_rows(&local_host_subject(), "view")
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return parse_view_list(&value);
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::GET,
                "view",
                None,
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        let value = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        parse_view_list(&value)
    }

    async fn load_view_snapshot(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: u32,
    ) -> Result<Value, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .read_view_snapshot(&local_host_subject(), view_id)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()));
        }
        let response = self
            .manas
            .send_backend_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::GET,
                &format!("/api/view/{view_id}/snapshot"),
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response)
            .await
    }

    async fn find_view_id_by_name(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        name: &str,
    ) -> Result<Option<i64>, TrailWidgetError> {
        let views = self.list_views(session_token, organ, bearer_token).await?;
        Ok(views
            .into_iter()
            .filter(|view| view.name == name)
            .map(|view| view.id)
            .max())
    }

    async fn create_view(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        name: &str,
        query: &str,
    ) -> Result<(), TrailWidgetError> {
        let payload = json!({ "name": name, "query": query });
        self.create_table_row(session_token, organ, bearer_token, "view", payload)
            .await
            .map(|_| ())
    }

    async fn update_view_query(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: i64,
        name: &str,
        query: &str,
    ) -> Result<(), TrailWidgetError> {
        let payload = json!({ "name": name, "query": query });
        self.update_table_row(session_token, organ, bearer_token, "view", view_id, payload)
            .await
            .map(|_| ())
    }

    async fn persist_runtime_state(
        &self,
        instance_id: &str,
        server_id: &str,
        trail_root_record_id: i64,
        derived_view_id: i64,
    ) -> Result<(), TrailWidgetError> {
        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            TrailWidgetError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        let widget_state = ensure_object(&mut card.widget_state);
        let runtime = ensure_nested_object(widget_state, TRAIL_RUNTIME_STATE_KEY);
        runtime.insert("server_id".into(), Value::String(server_id.to_string()));
        runtime.insert(
            "trail_root_record_id".into(),
            Value::Number(Number::from(trail_root_record_id)),
        );
        runtime.insert(
            "derived_view_id".into(),
            Value::Number(Number::from(derived_view_id)),
        );
        self.board_state
            .replace(board_state)
            .await
            .map_err(TrailWidgetError::Internal)?;
        Ok(())
    }

    async fn upsert_trail_sync_config(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedTrailInstance,
        copied_root_id: i64,
        source_record_id: i64,
        scope: &str,
        fields: &str,
    ) -> Result<TrailSyncMetadata, TrailWidgetError> {
        let existing = self
            .load_trail_sync_metadata(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                copied_root_id,
            )
            .await?;
        let condition_token = build_sr_token(scope, fields, source_record_id);
        let consequence_token = build_sr_token(scope, fields, copied_root_id);

        let (condition_id, consequence_id, karma_id) = if let Some(existing) = existing.as_ref() {
            let condition_id = existing.sync_condition_id.ok_or_else(|| {
                TrailWidgetError::Invalid(
                    "Sync condition id ausente na configuracao existente.".into(),
                )
            })?;
            let consequence_id = existing.sync_consequence_id.ok_or_else(|| {
                TrailWidgetError::Invalid(
                    "Sync consequence id ausente na configuracao existente.".into(),
                )
            })?;
            let karma_id = existing.sync_karma_id.ok_or_else(|| {
                TrailWidgetError::Invalid("Sync karma id ausente na configuracao existente.".into())
            })?;
            self.update_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma_condition",
                condition_id,
                json!({
                    "name": format!("Trail source {source_record_id}"),
                    "condition": condition_token,
                }),
            )
            .await?;
            self.update_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma_consequence",
                consequence_id,
                json!({
                    "name": format!("Trail sync {copied_root_id}"),
                    "consequence": consequence_token,
                }),
            )
            .await?;
            self.update_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma",
                karma_id,
                json!({
                    "name": format!("Trail sync {copied_root_id}"),
                    "condition_id": condition_id,
                    "operator": "=",
                    "consequence_id": consequence_id,
                }),
            )
            .await?;
            (condition_id, consequence_id, karma_id)
        } else {
            let condition = self
                .create_table_row(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    "karma_condition",
                    json!({
                        "name": format!("Trail source {source_record_id}"),
                        "condition": condition_token,
                    }),
                )
                .await?;
            let condition_id = condition.last_insert_rowid.ok_or_else(|| {
                TrailWidgetError::Internal("Criacao da condition nao retornou id.".into())
            })?;
            let consequence = self
                .create_table_row(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    "karma_consequence",
                    json!({
                        "name": format!("Trail sync {copied_root_id}"),
                        "consequence": consequence_token,
                    }),
                )
                .await?;
            let consequence_id = consequence.last_insert_rowid.ok_or_else(|| {
                TrailWidgetError::Internal("Criacao da consequence nao retornou id.".into())
            })?;
            let karma = self
                .create_table_row(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    "karma",
                    json!({
                        "name": format!("Trail sync {copied_root_id}"),
                        "condition_id": condition_id,
                        "operator": "=",
                        "consequence_id": consequence_id,
                    }),
                )
                .await?;
            let karma_id = karma.last_insert_rowid.ok_or_else(|| {
                TrailWidgetError::Internal("Criacao da karma nao retornou id.".into())
            })?;
            (condition_id, consequence_id, karma_id)
        };

        let sync = TrailSyncMetadata {
            trail_root_record_id: copied_root_id,
            sync_source_record_id: source_record_id,
            scope: scope.to_string(),
            fields: normalize_fields(fields),
            sync_karma_id: Some(karma_id),
            sync_condition_id: Some(condition_id),
            sync_consequence_id: Some(consequence_id),
            condition_token: Some(build_sr_token(scope, fields, source_record_id)),
            consequence_token: Some(build_sr_token(scope, fields, copied_root_id)),
            operator: Some("=".into()),
        };

        self.sync_extension(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            copied_root_id,
            "trail.sync",
            Some(build_trail_sync_extension_value(&sync)),
        )
        .await?;
        Ok(sync)
    }

    async fn load_trail_sync_metadata(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        trail_root_record_id: i64,
    ) -> Result<Option<TrailSyncMetadata>, TrailWidgetError> {
        let extensions = self
            .list_record_extensions(session_token, organ, bearer_token)
            .await?;
        let Some(extension) = extensions
            .into_iter()
            .find(|row| row.record_id == trail_root_record_id && row.namespace == "trail.sync")
        else {
            return Ok(None);
        };
        let mut sync =
            serde_json::from_str::<TrailSyncMetadata>(&extension.freestyle_data_structure)
                .map_err(|error| {
                    TrailWidgetError::Internal(format!(
                        "trail.sync invalido no record {trail_root_record_id}: {error}"
                    ))
                })?;
        sync.scope = normalize_scope(&sync.scope);
        sync.fields = normalize_fields(&sync.fields);
        Ok(Some(sync))
    }

    async fn list_record_rows_filtered(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        request: &SearchTrailRequest,
    ) -> Result<Vec<RecordSearchRow>, TrailWidgetError> {
        let value = if !organ_requires_auth(organ, self.local_auth_required) {
            self.backend
                .list_table_rows_filtered(
                    &local_host_subject(),
                    "record",
                    &crate::infrastructure::backend_api_store::TableListQuery {
                        head_contains: request.head_contains.clone(),
                        category: request.category.clone(),
                        assignee: request.assignee.clone(),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?
        } else {
            let path = build_filtered_record_path(request);
            let response = self
                .manas
                .send_backend_request(
                    &organ.base_url,
                    bearer_token.ok_or_else(|| {
                        TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                    })?,
                    Method::GET,
                    &path,
                    None,
                )
                .await
                .map_err(TrailWidgetError::BadGateway)?;
            self.read_remote_json(session_token, &organ.id, response)
                .await?
        };
        serde_json::from_value::<Vec<RecordSearchRow>>(value).map_err(|error| {
            TrailWidgetError::Internal(format!("Resposta invalida de busca de records: {error}"))
        })
    }

    async fn get_record_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
    ) -> Result<RecordSearchRow, TrailWidgetError> {
        let value = if !organ_requires_auth(organ, self.local_auth_required) {
            self.backend
                .get_table_row(&local_host_subject(), "record", record_id)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?
        } else {
            let response = self
                .manas
                .send_table_request(
                    &organ.base_url,
                    bearer_token.ok_or_else(|| {
                        TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                    })?,
                    Method::GET,
                    "record",
                    Some(record_id),
                    None,
                )
                .await
                .map_err(TrailWidgetError::BadGateway)?;
            self.read_remote_json(session_token, &organ.id, response)
                .await?
        };
        serde_json::from_value::<RecordSearchRow>(value).map_err(|error| {
            TrailWidgetError::Internal(format!("Resposta invalida ao carregar record: {error}"))
        })
    }

    async fn list_record_extensions(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordExtensionRow>, TrailWidgetError> {
        let value = self
            .list_table_rows(session_token, organ, bearer_token, "record_extension")
            .await?;
        serde_json::from_value(value).map_err(|error| {
            TrailWidgetError::Internal(format!("Resposta invalida de record_extension: {error}"))
        })
    }

    async fn list_record_links(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordLinkRow>, TrailWidgetError> {
        let value = self
            .list_table_rows(session_token, organ, bearer_token, "record_link")
            .await?;
        serde_json::from_value(value).map_err(|error| {
            TrailWidgetError::Internal(format!("Resposta invalida de record_link: {error}"))
        })
    }

    async fn list_app_users_filtered(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        query: Option<&str>,
    ) -> Result<Vec<AppUserRow>, TrailWidgetError> {
        let value = if !organ_requires_auth(organ, self.local_auth_required) {
            self.backend
                .list_table_rows_filtered(
                    &local_host_subject(),
                    "app_user",
                    &crate::infrastructure::backend_api_store::TableListQuery {
                        identity: query.map(str::to_string),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?
        } else {
            let path = build_filtered_app_user_path(query);
            let response = self
                .manas
                .send_backend_request(
                    &organ.base_url,
                    bearer_token.ok_or_else(|| {
                        TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                    })?,
                    Method::GET,
                    &path,
                    None,
                )
                .await
                .map_err(TrailWidgetError::BadGateway)?;
            self.read_remote_json(session_token, &organ.id, response)
                .await?
        };
        serde_json::from_value(value).map_err(|error| {
            TrailWidgetError::Internal(format!("Resposta invalida de app_user: {error}"))
        })
    }

    async fn list_table_rows(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
    ) -> Result<Value, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .list_table_rows(&local_host_subject(), table)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()));
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::GET,
                table,
                None,
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response)
            .await
    }

    async fn create_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        payload: Value,
    ) -> Result<MutationResponse, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().ok_or_else(|| {
                TrailWidgetError::Invalid("Payload de criacao precisa ser um objeto.".into())
            })?;
            let outcome = self
                .backend
                .create_table_row(&local_host_subject(), table, object)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::POST,
                table,
                None,
                Some(payload),
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        parse_mutation_response(
            &self
                .read_remote_json(session_token, &organ.id, response)
                .await?,
        )
    }

    async fn update_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        id: i64,
        payload: Value,
    ) -> Result<MutationResponse, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().ok_or_else(|| {
                TrailWidgetError::Invalid("Payload de update precisa ser um objeto.".into())
            })?;
            let outcome = self
                .backend
                .update_table_row(&local_host_subject(), table, id, object)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::PATCH,
                table,
                Some(id),
                Some(payload),
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        parse_mutation_response(
            &self
                .read_remote_json(session_token, &organ.id, response)
                .await?,
        )
    }

    async fn delete_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        id: i64,
    ) -> Result<MutationResponse, TrailWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let outcome = self
                .backend
                .delete_table_row(&local_host_subject(), table, id)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::DELETE,
                table,
                Some(id),
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        parse_mutation_response(
            &self
                .read_remote_json(session_token, &organ.id, response)
                .await?,
        )
    }

    async fn sync_extension(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        namespace: &str,
        payload: Option<Value>,
    ) -> Result<(), TrailWidgetError> {
        let existing = self
            .list_record_extensions(session_token, organ, bearer_token)
            .await?
            .into_iter()
            .find(|row| row.record_id == record_id && row.namespace == namespace);
        match (existing, payload) {
            (Some(row), Some(freestyle_data_structure)) => {
                self.update_table_row(
                    session_token,
                    organ,
                    bearer_token,
                    "record_extension",
                    row.id,
                    json!({
                        "record_id": record_id,
                        "namespace": namespace,
                        "version": 1,
                        "freestyle_data_structure": freestyle_data_structure.to_string(),
                    }),
                )
                .await?;
            }
            (None, Some(freestyle_data_structure)) => {
                self.create_table_row(
                    session_token,
                    organ,
                    bearer_token,
                    "record_extension",
                    json!({
                        "record_id": record_id,
                        "namespace": namespace,
                        "version": 1,
                        "freestyle_data_structure": freestyle_data_structure.to_string(),
                    }),
                )
                .await?;
            }
            (Some(row), None) => {
                self.delete_table_row(
                    session_token,
                    organ,
                    bearer_token,
                    "record_extension",
                    row.id,
                )
                .await?;
            }
            (None, None) => {}
        }
        Ok(())
    }

    async fn sync_categories_extension(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        categories: &[String],
    ) -> Result<(), TrailWidgetError> {
        let payload = if categories.is_empty() {
            None
        } else {
            Some(json!({ "categories": categories }))
        };
        self.sync_extension(
            session_token,
            organ,
            bearer_token,
            record_id,
            "task.categories",
            payload,
        )
        .await
    }

    async fn sync_assignees(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        assignee_ids: &[i64],
    ) -> Result<(), TrailWidgetError> {
        let desired = normalize_integer_ids(assignee_ids.to_vec());
        let existing = self
            .list_record_links(session_token, organ, bearer_token)
            .await?
            .into_iter()
            .filter(|row| {
                row.record_id == record_id
                    && row.link_type == "assigned_to"
                    && row.target_table == "app_user"
            })
            .collect::<Vec<_>>();
        for row in &existing {
            if !desired.contains(&row.target_id) {
                self.delete_table_row(session_token, organ, bearer_token, "record_link", row.id)
                    .await?;
            }
        }
        for assignee_id in desired {
            if existing.iter().any(|row| row.target_id == assignee_id) {
                continue;
            }
            self.create_table_row(
                session_token,
                organ,
                bearer_token,
                "record_link",
                json!({
                    "record_id": record_id,
                    "link_type": "assigned_to",
                    "target_table": "app_user",
                    "target_id": assignee_id,
                    "position": null,
                    "freestyle_data_structure": null,
                }),
            )
            .await?;
        }
        Ok(())
    }

    async fn ensure_record_exists(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
    ) -> Result<(), TrailWidgetError> {
        self.get_record_row(session_token, organ, bearer_token, record_id)
            .await
            .map(|_| ())
    }

    async fn resolve_app_user_id(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        assignee_query: &str,
    ) -> Result<i64, TrailWidgetError> {
        let normalized = assignee_query.trim();
        if normalized.is_empty() {
            return Err(TrailWidgetError::Invalid(
                "assignee precisa ser informado.".into(),
            ));
        }
        let normalized_lower = normalized.to_lowercase();
        let users = self
            .list_app_users_filtered(session_token, organ, bearer_token, Some(normalized))
            .await?;

        if let Ok(exact_id) = normalized.parse::<i64>()
            && exact_id > 0
            && let Some(user) = users.iter().find(|user| user.id == exact_id)
        {
            return Ok(user.id);
        }

        let exact_username = users
            .iter()
            .find(|user| user.username.eq_ignore_ascii_case(normalized))
            .map(|user| user.id);
        if let Some(user_id) = exact_username {
            return Ok(user_id);
        }

        let exact_name_matches = users
            .iter()
            .filter(|user| user.name.eq_ignore_ascii_case(normalized))
            .map(|user| user.id)
            .collect::<Vec<_>>();
        if exact_name_matches.len() == 1 {
            return Ok(exact_name_matches[0]);
        }
        if exact_name_matches.len() > 1 {
            return Err(TrailWidgetError::Invalid(
                "Mais de um app_user possui esse nome. Use username ou id.".into(),
            ));
        }

        let fuzzy_matches = users
            .into_iter()
            .filter(|user| {
                user.name.to_lowercase().contains(&normalized_lower)
                    || user.username.to_lowercase().contains(&normalized_lower)
            })
            .map(|user| user.id)
            .collect::<Vec<_>>();
        match fuzzy_matches.as_slice() {
            [user_id] => Ok(*user_id),
            [] => Err(TrailWidgetError::Invalid(
                "Nenhum app_user corresponde a esse assignee.".into(),
            )),
            _ => Err(TrailWidgetError::Invalid(
                "Mais de um app_user corresponde a esse assignee. Refine com username ou id."
                    .into(),
            )),
        }
    }

    async fn execute_karma(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        karma_id: i64,
    ) -> Result<(), TrailWidgetError> {
        if karma_id <= 0 {
            return Err(TrailWidgetError::Invalid(
                "Karma id invalido para sync.".into(),
            ));
        }
        if !organ_requires_auth(organ, self.local_auth_required) {
            self.backend
                .execute_karma(&local_host_subject(), karma_id)
                .await
                .map_err(|error| TrailWidgetError::Internal(error.to_string()))?;
            return Ok(());
        }
        let response = self
            .manas
            .send_backend_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    TrailWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::POST,
                &format!("/api/karma/{karma_id}/execute"),
                None,
            )
            .await
            .map_err(TrailWidgetError::BadGateway)?;
        let _ = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        Ok(())
    }

    async fn read_remote_json(
        &self,
        session_token: Option<&str>,
        server_id: &str,
        response: reqwest::Response,
    ) -> Result<Value, TrailWidgetError> {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.auth
                .expire_server_session(
                    session_token,
                    server_id,
                    "Sessao remota expirada. Conecte esse servidor novamente.",
                )
                .await;
            return Err(TrailWidgetError::Unauthorized(
                "Sessao remota expirada. Conecte esse servidor novamente.".into(),
            ));
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(TrailWidgetError::BadGateway(if body.trim().is_empty() {
                format!("Servidor remoto recusou a operacao com status {status}.")
            } else {
                body
            }));
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| TrailWidgetError::BadGateway(error.to_string()))
    }
}

pub enum PreparedTrailStream {
    Local {
        handle: SubscriptionHandle,
        binding: TrailBindingPayload,
    },
    Remote {
        response: reqwest::Response,
        binding: TrailBindingPayload,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailBindingPayload {
    pub trail_root_record_id: i64,
    pub view_id: i64,
    pub sync: Option<TrailSyncMetadata>,
}

#[derive(Debug, Clone)]
pub enum TrailWidgetError {
    NotFound(String),
    Misconfigured(String),
    Unauthorized(String),
    Forbidden(String),
    Disabled(String),
    Invalid(String),
    BadGateway(String),
    Internal(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchTrailRequest {
    head_contains: Option<String>,
    category: Option<String>,
    assignee: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchAssigneesRequest {
    query: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindTrailRequest {
    trail_root_record_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTrailRequest {
    source_record_id: i64,
    assignee: String,
    view_name: Option<String>,
    scope: Option<String>,
    fields: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunTrailSyncRequest {
    trail_root_record_id: Option<i64>,
    scope: Option<String>,
    fields: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetTrailNodeQuantityRequest {
    record_id: i64,
    quantity: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewSnapshotPayload {
    rows: Vec<std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailSyncMetadata {
    #[serde(alias = "trail_root_record_id")]
    pub trail_root_record_id: i64,
    #[serde(alias = "sync_source_record_id")]
    pub sync_source_record_id: i64,
    pub scope: String,
    pub fields: String,
    #[serde(alias = "sync_karma_id")]
    pub sync_karma_id: Option<i64>,
    #[serde(alias = "sync_condition_id")]
    pub sync_condition_id: Option<i64>,
    #[serde(alias = "sync_consequence_id")]
    pub sync_consequence_id: Option<i64>,
    #[serde(alias = "condition_token")]
    pub condition_token: Option<String>,
    #[serde(alias = "consequence_token")]
    pub consequence_token: Option<String>,
    pub operator: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedTrailInstance {
    card: BoardCard,
    organ: Organ,
    bearer_token: Option<String>,
    is_local: bool,
    requires_auth: bool,
    global_streams_enabled: bool,
}

#[derive(Debug, Default)]
struct TrailRuntimeState {
    server_id: Option<String>,
    trail_root_record_id: Option<i64>,
    derived_view_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordSearchRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
    #[serde(alias = "primary_category")]
    primary_category: Option<String>,
    #[serde(alias = "categories_json")]
    categories_json: Option<String>,
    #[serde(alias = "assignee_ids_json")]
    assignee_ids_json: Option<String>,
    #[serde(alias = "assignee_names_json")]
    assignee_names_json: Option<String>,
    #[serde(alias = "assignee_usernames_json")]
    assignee_usernames_json: Option<String>,
    #[serde(alias = "parent_ids_json")]
    parent_ids_json: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrailQuantityChange {
    record_id: i64,
    quantity: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordExtensionRow {
    id: i64,
    record_id: i64,
    namespace: String,
    freestyle_data_structure: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordLinkRow {
    id: i64,
    record_id: i64,
    link_type: String,
    target_table: String,
    target_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AppUserRow {
    id: i64,
    name: String,
    username: String,
}

#[derive(Debug, Clone)]
struct ViewDefinition {
    id: i64,
    name: String,
}

#[derive(Debug, Clone)]
struct MutationResponse {
    last_insert_rowid: Option<i64>,
}

enum TrailPermission {
    Read,
    Write,
}

impl TrailPermission {
    fn check(&self, card: &BoardCard) -> Result<(), TrailWidgetError> {
        let has_read = has_permission(card, "read_view_stream");
        let has_write_records = has_permission(card, "write_records");
        let has_write_table = has_permission(card, "write_table");
        match self {
            Self::Read if !has_read => Err(TrailWidgetError::Forbidden(
                "Esse Trail Relation nao declara permissao read_view_stream.".into(),
            )),
            Self::Write if !(has_write_records && has_write_table) => {
                Err(TrailWidgetError::Forbidden(
                    "Esse Trail Relation nao declara as permissoes necessarias.".into(),
                ))
            }
            _ => Ok(()),
        }
    }
}

fn build_filtered_record_path(request: &SearchTrailRequest) -> String {
    let mut query = Vec::new();
    if let Some(head_contains) = request
        .head_contains
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query.push(format!("head_contains={}", encode(head_contains)));
    }
    if let Some(category) = request
        .category
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query.push(format!("category={}", encode(category)));
    }
    if let Some(assignee) = request
        .assignee
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        query.push(format!("assignee={}", encode(assignee)));
    }
    if query.is_empty() {
        "/api/table/record".into()
    } else {
        format!("/api/table/record?{}", query.join("&"))
    }
}

fn build_filtered_app_user_path(query: Option<&str>) -> String {
    let Some(identity) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return "/api/table/app_user".into();
    };
    format!("/api/table/app_user?identity={}", encode(identity))
}

fn build_sr_token(scope: &str, fields: &str, record_id: i64) -> String {
    let scope = normalize_scope(scope);
    let fields = normalize_fields(fields);
    if fields == "qhb" {
        format!("sr{scope}{record_id}")
    } else {
        format!("sr{scope}{fields}{record_id}")
    }
}

fn build_trail_sync_extension_value(sync: &TrailSyncMetadata) -> Value {
    json!({
        "trail_root_record_id": sync.trail_root_record_id,
        "sync_source_record_id": sync.sync_source_record_id,
        "scope": sync.scope,
        "fields": sync.fields,
        "sync_karma_id": sync.sync_karma_id,
        "sync_condition_id": sync.sync_condition_id,
        "sync_consequence_id": sync.sync_consequence_id,
        "condition_token": sync.condition_token,
        "consequence_token": sync.consequence_token,
        "operator": sync.operator,
    })
}

fn normalize_scope(scope: &str) -> String {
    match scope.trim() {
        "n" => "n".into(),
        "nt" => "nt".into(),
        _ => "t".into(),
    }
}

fn normalize_fields(fields: &str) -> String {
    let has_q = fields.contains('q');
    let has_h = fields.contains('h');
    let has_b = fields.contains('b');
    let mut normalized = String::new();
    if has_q {
        normalized.push('q');
    }
    if has_h {
        normalized.push('h');
    }
    if has_b {
        normalized.push('b');
    }
    if normalized.is_empty() {
        "qhb".into()
    } else {
        normalized
    }
}

fn validate_scope(scope: &Option<String>) -> Result<(), TrailWidgetError> {
    if let Some(scope) = scope
        && !matches!(scope.trim(), "t" | "n" | "nt")
    {
        return Err(TrailWidgetError::Invalid(
            "scope precisa ser t, n, ou nt.".into(),
        ));
    }
    Ok(())
}

fn validate_fields(fields: &Option<String>) -> Result<(), TrailWidgetError> {
    if let Some(fields) = fields {
        let normalized = normalize_fields(fields);
        if !matches!(
            normalized.as_str(),
            "q" | "h" | "b" | "qh" | "qb" | "hb" | "qhb"
        ) {
            return Err(TrailWidgetError::Invalid(
                "fields precisa ser uma combinacao em ordem qhb.".into(),
            ));
        }
    }
    Ok(())
}

fn parse_mutation_response(value: &Value) -> Result<MutationResponse, TrailWidgetError> {
    let object = value
        .as_object()
        .ok_or_else(|| TrailWidgetError::Internal("Resposta invalida de mutacao.".into()))?;
    Ok(MutationResponse {
        last_insert_rowid: object.get("last_insert_rowid").and_then(Value::as_i64),
    })
}

fn parse_view_definition(value: &Value) -> Result<ViewDefinition, TrailWidgetError> {
    let object = value.as_object().ok_or_else(|| {
        TrailWidgetError::Internal("Resposta invalida ao carregar a view.".into())
    })?;
    Ok(ViewDefinition {
        id: object
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| TrailWidgetError::Internal("View sem id.".into()))?,
        name: object
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| TrailWidgetError::Internal("View sem name.".into()))?,
    })
}

fn parse_view_list(value: &Value) -> Result<Vec<ViewDefinition>, TrailWidgetError> {
    let array = value
        .as_array()
        .ok_or_else(|| TrailWidgetError::Internal("Resposta invalida ao listar views.".into()))?;
    array.iter().map(parse_view_definition).collect()
}

fn parse_view_snapshot_rows(
    value: &Value,
) -> Result<Vec<std::collections::BTreeMap<String, String>>, TrailWidgetError> {
    serde_json::from_value::<ViewSnapshotPayload>(value.clone())
        .map(|snapshot| snapshot.rows)
        .map_err(|error| {
            TrailWidgetError::Internal(format!(
                "Resposta invalida ao ler snapshot da view: {error}"
            ))
        })
}

fn snapshot_trail_root_record_id(snapshot: &Value) -> Option<i64> {
    parse_view_snapshot_rows(snapshot)
        .ok()?
        .into_iter()
        .find_map(|row| {
            row.get("trail_root_record_id")
                .or_else(|| row.get("trailRootRecordId"))
                .and_then(|value| value.trim().parse::<i64>().ok())
                .filter(|value| *value > 0)
        })
}

fn compute_trail_quantity_changes(
    rows: &[std::collections::BTreeMap<String, String>],
    trail_root_record_id: i64,
    record_id: i64,
    quantity: f64,
) -> Result<Vec<TrailQuantityChange>, TrailWidgetError> {
    if rows.is_empty() || trail_root_record_id <= 0 {
        return Err(TrailWidgetError::Invalid(
            "Snapshot da trail ainda nao foi carregado.".into(),
        ));
    }

    let next_quantity = normalized_trail_quantity(quantity);
    let mut quantities = std::collections::HashMap::new();
    let mut parent_map = std::collections::HashMap::<i64, Vec<i64>>::new();
    let mut child_map = std::collections::HashMap::<i64, Vec<i64>>::new();
    let row_ids = rows
        .iter()
        .filter_map(|row| trail_snapshot_record_id(row))
        .collect::<std::collections::HashSet<_>>();

    for row in rows {
        let Some(row_id) = trail_snapshot_record_id(row) else {
            continue;
        };
        let parents = trail_snapshot_parent_ids(row)
            .into_iter()
            .filter(|parent_id| row_ids.contains(parent_id))
            .collect::<Vec<_>>();
        parent_map.insert(row_id, parents.clone());
        quantities.insert(row_id, trail_snapshot_quantity(row));
        for parent_id in parents {
            child_map.entry(parent_id).or_default().push(row_id);
        }
    }

    let mut next_quantities = quantities.clone();
    let root_id = trail_root_record_id;

    if record_id == root_id {
        next_quantities.insert(record_id, next_quantity);
    } else if next_quantity == 1 {
        if !trail_parents_complete(record_id, &parent_map, &next_quantities) {
            return Err(TrailWidgetError::Invalid(
                "All parents must be 1 before this record can be completed.".into(),
            ));
        }
        next_quantities.insert(record_id, 1);
    } else if next_quantity == -1 {
        next_quantities.insert(
            record_id,
            if trail_parents_complete(record_id, &parent_map, &next_quantities) {
                -1
            } else {
                0
            },
        );
    } else {
        next_quantities.insert(record_id, 0);
    }

    let mut queue = std::collections::VecDeque::from([record_id]);
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = queue.pop_front() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(children) = child_map.get(&current) {
            for child_id in children.iter().copied() {
                let existing = next_quantities
                    .get(&child_id)
                    .copied()
                    .map(|value| normalized_trail_quantity(value as f64))
                    .unwrap_or_default();
                if trail_parents_complete(child_id, &parent_map, &next_quantities) {
                    if existing == 0 {
                        next_quantities.insert(child_id, -1);
                    }
                } else if existing != 1 {
                    next_quantities.insert(child_id, 0);
                }
                queue.push_back(child_id);
            }
        }
    }

    let mut changes = Vec::new();
    for (changed_record_id, resolved_quantity) in next_quantities {
        let current = quantities
            .get(&changed_record_id)
            .copied()
            .unwrap_or_default();
        if current != resolved_quantity {
            changes.push(TrailQuantityChange {
                record_id: changed_record_id,
                quantity: resolved_quantity,
            });
        }
    }
    changes.sort_by_key(|change| change.record_id);

    Ok(changes)
}

fn trail_snapshot_record_id(row: &std::collections::BTreeMap<String, String>) -> Option<i64> {
    row.get("id")
        .and_then(|value| value.trim().parse::<i64>().ok())
}

fn trail_snapshot_quantity(row: &std::collections::BTreeMap<String, String>) -> i64 {
    row.get("quantity")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .map(normalized_trail_quantity)
        .unwrap_or_default()
}

fn trail_snapshot_parent_ids(row: &std::collections::BTreeMap<String, String>) -> Vec<i64> {
    serde_json::from_str::<Vec<i64>>(
        row.get("parent_ids_json")
            .map(String::as_str)
            .unwrap_or("[]"),
    )
    .unwrap_or_default()
    .into_iter()
    .filter(|value| *value > 0)
    .collect()
}

fn trail_parents_complete(
    record_id: i64,
    parent_map: &std::collections::HashMap<i64, Vec<i64>>,
    quantities: &std::collections::HashMap<i64, i64>,
) -> bool {
    let parent_ids = parent_map.get(&record_id).cloned().unwrap_or_default();
    if parent_ids.is_empty() {
        return true;
    }
    parent_ids
        .into_iter()
        .all(|parent_id| quantities.get(&parent_id).copied().unwrap_or_default() == 1)
}

fn normalized_trail_quantity(value: f64) -> i64 {
    if !value.is_finite() {
        return 0;
    }
    if (value - 1.0).abs() < 0.000001 {
        1
    } else if (value + 1.0).abs() < 0.000001 {
        -1
    } else if value.abs() < 0.000001 {
        0
    } else {
        value.round() as i64
    }
}

fn build_trail_view_query(trail_root_record_id: i64) -> String {
    format!(
        "
        WITH RECURSIVE walk(id, depth) AS (
            SELECT {trail_root_record_id}, 0
            UNION ALL
            SELECT rl.record_id, walk.depth + 1
            FROM record_link rl
            JOIN walk ON walk.id = rl.target_id
            WHERE rl.link_type = 'parent' AND rl.target_table = 'record'
        ),
        tree AS (
            SELECT id, MIN(depth) AS depth
            FROM walk
            GROUP BY id
        ),
        categories AS (
            SELECT
                re.record_id,
                json_extract(re.freestyle_data_structure, '$.categories') AS categories_json,
                json_extract(re.freestyle_data_structure, '$.categories[0]') AS primary_category
            FROM record_extension re
            WHERE re.namespace = 'task.categories'
        ),
        assignees AS (
            SELECT
                rl.record_id,
                json_group_array(rl.target_id) AS assignee_ids_json,
                json_group_array(au.name) AS assignee_names_json
            FROM record_link rl
            JOIN app_user au ON au.id = rl.target_id
            WHERE rl.link_type = 'assigned_to' AND rl.target_table = 'app_user'
            GROUP BY rl.record_id
        ),
        parents AS (
            SELECT
                rl.record_id,
                json_group_array(rl.target_id) AS parent_ids_json,
                json_group_array(COALESCE(parent_record.head, '')) AS parent_heads_json
            FROM record_link rl
            JOIN tree ON tree.id = rl.record_id
            JOIN tree parent_tree ON parent_tree.id = rl.target_id
            LEFT JOIN record parent_record ON parent_record.id = rl.target_id
            WHERE rl.link_type = 'parent' AND rl.target_table = 'record'
            GROUP BY rl.record_id
        ),
        children AS (
            SELECT
                rl.target_id AS record_id,
                COUNT(DISTINCT rl.record_id) AS children_count,
                json_group_array(
                    json_object(
                        'id', child.id,
                        'head', COALESCE(child.head, ''),
                        'quantity', CAST(child.quantity AS INTEGER)
                    )
                ) AS children_json
            FROM record_link rl
            JOIN tree ON tree.id = rl.target_id
            JOIN tree child_tree ON child_tree.id = rl.record_id
            JOIN record child ON child.id = rl.record_id
            WHERE rl.link_type = 'parent' AND rl.target_table = 'record'
            GROUP BY rl.target_id
        ),
        sync_meta AS (
            SELECT
                re.record_id,
                CAST(
                    COALESCE(
                        json_extract(re.freestyle_data_structure, '$.sync_source_record_id'),
                        json_extract(re.freestyle_data_structure, '$.syncSourceRecordId')
                    ) AS INTEGER
                ) AS sync_source_record_id,
                CAST(
                    COALESCE(
                        json_extract(re.freestyle_data_structure, '$.trail_root_record_id'),
                        json_extract(re.freestyle_data_structure, '$.trailRootRecordId')
                    ) AS INTEGER
                ) AS trail_root_record_id
            FROM record_extension re
            WHERE re.namespace = 'trail.sync'
        )
        SELECT
            r.id AS id,
            CAST(r.quantity AS INTEGER) AS quantity,
            COALESCE(r.head, '') AS head,
            COALESCE(r.body, '') AS body,
            tree.depth AS depth,
            categories.primary_category AS primary_category,
            COALESCE(categories.categories_json, '[]') AS categories_json,
            COALESCE(assignees.assignee_ids_json, '[]') AS assignee_ids_json,
            COALESCE(assignees.assignee_names_json, '[]') AS assignee_names_json,
            COALESCE(parents.parent_ids_json, '[]') AS parent_ids_json,
            COALESCE(parents.parent_heads_json, '[]') AS parent_heads_json,
            COALESCE(children.children_count, 0) AS children_count,
            COALESCE(children.children_json, '[]') AS children_json,
            COALESCE(sync_meta.sync_source_record_id, 0) AS sync_source_record_id,
            COALESCE(sync_meta.trail_root_record_id, {trail_root_record_id}) AS trail_root_record_id
        FROM tree
        JOIN record r ON r.id = tree.id
        LEFT JOIN categories ON categories.record_id = r.id
        LEFT JOIN assignees ON assignees.record_id = r.id
        LEFT JOIN parents ON parents.record_id = r.id
        LEFT JOIN children ON children.record_id = r.id
        LEFT JOIN sync_meta ON sync_meta.record_id = r.id
        ORDER BY tree.depth ASC, lower(COALESCE(r.head, '')) ASC, r.id ASC
        "
    )
}

fn derived_view_name(instance_id: &str, view_name: Option<&str>) -> String {
    let custom = view_name.map(str::trim).filter(|value| !value.is_empty());
    if let Some(custom) = custom {
        custom.to_string()
    } else {
        format!("{TRAIL_DERIVED_VIEW_NAME_PREFIX}{instance_id}")
    }
}

fn has_permission(card: &BoardCard, permission: &str) -> bool {
    card.permissions.iter().any(|value| value == permission)
}

fn validate_trail_card(card: &BoardCard) -> Result<(), TrailWidgetError> {
    if card.kind.trim() != "package" {
        return Err(TrailWidgetError::Misconfigured(
            "Esse widget nao e um package oficial.".into(),
        ));
    }
    if !is_supported_trail_package_filename(&card.package_name) {
        return Err(TrailWidgetError::Misconfigured(
            "Esse widget nao usa o package Trail Relation.".into(),
        ));
    }
    Ok(())
}

fn parse_runtime_state(widget_state: &Value) -> TrailRuntimeState {
    let Some(runtime) = widget_state
        .get(TRAIL_RUNTIME_STATE_KEY)
        .and_then(Value::as_object)
    else {
        return TrailRuntimeState::default();
    };
    TrailRuntimeState {
        server_id: runtime
            .get("server_id")
            .or_else(|| runtime.get("serverId"))
            .and_then(Value::as_str)
            .map(str::to_string),
        trail_root_record_id: runtime
            .get("trail_root_record_id")
            .or_else(|| runtime.get("trailRootRecordId"))
            .and_then(Value::as_i64),
        derived_view_id: runtime
            .get("derived_view_id")
            .or_else(|| runtime.get("derivedViewId"))
            .and_then(Value::as_i64),
    }
}

fn find_board_card(board_state: &BoardState, instance_id: &str) -> Option<BoardCard> {
    board_state
        .workspaces
        .iter()
        .flat_map(|workspace| workspace.cards.iter())
        .find(|card| card.id == instance_id)
        .cloned()
}

fn find_board_card_mut<'a>(
    board_state: &'a mut BoardState,
    instance_id: &str,
) -> Option<&'a mut BoardCard> {
    board_state
        .workspaces
        .iter_mut()
        .flat_map(|workspace| workspace.cards.iter_mut())
        .find(|card| card.id == instance_id)
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .expect("widget state object should exist")
}

fn ensure_nested_object<'a>(
    object: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    entry.as_object_mut().expect("nested object should exist")
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}

fn parse_json_strings(value: Option<&str>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<String>>(value).unwrap_or_default()
}

fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for category in categories {
        let trimmed = category.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lowered = trimmed.to_lowercase();
        if seen.insert(lowered) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

fn normalize_integer_ids(values: Vec<i64>) -> Vec<i64> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for value in values {
        if value <= 0 || !seen.insert(value) {
            continue;
        }
        normalized.push(value);
    }
    normalized
}
