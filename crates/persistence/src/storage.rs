use aws_config::BehaviorVersion;
use aws_sdk_s3::{
    Client,
    config::{Credentials, Region},
    primitives::ByteStream,
};
use serde::Serialize;
use std::{
    env,
    io::{Error, ErrorKind},
};

const DEFAULT_BUCKET_NAME: &str = "lince";
const DEFAULT_BUCKET_REGION: &str = "us-east-1";

#[derive(Clone)]
pub struct StorageService {
    client: Client,
    bucket: String,
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

impl StorageService {
    pub async fn from_env() -> Result<Self, Error> {
        let endpoint = normalize_endpoint(
            env::var("BUCKET_URI")
                .map_err(|_| Error::other("BUCKET_URI is required for RustFS integration"))?,
        );
        let access_key = env::var("BUCKET_USERNAME")
            .map_err(|_| Error::other("BUCKET_USERNAME is required for RustFS integration"))?;
        let secret_key = env::var("BUCKET_PASSWORD")
            .map_err(|_| Error::other("BUCKET_PASSWORD is required for RustFS integration"))?;
        let bucket = env::var("BUCKET_NAME").unwrap_or_else(|_| DEFAULT_BUCKET_NAME.to_string());
        let region =
            env::var("BUCKET_REGION").unwrap_or_else(|_| DEFAULT_BUCKET_REGION.to_string());

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
            client: Client::from_conf(config),
            bucket,
        })
    }

    pub async fn ensure_bucket_exists(&self) -> Result<(), Error> {
        if self
            .client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .is_ok()
        {
            return Ok(());
        }

        self.client
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
        let mut request = self
            .client
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

        let mut request = self
            .client
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

        let response = self
            .client
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

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|error| classify_object_error(error, key, "delete"))?;

        Ok(())
    }
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
