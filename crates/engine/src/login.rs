use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use store::session_access::{self, AuthenticationState, DeviceAdmission};
use store::sqlx::SqliteConnection;
use tokio::sync::watch;

use crate::private_password::{PasswordHash, PasswordInput};
use crate::{Engine, EngineError};

const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
pub const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);
const IDLE_TTL: Duration = Duration::from_secs(30 * 60);

pub(crate) struct LoginAttempts {
    accounts: HashMap<String, (Instant, u32)>,
    window: Instant,
    count: u32,
}

impl Default for LoginAttempts {
    fn default() -> Self {
        Self {
            accounts: HashMap::new(),
            window: Instant::now(),
            count: 0,
        }
    }
}

impl LoginAttempts {
    fn admit(&mut self, account: &str) -> Result<(), EngineError> {
        let now = Instant::now();
        let window = Duration::from_secs(60);
        self.accounts
            .retain(|_, (start, _)| now.duration_since(*start) < window);
        if now.duration_since(self.window) >= window {
            self.window = now;
            self.count = 0;
        }
        if self.count >= 100 || self.accounts.len() >= 4096 {
            return Err(EngineError::Conflict {
                code: "login_throttled",
                message: "Too many login attempts; try again shortly".into(),
            });
        }
        let attempt = self.accounts.entry(account.into()).or_insert((now, 0));
        if attempt.1 >= 10 {
            return Err(EngineError::Conflict {
                code: "login_throttled",
                message: "Too many login attempts; try again shortly".into(),
            });
        }
        attempt.1 += 1;
        self.count += 1;
        Ok(())
    }
}

tokio::task_local! {
    static ACCESS_SCOPE: (usize, bool);
    static CURRENT_LOGIN: LoginSession;
}

#[derive(Clone)]
enum Admission {
    Password(AuthenticationState),
    Device(DeviceAdmission),
}

#[derive(Clone)]
pub struct LoginSession {
    admission: Admission,
    organ: String,
    expires: Instant,
    last_used: Arc<std::sync::Mutex<Instant>>,
    revoked: Arc<watch::Sender<bool>>,
}

impl LoginSession {
    pub fn person_uid(&self) -> &str {
        match &self.admission {
            Admission::Password(state) => state.person_uid(),
            Admission::Device(state) => state.authentication().person_uid(),
        }
    }

    pub fn organ_uid(&self) -> &str {
        &self.organ
    }

    pub fn expires(&self) -> Instant {
        self.last_used
            .lock()
            .map(|last| self.expires.min(*last + IDLE_TTL))
            .unwrap_or_else(|_| Instant::now())
    }

    pub fn touch(&self) {
        if let Ok(mut last) = self.last_used.lock()
            && Instant::now() < (*last + IDLE_TTL).min(self.expires)
        {
            *last = Instant::now();
        }
    }

    pub fn revoke(&self) {
        self.revoked.send_replace(true);
    }

    pub fn watch_revocation(&self) -> watch::Receiver<bool> {
        self.revoked.subscribe()
    }

    pub fn is_live(&self) -> bool {
        !*self.revoked.borrow() && Instant::now() < self.expires()
    }

    pub async fn require_on(&self, connection: &mut SqliteConnection) -> Result<(), EngineError> {
        if !self.is_live() {
            return Err(refused());
        }
        let organ: Option<String> = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE uid = ? AND slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL",
        )
        .bind(&self.organ)
        .fetch_optional(&mut *connection)
        .await?;
        if organ.is_none() {
            return Err(refused());
        }
        match &self.admission {
            Admission::Password(state) => {
                session_access::require_authentication_on(connection, state).await
            }
            Admission::Device(state) => {
                session_access::require_admission_on(connection, state).await
            }
        }
        .map_err(|_| refused())
    }

    pub async fn require(&self, engine: &Engine) -> Result<(), EngineError> {
        let mut tx = engine.store.pool.begin().await?;
        self.require_on(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub fn run<'a, T: 'a>(
        &'a self,
        engine: &'a Engine,
        write: bool,
        operation: impl Future<Output = Result<T, EngineError>> + 'a,
    ) -> impl Future<Output = Result<T, EngineError>> + 'a {
        let operation = Box::pin(operation);
        async move {
            engine
                .access_scope(write, async {
                    self.require(engine).await?;
                    CURRENT_LOGIN.scope(self.clone(), operation).await
                })
                .await
        }
    }
}

fn refused() -> EngineError {
    EngineError::Forbidden("Login is unavailable or expired".into())
}

