use std::fmt;
use std::future::{Future, ready};

use iroh::endpoint::Connection;
use store::Store;
use store::session_access::{
    self, AuthenticationState, ContactTrust, DeviceAdmission, PeerContact, PersonDevice,
};

use crate::access::{self, AccessLimits};
use crate::private_password::{PasswordError, PasswordHash, PasswordInput, PasswordWork};

const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthenticationError {
    Refused,
    Busy,
}

impl fmt::Display for AuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Refused => "authentication refused",
            Self::Busy => "authentication busy",
        })
    }
}

impl std::error::Error for AuthenticationError {}

impl From<store::StoreError> for AuthenticationError {
    fn from(_: store::StoreError) -> Self {
        Self::Refused
    }
}

impl From<PasswordError> for AuthenticationError {
    fn from(error: PasswordError) -> Self {
        match error {
            PasswordError::Busy => Self::Busy,
            _ => Self::Refused,
        }
    }
}

struct Captured {
    authentication: AuthenticationState,
    peer: Option<PeerContact>,
    device: Option<PersonDevice>,
}

pub struct Authenticator {
    store: Store,
    hosted_organ: String,
    passwords: PasswordWork,
    accept_credentials: bool,
    limits: AccessLimits,
}

impl Authenticator {
    pub fn new(
        store: Store,
        hosted_organ: &str,
        passwords: PasswordWork,
        accept_credentials: bool,
        limits: AccessLimits,
    ) -> Result<Self, AuthenticationError> {
        if !nucleus::valid_uid(hosted_organ, "r") {
            return Err(AuthenticationError::Refused);
        }
        Ok(Self {
            store,
            hosted_organ: hosted_organ.into(),
            passwords,
            accept_credentials,
            limits,
        })
    }

    pub async fn password(
        &self,
        connection: &Connection,
        username: &str,
        password: PasswordInput,
    ) -> Result<DeviceAdmission, AuthenticationError> {
        self.password_phases(connection, username, password, ready(()), ready(()))
            .await
    }

    pub async fn granted(
        &self,
        connection: &Connection,
    ) -> Result<DeviceAdmission, AuthenticationError> {
        self.granted_phases(connection, ready(())).await
    }

    async fn capture_password(
        &self,
        peer_id: &str,
        username: &str,
    ) -> Result<Option<(Captured, PasswordHash)>, AuthenticationError> {
        let mut tx = self.store.pool.begin().await?;
        let peer = session_access::peer_contact_on(&mut tx, peer_id).await?;
        if peer
            .as_ref()
            .is_some_and(|peer| peer.trust == ContactTrust::Blocked)
        {
            return Err(AuthenticationError::Refused);
        }
        let Some(credential) = session_access::password_on(&mut tx, username).await? else {
            tx.commit().await?;
            return Ok(None);
        };
        let hash = PasswordHash::from_phc(credential.password_hash().into())?;
        let authentication = credential.authentication().clone();
        let device =
            session_access::device_on(&mut tx, authentication.person_uid(), peer_id).await?;
        tx.commit().await?;
        Ok(Some((
            Captured {
                authentication,
                peer,
                device,
            },
            hash,
        )))
    }

    async fn password_phases<Capture: Future<Output = ()>, Verified: Future<Output = ()>>(
        &self,
        connection: &Connection,
        username: &str,
        password: PasswordInput,
        after_capture: Capture,
        after_verification: Verified,
    ) -> Result<DeviceAdmission, AuthenticationError> {
        if !self.accept_credentials {
            return Err(AuthenticationError::Refused);
        }
        let peer_id = connection.remote_id().to_string();
        let captured = self.capture_password(&peer_id, username).await?;
        let (captured, hash) = match captured {
            Some((captured, hash)) => (Some(captured), hash),
            None => (None, PasswordHash::from_phc(DUMMY_HASH.into())?),
        };
        after_capture.await;
        if !self.passwords.verify(password, hash).await? {
            return Err(AuthenticationError::Refused);
        }
        let captured = captured.ok_or(AuthenticationError::Refused)?;
        after_verification.await;
        self.finish(&peer_id, captured).await
    }

