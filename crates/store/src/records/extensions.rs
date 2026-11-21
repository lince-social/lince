mod authoring;
mod numbers;
mod policy;
mod read;

pub(crate) use authoring::set_transfer_public_history_on;
pub use authoring::{
    delete_extension, delete_extension_on, set_audience_plan_on, set_extension, set_extension_on,
    set_extension_raw,
};
pub use numbers::{
    ExtensionNumbers, extension_number, extension_numeric_at, numeric_extensions,
    numeric_extensions_for_records,
};
pub use read::{all_extensions, extensions_for_records, get_extension, get_extensions};
pub(crate) use read::{extension_rows, map_extensions};

use super::{protocol, require_active_on};
use crate::StoreError;

/// Maximum encoded byte length of a Record extension.
pub const MAX_EXTENSION_BYTES: usize = 1024 * 1024;
/// Maximum number of top-level fields in a Record extension.
pub const MAX_EXTENSION_KEYS: usize = 4096;
/// Maximum byte length of an extension namespace or field name.
pub const MAX_EXTENSION_NAME_BYTES: usize = 200;

const SQLITE_BYTE_LIMIT: i64 = MAX_EXTENSION_BYTES as i64;

fn validate_extension_name(value: &str, label: &str, allow_dot: bool) -> Result<(), StoreError> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > MAX_EXTENSION_NAME_BYTES
        || value.chars().any(char::is_control)
        || (!allow_dot && value.contains('.'))
    {
        return Err(protocol(format!("Record extension {label} is invalid")));
    }
    Ok(())
}
