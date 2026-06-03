use crate::infrastructure::backend_api_store::{
    ApiTable, BackendApiStore, TableCreateSchemaResponse, TableListQuery, validate_file_key,
};
use ::application::karma::{karma_deliver, refresh_karma_cache};
use ::application::{
    auth::{AuthService, AuthSubject},
    subscription::{SubscriptionHandle, SubscriptionRegistry},
    view::ViewReadService,
    write,
};
use injection::cross_cutting::InjectedServices;
use persistence::{
    storage::{DownloadedObject, StorageList},
    write_coordinator::{SqlParameter, WriteOutcome},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Error, ErrorKind},
    sync::Arc,
    time::Duration,
};
use utils::file_access::{
    FileAccessAction, FileAccessClaims, decode_file_access_token, issue_file_access_token,
};

const FILE_LINK_TTL_SECS: u64 = 300;

#[derive(Clone)]
pub struct BackendApiService {
    services: InjectedServices,
    store: BackendApiStore,
    auth: AuthService,
    subscriptions: SubscriptionRegistry,
    view_reads: ViewReadService,
    jwt_secret: Arc<String>,
}

pub struct FileLink {
    pub method: &'static str,
    pub url: String,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordQuantityBatchUpdateRequest {
    pub rows: Vec<RecordQuantityBatchUpdateRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordQuantityBatchUpdateRow {
    pub id: i64,
    pub quantity: f64,
}

impl BackendApiService {
    pub fn new(services: InjectedServices, jwt_secret: Arc<String>) -> Self {
        let auth = AuthService::new(services.clone(), jwt_secret.clone());
        let view_reads = ::application::view::ViewReadService::new(services.clone());
        let subscriptions = SubscriptionRegistry::new(view_reads.clone(), services.writer.clone());
        let store = BackendApiStore::new(services.clone());

        Self {
            services,
            store,
            auth,
            subscriptions,
            view_reads,
            jwt_secret,
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<String, Error> {
        self.auth.login(username, password).await
    }

    pub async fn authenticate_authorization(
        &self,
        authorization: &str,
    ) -> Result<AuthSubject, Error> {
        self.auth.authenticate_authorization(authorization).await
    }

    pub async fn list_table_rows(
        &self,
        _claims: &AuthSubject,
        table_name: &str,
    ) -> Result<Value, Error> {
        self.list_table_rows_filtered(_claims, table_name, &TableListQuery::default())
            .await
    }

    pub async fn list_table_rows_filtered(
        &self,
        _claims: &AuthSubject,
        table_name: &str,
        query: &TableListQuery,
    ) -> Result<Value, Error> {
        let table = self.store.parse_table(table_name)?;
        self.store.list_table_rows_filtered(table, query).await
    }

    pub async fn get_table_row(
        &self,
        _claims: &AuthSubject,
        table_name: &str,
        id: i64,
    ) -> Result<Value, Error> {
        let table = self.store.parse_table(table_name)?;
        self.store.get_table_row(table, id).await
    }

    pub fn table_create_schema_response(
        &self,
        preferred_table: Option<&str>,
    ) -> TableCreateSchemaResponse {
        self.store.table_create_schema_response(preferred_table)
    }

    pub async fn create_table_row(
        &self,
        claims: &AuthSubject,
        table_name: &str,
        object: &Map<String, Value>,
    ) -> Result<WriteOutcome, Error> {
        let table = self.store.parse_table(table_name)?;

        match table {
            ApiTable::View => {
                let (sql, params) = self.store.build_standard_insert(table, object)?;
                self.services
                    .writer
                    .execute_view_insert_returning_id(sql, params)
                    .await
            }
            ApiTable::Record => {
                let (object, confirmed) = strip_karma_confirmation(table, object);
                self.require_karma_loop_confirmation(table, None, &object, confirmed)
                    .await?;
                let (sql, params) = self.store.build_standard_insert(table, &object)?;
                write::execute_record_insert_returning_id(self.services.clone(), sql, params).await
            }
            ApiTable::RecordExtension
            | ApiTable::RecordLink
            | ApiTable::RecordComment
            | ApiTable::RecordWorklog
            | ApiTable::RecordResourceRef
            | ApiTable::Command
            | ApiTable::Query
            | ApiTable::Frequency
            | ApiTable::KarmaCondition
            | ApiTable::KarmaConsequence
            | ApiTable::Karma
            | ApiTable::Configuration => {
                let (object, confirmed) = strip_karma_confirmation(table, object);
                self.require_karma_loop_confirmation(table, None, &object, confirmed)
                    .await?;
                let (sql, params) = self.store.build_standard_insert(table, &object)?;
                let outcome = self
                    .services
                    .writer
                    .execute_statement_returning_id(sql, params)
                    .await?;
                if outcome.rows_affected > 0
                    && matches!(
                        table,
                        ApiTable::Karma | ApiTable::KarmaCondition | ApiTable::KarmaConsequence
                    )
                {
                    refresh_karma_cache(self.services.clone()).await?;
                }
                Ok(outcome)
            }
            ApiTable::AppUser => {
                require_admin(claims)?;
                let password = required_text_field(object, "password")?;
                let password_hash = self.auth.hash_password(&password)?;
                let (sql, params) = self
                    .store
                    .build_app_user_insert(object, password_hash)
                    .await?;
                let outcome = self
                    .services
                    .writer
                    .execute_statement_returning_id(sql, params)
                    .await?;
                self.auth.refresh_cache().await?;
                Ok(outcome)
            }
            ApiTable::Role => {
                require_admin(claims)?;
                let (sql, params) = self.store.build_role_insert(object)?;
                let outcome = self
                    .services
                    .writer
                    .execute_statement_returning_id(sql, params)
                    .await?;
                self.auth.refresh_cache().await?;
                Ok(outcome)
            }
        }
    }

    pub async fn update_table_row(
        &self,
        claims: &AuthSubject,
        table_name: &str,
        id: i64,
        object: &Map<String, Value>,
    ) -> Result<WriteOutcome, Error> {
        let table = self.store.parse_table(table_name)?;

        match table {
            ApiTable::View => {
                let (sql, params) = self.store.build_standard_update(table, id, object)?;
                self.services
                    .writer
                    .execute_view_update(id, sql, params)
                    .await
            }
            ApiTable::Record => {
                let (object, confirmed) = strip_karma_confirmation(table, object);
                self.require_karma_loop_confirmation(table, Some(id), &object, confirmed)
                    .await?;
                let (sql, params) = self.store.build_standard_update(table, id, &object)?;
                write::execute_record_update(self.services.clone(), [id as u32], sql, params).await
            }
            ApiTable::RecordExtension
            | ApiTable::RecordLink
            | ApiTable::RecordComment
            | ApiTable::RecordWorklog
            | ApiTable::RecordResourceRef
            | ApiTable::Command
            | ApiTable::Query
            | ApiTable::Frequency
            | ApiTable::KarmaCondition
            | ApiTable::KarmaConsequence
            | ApiTable::Karma
            | ApiTable::Configuration => {
                let (object, confirmed) = strip_karma_confirmation(table, object);
                self.require_karma_loop_confirmation(table, Some(id), &object, confirmed)
                    .await?;
                let (sql, params) = self.store.build_standard_update(table, id, &object)?;
                let outcome = self.services.writer.execute_statement(sql, params).await?;
                if outcome.rows_affected > 0
                    && matches!(
                        table,
                        ApiTable::Karma | ApiTable::KarmaCondition | ApiTable::KarmaConsequence
                    )
                {
                    refresh_karma_cache(self.services.clone()).await?;
                }
                Ok(outcome)
            }
            ApiTable::AppUser => {
                ensure_self_or_admin(claims, id)?;
                let password_hash = object
                    .get("password")
                    .map(|value| parse_text_value("password", value))
                    .transpose()?
                    .map(|password| self.auth.hash_password(&password))
                    .transpose()?;
                let (sql, params) = self
                    .store
                    .build_app_user_update(claims, id, object, password_hash)
                    .await?;
                let outcome = self.services.writer.execute_statement(sql, params).await?;
                self.auth.refresh_cache().await?;
                Ok(outcome)
            }
            ApiTable::Role => {
                require_admin(claims)?;
                let (sql, params) = self.store.build_role_update(id, object)?;
                let outcome = self.services.writer.execute_statement(sql, params).await?;
                self.auth.refresh_cache().await?;
                Ok(outcome)
            }
        }
    }

    pub async fn update_table_rows(
        &self,
        _claims: &AuthSubject,
        table_name: &str,
        rows: &[Map<String, Value>],
    ) -> Result<WriteOutcome, Error> {
        let table = self.store.parse_table(table_name)?;
        match table {
            ApiTable::Record => {
                let (sql, params) = self.store.build_record_batch_update(rows)?;
                let record_ids = collect_record_ids_from_rows(rows)?;
                write::execute_record_update(self.services.clone(), record_ids, sql, params).await
            }
            _ => Err(Error::new(
                ErrorKind::InvalidInput,
                "Batch update over collection is currently supported only for record",
            )),
        }
    }

    pub async fn batch_update_record_quantities(
        &self,
        _claims: &AuthSubject,
        request: RecordQuantityBatchUpdateRequest,
    ) -> Result<WriteOutcome, Error> {
        let updates = normalize_record_quantity_batch_rows(request.rows)?;
        if updates.is_empty() {
            return Ok(WriteOutcome {
                rows_affected: 0,
                changed_tables: BTreeSet::new(),
                last_insert_rowid: None,
            });
        }
        let record_ids = updates.iter().map(|row| row.id as u32).collect::<Vec<_>>();
        let (sql, params) = build_record_quantity_batch_update(&updates);
        write::execute_record_update(self.services.clone(), record_ids, sql, params).await
    }

    pub async fn delete_table_row(
        &self,
        claims: &AuthSubject,
        table_name: &str,
        id: i64,
    ) -> Result<WriteOutcome, Error> {
        let table = self.store.parse_table(table_name)?;
        match table {
            ApiTable::AppUser => ensure_self_or_admin(claims, id)?,
            ApiTable::Role => require_admin(claims)?,
            _ => {}
        }

        let sql = format!("DELETE FROM {} WHERE id = ?", table.as_table_name());
        let params = vec![persistence::write_coordinator::SqlParameter::Integer(id)];
        let outcome = match table {
            ApiTable::View => {
                self.services
                    .writer
                    .execute_view_delete(id, sql, params)
                    .await
            }
            ApiTable::Record => {
                write::execute_record_delete(self.services.clone(), id as u32, sql, params).await
            }
            _ => self.services.writer.execute_statement(sql, params).await,
        }?;

        if outcome.rows_affected > 0
            && matches!(
                table,
                ApiTable::Karma | ApiTable::KarmaCondition | ApiTable::KarmaConsequence
            )
        {
            refresh_karma_cache(self.services.clone()).await?;
        }
        if matches!(table, ApiTable::AppUser | ApiTable::Role) {
            self.auth.refresh_cache().await?;
        }

        Ok(outcome)
    }

    pub async fn subscribe_view(
        &self,
        claims: AuthSubject,
        view_id: u32,
    ) -> Result<SubscriptionHandle, Error> {
        self.subscriptions.subscribe_view(claims, view_id).await
    }

    pub async fn read_view_snapshot(
        &self,
        _claims: &AuthSubject,
        view_id: u32,
    ) -> Result<Value, Error> {
        let snapshot = self.view_reads.read_snapshot(view_id).await?;
        serde_json::to_value(snapshot.snapshot).map_err(Error::other)
    }

    pub async fn execute_karma(&self, _claims: &AuthSubject, karma_id: i64) -> Result<(), Error> {
        let karma = self
            .services
            .repository
            .karma
            .get(None)
            .await?
            .into_iter()
            .find(|entry| i64::from(entry.id) == karma_id)
            .ok_or_else(|| Error::new(ErrorKind::NotFound, "Karma not found"))?;
        karma_deliver(self.services.clone(), vec![karma]).await
    }

    async fn require_karma_loop_confirmation(
        &self,
        table: ApiTable,
        row_id: Option<i64>,
        object: &Map<String, Value>,
        confirmed: bool,
    ) -> Result<(), Error> {
        if confirmed
            || !matches!(
                table,
                ApiTable::Karma | ApiTable::KarmaCondition | ApiTable::KarmaConsequence
            )
            || !self
                .karma_has_potential_record_loop_after(table, row_id, object)
                .await?
        {
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::WouldBlock,
            r#"{"status":"confirmation_required","confirmationKind":"karma_check_loop"}"#,
        ))
    }

    async fn karma_has_potential_record_loop_after(
        &self,
        table: ApiTable,
        row_id: Option<i64>,
        object: &Map<String, Value>,
    ) -> Result<bool, Error> {
        let mut karmas = serde_json::from_value::<Vec<LoopKarmaRow>>(
            self.store
                .list_table_rows_filtered(ApiTable::Karma, &TableListQuery::default())
                .await?,
        )
        .map_err(Error::other)?;
        let mut conditions = serde_json::from_value::<Vec<LoopConditionRow>>(
            self.store
                .list_table_rows_filtered(ApiTable::KarmaCondition, &TableListQuery::default())
                .await?,
        )
        .map_err(Error::other)?;
        let mut consequences = serde_json::from_value::<Vec<LoopConsequenceRow>>(
            self.store
                .list_table_rows_filtered(ApiTable::KarmaConsequence, &TableListQuery::default())
                .await?,
        )
        .map_err(Error::other)?;

        match table {
            ApiTable::Karma => apply_loop_karma_mutation(&mut karmas, row_id, object),
            ApiTable::KarmaCondition => {
                apply_loop_condition_mutation(&mut conditions, row_id, object)
            }
            ApiTable::KarmaConsequence => {
                apply_loop_consequence_mutation(&mut consequences, row_id, object)
            }
            _ => {}
        }

        let regex = Regex::new(r"rq(\d+)").map_err(Error::other)?;
        let condition_by_id = conditions
            .into_iter()
            .map(|row| (row.id, row))
            .collect::<BTreeMap<_, _>>();
        let consequence_by_id = consequences
            .into_iter()
            .map(|row| (row.id, row))
            .collect::<BTreeMap<_, _>>();
        let mut condition_refs = BTreeMap::<i64, BTreeSet<u32>>::new();
        for karma in &karmas {
            let Some(condition) = condition_by_id.get(&karma.condition_id) else {
                continue;
            };
            let refs = regex
                .captures_iter(&condition.condition)
                .filter_map(|caps| caps.get(1)?.as_str().parse::<u32>().ok())
                .collect::<BTreeSet<_>>();
            condition_refs.insert(karma.id, refs);
        }

        let mut edges = BTreeMap::<i64, Vec<i64>>::new();
        for source in &karmas {
            if source.quantity <= 0 {
                continue;
            }
            let Some(source_consequence) = consequence_by_id.get(&source.consequence_id) else {
                continue;
            };
            if source_consequence.quantity <= 0 {
                continue;
            }
            let Some(record_id) = regex
                .captures(&source_consequence.consequence)
                .and_then(|caps| caps.get(1)?.as_str().parse::<u32>().ok())
            else {
                continue;
            };
            for target in &karmas {
                if target.quantity <= 0 {
                    continue;
                }
                let Some(target_condition) = condition_by_id.get(&target.condition_id) else {
                    continue;
                };
                if target_condition.quantity <= 0 {
                    continue;
                }
                if condition_refs
                    .get(&target.id)
                    .is_some_and(|refs| refs.contains(&record_id))
                {
                    edges.entry(source.id).or_default().push(target.id);
                }
            }
        }
        Ok(has_rule_cycle(&edges))
    }

    pub async fn list_files(
        &self,
        _claims: &AuthSubject,
        prefix: Option<&str>,
        limit: i32,
        cursor: Option<&str>,
    ) -> Result<StorageList, Error> {
        self.services
            .storage
            .list_objects(prefix, limit, cursor)
            .await
            .map_err(Error::other)
    }

    pub async fn download_file(&self, key: &str) -> Result<DownloadedObject, Error> {
        let key = validate_file_key(key)?;
        self.services.storage.download_object(&key).await
    }

    pub async fn upload_file(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), Error> {
        let key = validate_file_key(key)?;
        self.services
            .storage
            .upload_object(&key, body, content_type)
            .await
    }

    pub async fn delete_file(&self, key: &str) -> Result<(), Error> {
        let key = validate_file_key(key)?;
        self.services.storage.delete_object(&key).await
    }

    pub fn issue_file_link(
        &self,
        _claims: &AuthSubject,
        key: &str,
        action: FileAccessAction,
    ) -> Result<FileLink, Error> {
        let key = validate_file_key(key)?;
        let token = issue_file_access_token(
            self.jwt_secret.as_str(),
            &key,
            action,
            Duration::from_secs(FILE_LINK_TTL_SECS),
        )?;

        Ok(FileLink {
            method: action.method(),
            url: format!("/files/access/{token}"),
            expires_in: FILE_LINK_TTL_SECS,
        })
    }

    pub fn authenticate_file_access(
        &self,
        token: &str,
        expected_action: FileAccessAction,
    ) -> Result<FileAccessClaims, Error> {
        let claims = decode_file_access_token(self.jwt_secret.as_str(), token)?;
        if claims.action != expected_action {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "File access token action mismatch",
            ));
        }
        Ok(claims)
    }

    pub async fn upload_via_link(
        &self,
        token: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), Error> {
        let claims = self.authenticate_file_access(token, FileAccessAction::Upload)?;
        self.services
            .storage
            .upload_object(&claims.key, body, content_type)
            .await
    }

    pub async fn download_via_link(&self, token: &str) -> Result<DownloadedObject, Error> {
        let claims = self.authenticate_file_access(token, FileAccessAction::Download)?;
        self.services.storage.download_object(&claims.key).await
    }

    pub async fn delete_via_link(&self, token: &str) -> Result<(), Error> {
        let claims = self.authenticate_file_access(token, FileAccessAction::Delete)?;
        self.services.storage.delete_object(&claims.key).await
    }
}

fn require_admin(claims: &AuthSubject) -> Result<(), Error> {
    if claims.is_admin() {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "Admin role required",
        ))
    }
}

