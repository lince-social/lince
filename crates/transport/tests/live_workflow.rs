//! The whole workflow, end to end (Ontology §11).
//!
//! Discover an Organ → add them → talk in a thread → grant them a login →
//! they open a live session over iroh and edit a record with me, cursors and
//! all → and messaging keeps working alongside it.
//!
//! Everything here rides iroh, which is the point: there is no hostname, no
//! certificate and no reverse proxy anywhere in it, so nothing breaks when a
//! machine changes network.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use engine::wire::{ALPN_LIVE, Reach, Wire};
use iroh::{EndpointAddr, SecretKey};
use transport::live::LiveHost;
use transport::protocol::{ClientMessage, ServerMessage};

async fn cell(base_url: &str) -> (Arc<Engine>, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, base_url)
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (Arc::new(e), organ)
}

fn loopback(wire: &Wire) -> EndpointAddr {
    let port = wire
        .endpoint()
        .bound_sockets()
        .into_iter()
        .map(|addr| addr.port())
        .next()
        .expect("bound");
    EndpointAddr::new(wire.node_id())
        .with_ip_addr(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
}

async fn know(us: &Engine, organ_uid: &str, node_id: &str) {
    store::organs::add_contact(&us.store.pool, organ_uid, None, "peer", "", 0)
        .await
        .expect("contact");
    store::organs::set_node_id(&us.store.pool, organ_uid, Some(node_id))
        .await
        .expect("node id");
    store::organs::set_trust(&us.store.pool, organ_uid, "known")
        .await
        .expect("trust");
    store::organs::set_sync_policy(&us.store.pool, organ_uid, true, true)
        .await
        .expect("policy");
}

/// Frame a client message the way `transport::live` reads them: a 4-byte
/// big-endian length, then the JSON.
async fn say(send: &mut iroh::endpoint::SendStream, message: &ClientMessage) {
    let body = serde_json::to_vec(message).expect("serialize");
    send.write_all(&(body.len() as u32).to_be_bytes())
        .await
        .expect("write length");
    send.write_all(&body).await.expect("write body");
}

async fn hear(recv: &mut iroh::endpoint::RecvStream) -> ServerMessage {
    let mut len = [0u8; 4];
    recv.read_exact(&mut len).await.expect("read length");
    let mut buf = vec![0u8; u32::from_be_bytes(len) as usize];
    recv.read_exact(&mut buf).await.expect("read body");
    serde_json::from_slice(&buf).expect("parse server message")
}

/// Reactive replica writes borrow the already-open live connection. Removing
/// the saved NodeId after opening makes the ordinary sync dial impossible, so
/// this test cannot pass by silently falling back to a second connection.
#[tokio::test]
async fn reactive_outbox_reuses_the_live_connection_and_clears_its_rows() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;
    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[61; 32]), Reach::Local)
            .await
            .expect("host binds"),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[62; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");
    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;
    host.act(
        Action::GrantOrganLogin {
            organ: guest_organ,
            person_name: "Marcia".into(),
        },
        None,
    )
    .await
    .expect("grant login");

    host_wire.set_live_handler(LiveHost::new(
        host.clone(),
        Arc::new(transport::LaneHub::new()),
    ));
    let serving = {
        let host_wire = host_wire.clone();
        tokio::spawn(async move { host_wire.serve().await })
    };
    let connection = guest_wire
        .endpoint()
        .connect(loopback(&host_wire), ALPN_LIVE)
        .await
        .expect("live dial");
    let (mut live_send, mut live_recv) = connection.accept_bi().await.expect("Protein stream");
    say(
        &mut live_send,
        &ClientMessage::Unsubscribe { id: "wake".into() },
    )
    .await;
    assert!(matches!(
        hear(&mut live_recv).await,
        ServerMessage::LiveHello {
            login_required: false
        }
    ));
    assert!(matches!(
        hear(&mut live_recv).await,
        ServerMessage::SessionChallenge { .. }
    ));
    guest_wire.remember_live_connection(&host_organ, &connection);

    // If reuse fails there is nowhere else to dial, making a false-green test
    // impossible. The live connection itself remains authenticated and open.
    store::organs::set_node_id(&guest.store.pool, &host_organ, None)
        .await
        .expect("remove fallback address");
    store::records::create(
        &guest.store.pool,
        store::records::NewRecord {
            slug: Some("live-reused"),
            kind: nucleus::RecordKind::Plain,
            head: "Rode the live connection",
            body: "one connection",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("write");
    assert!(
        !store::sync_ops::outbox_due(&guest.store.pool)
            .await
            .expect("queued")
            .is_empty(),
        "the test needs a reactive row to deliver"
    );

    assert_eq!(guest_wire.push_outbox().await.expect("drain"), 1);
    assert!(
        store::sync_ops::outbox_due(&guest.store.pool)
            .await
            .expect("drained")
            .is_empty(),
        "a live delivery must run the same seq-guarded outbox delete"
    );
    assert!(
        store::records::resolve(&host.store.pool, "live-reused")
            .await
            .expect("host read")
            .is_some(),
        "the delta did not arrive over the live connection"
    );

    connection.close(0u32.into(), b"test complete");
    serving.abort();
}

/// A contact with a login granted gets a real session on my Cell — over iroh,
/// as the Person I named — and what they can see is what that Person can see.
#[tokio::test]
async fn a_contact_with_a_login_drives_a_live_session_over_iroh() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[71; 32]), Reach::Local)
            .await
            .expect("host binds"),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[72; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");

    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;

    // Without a login there is no session to open, however known they are.
    let hub = Arc::new(transport::LaneHub::new());
    host_wire.set_live_handler(LiveHost::new(host.clone(), hub.clone()));
    let serving = {
        let host_wire = host_wire.clone();
        tokio::spawn(async move { host_wire.serve().await })
    };
    let host_addr = loopback(&host_wire);

    // Being a known contact is not being logged in. Without a granted binding
    // they are asked for a password like anyone else, and get NOTHING — no
    // challenge, no session — until they produce one.
    {
        let connection = guest_wire
            .endpoint()
            .connect(host_addr.clone(), ALPN_LIVE)
            .await
            .expect("dial");
        let (_send, mut recv) = connection.accept_bi().await.expect("stream");
        assert!(
            matches!(
                hear(&mut recv).await,
                ServerMessage::LiveHello {
                    login_required: true
                }
            ),
            "a known contact with NO login granted must be asked to log in",
        );
        // And nothing follows it. The session is not started, so no challenge
        // is ever written — a read here must time out rather than return.
        let leaked =
            tokio::time::timeout(std::time::Duration::from_millis(300), hear(&mut recv)).await;
        assert!(
            leaked.is_err(),
            "nothing may be served before the login: got {leaked:?}",
        );
        connection.close(0u32.into(), b"done");
    }

    // The Cell owner grants the login, naming the Person they act as.
    let person = host
        .act(
            Action::GrantOrganLogin {
                organ: guest_organ.clone(),
                person_name: "Marcia".into(),
            },
            None,
        )
        .await
        .expect("grant")
        .created
        .expect("the Person uid comes back");

    assert_eq!(
        store::logins::person_for_organ(&host.store.pool, &guest_organ)
            .await
            .expect("lookup"),
        Some(person.clone())
    );

    // Now the session opens.
    let connection = guest_wire
        .endpoint()
        .connect(host_addr, ALPN_LIVE)
        .await
        .expect("live dial");
    let (mut send, mut recv) = connection.accept_bi().await.expect("session stream");
    // The driver writes its first frame only once the stream exists, so poke
    // it before reading.
    say(
        &mut send,
        &ClientMessage::Subscribe {
            id: "q".into(),
            protein: serde_json::from_value(serde_json::json!({
                "source": "record",
                "where": [{ "kind_eq": "plain" }],
            }))
            .expect("protein"),
        },
    )
    .await;

    // First frame is the action-intent challenge; then our snapshot.
    let mut snapshot = None;
    for _ in 0..3 {
        match hear(&mut recv).await {
            ServerMessage::Snapshot { id, rows } => {
                assert_eq!(id, "q");
                snapshot = Some(rows);
                break;
            }
            _ => continue,
        }
    }
    let rows = snapshot.expect("the live session answers a Protein subscription");

    // Visibility is the Person's, not the Cell's: nothing has been shared
    // with Marcia, so a session that answered at all must answer empty.
    assert!(
        rows.is_empty(),
        "a live guest sees what their Person may see — granting a login is not \
         granting sight of everything: {rows:?}"
    );

    // Revoking is local and immediate.
    host.act(
        Action::RevokeOrganLogin {
            organ: guest_organ.clone(),
        },
        None,
    )
    .await
    .expect("revoke");
    assert!(
        store::logins::person_for_organ(&host.store.pool, &guest_organ)
            .await
            .expect("lookup")
            .is_none()
    );

    serving.abort();
}

