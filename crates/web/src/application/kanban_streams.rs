use {
    crate::{
        application::{
            backend_api::BackendApiService,
            kanban_filters::{KanbanFilterService, RawKanbanFilterRow},
        },
        domain::{
            board::{BoardCard, BoardState},
            lince_package::normalize_package_filename,
        },
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, organ_requires_auth},
        },
    },
    ::application::{
        auth::AuthSubject,
        subscription::SubscriptionHandle,
    },
    persistence::repositories::view::is_special_view_query,
    reqwest::Method,
    serde_json::{Map, Number, Value},
};

const KANBAN_PACKAGE_FILENAME: &str = "kanban-record-view.html";
const KANBAN_RUNTIME_STATE_KEY: &str = "kanban_runtime";
const KANBAN_DERIVED_VIEW_NAME_PREFIX: &str = "__lince_web_kanban_";

#[derive(Clone)]
pub struct KanbanStreamService {
    auth: AppAuth,
    backend: BackendApiService,
    board_state: BoardStateStore,
    kanban_filters: KanbanFilterService,
    local_auth_required: bool,
    manas: ManasGateway,
    organs: OrganStore,
}

impl KanbanStreamService {
    pub fn new(
        auth: AppAuth,
        backend: BackendApiService,
        board_state: BoardStateStore,
        kanban_filters: KanbanFilterService,
        local_auth_required: bool,
        manas: ManasGateway,
        organs: OrganStore,
    ) -> Self {
        Self {
            auth,
            backend,
            board_state,
            kanban_filters,
            local_auth_required,
            manas,
            organs,
        }
    }

