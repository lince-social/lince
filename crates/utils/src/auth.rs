use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::{
    io::{Error, ErrorKind},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub sub: u64,
    pub username: String,
    pub exp: usize,
}

pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| Error::other(format!("Failed to hash password: {error}")))
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, Error> {
    let parsed_hash = PasswordHash::new(password_hash).map_err(|error| {
        Error::new(
            ErrorKind::InvalidData,
            format!("Stored password hash is invalid: {error}"),
        )
    })?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn issue_jwt(
    secret: &str,
    user_id: u64,
    username: &str,
    ttl: Duration,
) -> Result<String, Error> {
    let expires_at = SystemTime::now()
        .checked_add(ttl)
        .ok_or_else(|| Error::other("JWT expiration overflow"))?
        .duration_since(UNIX_EPOCH)
        .map_err(Error::other)?
        .as_secs() as usize;

    let claims = AuthClaims {
        sub: user_id,
        username: username.to_string(),
        exp: expires_at,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|error| Error::other(format!("Failed to encode JWT: {error}")))
}

pub fn decode_jwt(secret: &str, token: &str) -> Result<AuthClaims, Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|error| Error::new(ErrorKind::PermissionDenied, format!("Invalid JWT: {error}")))
}