fn ensure_self_or_admin(claims: &AuthSubject, id: i64) -> Result<(), Error> {
    if claims.is_admin() || claims.user_id as i64 == id {
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::PermissionDenied,
            "You may only modify your own user unless you are an admin",
        ))
    }
}

fn required_text_field(object: &Map<String, Value>, field_name: &str) -> Result<String, Error> {
    let value = object.get(field_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Missing required field: {field_name}"),
        )
    })?;
    parse_text_value(field_name, value)
}

fn parse_text_value(field_name: &str, value: &Value) -> Result<String, Error> {
    value.as_str().map(str::to_string).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidInput,
            format!("Expected string for field {field_name}"),
        )
    })
}

fn collect_record_ids_from_rows(rows: &[Map<String, Value>]) -> Result<Vec<u32>, Error> {
    let mut record_ids = BTreeSet::<u32>::new();

    for row in rows {
        let id = row
            .get("id")
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Missing required field: id"))?;
        let id = id
            .as_i64()
            .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Expected integer for field id"))?;
        if id <= 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Record id must be positive",
            ));
        }
        record_ids.insert(id as u32);
    }

    Ok(record_ids.into_iter().collect())
}

fn normalize_record_quantity_batch_rows(
    rows: Vec<RecordQuantityBatchUpdateRow>,
) -> Result<Vec<RecordQuantityBatchUpdateRow>, Error> {
    let mut deduped = BTreeMap::<i64, f64>::new();
    for row in rows {
        if row.id <= 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Record id must be positive",
            ));
        }
        if !row.quantity.is_finite() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Record quantity must be finite",
            ));
        }
        deduped.insert(row.id, row.quantity);
    }
    Ok(deduped
        .into_iter()
        .map(|(id, quantity)| RecordQuantityBatchUpdateRow { id, quantity })
        .collect())
}