    pub async fn prepare_stream(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<PreparedKanbanStream, KanbanStreamError> {
        let resolved = self.resolve_instance(session_token, instance_id).await?;
        if !resolved.card.permissions.iter().any(|value| value == "read_view_stream") {
            return Err(KanbanStreamError::Forbidden(
                "Esse Kanban nao declara permissao read_view_stream.".into(),
            ));
        }
        if !resolved.effective_streams_enabled {
            return Err(KanbanStreamError::Disabled(
                "Streams desativados para esse widget.".into(),
            ));
        }

        let filters = parse_filter_rows(&resolved.card.widget_state)?;
        let base_view = self
            .load_view_definition(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                i64::from(resolved.view_id),
            )
            .await?;
        if is_special_view_query(&base_view.query) {
            return Err(KanbanStreamError::Misconfigured(
                "A view configurada nao e uma query SQL streamable.".into(),
            ));
        }

        let derived_query = self
            .kanban_filters
            .build_filtered_query(&base_view.query, &filters)
            .map_err(map_filter_error_to_stream_error)?;
        let derived_view_id = self
            .ensure_derived_view(
                session_token,
                &resolved,
                &base_view,
                &derived_query.sql,
            )
            .await?;

        if resolved.is_local {
            let handle = self
                .backend
                .subscribe_view(local_host_subject(), derived_view_id as u32)
                .await
                .map_err(|error| KanbanStreamError::Internal(error.to_string()))?;
            Ok(PreparedKanbanStream::Local { handle })
        } else {
            let bearer_token = resolved
                .bearer_token
                .as_deref()
                .ok_or_else(|| KanbanStreamError::Unauthorized("Sessao remota ausente.".into()))?;
            let response = self
                .manas
                .open_view_stream(&resolved.organ.base_url, bearer_token, derived_view_id as u64)
                .await
                .map_err(KanbanStreamError::BadGateway)?;

            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                self.auth
                    .expire_server_session(
                        session_token,
                        &resolved.organ.id,
                        "Sessao remota expirada. Conecte esse servidor novamente.",
                    )
                    .await;
                return Err(KanbanStreamError::Unauthorized(
                    "Sessao remota expirada. Conecte esse servidor novamente.".into(),
                ));
            }

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(KanbanStreamError::BadGateway(if body.trim().is_empty() {
                    format!("Stream remoto recusou a conexao com status {status}.")
                } else {
                    body
                }));
            }

            Ok(PreparedKanbanStream::Remote { response })
        }
    }

    async fn resolve_instance(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<ResolvedKanbanInstance, KanbanStreamError> {
        let instance_id = instance_id.trim();
        if instance_id.is_empty() {
            return Err(KanbanStreamError::NotFound(
                "Widget instance ausente.".into(),
            ));
        }

        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            KanbanStreamError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_kanban_card(&card)?;

        let server_id = card.server_id.trim().to_string();
        if server_id.is_empty() {
            return Err(KanbanStreamError::Misconfigured(
                "Kanban sem server_id configurado no host.".into(),
            ));
        }
        let view_id = card.view_id.filter(|value| *value > 0).ok_or_else(|| {
            KanbanStreamError::Misconfigured("Kanban sem view_id valido configurado no host.".into())
        })?;

        let organ = self
            .organs
            .get(&server_id)
            .await
            .map_err(KanbanStreamError::Internal)?
            .ok_or_else(|| {
                KanbanStreamError::Misconfigured(
                    "O server_id configurado no Kanban nao existe mais.".into(),
                )
            })?;
        let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
        let is_local = !requires_auth;
        let effective_streams_enabled = board_state.global_streams_enabled && card.streams_enabled;
        let bearer_token = if requires_auth {
            let session = self
                .auth
                .server_session(session_token, &server_id)
                .await
                .ok_or_else(|| {
                    KanbanStreamError::Unauthorized(
                        "Essa sessao local nao esta conectada a esse servidor.".into(),
                    )
                })?;
            Some(session.bearer_token)
        } else {
            None
        };

        Ok(ResolvedKanbanInstance {
            card,
            server_id,
            view_id,
            organ,
            bearer_token,
            effective_streams_enabled,
            is_local,
        })
    }

    async fn ensure_derived_view(
        &self,
        session_token: Option<&str>,
        resolved: &ResolvedKanbanInstance,
        base_view: &ViewDefinition,
        filtered_query: &str,
    ) -> Result<i64, KanbanStreamError> {
        let derived_name = derived_view_name(&resolved.card.id);
        let runtime_state = parse_runtime_state(&resolved.card.widget_state);
        let mut derived_view_id = None;

        if runtime_state.server_id.as_deref() == Some(resolved.server_id.as_str())
            && runtime_state.source_view_id == Some(base_view.id)
            && let Some(existing_id) = runtime_state.derived_view_id
            && let Some(existing_view) = self
                .load_view_definition(session_token, &resolved.organ, resolved.bearer_token.as_deref(), existing_id)
                .await
                .ok()
            && existing_view.name == derived_name
        {
            derived_view_id = Some(existing_id);
        }

        if derived_view_id.is_none() {
            derived_view_id = self
                .find_view_id_by_name(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    &derived_name,
                )
                .await?;
        }

        let derived_view_id = match derived_view_id {
            Some(view_id) => {
                self.update_view_query(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    view_id,
                    &derived_name,
                    filtered_query,
                )
                .await?;
                view_id
            }
            None => {
                self.create_view(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    &derived_name,
                    filtered_query,
                )
                .await?;
                self.find_view_id_by_name(
                    session_token,
                    &resolved.organ,
                    resolved.bearer_token.as_deref(),
                    &derived_name,
                )
                .await?
                .ok_or_else(|| {
                    KanbanStreamError::Internal(
                        "Nao consegui localizar a view derivada apos cria-la.".into(),
                    )
                })?
            }
        };

        self.persist_runtime_state(
            &resolved.card.id,
            &resolved.server_id,
            base_view.id,
            derived_view_id,
        )
        .await?;

        Ok(derived_view_id)
    }

    async fn load_view_definition(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: i64,
    ) -> Result<ViewDefinition, KanbanStreamError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let value = self
                .backend
                .get_table_row(&local_host_subject(), "view", view_id)
                .await
                .map_err(|error| KanbanStreamError::Internal(error.to_string()))?;
            return parse_view_definition(&value);
        }

        let bearer_token = bearer_token.ok_or_else(|| {
            KanbanStreamError::Unauthorized("Sessao remota ausente.".into())
        })?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::GET,
                "view",
                Some(view_id),
                None,
            )
            .await
            .map_err(KanbanStreamError::BadGateway)?;
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
    ) -> Result<Vec<ViewDefinition>, KanbanStreamError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let value = self
                .backend
                .list_table_rows(&local_host_subject(), "view")
                .await
                .map_err(|error| KanbanStreamError::Internal(error.to_string()))?;
            return parse_view_list(&value);
        }

        let bearer_token = bearer_token.ok_or_else(|| {
            KanbanStreamError::Unauthorized("Sessao remota ausente.".into())
        })?;
        let response = self
            .manas
            .send_table_request(&organ.base_url, bearer_token, Method::GET, "view", None, None)
            .await
            .map_err(KanbanStreamError::BadGateway)?;
        let value = self
            .read_remote_json(session_token, &organ.id, response)
            .await?;
        parse_view_list(&value)
    }

    async fn find_view_id_by_name(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        expected_name: &str,
    ) -> Result<Option<i64>, KanbanStreamError> {
        let views = self.list_views(session_token, organ, bearer_token).await?;
        Ok(views
            .into_iter()
            .filter(|view| view.name == expected_name)
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
    ) -> Result<(), KanbanStreamError> {
        let payload = serde_json::json!({
            "name": name,
            "query": query,
        });
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().expect("view payload object");
            self.backend
                .create_table_row(&local_host_subject(), "view", object)
                .await
                .map_err(|error| KanbanStreamError::Internal(error.to_string()))?;
            return Ok(());
        }

        let bearer_token = bearer_token.ok_or_else(|| {
            KanbanStreamError::Unauthorized("Sessao remota ausente.".into())
        })?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::POST,
                "view",
                None,
                Some(payload),
            )
            .await
            .map_err(KanbanStreamError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response).await?;
        Ok(())
    }

    async fn update_view_query(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: i64,
        name: &str,
        query: &str,
    ) -> Result<(), KanbanStreamError> {
        let payload = serde_json::json!({
            "name": name,
            "query": query,
        });
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload.as_object().expect("view payload object");
            self.backend
                .update_table_row(&local_host_subject(), "view", view_id, object)
                .await
                .map_err(|error| KanbanStreamError::Internal(error.to_string()))?;
            return Ok(());
        }

        let bearer_token = bearer_token.ok_or_else(|| {
            KanbanStreamError::Unauthorized("Sessao remota ausente.".into())
        })?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::PATCH,
                "view",
                Some(view_id),
                Some(payload),
            )
            .await
            .map_err(KanbanStreamError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response).await?;
        Ok(())
    }

    async fn read_remote_json(
        &self,
        session_token: Option<&str>,
        server_id: &str,
        response: reqwest::Response,
    ) -> Result<Value, KanbanStreamError> {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.auth
                .expire_server_session(
                    session_token,
                    server_id,
                    "Sessao remota expirada. Conecte esse servidor novamente.",
                )
                .await;
            return Err(KanbanStreamError::Unauthorized(
                "Sessao remota expirada. Conecte esse servidor novamente.".into(),
            ));
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(KanbanStreamError::BadGateway(if body.trim().is_empty() {
                format!("Servidor remoto recusou a operacao com status {status}.")
            } else {
                body
            }));
        }

        response
            .json::<Value>()
            .await
            .map_err(|error| KanbanStreamError::BadGateway(error.to_string()))
    }

    async fn persist_runtime_state(
        &self,
        instance_id: &str,
        server_id: &str,
        source_view_id: i64,
        derived_view_id: i64,
    ) -> Result<(), KanbanStreamError> {
        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            KanbanStreamError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        let widget_state = ensure_object(&mut card.widget_state);
        let runtime_state = ensure_nested_object(widget_state, KANBAN_RUNTIME_STATE_KEY);
        runtime_state.insert(
            "derived_view_id".into(),
            Value::Number(Number::from(derived_view_id)),
        );
        runtime_state.insert("server_id".into(), Value::String(server_id.to_string()));
        runtime_state.insert(
            "source_view_id".into(),
            Value::Number(Number::from(source_view_id)),
        );
        self.board_state
            .replace(board_state)
            .await
            .map_err(KanbanStreamError::Internal)?;
        Ok(())
    }
}

