//! Composes the policies contributed by a Person's Roles for each operation.

use super::READ_PERMISSION;
use crate::{
    Predicate, ProteinError,
    authority::{MutationGrant, Operation, RolePolicy},
};
use nucleus::performance::{Span, add};
use serde_json::from_value;
use std::borrow::Cow;
use store::{
    auth::role_permission_keys_by_id_on,
    person_roles::ids_on,
    role_policies::get_on,
    sqlx::{Error::Protocol, SqliteConnection},
};

fn permission(operation: Option<Operation>) -> &'static str {
    match operation {
        None => READ_PERMISSION,
        Some(Operation::Create) => "record:create",
        Some(Operation::Update | Operation::Restore) => "record:update",
        Some(Operation::Delete) => "record:delete",
    }
}

struct Composition {
    operation: Option<Operation>,
    read: Vec<Predicate>,
    grants: Vec<MutationGrant>,
    unrestricted_read: bool,
    unrestricted_write: bool,
}

impl Composition {
    fn new(operation: Option<Operation>) -> Self {
        Self {
            operation,
            read: Vec::new(),
            grants: Vec::new(),
            unrestricted_read: false,
            unrestricted_write: false,
        }
    }

    fn unrestricted(&self) -> bool {
        self.unrestricted_write || self.operation.is_none() && self.unrestricted_read
    }

    fn include(&mut self, permissions: &[String], policy: Option<Cow<'_, RolePolicy>>) {
        let allows = |key: &str| permissions.iter().any(|permission| permission == key);
        let read_allowed = allows(READ_PERMISSION);
        let write_allowed = allows(permission(self.operation))
            || self.operation == Some(Operation::Delete) && allows("record:delete_own");
        let Some(policy) = policy else {
            self.unrestricted_read |= read_allowed;
            self.unrestricted_write |= self.operation.is_some() && write_allowed;
            if self.unrestricted_read {
                self.read.clear();
            }
            if self.unrestricted() {
                self.grants.clear();
            }
            return;
        };
        if self.unrestricted() {
            return;
        }
        let read = read_allowed && !self.unrestricted_read;
        let selects = |grant: &MutationGrant| {
            self.operation
                .is_none_or(|operation| grant.operation == operation)
        };
        match policy {
            Cow::Borrowed(policy) => {
                if read {
                    self.read.push(policy.read.clone());
                }
                if write_allowed {
                    self.grants
                        .extend(policy.grants.iter().filter(|grant| selects(grant)).cloned());
                }
            }
            Cow::Owned(policy) => {
                if read {
                    self.read.push(policy.read);
                }
                if write_allowed {
                    self.grants
                        .extend(policy.grants.into_iter().filter(selects));
                }
            }
        }
    }

    fn finish(self) -> Option<RolePolicy> {
        if self.unrestricted() {
            return None;
        }
        Some(RolePolicy {
            read: if self.unrestricted_read {
                Predicate::All(vec![])
            } else {
                Predicate::Any(self.read)
            },
            grants: self.grants,
        })
    }
}

/// Reads a Person's Roles once and composes the policy for each requested operation.
///
/// A missing policy in the result means that operation is unrestricted by Role policy.
///
/// # Errors
/// Returns an error if membership, permissions, or a saved policy cannot be read.
pub async fn policies_for_on<const N: usize>(
    connection: &mut SqliteConnection,
    person: &str,
    operations: [Option<Operation>; N],
) -> Result<[Option<RolePolicy>; N], ProteinError> {
    let _read = Span::new("role-authority-policy-read-await");
    add("role-authority-policies-requested", N as u64);
    let mut compositions = operations.map(Composition::new);
    let Some((last, first)) = compositions.split_last_mut() else {
        return Ok(operations.map(|_| None));
    };
    let roles = ids_on(connection, person).await?;
    let mut decoded = 0;
    for &role in &roles {
        let permissions = role_permission_keys_by_id_on(connection, role).await?;
        let policy: Option<RolePolicy> = get_on(connection, role)
            .await?
            .and_then(|row| row.policy)
            .map(|raw| {
                decoded += 1;
                from_value(raw).map_err(|error| Protocol(error.to_string()))
            })
            .transpose()?;
        for composition in &mut *first {
            composition.include(&permissions, policy.as_ref().map(Cow::Borrowed));
        }
        last.include(&permissions, policy.map(Cow::Owned));
    }
    add("role-authority-roles-read", roles.len() as u64);
    add("role-authority-policy-decodes", decoded);
    Ok(compositions.map(Composition::finish))
}
