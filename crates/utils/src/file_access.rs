use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::{
    io::{Error, ErrorKind},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileAccessAction {
    Upload,
    Download,
    Delete,
}

impl FileAccessAction {
    pub fn method(self) -> &'static str {
        match self {
            Self::Upload => "PUT",
            Self::Download => "GET",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAccessClaims {
    pub key: String,
    pub action: FileAccessAction,
    pub exp: usize,
}

pub fn issue_file_access_token(
    secret: &str,
    key: &str,
    action: FileAccessAction,
    ttl: Duration,
) -> Result<String, Error> {
    let expires_at = SystemTime::now()
        .checked_add(ttl)
        .ok_or_else(|| Error::other("File access token expiration overflow"))?
        .duration_since(UNIX_EPOCH)
        .map_err(Error::other)?
        .as_secs() as usize;

    encode(
        &Header::new(Algorithm::HS256),
        &FileAccessClaims {
            key: key.to_string(),
            action,
            exp: expires_at,
        },
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|error| Error::other(format!("Failed to encode file access token: {error}")))
}

pub fn decode_file_access_token(secret: &str, token: &str) -> Result<FileAccessClaims, Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<FileAccessClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|error| {
        Error::new(
            ErrorKind::PermissionDenied,
            format!("Invalid file access token: {error}"),
        )
    })
}