pub enum PreparedKanbanStream {
    Local { handle: SubscriptionHandle },
    Remote { response: reqwest::Response },
}

#[derive(Debug)]
pub enum KanbanStreamError {
    NotFound(String),
    Misconfigured(String),
    Unauthorized(String),
    Forbidden(String),
    Disabled(String),
    BadGateway(String),
    Invalid(String),
    Internal(String),
}

struct ResolvedKanbanInstance {
    card: BoardCard,
    server_id: String,
    view_id: u32,
    organ: Organ,
    bearer_token: Option<String>,
    effective_streams_enabled: bool,
    is_local: bool,
}

struct ViewDefinition {
    id: i64,
    name: String,
    query: String,
}

#[derive(Default)]
struct KanbanRuntimeState {
    derived_view_id: Option<i64>,
    server_id: Option<String>,
    source_view_id: Option<i64>,
}

fn map_filter_error_to_stream_error(error: crate::application::kanban_filters::KanbanFilterError) -> KanbanStreamError {
    match error {
        crate::application::kanban_filters::KanbanFilterError::NotFound(message)
        | crate::application::kanban_filters::KanbanFilterError::Unsupported(message)
        | crate::application::kanban_filters::KanbanFilterError::Invalid(message) => {
            KanbanStreamError::Invalid(message)
        }
        crate::application::kanban_filters::KanbanFilterError::Internal(message) => {
            KanbanStreamError::Internal(message)
        }
    }
}

