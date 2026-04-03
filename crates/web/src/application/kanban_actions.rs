use {
    crate::{
        application::{
            backend_api::BackendApiService, kanban_filters::RawKanbanFilterRow,
            kanban_identity::is_supported_kanban_package_filename,
        },
        domain::board::{BoardCard, BoardState},
        infrastructure::{
            auth::AppAuth,
            board_state_store::BoardStateStore,
            manas::ManasGateway,
            organ_store::{Organ, OrganStore, organ_requires_auth},
        },
    },
    ::application::auth::AuthSubject,
    chrono::{DateTime, NaiveDateTime, Utc},
    maud::html,
    reqwest::Method,
    serde::Deserialize,
    serde_json::{Value, json},
    std::collections::{BTreeMap, BTreeSet},
};

const VALID_TASK_TYPES: [&str; 4] = ["epic", "feature", "task", "other"];

#[derive(Clone)]
pub struct KanbanActionService {
    auth: AppAuth,
    backend: BackendApiService,
    board_state: BoardStateStore,
    local_auth_required: bool,
    manas: ManasGateway,
    organs: OrganStore,
}

impl KanbanActionService {
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

    pub async fn create_record(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: CreateRecordRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(
                session_token,
                instance_id,
                ActionPermission::WriteRecordAndMaybeTable {
                    needs_table: payload.needs_write_table(),
                },
            )
            .await?;
        validate_task_type(payload.task_type.as_deref())?;
        validate_parent_id(payload.parent_id)?;
        self.validate_assignee_ids(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            &payload.assignee_ids,
        )
        .await?;
        if let Some(parent_id) = payload.parent_id {
            self.ensure_record_exists(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                parent_id,
            )
            .await?;
        }

        let record_payload = json!({
            "quantity": payload.record.quantity,
            "head": empty_to_null(&payload.record.head),
            "body": empty_to_null(&payload.record.body),
        });
        let created = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "record",
                record_payload,
            )
            .await?;
        let record_id = created.last_insert_rowid.ok_or_else(|| {
            KanbanActionError::Internal("Criacao do record nao retornou last_insert_rowid.".into())
        })?;

