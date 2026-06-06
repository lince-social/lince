use {
    crate::{
        application::{
            backend_api::BackendApiService,
            karma_orchestra_identity::is_supported_karma_orchestra_package_filename,
        },
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, organ_requires_auth},
        },
    },
    ::application::{
        auth::AuthSubject,
        karma_analysis::{
            KarmaOrchestraRuleInput, KarmaTokenCatalog, NamedToken, RecordToken,
            build_karma_orchestra_snapshot, expression_display, record_ids_in_expression,
        },
    },
    persistence::repositories::view::is_special_view_query,
    reqwest::Method,
    serde::{Deserialize, Serialize},
    serde_json::{Map, Number, Value, json},
};

const KARMA_ORCHESTRA_RUNTIME_STATE_KEY: &str = "karma_orchestra_runtime";
const KARMA_ORCHESTRA_VIEW_QUERY: &str = "karma_orchestra";

#[derive(Clone)]
pub struct KarmaOrchestraWidgetService {
    auth: AppAuth,
    backend: BackendApiService,
    board_state: BoardStateStore,
    local_auth_required: bool,
    manas: ManasGateway,
    organs: OrganStore,
}

impl KarmaOrchestraWidgetService {
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
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let binding = if let Some(view_id) = runtime.view_id {
            self.load_view_definition(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                view_id,
            )
            .await
            .ok()
            .filter(|view| is_special_view_query(&view.query))
            .map(|view| {
                json!({
                    "viewId": view.id,
                    "viewName": view.name,
                })
            })
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
            },
            "binding": binding,
            "state": {
                "distinctness": runtime.distinctness,
            },
            "actions": [
                "list-views",
                "create-view",
                "use-view",
                "load-graph",
                "set-distinctness",
                "load-karma-editor",
                "set-karma-active",
                "delete-karma",
                "create-condition",
                "create-consequence",
                "update-condition",
                "update-consequence",
                "create-karma",
                "update-karma"
            ],
            "dataContract": {
                "specialViewQuery": KARMA_ORCHESTRA_VIEW_QUERY,
                "normalSqlViewsAccepted": false,
                "runControlFields": ["karma.parallel", "karma.timeout_seconds"],
            }
        }))
    }

    pub async fn action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        match action {
            "list-views" => self.list_views_action(session_token, instance_id).await,
            "create-view" => {
                let request =
                    serde_json::from_value::<CreateViewRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.create_view_action(session_token, instance_id, request)
                    .await
            }
            "use-view" => {
                let request =
                    serde_json::from_value::<UseViewRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.use_view_action(session_token, instance_id, request)
                    .await
            }
            "load-graph" => self.load_graph_action(session_token, instance_id).await,
            "set-distinctness" => {
                let request =
                    serde_json::from_value::<SetDistinctnessRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.set_distinctness_action(session_token, instance_id, request)
                    .await
            }
            "load-karma-editor" => {
                let request =
                    serde_json::from_value::<LoadKarmaEditorRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.load_karma_editor_action(session_token, instance_id, request)
                    .await
            }
            "set-karma-active" => {
                let request =
                    serde_json::from_value::<SetKarmaActiveRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.set_karma_active_action(session_token, instance_id, request)
                    .await
            }
            "delete-karma" => {
                let request =
                    serde_json::from_value::<KarmaIdRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.delete_karma_action(session_token, instance_id, request)
                    .await
            }
            "create-condition" => {
                let request = serde_json::from_value::<ConditionMutationRequest>(payload).map_err(
                    |error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    },
                )?;
                self.create_condition_action(session_token, instance_id, request)
                    .await
            }
            "create-consequence" => {
                let request = serde_json::from_value::<ConsequenceMutationRequest>(payload)
                    .map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.create_consequence_action(session_token, instance_id, request)
                    .await
            }
            "update-condition" => {
                let request = serde_json::from_value::<ConditionMutationRequest>(payload).map_err(
                    |error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    },
                )?;
                self.update_condition_action(session_token, instance_id, request)
                    .await
            }
            "update-consequence" => {
                let request = serde_json::from_value::<ConsequenceMutationRequest>(payload)
                    .map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.update_consequence_action(session_token, instance_id, request)
                    .await
            }
            "create-karma" => {
                let request =
                    serde_json::from_value::<KarmaMutationRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.create_karma_action(session_token, instance_id, request)
                    .await
            }
            "update-karma" => {
                let request =
                    serde_json::from_value::<KarmaMutationRequest>(payload).map_err(|error| {
                        KarmaOrchestraWidgetError::Invalid(format!("Payload invalido: {error}"))
                    })?;
                self.update_karma_action(session_token, instance_id, request)
                    .await
            }
            _ => Err(KarmaOrchestraWidgetError::Invalid(
                "Acao de Karma Orchestra desconhecida.".into(),
            )),
        }
    }

    async fn list_views_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        let views = self
            .list_views(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?
            .into_iter()
            .filter(|view| is_special_view_query(&view.query))
            .collect::<Vec<_>>();
        Ok(json!({
            "ok": true,
            "action": "list-views",
            "views": views,
        }))
    }

    async fn create_view_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: CreateViewRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        let base_name = request
            .name
            .trim()
            .is_empty()
            .then_some("Karma Orchestra")
            .unwrap_or(request.name.trim());
        let name = self
            .next_available_view_name(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                base_name,
            )
            .await?;
        self.create_view(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            &name,
            KARMA_ORCHESTRA_VIEW_QUERY,
        )
        .await?;
        let view = self
            .list_views(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?
            .into_iter()
            .filter(|view| view.name == name && is_special_view_query(&view.query))
            .max_by_key(|view| view.id)
            .ok_or_else(|| {
                KarmaOrchestraWidgetError::Internal(
                    "Nao encontrei a View Karma Orchestra recem criada.".into(),
                )
            })?;
        self.persist_runtime_state(&resolved.card.id, &resolved.organ.id, Some(view.id), None)
            .await?;
        Ok(json!({
            "ok": true,
            "action": "create-view",
            "binding": {
                "viewId": view.id,
                "viewName": view.name,
            }
        }))
    }

    async fn use_view_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: UseViewRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        let view = self
            .load_view_definition(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                request.view_id,
            )
            .await?;
        if !is_special_view_query(&view.query) {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "Essa View nao e uma Karma Orchestra View.".into(),
            ));
        }
        self.persist_runtime_state(&resolved.card.id, &resolved.organ.id, Some(view.id), None)
            .await?;
        Ok(json!({
            "ok": true,
            "action": "use-view",
            "binding": {
                "viewId": view.id,
                "viewName": view.name,
            }
        }))
    }

    async fn set_distinctness_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: SetDistinctnessRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        let distinctness = normalize_distinctness(&request.distinctness);
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        self.persist_runtime_state(
            &resolved.card.id,
            &resolved.organ.id,
            runtime.view_id,
            Some(&distinctness),
        )
        .await?;
        Ok(json!({
            "ok": true,
            "action": "set-distinctness",
            "distinctness": distinctness,
        }))
    }

    async fn load_graph_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let view_id = runtime.view_id.ok_or_else(|| {
            KarmaOrchestraWidgetError::Invalid("Nenhuma Karma Orchestra View foi escolhida.".into())
        })?;
        let view = self
            .load_view_definition(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                view_id,
            )
            .await?;
        if !is_special_view_query(&view.query) {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "A View escolhida deixou de ser uma Karma Orchestra View.".into(),
            ));
        }
        let rows = self
            .load_graph_rows(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        let snapshot = build_karma_orchestra_snapshot(
            u32::try_from(view.id).unwrap_or_default(),
            view.name,
            rows.rules,
            rows.catalog,
        );
        Ok(json!({
            "ok": true,
            "action": "load-graph",
            "graph": snapshot,
        }))
    }

    async fn load_karma_editor_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: LoadKarmaEditorRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Read)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let rows = self
            .load_editor_rows(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        Ok(json!({
            "ok": true,
            "action": "load-karma-editor",
            "editor": build_editor_payload(request.karma_id, rows)?,
        }))
    }

    async fn set_karma_active_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: SetKarmaActiveRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "karma",
            request.karma_id,
            json!({ "quantity": if request.active { 1 } else { 0 } }),
        )
        .await?;
        self.load_editor_response(
            session_token,
            &resolved,
            Some(request.karma_id),
            "set-karma-active",
        )
        .await
    }

    async fn delete_karma_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: KarmaIdRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        self.delete_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "karma",
            request.karma_id,
        )
        .await?;
        Ok(json!({
            "ok": true,
            "action": "delete-karma",
        }))
    }

    async fn create_condition_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: ConditionMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let name = fallback_name(request.name.as_deref(), "Condition");
        let id = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma_condition",
                json!({ "name": name, "condition": request.code, "quantity": 1 }),
            )
            .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            request.karma_id,
            id,
            None,
            "create-condition",
        )
        .await
    }

    async fn create_consequence_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: ConsequenceMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        validate_consequence_code(&request.code)?;
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let name = fallback_name(request.name.as_deref(), "Consequence");
        let id = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma_consequence",
                json!({ "name": name, "consequence": request.code, "quantity": 1 }),
            )
            .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            request.karma_id,
            None,
            id,
            "create-consequence",
        )
        .await
    }

    async fn update_condition_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: ConditionMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let Some(id) = request.id else {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "Condition id ausente.".into(),
            ));
        };
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let name = fallback_name(request.name.as_deref(), "Condition");
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "karma_condition",
            id,
            json!({ "name": name, "condition": request.code }),
        )
        .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            request.karma_id,
            Some(id),
            None,
            "update-condition",
        )
        .await
    }

    async fn update_consequence_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: ConsequenceMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let Some(id) = request.id else {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "Consequence id ausente.".into(),
            ));
        };
        validate_consequence_code(&request.code)?;
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let name = fallback_name(request.name.as_deref(), "Consequence");
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "karma_consequence",
            id,
            json!({ "name": name, "consequence": request.code }),
        )
        .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            request.karma_id,
            None,
            Some(id),
            "update-consequence",
        )
        .await
    }

    async fn create_karma_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: KarmaMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        validate_karma_mutation(&request)?;
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        let id = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "karma",
                json!({
                    "name": fallback_name(request.name.as_deref(), "Karma"),
                    "quantity": if request.active { 1 } else { 0 },
                    "condition_id": request.condition_id,
                    "operator": normalize_operator(&request.operator)?,
                    "consequence_id": request.consequence_id,
                    "confirm_karma_check_loops": true,
                }),
            )
            .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            id,
            Some(request.condition_id),
            Some(request.consequence_id),
            "create-karma",
        )
        .await
    }

    async fn update_karma_action(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        request: KarmaMutationRequest,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let Some(id) = request.karma_id else {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "Karma id ausente.".into(),
            ));
        };
        validate_karma_mutation(&request)?;
        let resolved = self
            .resolve_instance(session_token, instance_id, KarmaOrchestraPermission::Write)
            .await?;
        self.validate_selected_karma_view(session_token, &resolved)
            .await?;
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "karma",
            id,
            json!({
                "name": fallback_name(request.name.as_deref(), "Karma"),
                "quantity": if request.active { 1 } else { 0 },
                "condition_id": request.condition_id,
                "operator": normalize_operator(&request.operator)?,
                "consequence_id": request.consequence_id,
                "confirm_karma_check_loops": true,
            }),
        )
        .await?;
        self.load_editor_response_with_selected(
            session_token,
            &resolved,
            Some(id),
            Some(request.condition_id),
            Some(request.consequence_id),
            "update-karma",
        )
        .await
    }

    async fn validate_selected_karma_view(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedKarmaOrchestraInstance,
    ) -> Result<(), KarmaOrchestraWidgetError> {
        let runtime = parse_runtime_state(&resolved.card.widget_state);
        let view_id = runtime.view_id.ok_or_else(|| {
            KarmaOrchestraWidgetError::Invalid("Nenhuma Karma Orchestra View foi escolhida.".into())
        })?;
        let view = self
            .load_view_definition(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                view_id,
            )
            .await?;
        if !is_special_view_query(&view.query) {
            return Err(KarmaOrchestraWidgetError::Invalid(
                "A View escolhida deixou de ser uma Karma Orchestra View.".into(),
            ));
        }
        Ok(())
    }

    async fn load_editor_response(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedKarmaOrchestraInstance,
        karma_id: Option<i64>,
        action: &str,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        self.load_editor_response_with_selected(
            session_token,
            resolved,
            karma_id,
            None,
            None,
            action,
        )
        .await
    }

    async fn load_editor_response_with_selected(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedKarmaOrchestraInstance,
        karma_id: Option<i64>,
        selected_condition_id: Option<i64>,
        selected_consequence_id: Option<i64>,
        action: &str,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        let rows = self
            .load_editor_rows(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        let mut editor = build_editor_payload(karma_id, rows)?;
        if let Some(object) = editor.as_object_mut() {
            if let Some(id) = selected_condition_id {
                object.insert(
                    "selectedConditionId".into(),
                    Value::Number(Number::from(id)),
                );
            }
            if let Some(id) = selected_consequence_id {
                object.insert(
                    "selectedConsequenceId".into(),
                    Value::Number(Number::from(id)),
                );
            }
        }
        Ok(json!({
            "ok": true,
            "action": action,
            "editor": editor,
        }))
    }

    async fn resolve_instance(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        permission: KarmaOrchestraPermission,
    ) -> Result<ResolvedKarmaOrchestraInstance, KarmaOrchestraWidgetError> {
        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            KarmaOrchestraWidgetError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_karma_orchestra_card(&card)?;
        permission.check(&card)?;
        let server_id = card.server_id.trim().to_string();
        if server_id.is_empty() {
            return Err(KarmaOrchestraWidgetError::Misconfigured(
                "Karma Orchestra sem server_id configurado.".into(),
            ));
        }
        let organ = self
            .organs
            .get(&server_id)
            .await
            .map_err(KarmaOrchestraWidgetError::Internal)?
            .ok_or_else(|| {
                KarmaOrchestraWidgetError::Misconfigured(
                    "O server_id configurado nao existe mais.".into(),
                )
            })?;
        let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
        let is_local = !requires_auth;
        let bearer_token = if requires_auth {
            let session = self
                .auth
                .server_session(session_token, &server_id)
                .await
                .ok_or_else(|| {
                    KarmaOrchestraWidgetError::Unauthorized(
                        "Essa sessao local nao esta conectada a esse servidor.".into(),
                    )
                })?;
            Some(session.bearer_token)
        } else {
            None
        };
        Ok(ResolvedKarmaOrchestraInstance {
            card,
            organ,
            bearer_token,
            is_local,
            requires_auth,
        })
    }

    async fn load_graph_rows(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<GraphRows, KarmaOrchestraWidgetError> {
        let karma_rows = serde_json::from_value::<Vec<KarmaRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!("Resposta invalida de karma: {error}"))
        })?;
        let condition_rows = serde_json::from_value::<Vec<KarmaConditionRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma_condition")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!(
                "Resposta invalida de karma_condition: {error}"
            ))
        })?;
        let consequence_rows = serde_json::from_value::<Vec<KarmaConsequenceRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma_consequence")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!(
                "Resposta invalida de karma_consequence: {error}"
            ))
        })?;
        let record_rows = serde_json::from_value::<Vec<RecordRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "record")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!("Resposta invalida de record: {error}"))
        })?;
        let command_rows = serde_json::from_value::<Vec<CommandRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "command")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();
        let query_rows = serde_json::from_value::<Vec<QueryRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "query")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();
        let frequency_rows = serde_json::from_value::<Vec<FrequencyRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "frequency")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();

        let conditions = condition_rows
            .into_iter()
            .map(|row| (row.id, row))
            .collect::<std::collections::BTreeMap<_, _>>();
        let consequences = consequence_rows
            .into_iter()
            .map(|row| (row.id, row))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut rules = Vec::new();
        for row in karma_rows {
            let Some(condition) = conditions.get(&row.condition_id) else {
                continue;
            };
            let Some(consequence) = consequences.get(&row.consequence_id) else {
                continue;
            };
            rules.push(KarmaOrchestraRuleInput {
                karma_id: row.id as u32,
                karma_name: row.name,
                karma_quantity: row.quantity as i32,
                karma_parallel: row.parallel.unwrap_or_default() != 0,
                karma_timeout_seconds: row.timeout_seconds.unwrap_or_default(),
                active: false,
                condition_id: condition.id as u32,
                condition_name: condition.name.clone(),
                condition_quantity: condition.quantity as i32,
                condition_code: condition.condition.clone(),
                operator: row.operator,
                consequence_id: consequence.id as u32,
                consequence_name: consequence.name.clone(),
                consequence_quantity: consequence.quantity as i32,
                consequence_code: consequence.consequence.clone(),
            });
        }
        rules.sort_by_key(|rule| rule.karma_id);

        let catalog = KarmaTokenCatalog {
            records: record_rows
                .into_iter()
                .map(|row| {
                    (
                        row.id as u32,
                        RecordToken {
                            id: row.id as u32,
                            quantity: row.quantity,
                            head: row.head.unwrap_or_else(|| format!("Record #{}", row.id)),
                        },
                    )
                })
                .collect(),
            commands: command_rows
                .into_iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: if row.name.trim().is_empty() {
                                "Nameless Command".into()
                            } else {
                                row.name
                            },
                        },
                    )
                })
                .collect(),
            queries: query_rows
                .into_iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: row
                                .name
                                .filter(|name| !name.trim().is_empty())
                                .unwrap_or_else(|| "Nameless Query".into()),
                        },
                    )
                })
                .collect(),
            frequencies: frequency_rows
                .into_iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: if row.name.trim().is_empty() {
                                "Nameless Frequency".into()
                            } else {
                                row.name
                            },
                        },
                    )
                })
                .collect(),
        };

        Ok(GraphRows { rules, catalog })
    }

    async fn load_editor_rows(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<EditorRows, KarmaOrchestraWidgetError> {
        let karma_rows = serde_json::from_value::<Vec<KarmaRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!("Resposta invalida de karma: {error}"))
        })?;
        let condition_rows = serde_json::from_value::<Vec<KarmaConditionRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma_condition")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!(
                "Resposta invalida de karma_condition: {error}"
            ))
        })?;
        let consequence_rows = serde_json::from_value::<Vec<KarmaConsequenceRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "karma_consequence")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!(
                "Resposta invalida de karma_consequence: {error}"
            ))
        })?;
        let record_rows = serde_json::from_value::<Vec<RecordRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "record")
                .await?,
        )
        .map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!("Resposta invalida de record: {error}"))
        })?;
        let command_rows = serde_json::from_value::<Vec<CommandRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "command")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();
        let query_rows = serde_json::from_value::<Vec<QueryRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "query")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();
        let frequency_rows = serde_json::from_value::<Vec<FrequencyRow>>(
            self.list_table_rows(session_token, organ, bearer_token, "frequency")
                .await
                .unwrap_or_else(|_| Value::Array(Vec::new())),
        )
        .unwrap_or_default();

        let catalog = KarmaTokenCatalog {
            records: record_rows
                .iter()
                .map(|row| {
                    (
                        row.id as u32,
                        RecordToken {
                            id: row.id as u32,
                            quantity: row.quantity,
                            head: row
                                .head
                                .clone()
                                .unwrap_or_else(|| format!("Record #{}", row.id)),
                        },
                    )
                })
                .collect(),
            commands: command_rows
                .iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: if row.name.trim().is_empty() {
                                "Nameless Command".into()
                            } else {
                                row.name.clone()
                            },
                        },
                    )
                })
                .collect(),
            queries: query_rows
                .iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: row
                                .name
                                .clone()
                                .filter(|name| !name.trim().is_empty())
                                .unwrap_or_else(|| "Nameless Query".into()),
                        },
                    )
                })
                .collect(),
            frequencies: frequency_rows
                .iter()
                .map(|row| {
                    (
                        row.id as u32,
                        NamedToken {
                            id: row.id as u32,
                            name: if row.name.trim().is_empty() {
                                "Nameless Frequency".into()
                            } else {
                                row.name.clone()
                            },
                        },
                    )
                })
                .collect(),
        };

        Ok(EditorRows {
            karma_rows,
            condition_rows,
            consequence_rows,
            catalog,
        })
    }

    async fn load_view_definition(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: i64,
    ) -> Result<ViewDefinition, KarmaOrchestraWidgetError> {
        let value = if !organ_requires_auth(organ, self.local_auth_required) {
            self.backend
                .get_table_row(&local_host_subject(), "view", view_id)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()))?
        } else {
            let response = self
                .manas
                .send_table_request(
                    &organ.base_url,
                    bearer_token.ok_or_else(|| {
                        KarmaOrchestraWidgetError::Unauthorized("Sessao remota ausente.".into())
                    })?,
                    Method::GET,
                    "view",
                    Some(view_id),
                    None,
                )
                .await
                .map_err(KarmaOrchestraWidgetError::BadGateway)?;
            self.read_remote_json(session_token, &organ.id, response)
                .await?
        };
        serde_json::from_value::<ViewDefinition>(value).map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!("Resposta invalida de view: {error}"))
        })
    }

    async fn list_views(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<ViewDefinition>, KarmaOrchestraWidgetError> {
        let value = self
            .list_table_rows(session_token, organ, bearer_token, "view")
            .await?;
        serde_json::from_value(value).map_err(|error| {
            KarmaOrchestraWidgetError::Internal(format!(
                "Resposta invalida ao listar views: {error}"
            ))
        })
    }

    async fn next_available_view_name(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        base: &str,
    ) -> Result<String, KarmaOrchestraWidgetError> {
        let existing = self
            .list_views(session_token, organ, bearer_token)
            .await?
            .into_iter()
            .map(|view| view.name)
            .collect::<std::collections::HashSet<_>>();
        if !existing.contains(base) {
            return Ok(base.into());
        }
        for suffix in 2..1000 {
            let candidate = format!("{base} {suffix}");
            if !existing.contains(&candidate) {
                return Ok(candidate);
            }
        }
        Err(KarmaOrchestraWidgetError::Invalid(
            "Nao consegui gerar um nome unico para a View.".into(),
        ))
    }

    async fn create_view(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        name: &str,
        query: &str,
    ) -> Result<(), KarmaOrchestraWidgetError> {
        let payload = json!({ "name": name, "query": query });
        let _ = self
            .create_table_row(session_token, organ, bearer_token, "view", payload)
            .await?;
        Ok(())
    }

    async fn list_table_rows(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .list_table_rows(&local_host_subject(), table)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()));
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    KarmaOrchestraWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::GET,
                table,
                None,
                None,
            )
            .await
            .map_err(KarmaOrchestraWidgetError::BadGateway)?;
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
    ) -> Result<Option<i64>, KarmaOrchestraWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().ok_or_else(|| {
                KarmaOrchestraWidgetError::Invalid("Payload de criacao precisa ser objeto.".into())
            })?;
            let outcome = self
                .backend
                .create_table_row(&local_host_subject(), table, object)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()))?;
            return Ok(outcome.last_insert_rowid);
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    KarmaOrchestraWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::POST,
                table,
                None,
                Some(payload),
            )
            .await
            .map_err(KarmaOrchestraWidgetError::BadGateway)?;
        let value = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        Ok(extract_mutation_id(&value))
    }

    async fn update_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        id: i64,
        payload: Value,
    ) -> Result<(), KarmaOrchestraWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().ok_or_else(|| {
                KarmaOrchestraWidgetError::Invalid("Payload de update precisa ser objeto.".into())
            })?;
            let _outcome = self
                .backend
                .update_table_row(&local_host_subject(), table, id, object)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()))?;
            return Ok(());
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    KarmaOrchestraWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::PATCH,
                table,
                Some(id),
                Some(payload),
            )
            .await
            .map_err(KarmaOrchestraWidgetError::BadGateway)?;
        let _ = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        Ok(())
    }

    async fn delete_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        id: i64,
    ) -> Result<(), KarmaOrchestraWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let _outcome = self
                .backend
                .delete_table_row(&local_host_subject(), table, id)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()))?;
            return Ok(());
        }
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token.ok_or_else(|| {
                    KarmaOrchestraWidgetError::Unauthorized("Sessao remota ausente.".into())
                })?,
                Method::DELETE,
                table,
                Some(id),
                None,
            )
            .await
            .map_err(KarmaOrchestraWidgetError::BadGateway)?;
        let _ = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        Ok(())
    }

    async fn persist_runtime_state(
        &self,
        instance_id: &str,
        server_id: impl ToString,
        view_id: Option<i64>,
        distinctness: Option<&str>,
    ) -> Result<(), KarmaOrchestraWidgetError> {
        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            KarmaOrchestraWidgetError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        let existing = parse_runtime_state(&card.widget_state);
        let widget_state = ensure_object(&mut card.widget_state);
        let runtime = ensure_nested_object(widget_state, KARMA_ORCHESTRA_RUNTIME_STATE_KEY);
        runtime.insert("server_id".into(), Value::String(server_id.to_string()));
        if let Some(view_id) = view_id.or(existing.view_id) {
            runtime.insert("view_id".into(), Value::Number(Number::from(view_id)));
        }
        runtime.insert(
            "distinctness".into(),
            Value::String(normalize_distinctness(
                distinctness.unwrap_or(existing.distinctness.as_str()),
            )),
        );
        self.board_state
            .replace(board_state)
            .await
            .map_err(KarmaOrchestraWidgetError::Internal)?;
        Ok(())
    }

    async fn read_remote_json(
        &self,
        session_token: Option<&str>,
        server_id: impl ToString,
        response: reqwest::Response,
    ) -> Result<Value, KarmaOrchestraWidgetError> {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.auth
                .expire_server_session(
                    session_token,
                    server_id,
                    "Sessao remota expirada. Conecte esse servidor novamente.",
                )
                .await;
            return Err(KarmaOrchestraWidgetError::Unauthorized(
                "Sessao remota expirada. Conecte esse servidor novamente.".into(),
            ));
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(KarmaOrchestraWidgetError::BadGateway(
                if body.trim().is_empty() {
                    format!("Servidor remoto recusou a operacao com status {status}.")
                } else {
                    body
                },
            ));
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| KarmaOrchestraWidgetError::BadGateway(error.to_string()))
    }
}

