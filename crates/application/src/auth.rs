use injection::cross_cutting::InjectedServices;
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
    time::Duration,
};
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
}

impl AuthService {
    pub fn new(services: InjectedServices, jwt_secret: Arc<String>) -> Self {
        Self {
            services,
            jwt_secret,
        }
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

    pub fn authenticate_authorization(&self, authorization: &str) -> Result<AuthSubject, Error> {
        let token = authorization
            .strip_prefix("Bearer ")
            .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "Expected Bearer token"))?;

        self.authenticate_token(token)
    }

    pub fn authenticate_token(&self, token: &str) -> Result<AuthSubject, Error> {
        let claims = decode_jwt(self.jwt_secret.as_str(), token)?;
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
}
