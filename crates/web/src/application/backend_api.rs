use crate::infrastructure::backend_api_store::{ApiTable, BackendApiStore, validate_file_key};
use ::application::{
    auth::{AuthService, AuthSubject},
    subscription::{SubscriptionHandle, SubscriptionRegistry},
    view::ViewReadService,
};
use injection::cross_cutting::InjectedServices;
use persistence::{
    storage::{DownloadedObject, StorageList},
    write_coordinator::WriteOutcome,
};
use serde_json::{Map, Value};
use std::{
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
        let table = self.store.parse_table(table_name)?;
        self.store.list_table_rows(table).await
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
                self.services.writer.execute_view_insert(sql, params).await
            }
            ApiTable::Record
            | ApiTable::RecordExtension
            | ApiTable::RecordLink
            | ApiTable::RecordComment
            | ApiTable::RecordWorklog
            | ApiTable::RecordResourceRef
            | ApiTable::Frequency
            | ApiTable::KarmaCondition
            | ApiTable::KarmaConsequence
            | ApiTable::Karma
            | ApiTable::Configuration => {
                let (sql, params) = self.store.build_standard_insert(table, object)?;
                self.services.writer.execute_statement(sql, params).await
            }
            ApiTable::AppUser => {
                require_admin(claims)?;
                let password = required_text_field(object, "password")?;
                let password_hash = self.auth.hash_password(&password)?;
                let (sql, params) = self
                    .store
                    .build_app_user_insert(object, password_hash)
                    .await?;
                let outcome = self.services.writer.execute_statement(sql, params).await?;
                self.auth.refresh_cache().await?;
                Ok(outcome)
            }
            ApiTable::Role => {
                require_admin(claims)?;
                let (sql, params) = self.store.build_role_insert(object)?;
                let outcome = self.services.writer.execute_statement(sql, params).await?;
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
            ApiTable::Record
            | ApiTable::RecordExtension
            | ApiTable::RecordLink
            | ApiTable::RecordComment
            | ApiTable::RecordWorklog
            | ApiTable::RecordResourceRef
            | ApiTable::Frequency
            | ApiTable::KarmaCondition
            | ApiTable::KarmaConsequence
            | ApiTable::Karma
            | ApiTable::Configuration => {
                let (sql, params) = self.store.build_standard_update(table, id, object)?;
                self.services.writer.execute_statement(sql, params).await
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
            _ => self.services.writer.execute_statement(sql, params).await,
        }?;

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

    pub async fn read_view_snapshot(&self, _claims: &AuthSubject, view_id: u32) -> Result<Value, Error> {
        let snapshot = self.view_reads.read_snapshot(view_id).await?;
        serde_json::to_value(snapshot.snapshot).map_err(Error::other)
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
            url: format!("/api/files/access/{token}"),
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
