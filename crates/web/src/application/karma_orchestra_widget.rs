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
            build_karma_orchestra_snapshot,
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
            "actions": ["list-views", "create-view", "use-view", "load-graph", "set-distinctness"],
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
        self.create_table_row(session_token, organ, bearer_token, "view", payload)
            .await
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
    ) -> Result<(), KarmaOrchestraWidgetError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().ok_or_else(|| {
                KarmaOrchestraWidgetError::Invalid("Payload de criacao precisa ser objeto.".into())
            })?;
            let outcome = self
                .backend
                .create_table_row(&local_host_subject(), table, object)
                .await
                .map_err(|error| KarmaOrchestraWidgetError::Internal(error.to_string()))?;
            let _ = outcome;
            return Ok(());
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