        self.sync_task_metadata(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            record_id,
            TaskMetadataInput {
                task_type: payload.task_type,
                categories: Some(payload.categories),
                start_at: payload.start_at,
                end_at: payload.end_at,
                estimate_seconds: payload.estimate_seconds,
                assignee_ids: Some(payload.assignee_ids),
                parent_id: Some(payload.parent_id),
            },
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "create-record".into(),
            message: "Record created.".into(),
            record_id: Some(record_id),
            await_stream_refresh: true,
            detail: json!({ "focus_record_id": record_id }),
        })
    }

    pub async fn update_record(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: UpdateRecordRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(
                session_token,
                instance_id,
                ActionPermission::WriteRecordAndMaybeTable { needs_table: true },
            )
            .await?;
        validate_task_type(payload.task_type.as_deref())?;
        validate_parent_id(payload.parent_id)?;
        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
        )
        .await?;
        self.validate_assignee_ids(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            &payload.assignee_ids,
        )
        .await?;
        if let Some(parent_id) = payload.parent_id {
            self.ensure_record_exists(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                parent_id,
            )
            .await?;
        }

        let record_payload = json!({
            "quantity": payload.quantity,
            "head": empty_to_null(&payload.head),
            "body": empty_to_null(&payload.body),
        });
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record",
            payload.record_id,
            record_payload,
        )
        .await?;

        self.sync_task_metadata(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
            TaskMetadataInput {
                task_type: payload.task_type,
                categories: Some(payload.categories),
                start_at: payload.start_at,
                end_at: payload.end_at,
                estimate_seconds: payload.estimate_seconds,
                assignee_ids: Some(payload.assignee_ids),
                parent_id: Some(payload.parent_id),
            },
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "update-record".into(),
            message: "Record updated.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: Value::Null,
        })
    }

    pub async fn update_record_body(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: UpdateRecordBodyRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(
                session_token,
                instance_id,
                ActionPermission::WriteRecordsOnly,
            )
            .await?;
        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
        )
        .await?;
        let body = payload.body.trim().to_string();

        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record",
            payload.record_id,
            json!({
                "body": if body.is_empty() {
                    Value::Null
                } else {
                    Value::String(body)
                },
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "update-record-body".into(),
            message: "Record body updated.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: json!({
                "record_id": payload.record_id,
            }),
        })
    }

    pub async fn move_record(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: MoveRecordRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(
                session_token,
                instance_id,
                ActionPermission::WriteRecordsOnly,
            )
            .await?;
        let record_payload = json!({ "quantity": payload.quantity });
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record",
            payload.record_id,
            record_payload,
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "move-record".into(),
            message: "Record moved.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: json!({ "quantity": payload.quantity }),
        })
    }

    pub async fn delete_record(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: DeleteRecordRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(
                session_token,
                instance_id,
                ActionPermission::WriteRecordsOnly,
            )
            .await?;
        self.delete_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record",
            payload.record_id,
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "delete-record".into(),
            message: "Record deleted.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: Value::Null,
        })
    }

    pub async fn load_record_detail(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: LoadRecordDetailRequest,
    ) -> Result<Value, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::ReadOnly)
            .await?;
        let current_user_id = self
            .resolve_current_user_id(session_token, instance_id, &resolved)
            .await
            .ok();
        let record = self
            .get_record_detail(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.record_id,
            )
            .await?;
        let detail = self
            .build_record_detail(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                &record,
                current_user_id,
            )
            .await?;
        let html = render_focus_card_html(&detail).into_string();

        Ok(json!({
            "record_id": payload.record_id,
            "target": "#kanban-focus-card",
            "mode": "replace",
            "html": html,
            "signals": {
                "focusedRecordId": payload.record_id,
                "focusMode": true
            },
            "detail": detail.as_json_value(),
        }))
    }

    pub async fn start_worklog(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: StartWorklogRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let current_user_id = self
            .resolve_current_user_id(session_token, instance_id, &resolved)
            .await?;
        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
        )
        .await?;

        let existing_intervals = self
            .list_record_worklogs(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        if existing_intervals.iter().any(|row| {
            row.record_id == payload.record_id
                && row.author_user_id == current_user_id
                && row.ended_at.is_none()
        }) {
            return Err(KanbanActionError::Validation(
                "Ja existe um worklog ativo para esse usuario nessa task.".into(),
            ));
        }

        let now = now_utc_string();
        let note = payload
            .note
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let created = self
            .create_table_row(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                "record_worklog",
                json!({
                    "record_id": payload.record_id,
                    "author_user_id": current_user_id,
                    "started_at": now,
                    "ended_at": null,
                    "last_heartbeat_at": now,
                    "seconds": 0,
                    "note": note,
                }),
            )
            .await?;
        let interval_id = created.last_insert_rowid.ok_or_else(|| {
            KanbanActionError::Internal("Criacao do worklog nao retornou last_insert_rowid.".into())
        })?;

        let interval = self
            .get_record_worklog_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                interval_id,
            )
            .await?;

        Ok(KanbanActionOutcome {
            action: "start-worklog".into(),
            message: "Worklog started.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: json!({
                "interval": interval.as_json_value(Some(Utc::now()))
            }),
        })
    }

    pub async fn stop_worklog(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: StopWorklogRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let current_user_id = self
            .resolve_current_user_id(session_token, instance_id, &resolved)
            .await?;
        let interval = self
            .get_record_worklog_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.interval_id,
            )
            .await?;

        if interval.record_id != payload.record_id {
            return Err(KanbanActionError::Validation(
                "Esse intervalo nao pertence ao record informado.".into(),
            ));
        }
        if interval.author_user_id != current_user_id {
            return Err(KanbanActionError::Forbidden(
                "O intervalo pertence a outro usuario.".into(),
            ));
        }
        if interval.ended_at.is_some() {
            return Err(KanbanActionError::Validation(
                "Esse worklog ja foi encerrado.".into(),
            ));
        }

        let ended_at = payload.ended_at.unwrap_or_else(now_utc_string);
        let seconds = derive_interval_seconds(&interval.started_at, &ended_at);
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_worklog",
            payload.interval_id,
            json!({
                "record_id": interval.record_id,
                "author_user_id": interval.author_user_id,
                "started_at": interval.started_at,
                "ended_at": ended_at,
                "last_heartbeat_at": ended_at,
                "seconds": seconds,
                "note": interval.note,
            }),
        )
        .await?;

        let actual_seconds = self
            .list_record_worklogs(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?
            .into_iter()
            .filter(|row| row.record_id == payload.record_id)
            .filter_map(|row| row.effective_seconds(None))
            .sum::<i64>();

        Ok(KanbanActionOutcome {
            action: "stop-worklog".into(),
            message: "Worklog stopped.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: json!({
                "interval_id": payload.interval_id,
                "actual_seconds": actual_seconds,
            }),
        })
    }

    pub async fn heartbeat_worklog(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: HeartbeatWorklogRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let current_user_id = self
            .resolve_current_user_id(session_token, instance_id, &resolved)
            .await?;
        let interval = self
            .get_record_worklog_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.interval_id,
            )
            .await?;

        if interval.record_id != payload.record_id {
            return Err(KanbanActionError::Validation(
                "Esse intervalo nao pertence ao record informado.".into(),
            ));
        }
        if interval.author_user_id != current_user_id {
            return Err(KanbanActionError::Forbidden(
                "O intervalo pertence a outro usuario.".into(),
            ));
        }
        if interval.ended_at.is_some() {
            return Err(KanbanActionError::Validation(
                "Esse worklog ja foi encerrado.".into(),
            ));
        }

        let heartbeat_at = now_utc_string();
        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_worklog",
            payload.interval_id,
            json!({
                "last_heartbeat_at": heartbeat_at,
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "heartbeat-worklog".into(),
            message: "Worklog heartbeat stored.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: false,
            detail: json!({
                "interval_id": payload.interval_id,
                "last_heartbeat_at": heartbeat_at,
            }),
        })
    }

    pub async fn load_form_options(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
    ) -> Result<Value, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::ReadOnly)
            .await?;
        let app_users = self
            .list_app_users(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        let extensions = self
            .list_record_extensions(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            KanbanActionError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        let parent_category_query = extract_parent_category_query(&card.widget_state);
        let active_parent_categories = parse_tag_list(&parent_category_query);
        let active_parent_category_keys = active_parent_categories
            .iter()
            .map(|value| value.to_lowercase())
            .collect::<BTreeSet<_>>();
        let snapshot_value = self
            .load_view_snapshot(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                resolved.view_id,
            )
            .await?;
        let snapshot = serde_json::from_value::<ViewSnapshotPayload>(snapshot_value)
            .map_err(|error| KanbanActionError::Internal(error.to_string()))?;

        let mut categories = Vec::new();
        let mut seen_categories = BTreeSet::new();
        let mut categories_by_record_id = BTreeMap::new();
        let mut parent_records_raw = Vec::new();
        let mut parent_records_have_categories = false;

        for extension in &extensions {
            if extension.namespace == "task.categories" {
                let normalized_categories = extract_categories(&extension.data_json)
                    .map(normalize_categories)
                    .unwrap_or_default();
                if !normalized_categories.is_empty() {
                    categories_by_record_id
                        .insert(extension.record_id, normalized_categories.clone());
                }
                for category in normalized_categories {
                    let normalized = category.trim().to_lowercase();
                    if !normalized.is_empty() && seen_categories.insert(normalized) {
                        categories.push(category);
                    }
                }
            }
        }

        for record in &snapshot.rows {
            let Some(id) = record
                .get("id")
                .and_then(|value| value.trim().parse::<i64>().ok())
            else {
                continue;
            };

            let head = record
                .get("head")
                .map(|value| value.trim().to_string())
                .unwrap_or_default();
            let head = if head.is_empty() {
                "Untitled".to_string()
            } else {
                head
            };
            let mut categories = categories_by_record_id
                .get(&id)
                .cloned()
                .unwrap_or_default();
            if let Some(primary_category) = record
                .get("primary_category")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                categories.push(primary_category);
            }
            if let Some(raw_categories) = record.get("categories_json")
                && let Ok(values) = serde_json::from_str::<Vec<String>>(raw_categories)
            {
                categories.extend(values);
            }
            categories = normalize_categories(categories);
            if !categories.is_empty() {
                parent_records_have_categories = true;
            }

            parent_records_raw.push((id, head, categories));
        }

        let use_parent_category_filter =
            !active_parent_category_keys.is_empty() && parent_records_have_categories;
        let mut parent_records = parent_records_raw
            .into_iter()
            .filter_map(|(id, head, categories)| {
                if use_parent_category_filter
                    && !categories.iter().any(|category| {
                        active_parent_category_keys.contains(&category.to_lowercase())
                    })
                {
                    return None;
                }

                Some(json!({
                    "id": id,
                    "head": head,
                    "categories": categories,
                }))
            })
            .collect::<Vec<_>>();
        parent_records.sort_by(|left, right| {
            left.get("head")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
                .cmp(
                    &right
                        .get("head")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase(),
                )
        });

        let assignees = app_users
            .iter()
            .map(|user| {
                json!({
                    "id": user.id,
                    "name": user.name,
                    "username": user.username,
                })
            })
            .collect::<Vec<_>>();

        Ok(json!({
            "taskTypes": VALID_TASK_TYPES,
            "quantities": [
                { "value": 0, "label": "Backlog" },
                { "value": -1, "label": "Next" },
                { "value": -2, "label": "WIP" },
                { "value": -3, "label": "Review" },
                { "value": 1, "label": "Done" }
            ],
            "assignees": assignees,
            "categories": categories,
            "parentRecords": parent_records,
            "parentCategoryQuery": if parent_records_have_categories {
                parent_category_query
            } else {
                String::new()
            },
        }))
    }

    async fn load_view_snapshot(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        view_id: u32,
    ) -> Result<Value, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .read_view_snapshot(&local_host_subject(), view_id)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()));
        }

        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_backend_request(
                &organ.base_url,
                bearer_token,
                Method::GET,
                &format!("/api/view/{view_id}/snapshot"),
                None,
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response)
            .await
    }

    pub async fn create_comment(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: CreateCommentRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let current_user_id = self
            .resolve_current_user_id(session_token, instance_id, &resolved)
            .await?;
        let body = payload.body.trim().to_string();
        if body.is_empty() {
            return Err(KanbanActionError::Validation(
                "Comentario vazio nao e valido.".into(),
            ));
        }

        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
        )
        .await?;

        self.create_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_comment",
            json!({
                "record_id": payload.record_id,
                "author_user_id": current_user_id,
                "body": body,
                "deleted_at": null,
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "create-comment".into(),
            message: "Comment created.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: Value::Null,
        })
    }

    pub async fn update_comment(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: UpdateCommentRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let body = payload.body.trim().to_string();
        if body.is_empty() {
            return Err(KanbanActionError::Validation(
                "Comentario vazio nao e valido.".into(),
            ));
        }
        let comment = self
            .get_record_comment_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.comment_id,
            )
            .await?;
        if comment.deleted_at.is_some() {
            return Err(KanbanActionError::Validation(
                "Esse comentario ja foi removido.".into(),
            ));
        }

        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_comment",
            payload.comment_id,
            json!({
                "body": body,
                "updated_at": now_utc_string(),
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "update-comment".into(),
            message: "Comment updated.".into(),
            record_id: Some(comment.record_id),
            await_stream_refresh: true,
            detail: json!({ "comment_id": payload.comment_id }),
        })
    }

    pub async fn delete_comment(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: DeleteCommentRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let comment = self
            .get_record_comment_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.comment_id,
            )
            .await?;

        self.update_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_comment",
            payload.comment_id,
            json!({
                "deleted_at": now_utc_string(),
                "updated_at": now_utc_string(),
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "delete-comment".into(),
            message: "Comment removed.".into(),
            record_id: Some(comment.record_id),
            await_stream_refresh: true,
            detail: json!({ "comment_id": payload.comment_id }),
        })
    }

    pub async fn create_resource_ref(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: CreateResourceRefRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let resource_path = payload.resource_path.trim().to_string();
        if resource_path.is_empty() {
            return Err(KanbanActionError::Validation(
                "resource_path vazio nao e valido.".into(),
            ));
        }

        self.ensure_record_exists(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            payload.record_id,
        )
        .await?;

        self.create_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_resource_ref",
            json!({
                "record_id": payload.record_id,
                "provider": payload.provider.trim(),
                "resource_kind": payload.resource_kind.trim(),
                "resource_path": resource_path,
                "title": payload.title.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
                "position": payload.position,
                "data_json": null,
            }),
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "create-resource-ref".into(),
            message: "Resource linked.".into(),
            record_id: Some(payload.record_id),
            await_stream_refresh: true,
            detail: Value::Null,
        })
    }

    pub async fn delete_resource_ref(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        payload: DeleteResourceRefRequest,
    ) -> Result<KanbanActionOutcome, KanbanActionError> {
        let resolved = self
            .resolve_instance(session_token, instance_id, ActionPermission::WriteTableOnly)
            .await?;
        let resource = self
            .get_record_resource_by_id(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
                payload.resource_ref_id,
            )
            .await?;

        self.delete_table_row(
            session_token,
            &resolved.organ,
            resolved.bearer_token.as_deref(),
            "record_resource_ref",
            payload.resource_ref_id,
        )
        .await?;

        Ok(KanbanActionOutcome {
            action: "delete-resource-ref".into(),
            message: "Resource removed.".into(),
            record_id: Some(resource.record_id),
            await_stream_refresh: true,
            detail: json!({ "resource_ref_id": payload.resource_ref_id }),
        })
    }

    async fn sync_task_metadata(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        input: TaskMetadataInput,
    ) -> Result<(), KanbanActionError> {
        self.sync_extension(
            session_token,
            organ,
            bearer_token,
            record_id,
            "task.type",
            task_type_payload(input.task_type)?,
        )
        .await?;
        self.sync_extension(
            session_token,
            organ,
            bearer_token,
            record_id,
            "task.categories",
            categories_payload(input.categories),
        )
        .await?;
        self.sync_extension(
            session_token,
            organ,
            bearer_token,
            record_id,
            "task.schedule",
            schedule_payload(input.start_at, input.end_at),
        )
        .await?;
        self.sync_extension(
            session_token,
            organ,
            bearer_token,
            record_id,
            "task.effort",
            effort_payload(input.estimate_seconds)?,
        )
        .await?;
        if let Some(assignee_ids) = input.assignee_ids {
            self.sync_assignees(session_token, organ, bearer_token, record_id, assignee_ids)
                .await?;
        }
        if let Some(parent_id) = input.parent_id {
            self.sync_parent_link(session_token, organ, bearer_token, record_id, parent_id)
                .await?;
        }
        Ok(())
    }

    async fn sync_extension(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        namespace: &str,
        payload: Option<Value>,
    ) -> Result<(), KanbanActionError> {
        let existing = self
            .find_record_extension(session_token, organ, bearer_token, record_id, namespace)
            .await?;
        match (existing, payload) {
            (Some(row), Some(data_json)) => {
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
                        "data_json": data_json.to_string(),
                    }),
                )
                .await?;
            }
            (None, Some(data_json)) => {
                self.create_table_row(
                    session_token,
                    organ,
                    bearer_token,
                    "record_extension",
                    json!({
                        "record_id": record_id,
                        "namespace": namespace,
                        "version": 1,
                        "data_json": data_json.to_string(),
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

    async fn sync_assignees(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        assignee_ids: Vec<i64>,
    ) -> Result<(), KanbanActionError> {
        let desired = normalize_integer_ids(assignee_ids);
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
                    "data_json": null,
                }),
            )
            .await?;
        }

        Ok(())
    }

    async fn sync_parent_link(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        parent_id: Option<i64>,
    ) -> Result<(), KanbanActionError> {
        let all_links = self
            .list_record_links(session_token, organ, bearer_token)
            .await?;
        let existing = all_links
            .iter()
            .filter(|row| {
                row.record_id == record_id
                    && row.link_type == "parent"
                    && row.target_table == "record"
            })
            .cloned()
            .collect::<Vec<_>>();

        if let Some(parent_id) = parent_id {
            if record_id == parent_id {
                return Err(KanbanActionError::Validation(
                    "Uma task nao pode ser pai dela mesma.".into(),
                ));
            }
            ensure_no_parent_cycle(record_id, parent_id, &all_links)?;
        }

        for row in &existing {
            if Some(row.target_id) != parent_id {
                self.delete_table_row(session_token, organ, bearer_token, "record_link", row.id)
                    .await?;
            }
        }

        if let Some(parent_id) = parent_id
            && !existing.iter().any(|row| row.target_id == parent_id)
        {
            self.create_table_row(
                session_token,
                organ,
                bearer_token,
                "record_link",
                json!({
                    "record_id": record_id,
                    "link_type": "parent",
                    "target_table": "record",
                    "target_id": parent_id,
                    "position": null,
                    "data_json": null,
                }),
            )
            .await?;
        }

        Ok(())
    }

    async fn build_record_detail(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record: &RecordRow,
        current_user_id: Option<i64>,
    ) -> Result<RecordDetailPayload, KanbanActionError> {
        let extensions = self
            .list_record_extensions(session_token, organ, bearer_token)
            .await?;
        let links = self
            .list_record_links(session_token, organ, bearer_token)
            .await?;
        let comments = self
            .list_record_comments(session_token, organ, bearer_token)
            .await?;
        let resources = self
            .list_record_resources(session_token, organ, bearer_token)
            .await?;
        let worklogs = self
            .list_record_worklogs(session_token, organ, bearer_token)
            .await?;
        let records = self
            .list_records(session_token, organ, bearer_token)
            .await?;
        let app_users = self
            .list_app_users(session_token, organ, bearer_token)
            .await?;

        let categories = extensions
            .iter()
            .find(|row| row.record_id == record.id && row.namespace == "task.categories")
            .and_then(|row| extract_categories(&row.data_json))
            .unwrap_or_default();
        let primary_category = categories.first().cloned();
        let task_type = extensions
            .iter()
            .find(|row| row.record_id == record.id && row.namespace == "task.type")
            .and_then(|row| extract_task_type(&row.data_json));
        let (start_at, end_at) = extensions
            .iter()
            .find(|row| row.record_id == record.id && row.namespace == "task.schedule")
            .map(|row| extract_schedule(&row.data_json))
            .unwrap_or((None, None));
        let estimate_seconds = extensions
            .iter()
            .find(|row| row.record_id == record.id && row.namespace == "task.effort")
            .and_then(|row| extract_estimate_seconds(&row.data_json));

        let assignees = links
            .iter()
            .filter(|row| {
                row.record_id == record.id
                    && row.link_type == "assigned_to"
                    && row.target_table == "app_user"
            })
            .filter_map(|link| {
                app_users
                    .iter()
                    .find(|user| user.id == link.target_id)
                    .map(|user| {
                        json!({
                            "id": user.id,
                            "name": user.name,
                        })
                    })
            })
            .collect::<Vec<_>>();

        let parent = links
            .iter()
            .find(|row| {
                row.record_id == record.id
                    && row.link_type == "parent"
                    && row.target_table == "record"
            })
            .and_then(|link| {
                records
                    .iter()
                    .find(|candidate| candidate.id == link.target_id)
                    .map(|parent| {
                        json!({
                            "id": parent.id,
                            "head": parent.head,
                        })
                    })
            });

        let children = links
            .iter()
            .filter(|row| {
                row.link_type == "parent"
                    && row.target_table == "record"
                    && row.target_id == record.id
            })
            .filter_map(|link| {
                records
                    .iter()
                    .find(|candidate| candidate.id == link.record_id)
                    .map(|child| {
                        json!({
                            "id": child.id,
                            "head": child.head,
                            "quantity": child.quantity,
                        })
                    })
            })
            .collect::<Vec<_>>();

        let comment_rows = comments
            .iter()
            .filter(|row| row.record_id == record.id && row.deleted_at.is_none())
            .cloned()
            .collect::<Vec<_>>();
        let comment_values = comment_rows
            .iter()
            .map(|comment| {
                json!({
                    "id": comment.id,
                    "author_user_id": comment.author_user_id,
                    "body": comment.body,
                    "created_at": comment.created_at,
                    "updated_at": comment.updated_at,
                })
            })
            .collect::<Vec<_>>();

        let resource_values = resources
            .iter()
            .filter(|row| row.record_id == record.id)
            .map(|resource| {
                json!({
                    "id": resource.id,
                    "provider": resource.provider,
                    "resource_kind": resource.resource_kind,
                    "resource_path": resource.resource_path,
                    "title": resource.title,
                    "position": resource.position,
                    "preview_url": format!(
                        "/host/integrations/servers/{}/files?path={}",
                        organ.id,
                        urlencoding::encode(&resource.resource_path),
                    ),
                })
            })
            .collect::<Vec<_>>();

        let now = Utc::now();
        let interval_rows = worklogs
            .iter()
            .filter(|row| row.record_id == record.id)
            .cloned()
            .collect::<Vec<_>>();
        let actual_seconds = interval_rows
            .iter()
            .filter_map(|row| row.effective_seconds(Some(now)))
            .sum::<i64>();
        let active_worklog_count = interval_rows
            .iter()
            .filter(|row| row.ended_at.is_none())
            .count() as i64;
        let intervals = interval_rows
            .iter()
            .map(|row| row.as_json_value(Some(now)))
            .collect::<Vec<_>>();
        let current_user_open_interval_id = current_user_id.and_then(|user_id| {
            interval_rows
                .iter()
                .find(|row| row.author_user_id == user_id && row.ended_at.is_none())
                .map(|row| row.id)
        });

        Ok(RecordDetailPayload {
            record_id: record.id,
            head: record.head.clone(),
            body: record.body.clone(),
            quantity: record.quantity,
            primary_category,
            categories,
            task_type,
            start_at,
            end_at,
            estimate_seconds,
            assignees,
            parent,
            children,
            comments: comment_values,
            resources: resource_values,
            actual_seconds,
            active_worklog_count,
            intervals,
            current_user_id,
            current_user_open_interval_id,
        })
    }

    async fn get_record_detail(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
    ) -> Result<RecordRow, KanbanActionError> {
        parse_record_row(
            self.get_table_row(session_token, organ, bearer_token, "record", record_id)
                .await?,
        )
    }

    async fn get_record_comment_by_id(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        comment_id: i64,
    ) -> Result<RecordCommentRow, KanbanActionError> {
        parse_record_comment_row(
            self.get_table_row(
                session_token,
                organ,
                bearer_token,
                "record_comment",
                comment_id,
            )
            .await?,
        )
    }

    async fn get_record_resource_by_id(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        resource_ref_id: i64,
    ) -> Result<RecordResourceRefRow, KanbanActionError> {
        parse_record_resource_ref_row(
            self.get_table_row(
                session_token,
                organ,
                bearer_token,
                "record_resource_ref",
                resource_ref_id,
            )
            .await?,
        )
    }

    async fn resolve_current_user_id(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        resolved: &ResolvedActionInstance,
    ) -> Result<i64, KanbanActionError> {
        if let Some(current_user_id) = self.cached_current_user_id(instance_id).await? {
            return Ok(current_user_id);
        }

        let app_users = self
            .list_app_users(
                session_token,
                &resolved.organ,
                resolved.bearer_token.as_deref(),
            )
            .await?;
        if app_users.is_empty() {
            return Err(KanbanActionError::Validation(
                "Nao existem app_user rows para autoria de worklog.".into(),
            ));
        }

        let current_user_id = resolved
            .username_hint
            .as_deref()
            .and_then(|hint| {
                app_users
                    .iter()
                    .find(|user| user.username == hint)
                    .map(|user| user.id)
            })
            .or_else(|| app_users.iter().map(|user| user.id).min())
            .ok_or_else(|| {
                KanbanActionError::Validation("Nao foi possivel resolver o app_user atual.".into())
            })?;

        self.persist_current_user_id(instance_id, current_user_id)
            .await?;
        Ok(current_user_id)
    }

    async fn cached_current_user_id(
        &self,
        instance_id: &str,
    ) -> Result<Option<i64>, KanbanActionError> {
        let board_state = self.board_state.snapshot().await;
        let Some(card) = find_board_card(&board_state, instance_id) else {
            return Ok(None);
        };
        Ok(card
            .widget_state
            .get("kanban_runtime")
            .and_then(Value::as_object)
            .and_then(|object| object.get("current_user_id"))
            .and_then(Value::as_i64)
            .filter(|value| *value > 0))
    }

    async fn persist_current_user_id(
        &self,
        instance_id: &str,
        current_user_id: i64,
    ) -> Result<(), KanbanActionError> {
        let mut board_state = self.board_state.snapshot().await;
        let card = find_board_card_mut(&mut board_state, instance_id).ok_or_else(|| {
            KanbanActionError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        let widget_state = ensure_object(&mut card.widget_state);
        let runtime = ensure_nested_object(widget_state, "kanban_runtime");
        runtime.insert("current_user_id".into(), json!(current_user_id));
        self.board_state
            .replace(board_state)
            .await
            .map_err(KanbanActionError::Internal)?;
        Ok(())
    }

    async fn find_record_extension(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
        namespace: &str,
    ) -> Result<Option<RecordExtensionRow>, KanbanActionError> {
        Ok(self
            .list_record_extensions(session_token, organ, bearer_token)
            .await?
            .into_iter()
            .find(|row| row.record_id == record_id && row.namespace == namespace))
    }

    async fn validate_assignee_ids(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        assignee_ids: &[i64],
    ) -> Result<(), KanbanActionError> {
        let desired = normalize_integer_ids(assignee_ids.to_vec());
        if desired.is_empty() {
            return Ok(());
        }

        let existing_ids = self
            .list_app_users(session_token, organ, bearer_token)
            .await?
            .into_iter()
            .map(|user| user.id)
            .collect::<std::collections::BTreeSet<_>>();
        if desired.iter().all(|id| existing_ids.contains(id)) {
            Ok(())
        } else {
            Err(KanbanActionError::Validation(
                "Assignee ids invalidos para app_user.".into(),
            ))
        }
    }

    async fn ensure_record_exists(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        record_id: i64,
    ) -> Result<(), KanbanActionError> {
        let _ = self
            .get_table_row(session_token, organ, bearer_token, "record", record_id)
            .await?;
        Ok(())
    }

    async fn resolve_instance(
        &self,
        session_token: Option<&str>,
        instance_id: &str,
        permission: ActionPermission,
    ) -> Result<ResolvedActionInstance, KanbanActionError> {
        let board_state = self.board_state.snapshot().await;
        let card = find_board_card(&board_state, instance_id).ok_or_else(|| {
            KanbanActionError::NotFound("Nao encontrei esse widget no board.".into())
        })?;
        validate_kanban_card(&card)?;
        permission.check(&card)?;

        let server_id = card.server_id.trim().to_string();
        if server_id.is_empty() {
            return Err(KanbanActionError::Misconfigured(
                "Kanban sem server_id configurado no host.".into(),
            ));
        }
        let view_id = card.view_id.filter(|value| *value > 0).ok_or_else(|| {
            KanbanActionError::Misconfigured(
                "Kanban sem view_id valido configurado no host.".into(),
            )
        })?;

        let organ = self
            .organs
            .get(&server_id)
            .await
            .map_err(KanbanActionError::Internal)?
            .ok_or_else(|| {
                KanbanActionError::Misconfigured(
                    "O server_id configurado no Kanban nao existe mais.".into(),
                )
            })?;
        let requires_auth = organ_requires_auth(&organ, self.local_auth_required);
        let (bearer_token, username_hint) = if requires_auth {
            let session = self
                .auth
                .server_session(session_token, &server_id)
                .await
                .ok_or_else(|| {
                    KanbanActionError::Unauthorized(
                        "Essa sessao local nao esta conectada a esse servidor.".into(),
                    )
                })?;
            (
                Some(session.bearer_token),
                Some(session.username_hint).filter(|value| !value.trim().is_empty()),
            )
        } else {
            (None, None)
        };

        Ok(ResolvedActionInstance {
            organ,
            bearer_token,
            username_hint,
            view_id,
        })
    }

    async fn get_table_row(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
        id: i64,
    ) -> Result<Value, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .get_table_row(&local_host_subject(), table, id)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()));
        }
        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::GET,
                table,
                Some(id),
                None,
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
        self.read_remote_json(session_token, &organ.id, response)
            .await
    }

    async fn list_table_rows(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        table: &str,
    ) -> Result<Value, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            return self
                .backend
                .list_table_rows(&local_host_subject(), table)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()));
        }
        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::GET,
                table,
                None,
                None,
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
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
    ) -> Result<MutationResponse, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload
                .as_object()
                .ok_or_else(|| KanbanActionError::Validation("Payload JSON invalido.".into()))?;
            let outcome = self
                .backend
                .create_table_row(&local_host_subject(), table, object)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                rows_affected: outcome.rows_affected,
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::POST,
                table,
                None,
                Some(payload),
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
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
    ) -> Result<MutationResponse, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let object = payload
                .as_object()
                .ok_or_else(|| KanbanActionError::Validation("Payload JSON invalido.".into()))?;
            let outcome = self
                .backend
                .update_table_row(&local_host_subject(), table, id, object)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                rows_affected: outcome.rows_affected,
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::PATCH,
                table,
                Some(id),
                Some(payload),
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
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
    ) -> Result<MutationResponse, KanbanActionError> {
        if !organ_requires_auth(organ, self.local_auth_required) {
            let outcome = self
                .backend
                .delete_table_row(&local_host_subject(), table, id)
                .await
                .map_err(|error| KanbanActionError::Internal(error.to_string()))?;
            return Ok(MutationResponse {
                rows_affected: outcome.rows_affected,
                last_insert_rowid: outcome.last_insert_rowid,
            });
        }
        let bearer_token = bearer_token
            .ok_or_else(|| KanbanActionError::Unauthorized("Sessao remota ausente.".into()))?;
        let response = self
            .manas
            .send_table_request(
                &organ.base_url,
                bearer_token,
                Method::DELETE,
                table,
                Some(id),
                None,
            )
            .await
            .map_err(KanbanActionError::BadGateway)?;
        parse_mutation_response(
            &self
                .read_remote_json(session_token, &organ.id, response)
                .await?,
        )
    }

    async fn read_remote_json(
        &self,
        session_token: Option<&str>,
        server_id: &str,
        response: reqwest::Response,
    ) -> Result<Value, KanbanActionError> {
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.auth
                .expire_server_session(
                    session_token,
                    server_id,
                    "Sessao remota expirada. Conecte esse servidor novamente.",
                )
                .await;
            return Err(KanbanActionError::Unauthorized(
                "Sessao remota expirada. Conecte esse servidor novamente.".into(),
            ));
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(KanbanActionError::BadGateway(if body.trim().is_empty() {
                format!("Servidor remoto recusou a operacao com status {status}.")
            } else {
                body
            }));
        }

        response
            .json::<Value>()
            .await
            .map_err(|error| KanbanActionError::BadGateway(error.to_string()))
    }

    async fn list_record_extensions(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordExtensionRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record_extension")
                .await?,
        )
    }

    async fn list_record_links(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordLinkRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record_link")
                .await?,
        )
    }

    async fn list_app_users(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<AppUserRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "app_user")
                .await?,
        )
    }

    async fn list_records(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record")
                .await?,
        )
    }

    async fn list_record_comments(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordCommentRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record_comment")
                .await?,
        )
    }

    async fn list_record_resources(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordResourceRefRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record_resource_ref")
                .await?,
        )
    }

    async fn list_record_worklogs(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
    ) -> Result<Vec<RecordWorklogRow>, KanbanActionError> {
        parse_rows(
            self.list_table_rows(session_token, organ, bearer_token, "record_worklog")
                .await?,
        )
    }

    async fn get_record_worklog_by_id(
        &self,
        session_token: Option<&str>,
        organ: &Organ,
        bearer_token: Option<&str>,
        interval_id: i64,
    ) -> Result<RecordWorklogRow, KanbanActionError> {
        parse_record_worklog_row(
            self.get_table_row(
                session_token,
                organ,
                bearer_token,
                "record_worklog",
                interval_id,
            )
            .await?,
        )
    }
}