impl Engine {
    pub async fn require_login_on(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<(), EngineError> {
        if let Ok(login) = CURRENT_LOGIN.try_with(Clone::clone) {
            login.require_on(connection).await?;
        }
        Ok(())
    }

    pub(crate) async fn change_login(
        &self,
        actor: Option<&str>,
        user: &str,
        update: Option<(String, String, String)>,
    ) -> Result<(), EngineError> {
        let uid = self.resolve(user).await?;
        let permission = if update.is_some() {
            "user:update"
        } else {
            "user:delete"
        };
        self.require_permission(actor, permission).await?;
        self.require_manageable_person(actor, &uid).await?;
        let target = store::auth::user_by_uid(&self.store.pool, &uid)
            .await?
            .ok_or_else(refused)?;
        if update.is_none() {
            if actor == Some(uid.as_str()) {
                return Err(EngineError::Forbidden(
                    "You cannot delete your own login".into(),
                ));
            }
            if target.role == "admin" {
                self.require_other_admin(&uid).await?;
            }
        }
        let generation = {
            let mut connection = self.store.pool.acquire().await?;
            store::auth::credential_generation_on(&mut connection, &uid).await?
        };
        let update = if let Some((username, name, password)) = update {
            if username.trim().is_empty()
                || username.len() > 256
                || name.len() > 500
                || password.len() > 1024
            {
                return Err(EngineError::Consequence("Invalid account details".into()));
            }
            let hash = if password.is_empty() {
                target.password_hash
            } else {
                self.passwords
                    .hash(PasswordInput::new(password.into_bytes()).map_err(|_| refused())?)
                    .await
                    .map_err(|_| refused())?
                    .as_phc()
                    .to_string()
            };
            Some((username, name, hash))
        } else {
            None
        };
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        if let Some(actor) = actor {
            let viewer = store::auth::principal_on(&mut tx, actor)
                .await?
                .ok_or_else(refused)?;
            let target = store::auth::assigned_role_on(&mut tx, &uid).await?;
            if !viewer.permits(permission)
                || target.as_ref().is_some_and(|target| {
                    target.permissions.iter().any(|key| !viewer.permits(key))
                        || (target.role == "admin" && viewer.role != "admin")
                })
            {
                return Err(refused());
            }
        }
        if let Some((username, name, hash)) = update {
            store::auth::replace_credential_on(&mut tx, &uid, username.trim(), &hash, generation)
                .await?;
            store::records::set_authoring_text_on(&mut tx, &uid, Some(name.trim()), None).await?;
        } else {
            store::auth::remove_credential_on(&mut tx, &uid, generation).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub fn access_scope<'a, T: 'a>(
        &'a self,
        write: bool,
        operation: impl Future<Output = Result<T, EngineError>> + 'a,
    ) -> impl Future<Output = Result<T, EngineError>> + 'a {
        let operation = Box::pin(operation);
        async move {
            let key = self as *const Self as usize;
            if let Ok((owner, exclusive)) = ACCESS_SCOPE.try_with(|scope| *scope)
                && owner == key
            {
                if write && !exclusive {
                    return Err(EngineError::Forbidden(
                        "A read operation cannot change access".into(),
                    ));
                }
                return operation.await;
            }
            if write {
                let _guard = self.access_gate.write().await;
                ACCESS_SCOPE.scope((key, true), operation).await
            } else {
                let _guard = self.access_gate.read().await;
                ACCESS_SCOPE.scope((key, false), operation).await
            }
        }
    }

    pub async fn login_password(
        &self,
        username: &str,
        password: PasswordInput,
        peer_node: Option<&str>,
    ) -> Result<LoginSession, EngineError> {
        if username.trim().is_empty() || username.len() > 256 {
            return Err(refused());
        }
        self.login_attempts.lock().await.admit(username.trim())?;
        let mut tx = self.store.pool.begin().await?;
        let credential = session_access::password_on(&mut tx, username.trim())
            .await
            .map_err(|_| refused())?;
        let authentication = credential.as_ref().map(|c| c.authentication().clone());
        let hash = PasswordHash::from_phc(
            credential
                .as_ref()
                .map_or(DUMMY_HASH, |c| c.password_hash())
                .into(),
        )
        .map_err(|_| refused())?;
        let captured_peer = if let Some(node) = peer_node {
            session_access::peer_contact_on(&mut tx, node).await?
        } else {
            None
        };
        let captured_device =
            if let (Some(node), Some(authentication)) = (peer_node, &authentication) {
                session_access::device_on(&mut tx, authentication.person_uid(), node).await?
            } else {
                None
            };
        tx.commit().await?;
        if !self
            .passwords
            .verify(password, hash)
            .await
            .map_err(|error| match error {
                crate::private_password::PasswordError::Busy => EngineError::Conflict {
                    code: "login_throttled",
                    message: "Password workers are busy; try again shortly".into(),
                },
                _ => refused(),
            })?
        {
            return Err(refused());
        }
        let authentication = authentication.ok_or_else(refused)?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        session_access::require_authentication_on(&mut tx, &authentication)
            .await
            .map_err(|_| refused())?;
        let admission = if let Some(node) = peer_node {
            if captured_peer != session_access::peer_contact_on(&mut tx, node).await?
                || captured_device
                    != session_access::device_on(&mut tx, authentication.person_uid(), node).await?
            {
                return Err(refused());
            }
            Admission::Device(
                session_access::register_device_on(&mut tx, &authentication, node)
                    .await
                    .map_err(|_| refused())?,
            )
        } else {
            Admission::Password(authentication)
        };
        tx.commit().await?;
        self.login_session(admission).await
    }

    pub async fn login_granted(&self, peer_node: &str) -> Result<LoginSession, EngineError> {
        let mut tx = store::write_tx(&self.store.pool).await?;
        let peer = session_access::peer_contact_on(&mut tx, peer_node)
            .await?
            .ok_or_else(refused)?;
        let authentication = session_access::granted_login_on(&mut tx, &peer.organ_uid, peer_node)
            .await?
            .ok_or_else(refused)?;
        let admission =
            session_access::register_device_on(&mut tx, &authentication, peer_node).await?;
        tx.commit().await?;
        self.login_session(Admission::Device(admission)).await
    }

    async fn login_session(&self, admission: Admission) -> Result<LoginSession, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(refused)?
            .uid;
        let (revoked, _) = watch::channel(false);
        let session = LoginSession {
            admission,
            organ,
            expires: Instant::now() + SESSION_TTL,
            last_used: Arc::new(std::sync::Mutex::new(Instant::now())),
            revoked: Arc::new(revoked),
        };
        session.require(self).await?;
        Ok(session)
    }
}
