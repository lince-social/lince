use injection::cross_cutting::InjectedServices;
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    sync::Arc,
    time::Duration,
};
use tokio::sync::RwLock;
use utils::auth::{decode_jwt, hash_password, issue_jwt, verify_password};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSubject {
    pub user_id: u64,
    pub username: String,
    pub role_id: u64,
    pub role: String,
}

impl AuthSubject {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
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
            Duration::from_secs(60 * 60 * 24),
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
                        let should_refresh = event
                            .changed_tables
                            .iter()
                            .any(|table| matches!(table.as_str(), "app_user" | "role"));

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