#[derive(Debug, Clone)]
pub enum KanbanActionError {
    NotFound(String),
    Misconfigured(String),
    Unauthorized(String),
    Forbidden(String),
    Validation(String),
    BadGateway(String),
    Internal(String),
}

#[derive(Debug, Clone)]
pub struct KanbanActionOutcome {
    pub action: String,
    pub message: String,
    pub record_id: Option<i64>,
    pub await_stream_refresh: bool,
    pub detail: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRecordRequest {
    pub record: RecordDraft,
    pub task_type: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub estimate_seconds: Option<i64>,
    #[serde(default)]
    pub assignee_ids: Vec<i64>,
    pub parent_id: Option<i64>,
}

impl CreateRecordRequest {
    fn needs_write_table(&self) -> bool {
        self.task_type.is_some()
            || !self.categories.is_empty()
            || self.start_at.is_some()
            || self.end_at.is_some()
            || self.estimate_seconds.is_some()
            || !self.assignee_ids.is_empty()
            || self.parent_id.is_some()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecordRequest {
    pub record_id: i64,
    pub head: Option<String>,
    pub body: Option<String>,
    pub quantity: i64,
    pub task_type: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
    pub estimate_seconds: Option<i64>,
    #[serde(default)]
    pub assignee_ids: Vec<i64>,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecordBodyRequest {
    pub record_id: i64,
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRecordRequest {
    pub record_id: i64,
    pub quantity: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRecordRequest {
    pub record_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadRecordDetailRequest {
    pub record_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartWorklogRequest {
    pub record_id: i64,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopWorklogRequest {
    pub record_id: i64,
    pub interval_id: i64,
    pub ended_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatWorklogRequest {
    pub record_id: i64,
    pub interval_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCommentRequest {
    pub record_id: i64,
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommentRequest {
    pub comment_id: i64,
    pub body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCommentRequest {
    pub comment_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResourceRefRequest {
    pub record_id: i64,
    pub provider: String,
    pub resource_kind: String,
    pub resource_path: String,
    pub title: Option<String>,
    pub position: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResourceRefRequest {
    pub resource_ref_id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordDraft {
    pub head: Option<String>,
    pub body: Option<String>,
    pub quantity: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordRow {
    id: i64,
    quantity: f64,
    head: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordExtensionRow {
    id: i64,
    record_id: i64,
    namespace: String,
    data_json: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordLinkRow {
    id: i64,
    record_id: i64,
    link_type: String,
    target_table: String,
    target_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct AppUserRow {
    id: i64,
    name: String,
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordCommentRow {
    id: i64,
    record_id: i64,
    author_user_id: Option<i64>,
    body: String,
    created_at: String,
    updated_at: String,
    deleted_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordResourceRefRow {
    id: i64,
    record_id: i64,
    provider: String,
    resource_kind: String,
    resource_path: String,
    title: Option<String>,
    position: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RecordWorklogRow {
    id: i64,
    record_id: i64,
    author_user_id: i64,
    started_at: String,
    ended_at: Option<String>,
    last_heartbeat_at: Option<String>,
    seconds: Option<f64>,
    note: Option<String>,
}

struct ResolvedActionInstance {
    organ: Organ,
    bearer_token: Option<String>,
    username_hint: Option<String>,
    view_id: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewSnapshotPayload {
    rows: Vec<BTreeMap<String, String>>,
}

struct RecordDetailPayload {
    record_id: i64,
    head: Option<String>,
    body: Option<String>,
    quantity: f64,
    primary_category: Option<String>,
    categories: Vec<String>,
    task_type: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    assignees: Vec<Value>,
    parent: Option<Value>,
    children: Vec<Value>,
    comments: Vec<Value>,
    resources: Vec<Value>,
    actual_seconds: i64,
    active_worklog_count: i64,
    intervals: Vec<Value>,
    current_user_id: Option<i64>,
    current_user_open_interval_id: Option<i64>,
}

impl RecordDetailPayload {
    fn as_json_value(&self) -> Value {
        json!({
            "record_id": self.record_id,
            "head": self.head,
            "body": self.body,
            "quantity": self.quantity,
            "primary_category": self.primary_category,
            "categories": self.categories,
            "task_type": self.task_type,
            "start_at": self.start_at,
            "end_at": self.end_at,
            "estimate_seconds": self.estimate_seconds,
            "assignees": self.assignees,
            "parent": self.parent,
            "children": self.children,
            "comments": self.comments,
            "resources": self.resources,
            "worklog": {
                "actual_seconds": self.actual_seconds,
                "active_worklog_count": self.active_worklog_count,
                "intervals": self.intervals,
                "current_user_id": self.current_user_id,
                "current_user_open_interval_id": self.current_user_open_interval_id,
            }
        })
    }
}

struct MutationResponse {
    #[allow(dead_code)]
    rows_affected: u64,
    last_insert_rowid: Option<i64>,
}

struct TaskMetadataInput {
    task_type: Option<String>,
    categories: Option<Vec<String>>,
    start_at: Option<String>,
    end_at: Option<String>,
    estimate_seconds: Option<i64>,
    assignee_ids: Option<Vec<i64>>,
    parent_id: Option<Option<i64>>,
}

enum ActionPermission {
    ReadOnly,
    WriteRecordsOnly,
    WriteTableOnly,
    WriteRecordAndMaybeTable { needs_table: bool },
}

impl ActionPermission {
    fn check(&self, card: &BoardCard) -> Result<(), KanbanActionError> {
        let has_write_records = card
            .permissions
            .iter()
            .any(|value| value == "write_records");
        let has_write_table = card.permissions.iter().any(|value| value == "write_table");
        match self {
            ActionPermission::ReadOnly => Ok(()),
            ActionPermission::WriteRecordsOnly if !has_write_records => {
                Err(KanbanActionError::Forbidden(
                    "Esse Kanban nao declara permissao write_records.".into(),
                ))
            }
            ActionPermission::WriteTableOnly if !has_write_table => {
                Err(KanbanActionError::Forbidden(
                    "Esse Kanban nao declara permissao write_table.".into(),
                ))
            }
            ActionPermission::WriteRecordAndMaybeTable { needs_table }
                if !has_write_records || (*needs_table && !has_write_table) =>
            {
                Err(KanbanActionError::Forbidden(
                    "Esse Kanban nao declara as permissoes necessarias.".into(),
                ))
            }
            _ => Ok(()),
        }
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

fn validate_kanban_card(card: &BoardCard) -> Result<(), KanbanActionError> {
    if card.kind.trim() != "package" {
        return Err(KanbanActionError::Misconfigured(
            "Esse widget nao e um package oficial.".into(),
        ));
    }
    if !is_supported_kanban_package_filename(&card.package_name) {
        return Err(KanbanActionError::Misconfigured(
            "Esse widget nao usa o package oficial do Kanban.".into(),
        ));
    }
    Ok(())
}

fn validate_task_type(task_type: Option<&str>) -> Result<(), KanbanActionError> {
    let Some(task_type) = task_type.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if VALID_TASK_TYPES.contains(&task_type) {
        Ok(())
    } else {
        Err(KanbanActionError::Validation(
            "task_type invalido para o Kanban.".into(),
        ))
    }
}

fn task_type_payload(task_type: Option<String>) -> Result<Option<Value>, KanbanActionError> {
    let task_type = task_type
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    validate_task_type(task_type.as_deref())?;
    Ok(task_type.map(|value| json!({ "task_type": value })))
}

fn categories_payload(categories: Option<Vec<String>>) -> Option<Value> {
    categories.map(normalize_categories).and_then(|categories| {
        if categories.is_empty() {
            None
        } else {
            Some(json!({ "categories": categories }))
        }
    })
}

fn schedule_payload(start_at: Option<String>, end_at: Option<String>) -> Option<Value> {
    if start_at.is_none() && end_at.is_none() {
        None
    } else {
        Some(json!({
            "start_at": start_at,
            "end_at": end_at,
        }))
    }
}

fn effort_payload(estimate_seconds: Option<i64>) -> Result<Option<Value>, KanbanActionError> {
    match estimate_seconds {
        Some(value) if value < 0 => Err(KanbanActionError::Validation(
            "estimate_seconds nao pode ser negativo.".into(),
        )),
        Some(value) => Ok(Some(json!({ "estimate_seconds": value }))),
        None => Ok(None),
    }
}

fn validate_parent_id(parent_id: Option<i64>) -> Result<(), KanbanActionError> {
    if parent_id.is_some_and(|value| value <= 0) {
        Err(KanbanActionError::Validation("parent_id invalido.".into()))
    } else {
        Ok(())
    }
}

fn normalize_categories(categories: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut normalized = Vec::new();
    for category in categories {
        let trimmed = category.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.to_lowercase();
        if seen.insert(key) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

fn parse_tag_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn extract_parent_category_query(widget_state: &Value) -> String {
    let Some(filters) = widget_state.get("filters") else {
        return String::new();
    };
    let Ok(rows) = serde_json::from_value::<Vec<RawKanbanFilterRow>>(filters.clone()) else {
        return String::new();
    };

    let mut seen = BTreeSet::new();
    let mut categories = Vec::new();
    for row in rows {
        if row.field.trim() != "categories_any_json" || row.operator.trim() != "any_of" {
            continue;
        }
        let Some(values) = row.value.as_array() else {
            continue;
        };
        for value in values {
            let Some(category) = value.as_str() else {
                continue;
            };
            let trimmed = category.trim();
            if trimmed.is_empty() {
                continue;
            }
            let normalized = trimmed.to_lowercase();
            if seen.insert(normalized) {
                categories.push(trimmed.to_string());
            }
        }
    }

    categories.join(", ")
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

fn empty_to_null(value: &Option<String>) -> Value {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null)
}

fn parse_rows<T>(value: Value) -> Result<Vec<T>, KanbanActionError>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value::<Vec<T>>(value)
        .map_err(|error| KanbanActionError::Internal(format!("Resposta JSON invalida: {error}")))
}

fn parse_record_row(value: Value) -> Result<RecordRow, KanbanActionError> {
    serde_json::from_value::<RecordRow>(value)
        .map_err(|error| KanbanActionError::Internal(format!("Resposta JSON invalida: {error}")))
}

fn parse_record_comment_row(value: Value) -> Result<RecordCommentRow, KanbanActionError> {
    serde_json::from_value::<RecordCommentRow>(value)
        .map_err(|error| KanbanActionError::Internal(format!("Resposta JSON invalida: {error}")))
}

fn parse_record_resource_ref_row(value: Value) -> Result<RecordResourceRefRow, KanbanActionError> {
    serde_json::from_value::<RecordResourceRefRow>(value)
        .map_err(|error| KanbanActionError::Internal(format!("Resposta JSON invalida: {error}")))
}

fn parse_record_worklog_row(value: Value) -> Result<RecordWorklogRow, KanbanActionError> {
    serde_json::from_value::<RecordWorklogRow>(value)
        .map_err(|error| KanbanActionError::Internal(format!("Resposta JSON invalida: {error}")))
}

fn parse_mutation_response(value: &Value) -> Result<MutationResponse, KanbanActionError> {
    let object = value
        .as_object()
        .ok_or_else(|| KanbanActionError::Internal("Resposta invalida de mutacao.".into()))?;
    Ok(MutationResponse {
        rows_affected: object
            .get("rows_affected")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        last_insert_rowid: object.get("last_insert_rowid").and_then(Value::as_i64),
    })
}

fn ensure_no_parent_cycle(
    record_id: i64,
    parent_id: i64,
    all_links: &[RecordLinkRow],
) -> Result<(), KanbanActionError> {
    let mut parent_map = std::collections::BTreeMap::new();
    for row in all_links {
        if row.link_type == "parent" && row.target_table == "record" {
            parent_map.insert(row.record_id, row.target_id);
        }
    }
    parent_map.insert(record_id, parent_id);

    let mut cursor = Some(parent_id);
    let mut visited = std::collections::BTreeSet::new();
    while let Some(current) = cursor {
        if current == record_id || !visited.insert(current) {
            return Err(KanbanActionError::Validation(
                "A hierarquia de tasks criaria um ciclo.".into(),
            ));
        }
        cursor = parent_map.get(&current).copied();
    }
    Ok(())
}

fn local_host_subject() -> AuthSubject {
    AuthSubject {
        user_id: 0,
        username: "local-host".into(),
        role_id: 0,
        role: "admin".into(),
    }
}

fn ensure_object(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(serde_json::Map::new());
    }
    value
        .as_object_mut()
        .expect("widget state object should exist")
}

fn ensure_nested_object<'a>(
    object: &'a mut serde_json::Map<String, Value>,
    key: &str,
) -> &'a mut serde_json::Map<String, Value> {
    let entry = object
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }
    entry.as_object_mut().expect("nested object should exist")
}

fn extract_categories(data_json: &str) -> Option<Vec<String>> {
    let value = serde_json::from_str::<Value>(data_json).ok()?;
    let categories = value
        .get("categories")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(categories)
}

fn extract_task_type(data_json: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(data_json).ok()?;
    value
        .get("task_type")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_schedule(data_json: &str) -> (Option<String>, Option<String>) {
    let Ok(value) = serde_json::from_str::<Value>(data_json) else {
        return (None, None);
    };
    (
        value
            .get("start_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        value
            .get("end_at")
            .and_then(Value::as_str)
            .map(str::to_string),
    )
}

fn extract_estimate_seconds(data_json: &str) -> Option<i64> {
    let value = serde_json::from_str::<Value>(data_json).ok()?;
    value.get("estimate_seconds").and_then(Value::as_i64)
}

fn now_utc_string() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc))
}

fn derive_interval_seconds(started_at: &str, ended_at: &str) -> Option<i64> {
    let started = parse_utc(started_at)?;
    let ended = parse_utc(ended_at)?;
    Some((ended - started).num_seconds().max(0))
}

impl RecordWorklogRow {
    fn effective_seconds(&self, now: Option<DateTime<Utc>>) -> Option<i64> {
        if let Some(seconds) = self.seconds {
            return Some(seconds as i64);
        }
        let started = parse_utc(&self.started_at)?;
        let ended = self.ended_at.as_deref().and_then(parse_utc).or(now)?;
        Some((ended - started).num_seconds().max(0))
    }

    fn as_json_value(&self, now: Option<DateTime<Utc>>) -> Value {
        json!({
            "id": self.id,
            "author_user_id": self.author_user_id,
            "started_at": self.started_at,
            "ended_at": self.ended_at,
            "last_heartbeat_at": self.last_heartbeat_at,
            "seconds": self.effective_seconds(now),
            "note": self.note,
        })
    }
}

fn render_focus_card_html(detail: &RecordDetailPayload) -> maud::Markup {
    let head = detail.head.as_deref().unwrap_or("Untitled");
    let task_type = detail.task_type.as_deref().unwrap_or("none");
    html! {
        section.kanban-focus-card {
            header.kanban-focus-card__header {
                h2.kanban-focus-card__title { (head) }
                .kanban-focus-card__meta {
                    span { "Type: " (task_type) }
                    span { "Quantity: " (detail.quantity) }
                    @if let Some(primary_category) = detail.primary_category.as_deref() {
                        span { "Category: " (primary_category) }
                    }
                    @if let Some(start_at) = detail.start_at.as_deref() {
                        span { "Start: " (start_at) }
                    }
                    @if let Some(end_at) = detail.end_at.as_deref() {
                        span { "End: " (end_at) }
                    }
                    @if let Some(estimate_seconds) = detail.estimate_seconds {
                        span { "Estimate: " (estimate_seconds) "s" }
                    }
                }
                .headerActions {
                    button.toolbarBtn type="button" data-open-edit=(detail.record_id) data-on:click="$focusMarkdown = false; window.KanbanWidget?.openEditSheet(Number(evt.currentTarget.dataset.openEdit || 0))" { "Edit" }
                    button.toolbarBtn type="button" data-delete-record=(detail.record_id) data-on:click="window.KanbanWidget?.deleteRecordFromUi(Number(evt.currentTarget.dataset.deleteRecord || 0))" { "Delete" }
                }
            }
            @if !detail.categories.is_empty() {
                .tagRow {
                    @for category in &detail.categories {
                        span.pill { (category) }
                    }
                }
            }
            @if !detail.assignees.is_empty() {
                section.kanban-focus-card__resources {
                    h3 { "Assignees" }
                    ul {
                        @for assignee in &detail.assignees {
                            li { (assignee.get("name").and_then(Value::as_str).unwrap_or("Unknown")) }
                        }
                    }
                }
            }
            @if let Some(parent) = &detail.parent {
                @if let Some(parent_head) = parent.get("head").and_then(Value::as_str) {
                    p.kanban-focus-card__parent {
                        "Parent: "
                        a href="#" data-record-link=(parent.get("id").and_then(Value::as_i64).unwrap_or_default()) data-on:click__prevent="window.KanbanWidget?.loadRecordDetail(Number(evt.currentTarget.dataset.recordLink || 0))" { (parent_head) }
                    }
                }
            }
            @if let Some(body) = detail.body.as_deref().filter(|value| !value.trim().is_empty()) {
                section.kanban-focus-card__body-wrap {
                    .sheetHeader {
                        .headerMeta {
                            h3 { "Body" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-on:click="$focusMarkdown = !$focusMarkdown" { "Toggle Markdown" }
                        }
                    }
                    pre.kanban-focus-card__body data-focus-body-raw="" data-show="!$focusMarkdown" { (body) }
                    article.kanban-focus-card__body-preview.markdownRender
                        data-focus-body-preview=""
                        data-record-id=(detail.record_id)
                        data-markdown-source=(body)
                        data-show="$focusMarkdown"
                    {}
                }
            }
            @if !detail.children.is_empty() {
                section.kanban-focus-card__children {
                    h3 { "Children" }
                    ul {
                        @for child in &detail.children {
                            li {
                                a href="#" data-record-link=(child.get("id").and_then(Value::as_i64).unwrap_or_default()) data-on:click__prevent="window.KanbanWidget?.loadRecordDetail(Number(evt.currentTarget.dataset.recordLink || 0))" {
                                    (child.get("head").and_then(Value::as_str).unwrap_or("Untitled"))
                                }
                            }
                        }
                    }
                }
            }
            @if !detail.comments.is_empty() {
                section.kanban-focus-card__comments {
                    .sheetHeader {
                        .headerMeta {
                            h3 { "Comments" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-create-comment=(detail.record_id) data-on:click="window.KanbanWidget?.createComment(Number(evt.currentTarget.dataset.createComment || 0))" { "Add comment" }
                        }
                    }
                    @for comment in &detail.comments {
                        article.kanban-focus-card__comment {
                            .sheetHeader {
                                .headerMeta {
                                    p.small {
                                        (comment.get("created_at").and_then(Value::as_str).unwrap_or(""))
                                    }
                                }
                                .headerActions {
                                    button.toolbarBtn type="button" data-edit-comment=(comment.get("id").and_then(Value::as_i64).unwrap_or_default()) data-on:click="window.KanbanWidget?.editComment(Number(evt.currentTarget.dataset.editComment || 0))" { "Edit" }
                                    button.toolbarBtn type="button" data-delete-comment=(comment.get("id").and_then(Value::as_i64).unwrap_or_default()) data-on:click="window.KanbanWidget?.deleteComment(Number(evt.currentTarget.dataset.deleteComment || 0))" { "Delete" }
                                }
                            }
                            p { (comment.get("body").and_then(Value::as_str).unwrap_or("")) }
                        }
                    }
                }
            } @else {
                section.kanban-focus-card__comments {
                    .sheetHeader {
                        .headerMeta {
                            h3 { "Comments" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-create-comment=(detail.record_id) data-on:click="window.KanbanWidget?.createComment(Number(evt.currentTarget.dataset.createComment || 0))" { "Add comment" }
                        }
                    }
                    p.small { "No comments yet." }
                }
            }
            @if !detail.resources.is_empty() {
                section.kanban-focus-card__resources {
                    .sheetHeader {
                        .headerMeta {
                            h3 { "Resources" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-add-resource=(detail.record_id) data-on:click="window.KanbanWidget?.createResourceRef(Number(evt.currentTarget.dataset.addResource || 0))" { "Link resource" }
                        }
                    }
                    ul {
                        @for resource in &detail.resources {
                            li {
                                @if resource.get("resource_kind").and_then(Value::as_str) == Some("image") {
                                    img
                                        class="kanban-focus-card__image"
                                        loading="lazy"
                                        src=(resource.get("preview_url").and_then(Value::as_str).unwrap_or(""))
                                        alt=(resource.get("title").and_then(Value::as_str).unwrap_or(resource.get("resource_path").and_then(Value::as_str).unwrap_or("Resource image")));
                                }
                                span { (resource.get("title").and_then(Value::as_str).unwrap_or(resource.get("resource_path").and_then(Value::as_str).unwrap_or(""))) }
                                button.toolbarBtn type="button" data-delete-resource=(resource.get("id").and_then(Value::as_i64).unwrap_or_default()) data-on:click="window.KanbanWidget?.deleteResourceRef(Number(evt.currentTarget.dataset.deleteResource || 0))" { "Remove" }
                            }
                        }
                    }
                }
            } @else {
                section.kanban-focus-card__resources {
                    .sheetHeader {
                        .headerMeta {
                            h3 { "Resources" }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-add-resource=(detail.record_id) data-on:click="window.KanbanWidget?.createResourceRef(Number(evt.currentTarget.dataset.addResource || 0))" { "Link resource" }
                        }
                    }
                    p.small { "No linked resources." }
                }
            }
            section.kanban-focus-card__worklog {
                .sheetHeader {
                    .headerMeta {
                        h3 { "Worklog" }
                    }
                    .headerActions {
                        @if let Some(interval_id) = detail.current_user_open_interval_id {
                            button.toolbarBtn type="button" data-stop-worklog=(interval_id) data-record-id=(detail.record_id) data-on:click="window.KanbanWidget?.stopWorklog(Number(evt.currentTarget.dataset.recordId || 0), Number(evt.currentTarget.dataset.stopWorklog || 0))" { "Stop" }
                        } @else {
                            button.toolbarBtn type="button" data-start-worklog=(detail.record_id) data-on:click="window.KanbanWidget?.startWorklog(Number(evt.currentTarget.dataset.startWorklog || 0))" { "Start" }
                        }
                    }
                }
                p { "Actual seconds: " (detail.actual_seconds) }
                p { "Active intervals: " (detail.active_worklog_count) }
                @if !detail.intervals.is_empty() {
                    ul {
                        @for interval in &detail.intervals {
                            li {
                                span {
                                    "User "
                                    (interval.get("author_user_id").and_then(Value::as_i64).unwrap_or_default())
                                    " · "
                                    (interval.get("started_at").and_then(Value::as_str).unwrap_or(""))
                                }
                                @if interval.get("ended_at").and_then(Value::as_str).is_none() {
                                    span.pill { "running" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
