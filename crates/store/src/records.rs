//! Stored Record queries and mutations.

mod extensions;
mod lifecycle;
mod quantity;
mod read;
mod write;

pub use extensions::{
    ExtensionNumbers, MAX_EXTENSION_BYTES, MAX_EXTENSION_KEYS, MAX_EXTENSION_NAME_BYTES,
    all_extensions, delete_extension, delete_extension_on, extension_number, extension_numeric_at,
    extensions_for_records, get_extension, get_extensions, numeric_extensions,
    numeric_extensions_for_records, set_audience_plan_on, set_extension, set_extension_on,
    set_extension_raw,
};
pub(crate) use extensions::{extension_rows, map_extensions, set_transfer_public_history_on};
pub use lifecycle::{
    NewRecord, create, create_in_root, create_with_uid, create_with_uid_on, mark_deleted,
    mark_deleted_on, restore, restore_on,
};
pub use quantity::{
    NamedLevel, all_levels, bump_imported_quantity, bump_quantity, named_levels, quantity,
    quantity_in_transaction,
};
pub use read::{
    GroupingFields, RecordBody, RecordGrouping, RecordRouting, RecordRow, RecordSelection,
    RecordText, ResolvedRecord, active_needs, body, created_at, exists, get, get_many, grouping,
    head, heads_for, list_all, live_heads_for, live_identifiers, live_slugs_for, organ_uid, read,
    resolve, resolve_kind, resolve_uid, routing, text,
};
pub(crate) use read::{METADATA_COLUMNS, map_row, read_rows};
pub use write::{
    set_authoring_text, set_authoring_text_on, set_file_origin_on, set_organ_origin, set_place,
    set_place_on, set_slug, set_slug_on, set_text, set_unit, set_unit_on,
};

use nucleus::{valid_slug, valid_uid};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};

use crate::{
    StoreError,
    sync_ops::{OpKind, log_local_tx},
};

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

async fn log_set_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    field: &str,
    value: Value,
) -> Result<(), StoreError> {
    log_local_tx(
        tx,
        "record",
        uid,
        field,
        OpKind::Set,
        Some(value.to_string()),
    )
    .await
}

fn validate_record_uid(uid: &str) -> Result<(), StoreError> {
    if valid_uid(uid, "r") {
        Ok(())
    } else {
        Err(protocol("Record authoring requires a canonical Record uid"))
    }
}

fn validate_slug(slug: Option<&str>) -> Result<(), StoreError> {
    if let Some(slug) = slug
        && !valid_slug(slug)
    {
        return Err(protocol(format!("invalid slug `{slug}`")));
    }
    Ok(())
}

async fn require_active_on(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<(), StoreError> {
    validate_record_uid(uid)?;
    let active =
        sqlx::query_scalar::<_, i64>("SELECT deleted_at IS NULL FROM record WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut **tx)
            .await?;
    match active {
        Some(1) => Ok(()),
        Some(_) => Err(protocol("Record authoring target is deleted")),
        None => Err(protocol("Record authoring target is missing")),
    }
}