    async fn granted_phases<Capture: Future<Output = ()>>(
        &self,
        connection: &Connection,
        after_capture: Capture,
    ) -> Result<DeviceAdmission, AuthenticationError> {
        let peer_id = connection.remote_id().to_string();
        let mut tx = self.store.pool.begin().await?;
        let peer = session_access::peer_contact_on(&mut tx, &peer_id)
            .await?
            .ok_or(AuthenticationError::Refused)?;
        let authentication = session_access::granted_login_on(&mut tx, &peer.organ_uid, &peer_id)
            .await?
            .ok_or(AuthenticationError::Refused)?;
        let device =
            session_access::device_on(&mut tx, authentication.person_uid(), &peer_id).await?;
        tx.commit().await?;
        let captured = Captured {
            authentication,
            peer: Some(peer),
            device,
        };
        after_capture.await;
        self.finish(&peer_id, captured).await
    }

    async fn finish(
        &self,
        peer_id: &str,
        captured: Captured,
    ) -> Result<DeviceAdmission, AuthenticationError> {
        let mut tx = store::write_tx(&self.store.pool).await?;
        session_access::require_authentication_on(&mut tx, &captured.authentication).await?;
        let peer = session_access::peer_contact_on(&mut tx, peer_id).await?;
        let device =
            session_access::device_on(&mut tx, captured.authentication.person_uid(), peer_id)
                .await?;
        if peer != captured.peer || device != captured.device {
            return Err(AuthenticationError::Refused);
        }
        let admission =
            session_access::register_device_on(&mut tx, &captured.authentication, peer_id).await?;
        let access = access::load_on(
            &mut tx,
            &admission,
            &self.hosted_organ,
            &[],
            self.limits.clone(),
        )
        .await
        .map_err(|_| AuthenticationError::Refused)?;
        drop(access);
        tx.commit().await?;
        Ok(admission)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use iroh::endpoint::presets;
    use iroh::{Endpoint, EndpointAddr, SecretKey};
    use protein::Predicate;
    use protein::authority::RolePolicy;

    use super::*;

    const PASSWORD: &[u8] = b"phase witness password";
    const ALPN: &[u8] = b"private-auth-phase-test/1";
    static FIXTURE_HASH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

    fn password() -> PasswordInput {
        PasswordInput::new(PASSWORD.to_vec()).unwrap()
    }

    struct Fixture {
        auth: Authenticator,
        person: String,
        role: i64,
        permission: i64,
        contact: String,
        node: String,
        connection: Connection,
        client_connection: Connection,
        server: Endpoint,
        client: Endpoint,
    }

    async fn endpoint(seed: u8) -> Endpoint {
        Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&[seed; 32]))
            .alpns(vec![ALPN.to_vec()])
            .portmapper_config(iroh::endpoint::PortmapperConfig::Disabled)
            .net_report_config(iroh::endpoint::NetReportConfig::minimal())
            .clear_ip_transports()
            .bind_addr("127.0.0.1:0")
            .unwrap()
            .bind()
            .await
            .unwrap()
    }

    impl Fixture {
        async fn new() -> Self {
            let store = Store::open_memory().await.unwrap();
            let hosted = store::organs::local(&store.pool)
                .await
                .unwrap()
                .unwrap()
                .uid;
            let person = store::records::create(
                &store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Person,
                    head: "Phase Person",
                    body: "",
                    quantity: store::exact::zero(),
                },
            )
            .await
            .unwrap()
            .uid;
            let role = store::auth::ensure_role(&store.pool, "phase authority")
                .await
                .unwrap();
            let permission = store::auth::ensure_permission(&store.pool, "record", "read")
                .await
                .unwrap();
            store::auth::grant(&store.pool, role, permission)
                .await
                .unwrap();
            let policy = RolePolicy {
                read: Predicate::All(Vec::new()),
                grants: Vec::new(),
            };
            store::role_policies::set(&store.pool, role, &serde_json::to_value(policy).unwrap(), 0)
                .await
                .unwrap();
            let work = PasswordWork::new(1).unwrap();
            let hash = FIXTURE_HASH
                .get_or_init(|| async { work.hash(password()).await.unwrap().as_phc().to_string() })
                .await;
            store::auth::create_credential(&store.pool, &person, "person", hash, role)
                .await
                .unwrap();
            let server = endpoint(11).await;
            let client = endpoint(12).await;
            let address = EndpointAddr::new(server.id()).with_ip_addr(server.bound_sockets()[0]);
            let (client_connection, connection) =
                tokio::time::timeout(Duration::from_secs(10), async {
                    tokio::join!(client.connect(address, ALPN), async {
                        server.accept().await.unwrap().await
                    })
                })
                .await
                .unwrap();
            let node = client.id().to_string();
            let contact = nucleus::new_uid("r");
            store::organs::add_contact(&store.pool, &contact, None, "Phase peer", "", 1)
                .await
                .unwrap();
            store::organs::set_node_id(&store.pool, &contact, Some(&node))
                .await
                .unwrap();
            store::organs::set_trust(&store.pool, &contact, "unknown")
                .await
                .unwrap();
            store::logins::grant(&store.pool, &contact, &person)
                .await
                .unwrap();
            let mut tx = store::write_tx(&store.pool).await.unwrap();
            let credential = session_access::password_on(&mut tx, "person")
                .await
                .unwrap()
                .unwrap();
            session_access::register_device_on(&mut tx, credential.authentication(), &node)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            Self {
                auth: Authenticator::new(store, &hosted, work, true, AccessLimits::default())
                    .unwrap(),
                person,
                role,
                permission,
                contact,
                node,
                connection: connection.unwrap(),
                client_connection: client_connection.unwrap(),
                server,
                client,
            }
        }

        async fn mutate(&self, change: Change) {
            let store = &self.auth.store;
            match change {
                Change::PasswordReplace | Change::PasswordRemove => {
                    let mut tx = store::write_tx(&store.pool).await.unwrap();
                    let generation = store::auth::credential_generation_on(&mut tx, &self.person)
                        .await
                        .unwrap();
                    if matches!(change, Change::PasswordReplace) {
                        store::auth::replace_credential_on(
                            &mut tx,
                            &self.person,
                            "person",
                            DUMMY_HASH,
                            generation,
                        )
                        .await
                        .unwrap();
                    } else {
                        store::auth::remove_credential_on(&mut tx, &self.person, generation)
                            .await
                            .unwrap();
                    }
                    tx.commit().await.unwrap();
                }
                Change::PersonDisable | Change::StandingAba => {
                    store::people::deactivate(
                        &store.pool,
                        &self.person,
                        "2026-01-01T00:00:00Z",
                        None,
                    )
                    .await
                    .unwrap();
                    if matches!(change, Change::StandingAba) {
                        store::people::reactivate(&store.pool, &self.person)
                            .await
                            .unwrap();
                    }
                }
                Change::ContactAba => {
                    store::organs::set_node_id(
                        &store.pool,
                        &self.contact,
                        Some(&format!("{:064x}", 42)),
                    )
                    .await
                    .unwrap();
                    store::organs::set_node_id(&store.pool, &self.contact, Some(&self.node))
                        .await
                        .unwrap();
                }
                Change::ContactBlock => {
                    store::organs::set_trust(&store.pool, &self.contact, "blocked")
                        .await
                        .unwrap();
                }
                Change::GrantAba => {
                    store::logins::revoke(&store.pool, &self.contact)
                        .await
                        .unwrap();
                    store::logins::grant(&store.pool, &self.contact, &self.person)
                        .await
                        .unwrap();
                }
                Change::RoleRemove | Change::FilterInvalid => {
                    let access = store::auth::person_access(&store.pool, &self.person)
                        .await
                        .unwrap()
                        .unwrap();
                    if matches!(change, Change::RoleRemove) {
                        store::auth::compare_and_set_role(
                            &store.pool,
                            &self.person,
                            None,
                            access.revision,
                        )
                        .await
                        .unwrap();
                    } else {
                        store::auth::compare_and_set_read_filter(
                            &store.pool,
                            &self.person,
                            Some("invalid filter"),
                            access.revision,
                        )
                        .await
                        .unwrap();
                    }
                }
                Change::PolicyClear => {
                    store::role_policies::clear(&store.pool, self.role, 1)
                        .await
                        .unwrap();
                }
                Change::PermissionRevoke => {
                    store::auth::revoke(&store.pool, self.role, self.permission)
                        .await
                        .unwrap();
                }
                Change::DeviceRevoke | Change::DeviceAba => {
                    let mut tx = store::write_tx(&store.pool).await.unwrap();
                    let device = session_access::device_on(&mut tx, &self.person, &self.node)
                        .await
                        .unwrap()
                        .unwrap();
                    let revoked = session_access::compare_and_set_revoked_on(
                        &mut tx,
                        &self.person,
                        &self.node,
                        device.revision,
                        true,
                    )
                    .await
                    .unwrap();
                    if matches!(change, Change::DeviceAba) {
                        session_access::compare_and_set_revoked_on(
                            &mut tx,
                            &self.person,
                            &self.node,
                            revoked.revision,
                            false,
                        )
                        .await
                        .unwrap();
                    }
                    tx.commit().await.unwrap();
                }
            }
        }

        async fn close(self) {
            self.connection
                .close(0u32.into(), b"phase fixture complete");
            self.client_connection
                .close(0u32.into(), b"phase fixture complete");
            self.client.close().await;
            self.server.close().await;
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum Change {
        PasswordReplace,
        PasswordRemove,
        PersonDisable,
        StandingAba,
        ContactAba,
        ContactBlock,
        GrantAba,
        RoleRemove,
        PolicyClear,
        FilterInvalid,
        PermissionRevoke,
        DeviceRevoke,
        DeviceAba,
    }

    const PASSWORD_CHANGES: &[Change] = &[
        Change::PasswordReplace,
        Change::PasswordRemove,
        Change::PersonDisable,
        Change::StandingAba,
        Change::ContactAba,
        Change::ContactBlock,
        Change::RoleRemove,
        Change::PolicyClear,
        Change::FilterInvalid,
        Change::PermissionRevoke,
        Change::DeviceRevoke,
        Change::DeviceAba,
    ];

    #[tokio::test]
    async fn private_auth_capture_races_recheck_real_password_and_current_authority() {
        for &change in PASSWORD_CHANGES {
            let fixture = Fixture::new().await;
            let result = fixture
                .auth
                .password_phases(
                    &fixture.connection,
                    "person",
                    password(),
                    fixture.mutate(change),
                    ready(()),
                )
                .await;
            assert_eq!(
                result.unwrap_err(),
                AuthenticationError::Refused,
                "{change:?}"
            );
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn private_auth_verified_races_cannot_use_a_completed_password_check_as_authority() {
        for &change in PASSWORD_CHANGES {
            let fixture = Fixture::new().await;
            let result = fixture
                .auth
                .password_phases(
                    &fixture.connection,
                    "person",
                    password(),
                    ready(()),
                    fixture.mutate(change),
                )
                .await;
            assert_eq!(
                result.unwrap_err(),
                AuthenticationError::Refused,
                "{change:?}"
            );
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn private_auth_grant_phase_rechecks_generation_contact_role_and_device() {
        for change in [
            Change::GrantAba,
            Change::ContactAba,
            Change::ContactBlock,
            Change::PersonDisable,
            Change::RoleRemove,
            Change::PolicyClear,
            Change::FilterInvalid,
            Change::DeviceRevoke,
        ] {
            let fixture = Fixture::new().await;
            let result = fixture
                .auth
                .granted_phases(&fixture.connection, fixture.mutate(change))
                .await;
            assert_eq!(
                result.unwrap_err(),
                AuthenticationError::Refused,
                "{change:?}"
            );
            fixture.close().await;
        }
    }

    #[tokio::test]
    async fn private_auth_password_worker_does_not_hold_a_sqlite_transaction() {
        let fixture = Fixture::new().await;
        let held = tokio::sync::Mutex::new(None);
        let after_capture = async {
            let tx = store::write_tx(&fixture.auth.store.pool).await.unwrap();
            *held.lock().await = Some(tx);
        };
        let after_verification = async {
            held.lock().await.take().unwrap().rollback().await.unwrap();
        };
        let admission = tokio::time::timeout(
            Duration::from_secs(10),
            fixture.auth.password_phases(
                &fixture.connection,
                "person",
                password(),
                after_capture,
                after_verification,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(admission.authentication().person_uid(), fixture.person);
        assert_eq!(admission.device().node_id, fixture.node);
        fixture.close().await;
    }

    #[tokio::test]
    async fn private_auth_valid_filter_narrowing_during_verification_is_used_immediately() {
        let fixture = Fixture::new().await;
        let narrowed = async {
            let store = &fixture.auth.store;
            let current = store::auth::person_access(&store.pool, &fixture.person)
                .await
                .unwrap()
                .unwrap();
            let filter =
                serde_json::to_string(&Predicate::Not(Box::new(Predicate::All(Vec::new()))))
                    .unwrap();
            store::auth::compare_and_set_read_filter(
                &store.pool,
                &fixture.person,
                Some(&filter),
                current.revision,
            )
            .await
            .unwrap();
        };
        let admission = fixture
            .auth
            .password_phases(
                &fixture.connection,
                "person",
                password(),
                narrowed,
                ready(()),
            )
            .await
            .unwrap();
        let mut tx = store::write_tx(&fixture.auth.store.pool).await.unwrap();
        let access = access::load_on(
            &mut tx,
            &admission,
            &fixture.auth.hosted_organ,
            &[],
            AccessLimits::default(),
        )
        .await
        .unwrap();
        assert!(access.readable().records.is_empty());
        drop(access);
        tx.rollback().await.unwrap();
        fixture.close().await;
    }
}