/// A login is for a KNOWN contact. Someone who has only knocked on the thread
/// door may not act as a Person inside this Cell.
#[tokio::test]
async fn an_unvetted_contact_cannot_be_given_a_login() {
    let (host, _) = cell("http://host.test").await;
    store::organs::add_contact(&host.store.pool, "o-stranger", None, "Stranger", "", 1)
        .await
        .expect("contact");
    store::organs::set_trust(&host.store.pool, "o-stranger", "unknown")
        .await
        .expect("trust");

    let refused = host
        .act(
            Action::GrantOrganLogin {
                organ: "o-stranger".into(),
                person_name: "Whoever".into(),
            },
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "reaching the thread door is not the same as being allowed inside"
    );
}

/// Messaging and live editing are the same relationship, not two features:
/// after a login is granted, the thread they were already using keeps working.
#[tokio::test]
async fn granting_a_login_leaves_the_conversation_untouched() {
    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    let host_wire = Wire::bind(host.clone(), SecretKey::from_bytes(&[73; 32]), Reach::Local)
        .await
        .expect("host binds");
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[74; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");
    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;
    host_wire.remember_addr(loopback(&guest_wire));

    let (conversation, thread) = host
        .start_conversation(&guest_organ, "Beach plans")
        .await
        .expect("conversation");
    store::replica::offer(&guest.store.pool, &conversation, &host_organ)
        .await
        .expect("offer");
    guest
        .accept_conversation(&conversation, &host_organ)
        .await
        .expect("accept");
    store::replica::accept(&host.store.pool, &conversation, &guest_organ)
        .await
        .expect("host learns");

    host.act(
        Action::GrantOrganLogin {
            organ: guest_organ.clone(),
            person_name: "Marcia".into(),
        },
        None,
    )
    .await
    .expect("grant");

    // Send AFTER the login exists, and it still arrives on the thread channel.
    host.send_message(&thread, "me", "still talking")
        .await
        .expect("message");
    let serving = {
        let guest_wire = guest_wire.clone();
        tokio::spawn(async move { guest_wire.serve().await })
    };
    for _ in 0..5 {
        if host_wire.sync_once().await.expect("sync") == 0 {
            break;
        }
    }

    let messages: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM record WHERE kind = 'message' AND replica_root = ?",
    )
    .bind(&conversation)
    .fetch_one(&guest.store.pool)
    .await
    .expect("count");
    assert_eq!(
        messages, 1,
        "a login is an addition to the relationship, not a replacement for it"
    );

    serving.abort();
}