fn derived_view_name(instance_id: &str) -> String {
    format!("{KANBAN_DERIVED_VIEW_NAME_PREFIX}{instance_id}")
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}

fn validate_kanban_card(card: &BoardCard) -> Result<(), KanbanStreamError> {
    if card.kind.trim() != "package" {
        return Err(KanbanStreamError::Misconfigured(
            "Esse widget nao e um package oficial.".into(),
        ));
    }
    if normalize_package_filename(&card.package_name) != KANBAN_PACKAGE_FILENAME {
        return Err(KanbanStreamError::Misconfigured(
            "Esse widget nao usa o package oficial do Kanban.".into(),
        ));
    }
    Ok(())
}

fn parse_filter_rows(widget_state: &Value) -> Result<Vec<RawKanbanFilterRow>, KanbanStreamError> {
    let Some(value) = widget_state.get("filters") else {
        return Ok(vec![]);
    };
    serde_json::from_value::<Vec<RawKanbanFilterRow>>(value.clone())
        .map_err(|error| KanbanStreamError::Invalid(format!("Filtros invalidos no widgetState: {error}")))
}

fn parse_runtime_state(widget_state: &Value) -> KanbanRuntimeState {
    let Some(runtime_state) = widget_state
        .get(KANBAN_RUNTIME_STATE_KEY)
        .and_then(Value::as_object)
    else {
        return KanbanRuntimeState::default();
    };

    KanbanRuntimeState {
        derived_view_id: runtime_state.get("derived_view_id").and_then(Value::as_i64),
        server_id: runtime_state
            .get("server_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        source_view_id: runtime_state.get("source_view_id").and_then(Value::as_i64),
    }
}

fn parse_view_definition(value: &Value) -> Result<ViewDefinition, KanbanStreamError> {
    let object = value.as_object().ok_or_else(|| {
        KanbanStreamError::Internal("Resposta invalida ao carregar a view.".into())
    })?;
    let id = object.get("id").and_then(Value::as_i64).ok_or_else(|| {
        KanbanStreamError::Internal("View carregada sem id.".into())
    })?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| KanbanStreamError::Internal("View carregada sem name.".into()))?;
    let query = object
        .get("query")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| KanbanStreamError::Internal("View carregada sem query.".into()))?;
    Ok(ViewDefinition { id, name, query })
}

fn parse_view_list(value: &Value) -> Result<Vec<ViewDefinition>, KanbanStreamError> {
    let list = value.as_array().ok_or_else(|| {
        KanbanStreamError::Internal("Resposta invalida ao listar views.".into())
    })?;
    list.iter().map(parse_view_definition).collect()
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
    value.as_object_mut().expect("widget state object should exist")
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
