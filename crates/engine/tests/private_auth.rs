use std::future::{Future, poll_fn};
use std::task::Poll;
use std::time::Duration;

use engine::access::AccessLimits;
use engine::private_auth::{AuthenticationError, Authenticator};
use engine::private_password::{PasswordHash, PasswordInput, PasswordWork};
use iroh::endpoint::{Connection, presets};
use iroh::{Endpoint, EndpointAddr, SecretKey};
use protein::Predicate;
use protein::authority::RolePolicy;
use store::Store;
use store::session_access::{self, ContactTrust};

const PASSWORD: &[u8] = b" exact private password \n";
const ALPN: &[u8] = b"private-auth-test/1";
static FIXTURE_HASH: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();

fn password(bytes: &[u8]) -> PasswordInput {
    PasswordInput::new(bytes.to_vec()).unwrap()
}

struct Peer {
    host: Endpoint,
    client: Endpoint,
    accepted: Connection,
    connected: Connection,
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

impl Peer {
    async fn new(seed: u8) -> Self {
        let host = endpoint(1).await;
        let client = endpoint(seed).await;
        let address = EndpointAddr::new(host.id()).with_ip_addr(host.bound_sockets()[0]);
        let (connected, accepted) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(client.connect(address, ALPN), async {
                host.accept().await.unwrap().await
            })
        })
        .await
        .unwrap();
        Self {
            host,
            client,
            accepted: accepted.unwrap(),
            connected: connected.unwrap(),
        }
    }

    fn node(&self) -> String {
        self.client.id().to_string()
    }

    async fn close(self) {
        self.connected.close(0u32.into(), b"fixture complete");
        self.accepted.close(0u32.into(), b"fixture complete");
        self.client.close().await;
        self.host.close().await;
    }
}

struct Fixture {
    store: Store,
    hosted: String,
    person: String,
    role: i64,
    permission: i64,
    work: PasswordWork,
}

impl Fixture {
    async fn new(credential: bool) -> Self {
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
                head: "Initial Person",
                body: "",
                quantity: store::exact::zero(),
            },
        )
        .await
        .unwrap()
        .uid;
        let role = store::auth::ensure_role(&store.pool, "not-a-magic-role")
            .await
            .unwrap();
        store::auth::compare_and_set_role(&store.pool, &person, Some(role), 0)
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
        if credential {
            let hash = FIXTURE_HASH
                .get_or_init(|| async {
                    work.hash(password(PASSWORD))
                        .await
                        .unwrap()
                        .as_phc()
                        .to_string()
                })
                .await;
            store::auth::create_credential(&store.pool, &person, "person", hash, role)
                .await
                .unwrap();
        }
        Self {
            store,
            hosted,
            person,
            role,
            permission,
            work,
        }
    }

    fn authenticator(&self, credentials: bool) -> Authenticator {
        Authenticator::new(
            self.store.clone(),
            &self.hosted,
            self.work.clone(),
            credentials,
            AccessLimits::default(),
        )
        .unwrap()
    }

    async fn contact(&self, peer: &Peer) -> String {
        let organ = nucleus::new_uid("r");
        store::organs::add_contact(&self.store.pool, &organ, None, "Peer", "", 1)
            .await
            .unwrap();
        store::organs::set_node_id(&self.store.pool, &organ, Some(&peer.node()))
            .await
            .unwrap();
        store::organs::set_trust(&self.store.pool, &organ, "unknown")
            .await
            .unwrap();
        organ
    }

    async fn device_count(&self) -> i64 {
        store::sqlx::query_scalar("SELECT count(*) FROM person_device")
            .fetch_one(&self.store.pool)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn private_auth_password_returns_only_current_actual_peer_admission() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let auth = fixture.authenticator(true);
    let admission = auth
        .password(&peer.accepted, "person", password(PASSWORD))
        .await
        .unwrap();
    assert_eq!(admission.authentication().person_uid(), fixture.person);
    assert_eq!(admission.device().node_id, peer.node());
    assert_ne!(admission.device().node_id, peer.host.id().to_string());
    assert_eq!(admission.peer_contact(), None);
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    session_access::require_admission_on(&mut tx, &admission)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let repeated = auth
        .password(&peer.accepted, "person", password(PASSWORD))
        .await
        .unwrap();
    assert_eq!(repeated, admission);
    assert_eq!(fixture.device_count().await, 1);
    peer.close().await;
}

#[tokio::test]
async fn private_auth_wrong_absent_and_untrimmed_passwords_have_one_refusal() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let auth = fixture.authenticator(true);
    for (username, bytes) in [
        ("person", b"wrong".as_slice()),
        ("absent", PASSWORD),
        ("person", b"exact private password".as_slice()),
    ] {
        assert_eq!(
            auth.password(&peer.accepted, username, password(bytes))
                .await
                .unwrap_err(),
            AuthenticationError::Refused
        );
        assert_eq!(fixture.device_count().await, 0);
    }
    peer.close().await;
}