#[derive(Debug, Clone)]
pub enum KarmaOrchestraWidgetError {
    NotFound(String),
    Misconfigured(String),
    Unauthorized(String),
    Forbidden(String),
    Invalid(String),
    BadGateway(String),
    Internal(String),
}

#[derive(Debug, Clone)]
struct ResolvedKarmaOrchestraInstance {
    card: BoardCard,
    organ: Organ,
    bearer_token: Option<String>,
    is_local: bool,
    requires_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewDefinition {
    id: i64,
    name: String,
    query: String,
}

#[derive(Debug, Default)]
struct KarmaOrchestraRuntimeState {
    view_id: Option<i64>,
    distinctness: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateViewRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UseViewRequest {
    view_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetDistinctnessRequest {
    distinctness: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadKarmaEditorRequest {
    karma_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KarmaIdRequest {
    karma_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetKarmaActiveRequest {
    karma_id: i64,
    active: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConditionMutationRequest {
    id: Option<i64>,
    karma_id: Option<i64>,
    name: Option<String>,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConsequenceMutationRequest {
    id: Option<i64>,
    karma_id: Option<i64>,
    name: Option<String>,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KarmaMutationRequest {
    karma_id: Option<i64>,
    name: Option<String>,
    active: bool,
    condition_id: i64,
    operator: String,
    consequence_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct KarmaRow {
    id: i64,
    quantity: i64,
    name: String,
    condition_id: i64,
    operator: String,
    consequence_id: i64,
    parallel: Option<i64>,
    timeout_seconds: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct KarmaConditionRow {
    id: i64,
    quantity: i64,
    name: String,
    condition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KarmaConsequenceRow {
    id: i64,
    quantity: i64,
    name: String,
    consequence: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct CommandRow {
    id: i64,
    name: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct QueryRow {
    id: i64,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct FrequencyRow {
    id: i64,
    name: String,
}

struct GraphRows {
    rules: Vec<KarmaOrchestraRuleInput>,
    catalog: KarmaTokenCatalog,
}

struct EditorRows {
    karma_rows: Vec<KarmaRow>,
    condition_rows: Vec<KarmaConditionRow>,
    consequence_rows: Vec<KarmaConsequenceRow>,
    catalog: KarmaTokenCatalog,
}

enum KarmaOrchestraPermission {
    Read,
    Write,
}

impl KarmaOrchestraPermission {
    fn check(&self, card: &BoardCard) -> Result<(), KarmaOrchestraWidgetError> {
        let has_bridge = has_permission(card, "bridge_state");
        let has_write_table = has_permission(card, "write_table");
        match self {
            Self::Read if !has_bridge => Err(KarmaOrchestraWidgetError::Forbidden(
                "Esse Karma Orchestra nao declara permissao bridge_state.".into(),
            )),
            Self::Write if !(has_bridge && has_write_table) => {
                Err(KarmaOrchestraWidgetError::Forbidden(
                    "Esse Karma Orchestra nao declara as permissoes necessarias.".into(),
                ))
            }
            _ => Ok(()),
        }
    }
}

fn validate_karma_orchestra_card(card: &BoardCard) -> Result<(), KarmaOrchestraWidgetError> {
    if card.kind.trim() != "package" {
        return Err(KarmaOrchestraWidgetError::Misconfigured(
            "Esse widget nao e um package oficial.".into(),
        ));
    }
    if !is_supported_karma_orchestra_package_filename(&card.package_name) {
        return Err(KarmaOrchestraWidgetError::Misconfigured(
            "Esse widget nao usa o package Karma Orchestra.".into(),
        ));
    }
    Ok(())
}

fn build_editor_payload(
    karma_id: Option<i64>,
    rows: EditorRows,
) -> Result<Value, KarmaOrchestraWidgetError> {
    let condition_refs = referenced_karma_ids_by_condition(&rows.karma_rows);
    let consequence_refs = referenced_karma_ids_by_consequence(&rows.karma_rows);
    let conditions = rows
        .condition_rows
        .iter()
        .map(|row| {
            let display = expression_display(&row.condition, &rows.catalog, true);
            json!({
                "id": row.id,
                "name": row.name,
                "quantity": row.quantity,
                "code": row.condition,
                "display": display,
                "referencedByKarmaIds": condition_refs.get(&row.id).cloned().unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let consequences = rows
        .consequence_rows
        .iter()
        .map(|row| {
            let display = consequence_display(&row.consequence, &rows.catalog);
            json!({
                "id": row.id,
                "name": row.name,
                "quantity": row.quantity,
                "code": row.consequence,
                "display": display,
                "referencedByKarmaIds": consequence_refs.get(&row.id).cloned().unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    let original = if let Some(id) = karma_id {
        let row = rows
            .karma_rows
            .iter()
            .find(|row| row.id == id)
            .ok_or_else(|| KarmaOrchestraWidgetError::NotFound("Karma nao encontrado.".into()))?;
        let condition = rows
            .condition_rows
            .iter()
            .find(|condition| condition.id == row.condition_id)
            .ok_or_else(|| {
                KarmaOrchestraWidgetError::NotFound("Condition do Karma nao encontrada.".into())
            })?;
        let consequence = rows
            .consequence_rows
            .iter()
            .find(|consequence| consequence.id == row.consequence_id)
            .ok_or_else(|| {
                KarmaOrchestraWidgetError::NotFound("Consequence do Karma nao encontrada.".into())
            })?;
        Some(json!({
            "karmaId": row.id,
            "karmaName": row.name,
            "karmaQuantity": row.quantity,
            "karmaActive": row.quantity != 0,
            "operator": row.operator,
            "conditionId": condition.id,
            "conditionName": condition.name,
            "conditionCode": condition.condition,
            "conditionDisplay": expression_display(&condition.condition, &rows.catalog, true),
            "consequenceId": consequence.id,
            "consequenceName": consequence.name,
            "consequenceCode": consequence.consequence,
            "consequenceDisplay": consequence_display(&consequence.consequence, &rows.catalog),
        }))
    } else {
        None
    };
    Ok(json!({
        "mode": if karma_id.is_some() { "update" } else { "create" },
        "original": original,
        "draft": {
            "conditionId": original.as_ref().and_then(|value| value.get("conditionId")).cloned(),
            "conditionName": original.as_ref().and_then(|value| value.get("conditionName")).cloned().unwrap_or(Value::String(String::new())),
            "conditionCode": original.as_ref().and_then(|value| value.get("conditionCode")).cloned().unwrap_or(Value::String(String::new())),
            "conditionMode": "selected",
            "operator": original.as_ref().and_then(|value| value.get("operator")).cloned().unwrap_or(Value::String("=".into())),
            "consequenceId": original.as_ref().and_then(|value| value.get("consequenceId")).cloned(),
            "consequenceName": original.as_ref().and_then(|value| value.get("consequenceName")).cloned().unwrap_or(Value::String(String::new())),
            "consequenceCode": original.as_ref().and_then(|value| value.get("consequenceCode")).cloned().unwrap_or(Value::String(String::new())),
            "consequenceMode": "selected",
            "active": original.as_ref().and_then(|value| value.get("karmaActive")).cloned().unwrap_or(Value::Bool(true)),
        },
        "conditions": conditions,
        "consequences": consequences,
        "tokens": build_token_options(&rows.catalog),
        "operators": ["=", "=*"],
    }))
}

fn consequence_display(
    code: &str,
    catalog: &KarmaTokenCatalog,
) -> ::application::karma_analysis::KarmaExpressionDisplay {
    expression_display(code, catalog, !record_ids_in_expression(code).is_empty())
}

fn referenced_karma_ids_by_condition(
    rows: &[KarmaRow],
) -> std::collections::BTreeMap<i64, Vec<i64>> {
    let mut refs = std::collections::BTreeMap::<i64, Vec<i64>>::new();
    for row in rows {
        refs.entry(row.condition_id).or_default().push(row.id);
    }
    refs
}

fn referenced_karma_ids_by_consequence(
    rows: &[KarmaRow],
) -> std::collections::BTreeMap<i64, Vec<i64>> {
    let mut refs = std::collections::BTreeMap::<i64, Vec<i64>>::new();
    for row in rows {
        refs.entry(row.consequence_id).or_default().push(row.id);
    }
    refs
}

fn build_token_options(catalog: &KarmaTokenCatalog) -> Vec<Value> {
    let mut tokens = Vec::new();
    tokens.extend(catalog.records.values().map(|record| {
        json!({
            "kind": "record_quantity",
            "id": record.id,
            "code": format!("rq{}", record.id),
            "human": record.head,
            "searchText": format!("{} rq{} {}", record.id, record.id, record.head),
            "numeric": record.quantity,
            "validForCondition": true,
            "validForConsequence": true,
        })
    }));
    tokens.extend(catalog.frequencies.values().map(|frequency| {
        json!({
            "kind": "frequency",
            "id": frequency.id,
            "code": format!("f{}", frequency.id),
            "human": frequency.name,
            "searchText": format!("{} f{} {}", frequency.id, frequency.id, frequency.name),
            "validForCondition": true,
            "validForConsequence": false,
        })
    }));
    tokens.extend(catalog.commands.values().map(|command| {
        json!({
            "kind": "command",
            "id": command.id,
            "code": format!("c{}", command.id),
            "human": command.name,
            "searchText": format!("{} c{} {}", command.id, command.id, command.name),
            "validForCondition": true,
            "validForConsequence": true,
        })
    }));
    tokens.extend(catalog.queries.values().map(|query| {
        json!({
            "kind": "query",
            "id": query.id,
            "code": format!("sql{}", query.id),
            "human": query.name,
            "searchText": format!("{} sql{} {}", query.id, query.id, query.name),
            "validForCondition": true,
            "validForConsequence": true,
        })
    }));
    tokens
}

fn validate_karma_mutation(
    request: &KarmaMutationRequest,
) -> Result<(), KarmaOrchestraWidgetError> {
    if request.condition_id <= 0 {
        return Err(KarmaOrchestraWidgetError::Invalid(
            "Condition invalida.".into(),
        ));
    }
    if request.consequence_id <= 0 {
        return Err(KarmaOrchestraWidgetError::Invalid(
            "Consequence invalida.".into(),
        ));
    }
    let _ = normalize_operator(&request.operator)?;
    Ok(())
}

fn normalize_operator(operator: &str) -> Result<&str, KarmaOrchestraWidgetError> {
    match operator.trim() {
        "=" => Ok("="),
        "=*" => Ok("=*"),
        _ => Err(KarmaOrchestraWidgetError::Invalid(
            "Operator invalido.".into(),
        )),
    }
}

fn validate_consequence_code(code: &str) -> Result<(), KarmaOrchestraWidgetError> {
    let trimmed = code.trim();
    let valid = regex::Regex::new(r"^(rq\d+|c\d+|sql\d+|(?:sr|sync-record)(?:nt|t|n)q?h?b?\d+|sync-record-(?:node-and-tree|node|tree)-(?:quantity-head-body|quantity-head|quantity-body|head-body|quantity|head|body)-\d+)$")
        .expect("valid consequence regex")
        .is_match(trimmed);
    if valid {
        Ok(())
    } else {
        Err(KarmaOrchestraWidgetError::Invalid(
            "Consequence precisa ser um unico alvo executavel.".into(),
        ))
    }
}

fn fallback_name(name: Option<&str>, fallback: &str) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn extract_mutation_id(value: &Value) -> Option<i64> {
    value
        .get("id")
        .or_else(|| value.get("rowId"))
        .or_else(|| value.get("row_id"))
        .or_else(|| value.get("lastInsertRowid"))
        .or_else(|| value.get("last_insert_rowid"))
        .and_then(Value::as_i64)
}

fn parse_runtime_state(widget_state: &Value) -> KarmaOrchestraRuntimeState {
    let Some(runtime) = widget_state
        .get(KARMA_ORCHESTRA_RUNTIME_STATE_KEY)
        .and_then(Value::as_object)
    else {
        return KarmaOrchestraRuntimeState {
            distinctness: "none".into(),
            ..Default::default()
        };
    };
    KarmaOrchestraRuntimeState {
        view_id: runtime
            .get("view_id")
            .or_else(|| runtime.get("viewId"))
            .and_then(Value::as_i64),
        distinctness: normalize_distinctness(
            runtime
                .get("distinctness")
                .and_then(Value::as_str)
                .unwrap_or("none"),
        ),
    }
}

fn normalize_distinctness(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "condition" => "condition".into(),
        "consequence" => "consequence".into(),
        "both" => "both".into(),
        _ => "none".into(),
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

fn has_permission(card: &BoardCard, permission: &str) -> bool {
    card.permissions.iter().any(|value| value == permission)
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}
