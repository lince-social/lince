use injection::cross_cutting::InjectedServices;
use std::{
    collections::{BTreeSet, HashMap},
    io::{Error, ErrorKind},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use utils::auth::{decode_jwt, hash_password, issue_jwt, verify_password};

const JWT_TTL: Duration = Duration::from_secs(60 * 60 * 24 * 365);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSubject {
    pub user_id: u64,
    pub username: String,
    pub role_id: u64,
    pub role: String,
    pub permissions: Vec<String>,
}

impl AuthSubject {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn system() -> Self {
        Self {
            user_id: 0,
            username: "local-host".into(),
            role_id: 0,
            role: "system".into(),
            permissions: all_permission_keys(),
        }
    }

    pub fn has_permission(&self, permission: PermissionKey) -> bool {
        let permission = permission.as_str();
        self.permissions.iter().any(|value| value == &permission)
    }

    pub fn require_permission(&self, permission: PermissionKey) -> Result<(), Error> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::PermissionDenied,
                format!("Permission required: {}", permission.as_str()),
            ))
        }
    }
}

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

#[derive(Clone)]
pub struct AuthService {
    services: InjectedServices,
    jwt_secret: Arc<String>,
    cache: Arc<RwLock<Option<HashMap<u64, CachedAuthUser>>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedAuthUser {
    id: i64,
    username: String,
    role_id: i64,
    role: String,
    permissions: Vec<String>,
}

impl AuthService {
    pub fn new(services: InjectedServices, jwt_secret: Arc<String>) -> Self {
        let service = Self {
            services,
            jwt_secret,
            cache: Arc::new(RwLock::new(None)),
        };
        service.spawn_invalidation_listener();
        service
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<String, Error> {
        let user = self
            .services
            .repository
            .user
            .get_by_username(username)
            .await?
            .ok_or_else(|| {
                Error::new(ErrorKind::PermissionDenied, "Invalid username or password")
            })?;

        if !verify_password(password, &user.password_hash)? {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "Invalid username or password",
            ));
        }

        issue_jwt(
            self.jwt_secret.as_str(),
            user.id as u64,
            &user.username,
            user.role_id as u64,
            &user.role,
            &user.permissions,
            JWT_TTL,
        )
    }

    pub async fn authenticate_authorization(
        &self,
        authorization: &str,
    ) -> Result<AuthSubject, Error> {
        let token = authorization
            .strip_prefix("Bearer ")
            .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "Expected Bearer token"))?;

        self.authenticate_token(token).await
    }

    pub async fn authenticate_token(&self, token: &str) -> Result<AuthSubject, Error> {
        let claims = decode_jwt(self.jwt_secret.as_str(), token)?;
        let cache = self.ensure_cache().await?;
        let current = cache.get(&claims.sub).ok_or_else(|| {
            Error::new(
                ErrorKind::PermissionDenied,
                "User from token no longer exists",
            )
        })?;

        if current.username != claims.username
            || current.role_id as u64 != claims.role_id
            || current.role != claims.role
            || current.permissions != normalized_permission_strings(claims.permissions.clone())
        {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "Token auth data is stale",
            ));
        }

        Ok(AuthSubject {
            user_id: claims.sub,
            username: claims.username,
            role_id: claims.role_id,
            role: claims.role,
            permissions: normalized_permission_strings(claims.permissions),
        })
    }

    pub fn hash_password(&self, password: &str) -> Result<String, Error> {
        hash_password(password)
    }

    pub async fn refresh_cache(&self) -> Result<(), Error> {
        let next = self.load_cache().await?;
        *self.cache.write().await = Some(next);
        Ok(())
    }

    async fn ensure_cache(&self) -> Result<HashMap<u64, CachedAuthUser>, Error> {
        {
            let guard = self.cache.read().await;
            if let Some(cache) = &*guard {
                return Ok(cache.clone());
            }
        }

        let next = self.load_cache().await?;
        let mut guard = self.cache.write().await;
        let cache = guard.get_or_insert(next);
        Ok(cache.clone())
    }

    async fn load_cache(&self) -> Result<HashMap<u64, CachedAuthUser>, Error> {
        let users = self.services.repository.user.list_auth_users().await?;

        Ok(users
            .into_iter()
            .map(|user| {
                (
                    user.id as u64,
                    CachedAuthUser {
                        id: user.id,
                        username: user.username,
                        role_id: user.role_id,
                        role: user.role,
                        permissions: user.permissions,
                    },
                )
            })
            .collect())
    }

    fn spawn_invalidation_listener(&self) {
        let service = self.clone();
        let mut invalidation_rx = self.services.writer.subscribe_invalidations();

        tokio::spawn(async move {
            loop {
                match invalidation_rx.recv().await {
                    Ok(event) => {
                        let should_refresh = event.changed_tables.iter().any(|table| {
                            matches!(
                                table.as_str(),
                                "app_user" | "role" | "permission" | "role_permission"
                            )
                        });

                        if should_refresh {
                            let _ = service.refresh_cache().await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let _ = service.refresh_cache().await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}
