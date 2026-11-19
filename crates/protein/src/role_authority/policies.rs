use crate::{
    Predicate, ProteinError,
    authority::{MutationGrant, Operation, RolePolicy},
};
use std::borrow::Cow;

fn permission(operation: Option<Operation>) -> &'static str {
    match operation {
        None => "record:read",
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
        let read_allowed = allows("record:read");
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
        let operation = self.operation;
        match policy {
            Cow::Borrowed(policy) => {
                if read_allowed && !self.unrestricted_read {
                    self.read.push(policy.read.clone());
                }
                if write_allowed {
                    self.grants.extend(
                        policy
                            .grants
                            .iter()
                            .filter(|grant| {
                                operation.is_none_or(|operation| grant.operation == operation)
                            })
                            .cloned(),
                    );
                }
            }
            Cow::Owned(policy) => {
                if read_allowed && !self.unrestricted_read {
                    self.read.push(policy.read);
                }
                if write_allowed {
                    self.grants
                        .extend(policy.grants.into_iter().filter(|grant| {
                            operation.is_none_or(|operation| grant.operation == operation)
                        }));
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

pub async fn policies_for_on<const N: usize>(
    connection: &mut store::sqlx::SqliteConnection,
    person: &str,
    operations: [Option<Operation>; N],
) -> Result<[Option<RolePolicy>; N], ProteinError> {
    let _read = nucleus::performance::Span::new("role-authority-policy-read-await");
    nucleus::performance::add("role-authority-policies-requested", N as u64);
    if N == 0 {
        return Ok(operations.map(|_| None));
    }
    let roles = store::person_roles::ids_on(connection, person).await?;
    let mut compositions = operations.map(Composition::new);
    let mut decoded = 0;
    for &role in &roles {
        let permissions = store::auth::role_permission_keys_by_id_on(connection, role).await?;
        let raw = store::role_policies::get_on(connection, role)
            .await?
            .and_then(|row| row.policy);
        let mut policy: Option<RolePolicy> = raw
            .map(|raw| {
                decoded += 1;
                serde_json::from_value(raw)
                    .map_err(|error| store::StoreError::Protocol(error.to_string()))
            })
            .transpose()?;
        for (index, composition) in compositions.iter_mut().enumerate() {
            let contribution = if index + 1 == N {
                policy.take().map(Cow::Owned)
            } else {
                policy.as_ref().map(Cow::Borrowed)
            };
            composition.include(&permissions, contribution);
        }
    }
    nucleus::performance::add("role-authority-roles-read", roles.len() as u64);
    nucleus::performance::add("role-authority-policy-decodes", decoded);
    Ok(compositions.map(Composition::finish))
}
