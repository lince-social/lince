use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    Client,
    config::{Credentials, Region},
    primitives::ByteStream,
};
use serde::Serialize;
use sqlx::{FromRow, Pool, Sqlite};
use std::io::{Error, ErrorKind};

const DEFAULT_BUCKET_NAME: &str = "lince";
const DEFAULT_BUCKET_REGION: &str = "us-east-1";

#[derive(Clone)]
pub struct StorageService {
    client: Option<Client>,
    bucket: String,
    enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageObject {
    pub key: String,
    pub size: i64,
    pub e_tag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageList {
    pub objects: Vec<StorageObject>,
    pub next_cursor: Option<String>,
}

pub struct DownloadedObject {
    pub body: ByteStream,
    pub content_type: Option<String>,
    pub content_length: Option<i64>,
    pub filename: String,
}

#[derive(Debug, FromRow)]
struct StorageConfigurationRow {
    bucket_enabled: i64,
    bucket_username: Option<String>,
    bucket_password: Option<String>,
    bucket_uri: Option<String>,
    bucket_name: Option<String>,
    bucket_region: Option<String>,
}

impl StorageService {
    pub async fn from_database(db: &Pool<Sqlite>) -> Result<Self, Error> {
        let row = sqlx::query_as::<_, StorageConfigurationRow>(
            "
            SELECT
                bucket_enabled,
                bucket_username,
                bucket_password,
                bucket_uri,
                bucket_name,
                bucket_region
            FROM configuration
            WHERE quantity = 1
            ORDER BY id
            LIMIT 1
            ",
        )
        .fetch_optional(db)
        .await
        .map_err(Error::other)?;

        if !storage_enabled(row.as_ref()) {
            return Ok(Self {
                client: None,
                bucket: row
                    .as_ref()
                    .and_then(|value| value.bucket_name.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(DEFAULT_BUCKET_NAME)
                    .to_string(),
                enabled: false,
            });
        }

        let endpoint =
            normalize_endpoint(required_bucket_value(row.as_ref(), "bucket_uri").map_err(
                |_| Error::other("bucket_uri is required when bucket storage is enabled"),
            )?);
        let access_key = required_bucket_value(row.as_ref(), "bucket_username").map_err(|_| {
            Error::other("bucket_username is required when bucket storage is enabled")
        })?;
        let secret_key = required_bucket_value(row.as_ref(), "bucket_password").map_err(|_| {
            Error::other("bucket_password is required when bucket storage is enabled")
        })?;
        let bucket = row
            .as_ref()
            .and_then(|value| value.bucket_name.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_BUCKET_NAME)
            .to_string();
        let region = row
            .as_ref()
            .and_then(|value| value.bucket_region.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_BUCKET_REGION)
            .to_string();

        let config = aws_sdk_s3::config::Builder::from(
            &aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(region))
                .credentials_provider(Credentials::new(
                    access_key,
                    secret_key,
                    None,
                    None,
                    "lince-rustfs",
                ))
                .load()
                .await,
        )
        .endpoint_url(endpoint)
        .force_path_style(true)
        .build();

        Ok(Self {
            client: Some(Client::from_conf(config)),
            bucket,
            enabled: true,
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub async fn ensure_bucket_exists(&self) -> Result<(), Error> {
        let Some(client) = self.client.as_ref() else {
            return Ok(());
        };

        if client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .is_ok()
        {
            return Ok(());
        }

        client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|error| {
                Error::other(format!("Failed to create bucket {}: {error}", self.bucket))
            })?;

        Ok(())
    }

    pub async fn list_objects(
        &self,
        prefix: Option<&str>,
        limit: i32,
        cursor: Option<&str>,
    ) -> Result<StorageList, Error> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| Error::other("Bucket storage is disabled"))?;

        let mut request = client
            .list_objects_v2()
            .bucket(&self.bucket)
            .max_keys(limit.clamp(1, 1_000));

        if let Some(prefix) = prefix.filter(|prefix| !prefix.is_empty()) {
            request = request.prefix(prefix);
        }
        if let Some(cursor) = cursor.filter(|cursor| !cursor.is_empty()) {
            request = request.continuation_token(cursor);
        }

        let response = request
            .send()
            .await
            .map_err(|error| Error::other(format!("Failed to list files: {error}")))?;

        let objects = response
            .contents()
            .iter()
            .filter_map(|object| {
                object.key().map(|key| StorageObject {
                    key: key.to_string(),
                    size: object.size().unwrap_or_default(),
                    e_tag: object.e_tag().map(str::to_string),
                    last_modified: object.last_modified().map(|value| value.to_string()),
                })
            })
            .collect();

        Ok(StorageList {
            objects,
            next_cursor: response.next_continuation_token().map(str::to_string),
        })
    }

    pub async fn upload_object(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<(), Error> {
        validate_storage_key(key)?;

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| Error::other("Bucket storage is disabled"))?;

        let mut request = client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(body));
        if let Some(content_type) = content_type.filter(|value| !value.is_empty()) {
            request = request.content_type(content_type);
        }

        request
            .send()
            .await
            .map_err(|error| Error::other(format!("Failed to upload file {key}: {error}")))?;

        Ok(())
    }

    pub async fn download_object(&self, key: &str) -> Result<DownloadedObject, Error> {
        validate_storage_key(key)?;

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| Error::other("Bucket storage is disabled"))?;

        let response = client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| classify_object_error(error, key, "download"))?;

        Ok(DownloadedObject {
            body: response.body,
            content_type: response.content_type,
            content_length: response.content_length,
            filename: filename_from_key(key),
        })
    }

    pub async fn delete_object(&self, key: &str) -> Result<(), Error> {
        validate_storage_key(key)?;

        let client = self
            .client
            .as_ref()
            .ok_or_else(|| Error::other("Bucket storage is disabled"))?;

        client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| classify_object_error(error, key, "delete"))?;

        Ok(())
    }
}

fn storage_enabled(row: Option<&StorageConfigurationRow>) -> bool {
    row.map(|value| value.bucket_enabled != 0).unwrap_or(false)
}

fn required_bucket_value(
    row: Option<&StorageConfigurationRow>,
    field: &str,
) -> Result<String, Error> {
    let row = row.ok_or_else(|| Error::other("Active configuration is missing"))?;
    let value = match field {
        "bucket_username" => row.bucket_username.as_deref(),
        "bucket_password" => row.bucket_password.as_deref(),
        "bucket_uri" => row.bucket_uri.as_deref(),
        _ => None,
    }
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| Error::other(format!("{field} is missing")))?;

    Ok(value.to_string())
}

fn normalize_endpoint(raw: String) -> String {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn validate_storage_key(key: &str) -> Result<(), Error> {
    if key.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "File key cannot be empty",
        ));
    }
    if key.starts_with('/') {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "File key cannot start with '/'",
        ));
    }
    if key.chars().any(char::is_control) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "File key cannot contain control characters",
        ));
    }
    Ok(())
}

fn filename_from_key(key: &str) -> String {
    key.rsplit('/').next().unwrap_or(key).to_string()
}

fn classify_object_error<E: std::fmt::Display>(error: E, key: &str, operation: &str) -> Error {
    let message = error.to_string();
    if message.contains("NoSuchKey") || message.contains("NotFound") || message.contains("404") {
        return Error::new(
            ErrorKind::NotFound,
            format!("File {key} not found for {operation}"),
        );
    }

    Error::other(format!("Failed to {operation} file {key}: {message}"))
}