fn build_record_quantity_batch_update(
    rows: &[RecordQuantityBatchUpdateRow],
) -> (String, Vec<SqlParameter>) {
    let mut sql = String::from("UPDATE record SET quantity = CASE id ");
    let mut params = Vec::with_capacity(rows.len() * 3);
    for entry in rows {
        sql.push_str("WHEN ? THEN ? ");
        params.push(SqlParameter::Integer(entry.id));
        params.push(SqlParameter::Real(entry.quantity));
    }
    sql.push_str("END WHERE id IN (");
    for (index, entry) in rows.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push('?');
        params.push(SqlParameter::Integer(entry.id));
    }
    sql.push(')');
    (sql, params)
}

fn strip_karma_confirmation(
    table: ApiTable,
    object: &Map<String, Value>,
) -> (Map<String, Value>, bool) {
    let mut object = object.clone();
    let confirmed = if matches!(
        table,
        ApiTable::Karma | ApiTable::KarmaCondition | ApiTable::KarmaConsequence
    ) {
        object
            .remove("confirmKarmaCheckLoops")
            .or_else(|| object.remove("confirm_karma_check_loops"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    } else {
        false
    };
    (object, confirmed)
}

#[derive(Debug, Clone, Deserialize)]
struct LoopKarmaRow {
    id: i64,
    quantity: i64,
    condition_id: i64,
    consequence_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct LoopConditionRow {
    id: i64,
    quantity: i64,
    condition: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LoopConsequenceRow {
    id: i64,
    quantity: i64,
    consequence: String,
}

fn apply_loop_karma_mutation(
    rows: &mut Vec<LoopKarmaRow>,
    row_id: Option<i64>,
    object: &Map<String, Value>,
) {
    if let Some(id) = row_id {
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            if let Some(value) = optional_i64_field(object, "quantity") {
                row.quantity = value;
            }
            if let Some(value) = optional_i64_field(object, "condition_id") {
                row.condition_id = value;
            }
            if let Some(value) = optional_i64_field(object, "consequence_id") {
                row.consequence_id = value;
            }
        }
        return;
    }

    let Some(condition_id) = optional_i64_field(object, "condition_id") else {
        return;
    };
    let Some(consequence_id) = optional_i64_field(object, "consequence_id") else {
        return;
    };
    let id = optional_i64_field(object, "id").unwrap_or_else(|| next_loop_row_id(rows));
    rows.push(LoopKarmaRow {
        id,
        quantity: optional_i64_field(object, "quantity").unwrap_or(1),
        condition_id,
        consequence_id,
    });
}

fn apply_loop_condition_mutation(
    rows: &mut Vec<LoopConditionRow>,
    row_id: Option<i64>,
    object: &Map<String, Value>,
) {
    if let Some(id) = row_id {
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            if let Some(value) = optional_i64_field(object, "quantity") {
                row.quantity = value;
            }
            if let Some(value) = optional_string_field(object, "condition") {
                row.condition = value;
            }
        }
        return;
    }

    let Some(condition) = optional_string_field(object, "condition") else {
        return;
    };
    let id = optional_i64_field(object, "id").unwrap_or_else(|| next_loop_row_id(rows));
    rows.push(LoopConditionRow {
        id,
        quantity: optional_i64_field(object, "quantity").unwrap_or(1),
        condition,
    });
}

fn apply_loop_consequence_mutation(
    rows: &mut Vec<LoopConsequenceRow>,
    row_id: Option<i64>,
    object: &Map<String, Value>,
) {
    if let Some(id) = row_id {
        if let Some(row) = rows.iter_mut().find(|row| row.id == id) {
            if let Some(value) = optional_i64_field(object, "quantity") {
                row.quantity = value;
            }
            if let Some(value) = optional_string_field(object, "consequence") {
                row.consequence = value;
            }
        }
        return;
    }

    let Some(consequence) = optional_string_field(object, "consequence") else {
        return;
    };
    let id = optional_i64_field(object, "id").unwrap_or_else(|| next_loop_row_id(rows));
    rows.push(LoopConsequenceRow {
        id,
        quantity: optional_i64_field(object, "quantity").unwrap_or(1),
        consequence,
    });
}

fn optional_i64_field(object: &Map<String, Value>, name: &str) -> Option<i64> {
    object
        .get(name)
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
}

fn optional_string_field(object: &Map<String, Value>, name: &str) -> Option<String> {
    object.get(name).and_then(Value::as_str).map(str::to_string)
}

trait LoopRowId {
    fn loop_row_id(&self) -> i64;
}

impl LoopRowId for LoopKarmaRow {
    fn loop_row_id(&self) -> i64 {
        self.id
    }
}

impl LoopRowId for LoopConditionRow {
    fn loop_row_id(&self) -> i64 {
        self.id
    }
}

impl LoopRowId for LoopConsequenceRow {
    fn loop_row_id(&self) -> i64 {
        self.id
    }
}

fn next_loop_row_id(rows: &[impl LoopRowId]) -> i64 {
    rows.iter().map(LoopRowId::loop_row_id).max().unwrap_or(0) + 1
}

fn has_rule_cycle(edges: &BTreeMap<i64, Vec<i64>>) -> bool {
    fn visit(
        node: i64,
        edges: &BTreeMap<i64, Vec<i64>>,
        visiting: &mut BTreeSet<i64>,
        visited: &mut BTreeSet<i64>,
    ) -> bool {
        if visiting.contains(&node) {
            return true;
        }
        if visited.contains(&node) {
            return false;
        }
        visiting.insert(node);
        for next in edges.get(&node).into_iter().flatten().copied() {
            if visit(next, edges, visiting, visited) {
                return true;
            }
        }
        visiting.remove(&node);
        visited.insert(node);
        false
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    edges
        .keys()
        .copied()
        .any(|node| visit(node, edges, &mut visiting, &mut visited))
}