#[tokio::test]
async fn private_auth_credential_flag_never_blocks_explicit_credential_free_grant() {
    let fixture = Fixture::new(false).await;
    let peer = Peer::new(2).await;
    let organ = fixture.contact(&peer).await;
    store::logins::grant(&fixture.store.pool, &organ, &fixture.person)
        .await
        .unwrap();
    let auth = fixture.authenticator(false);
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    let admission = auth.granted(&peer.accepted).await.unwrap();
    assert_eq!(admission.authentication().person_uid(), fixture.person);
    assert_eq!(admission.peer_contact().unwrap().organ_uid, organ);
    assert_eq!(
        admission.peer_contact().unwrap().trust,
        ContactTrust::Unknown
    );
    assert!(
        !store::auth::has_credential(&fixture.store.pool, &fixture.person)
            .await
            .unwrap()
    );
    let grants: i64 = store::sqlx::query_scalar("SELECT count(*) FROM replica_grant")
        .fetch_one(&fixture.store.pool)
        .await
        .unwrap();
    assert_eq!(grants, 0);
    let other = Peer::new(3).await;
    assert_eq!(
        auth.granted(&other.accepted).await.unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(fixture.device_count().await, 1);
    other.close().await;
    peer.close().await;
}

#[tokio::test]
async fn private_auth_disabled_deleted_and_malformed_accounts_do_not_enroll() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let auth = fixture.authenticator(true);
    store::people::deactivate(
        &fixture.store.pool,
        &fixture.person,
        "2026-01-01T00:00:00Z",
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    store::people::reactivate(&fixture.store.pool, &fixture.person)
        .await
        .unwrap();
    store::sqlx::query("UPDATE record SET deleted_at = '2026-01-01T00:00:00Z' WHERE uid = ?")
        .bind(&fixture.person)
        .execute(&fixture.store.pool)
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    store::sqlx::query("UPDATE record SET deleted_at = NULL WHERE uid = ?")
        .bind(&fixture.person)
        .execute(&fixture.store.pool)
        .await
        .unwrap();
    for hash in [
        "not a PHC",
        "$argon2id$v=19$m=999999999,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        store::sqlx::query("UPDATE person_credential SET password_hash = ? WHERE person_uid = ?")
            .bind(hash)
            .bind(&fixture.person)
            .execute(&fixture.store.pool)
            .await
            .unwrap();
        assert_eq!(
            auth.password(&peer.accepted, "person", password(PASSWORD))
                .await
                .unwrap_err(),
            AuthenticationError::Refused
        );
    }
    assert_eq!(fixture.device_count().await, 0);
    peer.close().await;
}

#[tokio::test]
async fn private_auth_missing_policy_and_role_roll_back_provisional_device() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let auth = fixture.authenticator(true);
    store::role_policies::clear(&fixture.store.pool, fixture.role, 1)
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(fixture.device_count().await, 0);
    let access = store::auth::person_access(&fixture.store.pool, &fixture.person)
        .await
        .unwrap()
        .unwrap();
    store::auth::compare_and_set_role(&fixture.store.pool, &fixture.person, None, access.revision)
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(fixture.device_count().await, 0);
    peer.close().await;
}

#[tokio::test]
async fn private_auth_corrupt_policy_filter_and_coarse_permission_refuse() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let auth = fixture.authenticator(true);
    store::role_policies::set(
        &fixture.store.pool,
        fixture.role,
        &serde_json::json!({"read": {"all": []}, "unknown": true}),
        1,
    )
    .await
    .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    let valid = serde_json::to_value(RolePolicy {
        read: Predicate::All(Vec::new()),
        grants: Vec::new(),
    })
    .unwrap();
    store::role_policies::set(&fixture.store.pool, fixture.role, &valid, 2)
        .await
        .unwrap();
    let access = store::auth::person_access(&fixture.store.pool, &fixture.person)
        .await
        .unwrap()
        .unwrap();
    let access = store::auth::compare_and_set_read_filter(
        &fixture.store.pool,
        &fixture.person,
        Some("{\"all\":[],\"unknown\":true}"),
        access.revision,
    )
    .await
    .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    store::auth::compare_and_set_read_filter(
        &fixture.store.pool,
        &fixture.person,
        None,
        access.revision,
    )
    .await
    .unwrap();
    store::auth::revoke(&fixture.store.pool, fixture.role, fixture.permission)
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(fixture.device_count().await, 0);
    peer.close().await;
}

