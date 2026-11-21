//! Store-backed credentials, role definitions, and Person access.

mod credentials;
mod identity;
mod person_access;
mod roles;

pub use credentials::{
    create_credential, create_credential_on, create_person_login, credential_generation_on,
    has_credential, remove_credential_on, replace_credential_on,
};
pub use identity::{
    AuthUser, Principal, assigned_role_on, list_actors, list_users, principal, principal_on,
    user_by_uid, user_by_username,
};
pub use person_access::{
    MAX_PERSON_ACCESS_BYTES, MAX_PERSON_ACCESS_ROWS, MAX_READ_FILTER_BYTES, PersonAccess,
    RetainedPersonAccess, compare_and_set_read_filter, compare_and_set_read_filter_on,
    compare_and_set_role, compare_and_set_role_on, person_access, person_access_on,
    retained_person_access_on, set_user_role,
};
pub use roles::{
    ADMIN_ROLE, LINCE_ROLE, MAX_PERMISSION_KEY_BYTES, MAX_ROLE_PERMISSION_BYTES,
    MAX_ROLE_PERMISSIONS, admin_exists, admins, ensure_permission, ensure_permission_on,
    ensure_role, ensure_role_on, grant, grant_on, list_roles, revoke, role_by_name,
    role_permission_keys, role_permission_keys_by_id, role_permission_keys_by_id_on,
};

use crate::StoreError;

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}
