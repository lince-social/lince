use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use std::{
    collections::BTreeSet,
    io::{Error, ErrorKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionKey {
    pub subject: &'static str,
    pub action: &'static str,
}

impl PermissionKey {
    pub const fn new(subject: &'static str, action: &'static str) -> Self {
        Self { subject, action }
    }

    pub fn as_str(self) -> String {
        format!("{}:{}", self.subject, self.action)
    }
}

pub const ALL_PERMISSIONS: &[PermissionKey] = &[
    PermissionKey::new("record", "create"),
    PermissionKey::new("record", "read"),
    PermissionKey::new("record", "update"),
    PermissionKey::new("record", "delete"),
    PermissionKey::new("record", "delete_own"),
    PermissionKey::new("view", "create"),
    PermissionKey::new("view", "read"),
    PermissionKey::new("view", "update"),
    PermissionKey::new("view", "delete"),
    PermissionKey::new("view", "stream"),
    PermissionKey::new("terminal", "execute"),
    PermissionKey::new("transfer", "create"),
    PermissionKey::new("transfer", "read"),
    PermissionKey::new("transfer", "update"),
    PermissionKey::new("transfer", "delete"),
    PermissionKey::new("organ", "create"),
    PermissionKey::new("organ", "read"),
    PermissionKey::new("organ", "update"),
    PermissionKey::new("organ", "delete"),
    PermissionKey::new("karma", "create"),
    PermissionKey::new("karma", "read"),
    PermissionKey::new("karma", "update"),
    PermissionKey::new("karma", "delete"),
    PermissionKey::new("karma", "execute"),
    PermissionKey::new("user", "create"),
    PermissionKey::new("user", "read"),
    PermissionKey::new("user", "update"),
    PermissionKey::new("user", "update_self"),
    PermissionKey::new("user", "delete"),
    PermissionKey::new("user", "assign_role"),
    PermissionKey::new("role", "create"),
    PermissionKey::new("role", "read"),
    PermissionKey::new("role", "update"),
    PermissionKey::new("role", "delete"),
    PermissionKey::new("permission", "read"),
    PermissionKey::new("permission", "assign"),
    PermissionKey::new("file", "read"),
    PermissionKey::new("file", "upload"),
    PermissionKey::new("file", "download"),
    PermissionKey::new("file", "delete"),
    PermissionKey::new("configuration", "create"),
    PermissionKey::new("configuration", "read"),
    PermissionKey::new("configuration", "update"),
    PermissionKey::new("configuration", "delete"),
    PermissionKey::new("command", "create"),
    PermissionKey::new("command", "read"),
    PermissionKey::new("command", "update"),
    PermissionKey::new("command", "delete"),
    PermissionKey::new("query", "create"),
    PermissionKey::new("query", "read"),
    PermissionKey::new("query", "update"),
    PermissionKey::new("query", "delete"),
    PermissionKey::new("frequency", "create"),
    PermissionKey::new("frequency", "read"),
    PermissionKey::new("frequency", "update"),
    PermissionKey::new("frequency", "delete"),
    PermissionKey::new("package", "create"),
    PermissionKey::new("package", "read"),
    PermissionKey::new("package", "update"),
    PermissionKey::new("package", "delete"),
    PermissionKey::new("board", "read"),
    PermissionKey::new("board", "update"),
    PermissionKey::new("sand", "read"),
    PermissionKey::new("sand", "create"),
    PermissionKey::new("sand", "update"),
    PermissionKey::new("sand", "delete"),
];

pub fn all_permission_keys() -> Vec<String> {
    normalized_permission_strings(ALL_PERMISSIONS.iter().map(|permission| permission.as_str()))
}

pub fn normalized_permission_strings<I>(permissions: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    permissions
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn hash_password(password: &str) -> Result<String, Error> {
    Argon2::default()
        .hash_password(password.as_bytes())
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