/// A live guest does not just READ — they act, and the write lands on the
/// host as the Person their login named.
///
/// This is the half of live mode that was missing. `Session` refuses a plain
/// `Act` from any authenticated session (a remote peer must sign its intent),
/// so the guest has to walk the real path a browser walks: take the server's
/// challenge, prove possession of an Ed25519 key for its Person, then send a
/// signed envelope. Nothing here is a shortcut for tests — it is the protocol.
#[tokio::test]
async fn a_live_guest_acts_on_the_host_and_the_write_lands_there() {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;
    use ed25519_dalek::{Signer as _, SigningKey};

    let (host, host_organ) = cell("http://host.test").await;
    let (guest, guest_organ) = cell("http://guest.test").await;

    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[81; 32]), Reach::Local)
            .await
            .expect("host binds"),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[82; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");
    know(&host, &guest_organ, &guest_wire.node_id().to_string()).await;
    know(&guest, &host_organ, &host_wire.node_id().to_string()).await;

    let hub = Arc::new(transport::LaneHub::new());
    host_wire.set_live_handler(LiveHost::new(host.clone(), hub.clone()));
    let serving = {
        let host_wire = host_wire.clone();
        tokio::spawn(async move { host_wire.serve().await })
    };
    let host_addr = loopback(&host_wire);

    let person = host
        .act(
            Action::GrantOrganLogin {
                organ: guest_organ.clone(),
                person_name: "Marcia".into(),
            },
            None,
        )
        .await
        .expect("grant")
        .created
        .expect("the Person uid comes back");

    let connection = guest_wire
        .endpoint()
        .connect(host_addr, ALPN_LIVE)
        .await
        .expect("live dial");
    let (mut send, mut recv) = connection.accept_bi().await.expect("session stream");

    // Poke the stream so the driver writes its first frame, then take the
    // challenge. The Person here is the host's decision, never our claim.
    say(&mut send, &ClientMessage::Unsubscribe { id: "wake".into() }).await;
    // A granted contact is told it needs no login, then gets the challenge.
    assert!(
        matches!(
            hear(&mut recv).await,
            ServerMessage::LiveHello {
                login_required: false
            }
        ),
        "a granted device must be told it is already in, not asked for a password",
    );
    let (session_id, challenge, announced_person) = match hear(&mut recv).await {
        ServerMessage::SessionChallenge {
            session_id,
            challenge,
            person,
            signing_required,
            ..
        } => {
            assert!(signing_required, "a live guest must sign its Actions");
            (session_id, challenge, person)
        }
        other => panic!("expected the session challenge first, got {other:?}"),
    };
    assert_eq!(
        announced_person.as_deref(),
        Some(person.as_str()),
        "the host must announce the Person the login bound, so the guest signs for the right identity",
    );

    // Prove possession of a key for that Person — the browser's WebCrypto step.
    let key = SigningKey::from_bytes(&[9u8; 32]);
    let public_key_base64 = B64.encode(key.verifying_key().as_bytes());
    let key_id = "guest-key-1".to_string();
    let proof_signature = B64.encode(
        key.sign(&nucleus::action_intent::session_authentication_bytes(
            &session_id,
            &challenge,
            &person,
            &key_id,
            &public_key_base64,
        ))
        .to_bytes(),
    );
    say(
        &mut send,
        &ClientMessage::SessionAuthenticate {
            id: "auth".into(),
            session_id: session_id.clone(),
            session_challenge: challenge.clone(),
            person_uid: person.clone(),
            key_id: key_id.clone(),
            public_key_base64,
            signature: proof_signature,
        },
    )
    .await;
    match hear(&mut recv).await {
        ServerMessage::Error { message, code, .. } => {
            panic!("key registration refused: {message} ({code:?})")
        }
        _ => {}
    }

    // Now the actual point: a signed Action.
    let action_base64 = B64.encode(
        serde_json::to_vec(&serde_json::json!({
            "action": "create-record",
            "kind": "plain",
            "head": "Written by the guest",
        }))
        .expect("action json"),
    );
    let message_id = "act-1".to_string();
    let signature = B64.encode(
        key.sign(&nucleus::action_intent::signing_bytes(
            &session_id,
            &challenge,
            1,
            &message_id,
            &action_base64,
        ))
        .to_bytes(),
    );
    say(
        &mut send,
        &ClientMessage::SignedAct {
            id: message_id.clone(),
            session_id,
            session_challenge: challenge,
            sequence: 1,
            action_base64,
            signature,
        },
    )
    .await;
    match hear(&mut recv).await {
        ServerMessage::ActionOk { .. } => {}
        ServerMessage::Error { message, code, .. } => {
            panic!("the guest's signed Action was refused: {message} ({code:?})")
        }
        other => panic!("expected ActionOk, got {other:?}"),
    }

    // The write is on the HOST, and it is attributed to the Person the login
    // named — not to the host Cell itself and not to the guest's Organ.
    let written = store::records::list_all(&host.store.pool)
        .await
        .expect("host records")
        .into_iter()
        .find(|record| record.head == "Written by the guest")
        .expect("the guest's record must exist on the host");

    let actor: Option<String> =
        store::sqlx::query_scalar("SELECT actor_uid FROM fact WHERE record_uid = ? LIMIT 1")
            .bind(&written.uid)
            .fetch_one(&host.store.pool)
            .await
            .expect("fact actor");
    assert_eq!(
        actor.as_deref(),
        Some(person.as_str()),
        "the guest's write must be attributed to their bound Person",
    );

    serving.abort();
}

