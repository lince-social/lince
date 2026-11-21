use serde::Serialize;

use crate::{json::encoded_len, valid_uid};

use super::MAX_BYTES;

mod component;
mod requests;
mod snapshot;

pub use component::validate_registry;

/// Checks the serialized Canvas byte limit without retaining the encoded value.
///
/// # Errors
///
/// Returns an error for a serialization failure or data over the byte limit.
pub fn bounded(value: &(impl Serialize + ?Sized)) -> Result<(), String> {
    if encoded_len(value).map_err(|error| error.to_string())? > MAX_BYTES {
        Err("Canvas data exceeds 256 KiB.".into())
    } else {
        Ok(())
    }
}

/// Checks whether a Canvas request has a stable UID.
///
/// # Errors
///
/// Returns an error if the identifier is invalid.
pub fn request_identifier(id: &str) -> Result<(), String> {
    if valid_uid(id, "request") {
        Ok(())
    } else {
        Err("Use a stable canvas request UID.".into())
    }
}

/// Checks whether a Canvas placement has a stable UID.
///
/// # Errors
///
/// Returns an error if the identifier is invalid.
pub fn placement_id(id: &str) -> Result<(), String> {
    if valid_uid(id, "placement") {
        Ok(())
    } else {
        Err("Use a stable canvas placement UID.".into())
    }
}

fn workspace_id(id: u64) -> Result<(), String> {
    if id == 0 {
        Err("Choose a nonzero workspace ID.".into())
    } else {
        Ok(())
    }
}
pub(super) fn workspace_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name
            .chars()
            .enumerate()
            .any(|(index, character)| index >= 80 || character.is_control())
    {
        Err("Use a workspace name of 1–80 characters.".into())
    } else {
        Ok(())
    }
}