#[tokio::test]
async fn private_auth_missing_or_wrong_kind_hosted_scope_rolls_back_device() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    for scope in [nucleus::new_uid("r"), fixture.person.clone()] {
        let auth = Authenticator::new(
            fixture.store.clone(),
            &scope,
            fixture.work.clone(),
            true,
            AccessLimits::default(),
        )
        .unwrap();
        assert_eq!(
            auth.password(&peer.accepted, "person", password(PASSWORD))
                .await
                .unwrap_err(),
            AuthenticationError::Refused
        );
        assert_eq!(fixture.device_count().await, 0);
    }
    assert!(
        Authenticator::new(
            fixture.store.clone(),
            "person",
            fixture.work.clone(),
            true,
            AccessLimits::default()
        )
        .is_err()
    );
    peer.close().await;
}

#[tokio::test]
async fn private_auth_revoked_device_never_reactivates_on_password_or_grant() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let organ = fixture.contact(&peer).await;
    store::logins::grant(&fixture.store.pool, &organ, &fixture.person)
        .await
        .unwrap();
    let auth = fixture.authenticator(true);
    let admission = auth
        .password(&peer.accepted, "person", password(PASSWORD))
        .await
        .unwrap();
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    let revoked = session_access::compare_and_set_revoked_on(
        &mut tx,
        &fixture.person,
        &peer.node(),
        admission.device().revision,
        true,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(
        auth.granted(&peer.accepted).await.unwrap_err(),
        AuthenticationError::Refused
    );
    let mut tx = store::write_tx(&fixture.store.pool).await.unwrap();
    assert_eq!(
        session_access::device_on(&mut tx, &fixture.person, &peer.node())
            .await
            .unwrap(),
        Some(revoked)
    );
    tx.rollback().await.unwrap();
    peer.close().await;
}

#[tokio::test]
async fn private_auth_blocked_or_corrupt_contact_refuses_password_and_grant() {
    let fixture = Fixture::new(true).await;
    let peer = Peer::new(2).await;
    let organ = fixture.contact(&peer).await;
    store::logins::grant(&fixture.store.pool, &organ, &fixture.person)
        .await
        .unwrap();
    let auth = fixture.authenticator(true);
    store::organs::set_trust(&fixture.store.pool, &organ, "blocked")
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(
        auth.granted(&peer.accepted).await.unwrap_err(),
        AuthenticationError::Refused
    );
    store::organs::set_trust(&fixture.store.pool, &organ, "unknown")
        .await
        .unwrap();
    store::sqlx::query("UPDATE organ_contact SET node_id = upper(node_id) WHERE record_uid = ?")
        .bind(&organ)
        .execute(&fixture.store.pool)
        .await
        .unwrap();
    assert_eq!(
        auth.password(&peer.accepted, "person", password(PASSWORD))
            .await
            .unwrap_err(),
        AuthenticationError::Refused
    );
    assert_eq!(fixture.device_count().await, 0);
    peer.close().await;
}

#[test]
fn private_auth_shared_password_capacity_returns_busy_for_real_and_dummy_work() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    runtime.block_on(async {
        let fixture = Fixture::new(true).await;
        let peer = Peer::new(2).await;
        let hash = fixture.work.hash(password(PASSWORD)).await.unwrap();
        let organ = fixture.contact(&peer).await;
        store::logins::grant(&fixture.store.pool, &organ, &fixture.person)
            .await
            .unwrap();
        let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let blocker = tokio::task::spawn_blocking(move || {
            let _ = entered_sender.send(());
            let _ = release_receiver.recv_timeout(Duration::from_secs(10));
        });
        entered_receiver.await.unwrap();
        let mut occupying = Box::pin(fixture.work.verify(password(PASSWORD), hash));
        poll_fn(|context| {
            assert!(occupying.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        let auth = fixture.authenticator(true);
        for username in ["person", "absent"] {
            assert_eq!(
                auth.password(&peer.accepted, username, password(PASSWORD))
                    .await
                    .unwrap_err(),
                AuthenticationError::Busy
            );
        }
        assert_eq!(fixture.device_count().await, 0);
        drop(occupying);
        assert_eq!(
            auth.granted(&peer.accepted)
                .await
                .unwrap()
                .authentication()
                .person_uid(),
            fixture.person
        );
        release_sender.send(()).unwrap();
        blocker.await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            match auth
                .password(&peer.accepted, "person", password(PASSWORD))
                .await
            {
                Ok(_) => break,
                Err(AuthenticationError::Busy) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(1)).await
                }
                result => panic!("password capacity did not recover: {result:?}"),
            }
        }
        peer.close().await;
    });
}

#[test]
fn private_auth_errors_and_password_inputs_are_content_free() {
    assert_eq!(
        AuthenticationError::Refused.to_string(),
        "authentication refused"
    );
    assert_eq!(AuthenticationError::Busy.to_string(), "authentication busy");
    assert_eq!(
        format!("{:?}", password(PASSWORD)),
        "PasswordInput([REDACTED])"
    );
    let hash = PasswordHash::from_phc("$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into()).unwrap();
    assert_eq!(format!("{hash:?}"), "PasswordHash([REDACTED])");
}