/// Logging in from a Lince that has never been seen before — the case the
/// whole feature exists for.
///
/// The guest here is NOT a contact. No pairing, no `organ_login`, no key of
/// theirs on the host, nothing on either side that says these two have ever
/// met. That is deliberate: a login bound to a device is not a login, it is an
/// enrolment, and it cannot answer "I am on someone else's computer in another
/// country and I want into my Lince". A username and password can, and this is
/// the same credential the HTTP login checks.
#[tokio::test]
async fn a_stranger_with_a_password_gets_in_and_a_wrong_one_never_does() {
    let (host, _host_organ) = cell("http://host.test").await;
    let (guest, _guest_organ) = cell("http://guest.test").await;

    // A person on the host, with a credential. Nothing else about them.
    let admin_role = store::auth::ensure_role(&host.store.pool, store::auth::ADMIN_ROLE)
        .await
        .expect("role");
    let hash = utils::auth::hash_password("correct horse battery").expect("hash");
    store::auth::create_person_login(&host.store.pool, "Eduardo", "eduardo", &hash, admin_role)
        .await
        .expect("credential");
    let person = store::auth::user_by_username(&host.store.pool, "eduardo")
        .await
        .expect("lookup")
        .expect("there")
        .uid;

    let host_wire = Arc::new(
        Wire::bind(host.clone(), SecretKey::from_bytes(&[81; 32]), Reach::Local)
            .await
            .expect("host binds"),
    );
    let guest_wire = Wire::bind(
        guest.clone(),
        SecretKey::from_bytes(&[82; 32]),
        Reach::Local,
    )
    .await
    .expect("guest binds");
    let hub = Arc::new(transport::LaneHub::new());
    host_wire.set_live_handler(LiveHost::new(host.clone(), hub.clone()));
    let host_addr = loopback(&host_wire);
    let serving = {
        let host_wire = host_wire.clone();
        tokio::spawn(async move { host_wire.serve().await })
    };

    // A wrong password gets one refusal and the session ends. It must not say
    // whether the USER exists — a message that distinguishes the two hands a
    // guesser half the answer.
    {
        let connection = guest_wire
            .endpoint()
            .connect(host_addr.clone(), ALPN_LIVE)
            .await
            .expect("dial");
        let (mut send, mut recv) = connection.accept_bi().await.expect("stream");
        assert!(
            matches!(
                hear(&mut recv).await,
                ServerMessage::LiveHello { login_required: true }
            ),
            "a peer with no granted binding must be asked to log in"
        );
        say(
            &mut send,
            &ClientMessage::LiveLogin {
                username: "eduardo".into(),
                password: "hunter2".into(),
            },
        )
        .await;
        let ServerMessage::LiveLoginError { message } = hear(&mut recv).await else {
            panic!("a wrong password must be refused");
        };
        assert_eq!(message, "Invalid username or password");
        connection.close(0u32.into(), b"refused");

        // And a nonexistent user is refused in exactly the same words.
        let connection2 = guest_wire
            .endpoint()
            .connect(host_addr.clone(), ALPN_LIVE)
            .await
            .expect("dial");
        let (mut send, mut recv) = connection2.accept_bi().await.expect("stream");
        let _ = hear(&mut recv).await;
        say(
            &mut send,
            &ClientMessage::LiveLogin {
                username: "nobody-at-all".into(),
                password: "hunter2".into(),
            },
        )
        .await;
        let ServerMessage::LiveLoginError { message: other } = hear(&mut recv).await else {
            panic!("an unknown user must be refused");
        };
        assert_eq!(
            other, message,
            "the refusal must not reveal whether the username exists"
        );
        connection2.close(0u32.into(), b"refused");
    }

    // The right password gets a real session, as that Person.
    let connection = guest_wire
        .endpoint()
        .connect(host_addr, ALPN_LIVE)
        .await
        .expect("dial");
    let (mut send, mut recv) = connection.accept_bi().await.expect("stream");
    let _ = hear(&mut recv).await; // hello
    say(
        &mut send,
        &ClientMessage::LiveLogin {
            username: "eduardo".into(),
            password: "correct horse battery".into(),
        },
    )
    .await;
    let ServerMessage::LiveLoginOk { person: got, .. } = hear(&mut recv).await else {
        panic!("the right password must get in");
    };
    assert_eq!(got, person, "and act as the Person that credential names");

    // The session that follows is a real one: the challenge names the same
    // Person, which is what every visibility decision downstream rests on.
    let mut bound = None;
    for _ in 0..3 {
        if let ServerMessage::SessionChallenge {
            person: challenged,
            signing_required,
            ..
        } = hear(&mut recv).await
        {
            assert!(signing_required, "a remote session must sign its Actions");
            bound = challenged;
            break;
        }
    }
    assert_eq!(
        bound,
        Some(person),
        "the session is bound to the Person the password proved, not to any key"
    );

    serving.abort();
}
